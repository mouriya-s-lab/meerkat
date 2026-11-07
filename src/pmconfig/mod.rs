//! The PM config repository as the config page uses it (decision §H):
//! the configured branch is the one authority. Its files are listed and
//! opened from GitHub every time; a candidate file is judged by every
//! program that reads it (council and guardians `POST /v1/config/check`,
//! the dispatch launcher's `POST /pm-config/check`, Quorum's own parsers);
//! a save is one contents-API commit that GitHub refuses when the branch
//! moved on; what each program runs is compared with the branch by git
//! blob sha. Nothing here keeps a copy of a file or a verdict.

#[cfg(not(target_arch = "wasm32"))]
pub mod checkout;
pub mod lines;

use std::collections::BTreeMap;

use base64::{Engine, engine::general_purpose::STANDARD};
use futures_util::future::{join_all, try_join_all};
use nsh_core::prelude::HttpRequest;
use serde::{Deserialize, Serialize};

use crate::{
    config::{self, BaseUrl, ConfigRepo, Endpoints, Reader},
    gh::GitHub,
    http,
};

/// A file and its git blob sha: a file of the branch, or one a program
/// loaded (`GET /v1/config`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileSha {
    pub path: String,
    pub sha: String,
}

/// The branch as last listed: its head commit and every file in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing {
    pub head: String,
    pub files: Vec<FileSha>,
}

impl Listing {
    /// The blob sha of `path` on the branch.
    pub fn sha_of(&self, path: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|file| file.path == path)
            .map(|file| file.sha.as_str())
    }
}

/// A file as it was opened from the branch: the sha a save replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    pub path: String,
    pub sha: String,
    pub text: String,
}

/// An error a reader found: the file, the key path in it, why.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Violation {
    pub file: String,
    pub key: String,
    pub message: String,
}

/// A reader's conclusion on a candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// It loads.
    Pass,
    /// It does not: every error the reader reported.
    Fail(Vec<Violation>),
    /// The reader gave no conclusion: why.
    Unreachable(String),
    /// The candidate set could not be read from the branch: why.
    NoInput(String),
}

/// What a program reports it loaded at startup (`GET /v1/config`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub loaded_at: String,
    pub files: Vec<FileSha>,
}

/// What runs, against the branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Version {
    /// Every loaded file has the branch's sha.
    Same,
    /// These loaded files differ from the branch (or are gone from it).
    Older(Vec<String>),
}

/// `loaded` (path and sha of every file a program loaded) against the
/// branch's files.
pub fn compare<'a>(loaded: impl IntoIterator<Item = (&'a str, &'a str)>, listing: &Listing) -> Version {
    let older: Vec<String> = loaded
        .into_iter()
        .filter(|(path, sha)| listing.sha_of(path) != Some(sha))
        .map(|(path, _)| path.to_owned())
        .collect();
    if older.is_empty() {
        Version::Same
    } else {
        Version::Older(older)
    }
}

/// What a save did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Saved {
    /// One commit on the branch: its sha and page.
    Committed { commit: String, url: String },
    /// The branch's file is no longer the one opened (GitHub's 409):
    /// nothing was written.
    Conflict,
}

/// The candidate files a reader judges.
type Files = BTreeMap<String, String>;

/// The config repository's branch and the readers of its files, at this
/// build's addresses.
#[derive(Debug, Clone)]
pub struct ConfigRepoClient {
    github: GitHub,
    config: ConfigRepo,
    endpoints: Endpoints,
}

impl ConfigRepoClient {
    pub fn new(github: GitHub, config: ConfigRepo, endpoints: Endpoints) -> Self {
        Self {
            github,
            config,
            endpoints,
        }
    }

    pub fn repo(&self) -> &str {
        self.config.repo.as_str()
    }

    pub fn branch(&self) -> &str {
        self.config.branch.as_str()
    }

    /// The programs that read `path`.
    pub fn readers(&self, path: &str) -> Vec<Reader> {
        self.config.readers.of(path)
    }

