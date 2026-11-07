//! Requests to the GitHub API at the configured root. The macOS build holds
//! a token and sends it; the web build holds none: its root is the
//! same-origin proxy, which adds the credential (`deploy/Caddyfile`).

use nsh_core::prelude::{HttpRequest, HttpResponse, Signal, signal};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{config::BaseUrl, http};

/// How requests are authorised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    /// A token sent as `Authorization: Bearer` (macOS).
    Token(String),
    /// None here: the proxy in front of the API adds it (web).
    Proxy,
    /// None could be found: why; every request fails with it.
    Missing(String),
}

impl Credential {
    /// The credential this build uses, `None` while it is fetched: the
    /// proxy's on the web, at once.
    #[cfg(target_arch = "wasm32")]
    pub fn for_this_build() -> Signal<Option<Self>> {
        signal(Some(Credential::Proxy))
    }

    /// The credential this build uses, `None` while it is fetched:
    /// `GH_TOKEN` at once, else what `gh auth token` prints, asked once on
    /// a blocking thread (it may take long or never answer) and set when
    /// it answers. Made under the current owner, which ends the wait.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn for_this_build() -> Signal<Option<Self>> {
        if let Some(token) = std::env::var("GH_TOKEN")
            .ok()
            .map(|token| token.trim().to_owned())
            .filter(|token| !token.is_empty())
        {
            return signal(Some(Credential::Token(token)));
        }
        let credential = signal(None);
        nsh_core::prelude::spawn(async move {
            let asked = nsh_core::prelude::blocking(|| {
                Credential::from_gh(std::process::Command::new("gh").args(["auth", "token"]).output())
            })
            .await;
            credential.set(Some(asked.unwrap_or_else(|error| {
                Credential::Missing(format!("GH_TOKEN 为空，`gh auth token` 失败：{error}"))
            })));
        });
        credential
    }

    /// The credential `gh auth token` gave, or why there is none.
    #[cfg(not(target_arch = "wasm32"))]
    fn from_gh(ran: std::io::Result<std::process::Output>) -> Self {
        match ran {
            Ok(output) if output.status.success() => {
                let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
                if token.is_empty() {
                    Credential::Missing("GH_TOKEN 为空，`gh auth token` 没有输出".to_owned())
                } else {
                    Credential::Token(token)
                }
            }
            Ok(output) => Credential::Missing(format!(
                "GH_TOKEN 为空，`gh auth token` 失败：{}",
                String::from_utf8_lossy(&output.stderr).trim()
            )),
            Err(error) => Credential::Missing(format!(
                "GH_TOKEN 为空，无法执行 `gh auth token`：{error}"
            )),
        }
    }
}

/// The GitHub API at one root, with one credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHub {
    api: BaseUrl,
    credential: Credential,
}

impl GitHub {
    pub fn new(api: BaseUrl, credential: Credential) -> Self {
        Self { api, credential }
    }

    fn request(&self, method: &str, path: &str) -> Result<HttpRequest, String> {
        let request = HttpRequest::new(method, self.api.join(path))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        match &self.credential {
            Credential::Token(token) => {
                Ok(request.header("Authorization", format!("Bearer {token}")))
            }
            Credential::Proxy => Ok(request),
            Credential::Missing(why) => Err(why.clone()),
        }
    }

    /// `GET path` (from the API root, with its query), parsed as `T`.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        http::json(self.request("GET", path)?).await
    }

    /// `GET path` as GitHub has it now, parsed as `T`: for what changes
    /// (a branch, a file on a branch). GitHub marks such answers fresh for a
    /// minute (`max-age=60`); `Cache-Control: no-cache` sends the request
    /// past the HTTP cache in front of it (the browser's, `NSURLCache`).
    pub async fn get_current<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        http::json(self.request("GET", path)?.header("Cache-Control", "no-cache")).await
    }

    /// `PUT path` with `body` as JSON: the response whatever its status, so
    /// the caller tells a conflict from a failure.
    pub async fn put<B: Serialize>(&self, path: &str, body: &B) -> Result<HttpResponse, String> {
        let body = serde_json::to_vec(body).map_err(|error| error.to_string())?;
        let request = self
            .request("PUT", path)?
            .header("Content-Type", "application/json")
            .body(body);
        http::send(request).await
    }

    /// The URL of `path` under the API root (for an error's text).
    pub fn url(&self, path: &str) -> String {
        self.api.join(path)
    }

    /// The GraphQL `query` with `variables`: its `data` as `T`, or the
    /// errors GitHub gave.
    pub async fn graphql<V: Serialize, T: DeserializeOwned>(
        &self,
        query: &str,
        variables: &V,
    ) -> Result<T, String> {
        #[derive(Serialize)]
        struct Body<'a, V> {
            query: &'a str,
            variables: &'a V,
        }
        #[derive(Deserialize)]
        struct Answer<T> {
            data: Option<T>,
            #[serde(default)]
            errors: Vec<GraphQlError>,
        }
        #[derive(Deserialize)]
        struct GraphQlError {
            message: String,
        }
        let body = serde_json::to_vec(&Body { query, variables })
            .map_err(|error| error.to_string())?;
        let request = self
            .request("POST", "/graphql")?
            .header("Content-Type", "application/json")
            .body(body);
        let answer: Answer<T> = http::json(request).await?;
        match (answer.data, answer.errors.is_empty()) {
            (Some(data), true) => Ok(data),
            (_, false) => Err(answer
                .errors
                .into_iter()
                .map(|error| error.message)
                .collect::<Vec<_>>()
                .join("; ")),
            (None, true) => Err("GraphQL 没有返回数据".to_owned()),
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::{
        io,
        os::unix::process::ExitStatusExt,
        process::{ExitStatus, Output},
    };

    use super::Credential;

    fn ran(code: i32, stdout: &str, stderr: &str) -> io::Result<Output> {
        Ok(Output {
            // A wait status: the exit code in the second byte.
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        })
    }

    #[test]
    fn gh_answers_become_the_credential_or_why_there_is_none() {
        assert_eq!(
            Credential::from_gh(ran(0, "gho_abc\n", "")),
            Credential::Token("gho_abc".to_owned())
        );
        assert_eq!(
            Credential::from_gh(ran(0, " \n", "")),
            Credential::Missing("GH_TOKEN 为空，`gh auth token` 没有输出".to_owned())
        );
        assert_eq!(
            Credential::from_gh(ran(1, "", "no oauth token\n")),
            Credential::Missing("GH_TOKEN 为空，`gh auth token` 失败：no oauth token".to_owned())
        );
        let Credential::Missing(why) =
            Credential::from_gh(Err(io::Error::from(io::ErrorKind::NotFound)))
        else {
            panic!("a gh that cannot run gives no credential");
        };
        assert!(why.starts_with("GH_TOKEN 为空，无法执行 `gh auth token`："), "{why}");
    }
}
