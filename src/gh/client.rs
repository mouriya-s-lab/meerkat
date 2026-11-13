//! Requests to the GitHub API at the configured root. The macOS build holds
//! no token: each request asks the local `gh` for the one it has now
//! (`gh auth token`), so the account `gh` is on is the one used. The web
//! build holds none either: its root is the same-origin proxy, which adds
//! the credential (`deploy/Caddyfile`).

use nsh_core::prelude::{HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[cfg(not(target_arch = "wasm32"))]
use crate::failure::NoCredential;
use crate::{config::BaseUrl, failure::Failure, http};

/// The token the local `gh` holds now, asked on a blocking thread: `gh`
/// may take long or never answer, and the UI keeps drawing meanwhile.
#[cfg(not(target_arch = "wasm32"))]
async fn gh_token() -> Result<String, NoCredential> {
    nsh_core::prelude::blocking(|| {
        token_from_gh(std::process::Command::new("gh").args(["auth", "token"]).output())
    })
    .await
    .unwrap_or_else(|error| Err(NoCredential::GhFailed(error.to_string())))
}

/// The token `gh auth token` printed, or why there is none.
#[cfg(not(target_arch = "wasm32"))]
fn token_from_gh(ran: std::io::Result<std::process::Output>) -> Result<String, NoCredential> {
    match ran {
        Ok(output) if output.status.success() => {
            let token = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if token.is_empty() { Err(NoCredential::GhSilent) } else { Ok(token) }
        }
        Ok(output) => Err(NoCredential::GhFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        )),
        Err(error) => Err(NoCredential::GhCannotRun(error.to_string())),
    }
}

/// The GitHub API at one root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHub {
    api: BaseUrl,
}

impl GitHub {
    pub fn new(api: BaseUrl) -> Self {
        Self { api }
    }

    /// A request to `path`, authorised with what `gh` holds as it is made
    /// (macOS).
    #[cfg(not(target_arch = "wasm32"))]
    async fn request(&self, method: &str, path: &str) -> Result<HttpRequest, Failure> {
        let token = gh_token().await.map_err(Failure::NoCredential)?;
        Ok(self.bare(method, path).header("Authorization", format!("Bearer {token}")))
    }

    /// A request to `path`, which the proxy authorises (web).
    #[cfg(target_arch = "wasm32")]
    async fn request(&self, method: &str, path: &str) -> Result<HttpRequest, Failure> {
        Ok(self.bare(method, path))
    }

    fn bare(&self, method: &str, path: &str) -> HttpRequest {
        HttpRequest::new(method, self.api.join(path))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
    }

    /// `GET path` (from the API root, with its query), parsed as `T`.
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, Failure> {
        http::json(self.request("GET", path).await?).await
    }

    /// `GET path` as GitHub has it now, parsed as `T`: for what changes
    /// (a branch, a file on a branch). GitHub marks such answers fresh for a
    /// minute (`max-age=60`); `Cache-Control: no-cache` sends the request
    /// past the HTTP cache in front of it (the browser's, `NSURLCache`).
    pub async fn get_current<T: DeserializeOwned>(&self, path: &str) -> Result<T, Failure> {
        http::json(self.request("GET", path).await?.header("Cache-Control", "no-cache")).await
    }

    /// `PUT path` with `body` as JSON: the response whatever its status, so
    /// the caller tells a conflict from a failure.
    pub async fn put<B: Serialize>(&self, path: &str, body: &B) -> Result<HttpResponse, Failure> {
        let body = serde_json::to_vec(body).map_err(|error| Failure::Said(error.to_string()))?;
        let request = self
            .request("PUT", path)
            .await?
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
    ) -> Result<T, Failure> {
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
            .map_err(|error| Failure::Said(error.to_string()))?;
        let request = self
            .request("POST", "/graphql")
            .await?
            .header("Content-Type", "application/json")
            .body(body);
        let answer: Answer<T> = http::json(request).await?;
        match (answer.data, answer.errors.is_empty()) {
            (Some(data), true) => Ok(data),
            (_, false) => Err(Failure::Said(
                answer
                    .errors
                    .into_iter()
                    .map(|error| error.message)
                    .collect::<Vec<_>>()
                    .join("; "),
            )),
            (None, true) => Err(Failure::GraphQlNoData),
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

    use super::token_from_gh;
    use crate::failure::NoCredential;

    fn ran(code: i32, stdout: &str, stderr: &str) -> io::Result<Output> {
        Ok(Output {
            // A wait status: the exit code in the second byte.
            status: ExitStatus::from_raw(code << 8),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        })
    }

    #[test]
    fn gh_answers_become_the_token_or_why_there_is_none() {
        assert_eq!(token_from_gh(ran(0, "gho_abc\n", "")), Ok("gho_abc".to_owned()));
        assert_eq!(token_from_gh(ran(0, " \n", "")), Err(NoCredential::GhSilent));
        assert_eq!(
            token_from_gh(ran(1, "", "no oauth token\n")),
            Err(NoCredential::GhFailed("no oauth token".to_owned()))
        );
        assert!(matches!(
            token_from_gh(Err(io::Error::from(io::ErrorKind::NotFound))),
            Err(NoCredential::GhCannotRun(_))
        ));
    }
}