    /// The branch's head commit and every file in it.
    pub async fn listing(&self) -> Result<Listing, String> {
        #[derive(Deserialize)]
        struct Branch {
            commit: Commit,
        }
        #[derive(Deserialize)]
        struct Commit {
            sha: String,
        }
        #[derive(Deserialize)]
        struct Tree {
            tree: Vec<Entry>,
            truncated: bool,
        }
        #[derive(Deserialize)]
        struct Entry {
            path: String,
            #[serde(rename = "type")]
            kind: String,
            sha: String,
        }
        let repo = self.repo();
        let branch: Branch = self
            .github
            .get(&format!("/repos/{repo}/branches/{}", self.branch()))
            .await?;
        let head = branch.commit.sha;
        let tree: Tree = self
            .github
            .get(&format!("/repos/{repo}/git/trees/{head}?recursive=1"))
            .await?;
        if tree.truncated {
            return Err(format!("{repo}@{head}: GitHub 截断了文件树"));
        }
        let files = tree
            .tree
            .into_iter()
            .filter(|entry| entry.kind == "blob")
            .map(|entry| FileSha {
                path: entry.path,
                sha: entry.sha,
            })
            .collect();
        Ok(Listing { head, files })
    }

    /// `path` as the branch has it now.
    pub async fn open(&self, path: &str) -> Result<Opened, String> {
        #[derive(Deserialize)]
        struct Contents {
            sha: String,
            content: String,
            encoding: String,
        }
        let contents: Contents = self
            .github
            .get(&format!(
                "/repos/{}/contents/{path}?ref={}",
                self.repo(),
                self.branch()
            ))
            .await?;
        if contents.encoding != "base64" {
            return Err(format!("{path}: 内容编码为 {}", contents.encoding));
        }
        let packed: String = contents.content.split_whitespace().collect();
        let bytes = STANDARD
            .decode(packed)
            .map_err(|error| format!("{path}: {error}"))?;
        let text = String::from_utf8(bytes).map_err(|_| format!("{path}: 不是 UTF-8 文本"))?;
        Ok(Opened {
            path: path.to_owned(),
            sha: contents.sha,
            text,
        })
    }

    /// The branch's text of each of `paths`, `path` replaced by `draft`
    /// (and added when the branch lacks it or the list does not name it).
    async fn candidate(
        &self,
        paths: impl IntoIterator<Item = String>,
        path: &str,
        draft: &str,
    ) -> Result<Files, String> {
        let others: Vec<String> = paths.into_iter().filter(|other| other != path).collect();
        let texts = try_join_all(others.iter().map(|other| self.open(other))).await?;
        let mut files: Files = texts
            .into_iter()
            .map(|opened| (opened.path, opened.text))
            .collect();
        files.insert(path.to_owned(), draft.to_owned());
        Ok(files)
    }

    /// `reader`'s conclusion on the branch's files with `path` as `draft`.
    pub async fn check(&self, reader: Reader, path: &str, draft: &str) -> Verdict {
        match reader {
            Reader::Council | Reader::Guardians => {
                let base = match reader {
                    Reader::Council => &self.endpoints.council,
                    _ => &self.endpoints.guardians,
                };
                let running = match running(base).await {
                    Ok(running) => running,
                    Err(error) => return Verdict::Unreachable(error),
                };
                let paths = running.files.into_iter().map(|file| file.path);
                match self.candidate(paths, path, draft).await {
                    Ok(files) => ask(base.join("/v1/config/check"), &files).await,
                    Err(error) => Verdict::NoInput(error),
                }
            }
            Reader::Dispatch => {
                let paths = DISPATCH_FILES.iter().map(|file| (*file).to_owned());
                match self.candidate(paths, path, draft).await {
                    Ok(mut files) => {
                        // The launcher takes exactly its three files.
                        files.retain(|file, _| DISPATCH_FILES.contains(&file.as_str()));
                        ask(self.endpoints.dispatch.join("/pm-config/check"), &files).await
                    }
                    Err(error) => Verdict::NoInput(error),
                }
            }
            Reader::Quorum => {
                let paths = config::FILES.iter().map(|file| (*file).to_owned());
                match self.candidate(paths, path, draft).await {
                    Ok(files) => {
                        let bytes = config::FILES.map(|file| files[file].as_bytes());
                        match config::check(bytes) {
                            Ok(()) => Verdict::Pass,
                            Err(error) => Verdict::Fail(vec![Violation {
                                file: error.file,
                                key: error.path,
                                message: error.message,
                            }]),
                        }
                    }
                    Err(error) => Verdict::NoInput(error),
                }
            }
        }
    }

    /// Every reader's conclusion on `draft` as `path`, asked at once, in
    /// the readers' order.
    pub async fn checks(&self, path: &str, draft: &str) -> Vec<(Reader, Verdict)> {
        let readers = self.readers(path);
        let verdicts = join_all(readers.iter().map(|&reader| self.check(reader, path, draft))).await;
        readers.into_iter().zip(verdicts).collect()
    }

    /// Commits `draft` as `opened.path` on the branch, replacing the file
    /// that was opened (`quorum: edit <path>`).
    pub async fn save(&self, opened: &Opened, draft: &str) -> Result<Saved, String> {
        #[derive(Serialize)]
        struct Put<'a> {
            message: String,
            content: String,
            sha: &'a str,
            branch: &'a str,
        }
        #[derive(Deserialize)]
        struct Answer {
            commit: Commit,
        }
        #[derive(Deserialize)]
        struct Commit {
            sha: String,
            html_url: String,
        }
        let path = format!("/repos/{}/contents/{}", self.repo(), opened.path);
        let response = self
            .github
            .put(
                &path,
                &Put {
                    message: format!("quorum: edit {}", opened.path),
                    content: STANDARD.encode(draft),
                    sha: &opened.sha,
                    branch: self.branch(),
                },
            )
            .await?;
        match response.status() {
            200 | 201 => {
                let answer: Answer = serde_json::from_slice(&response.body)
                    .map_err(|error| format!("{}: 响应无法解析：{error}", self.github.url(&path)))?;
                Ok(Saved::Committed {
                    commit: answer.commit.sha,
                    url: answer.commit.html_url,
                })
            }
            409 => Ok(Saved::Conflict),
            _ => Err(http::failure(&self.github.url(&path), &response)),
        }
    }
}

/// The three PM files the dispatch launcher judges, all together.
const DISPATCH_FILES: [&str; 3] = ["shared/repos.yaml", "shared/project.yaml", "dispatch/delivery.yaml"];

/// What the program at `base` loaded at startup (`GET /v1/config`).
pub async fn running(base: &BaseUrl) -> Result<Running, String> {
    http::json(HttpRequest::get(base.join("/v1/config"))).await
}

/// `POST url {"files": …}`: the reader's `{ok}` or `{ok: false, errors}`.
async fn ask(url: String, files: &Files) -> Verdict {
    #[derive(Serialize)]
    struct Body<'a> {
        files: &'a Files,
    }
    #[derive(Deserialize)]
    struct Answer {
        ok: bool,
        #[serde(default)]
        errors: Vec<Violation>,
    }
    let body = match serde_json::to_vec(&Body { files }) {
        Ok(body) => body,
        Err(error) => return Verdict::Unreachable(error.to_string()),
    };
    let request = HttpRequest::post(url, body).header("Content-Type", "application/json");
    match http::json::<Answer>(request).await {
        Ok(Answer { ok: true, .. }) => Verdict::Pass,
        Ok(Answer { ok: false, errors }) => Verdict::Fail(errors),
        Err(error) => Verdict::Unreachable(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing(files: &[(&str, &str)]) -> Listing {
        Listing {
            head: "c0ffee".to_owned(),
            files: files
                .iter()
                .map(|(path, sha)| FileSha {
                    path: (*path).to_owned(),
                    sha: (*sha).to_owned(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_program_runs_the_branch_when_every_loaded_sha_matches() {
        let branch = listing(&[("council/seats.yaml", "a"), ("shared/repos.yaml", "b")]);
        assert_eq!(compare([("council/seats.yaml", "a")], &branch), Version::Same);
        assert_eq!(
            compare([("council/seats.yaml", "old"), ("shared/repos.yaml", "b")], &branch),
            Version::Older(vec!["council/seats.yaml".to_owned()])
        );
        assert_eq!(
            compare([("council/gone.yaml", "x")], &branch),
            Version::Older(vec!["council/gone.yaml".to_owned()])
        );
    }

    #[test]
    fn the_check_answers_parse_into_verdicts() {
        let pass: Running = serde_json::from_str(
            r#"{"loadedAt":"2026-11-07T12:06:21Z","files":[{"path":"guardians/bases.yaml","sha":"24d7"}]}"#,
        )
        .unwrap();
        assert_eq!(pass.files[0].path, "guardians/bases.yaml");
        let violation: Violation = serde_json::from_str(
            r#"{"file":"shared/repos.yaml","key":"bogus","message":"unknown field \"bogus\""}"#,
        )
        .unwrap();
        assert_eq!(violation.key, "bogus");
    }
}
