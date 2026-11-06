//! The three PM config files the dashboard reads, as types: what they may
//! hold is what parses. `build.rs` includes this file too, so the web build
//! rejects a config with the same messages the macOS app shows.
//!
//! Every error names the file and the key path (`targets.web.council`).

use std::{fmt, num::NonZeroU32};

use serde::{Deserialize, de::DeserializeOwned};

/// `dashboard/dashboard.yaml`, relative to the PM config directory.
pub const DASHBOARD_FILE: &str = "dashboard/dashboard.yaml";
/// `shared/repos.yaml`, relative to the PM config directory.
pub const REPOS_FILE: &str = "shared/repos.yaml";
/// `shared/project.yaml`, relative to the PM config directory.
pub const PROJECT_FILE: &str = "shared/project.yaml";

/// Why a config cannot be used: the file, the key path in it (empty for
/// the file as a whole) and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    pub file: String,
    pub path: String,
    pub message: String,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}: {}", self.file, self.message)
        } else {
            write!(f, "{}: {}: {}", self.file, self.path, self.message)
        }
    }
}

/// The text of the three files.
#[derive(Debug, Clone, Copy)]
pub struct ConfigText<'a> {
    pub dashboard: &'a str,
    pub repos: &'a str,
    pub project: &'a str,
}

/// The three files, parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub dashboard: Dashboard,
    pub repos: Repos,
    pub project: Project,
}

/// Parses the three files; the first error found, with its file and key
/// path.
pub fn parse(text: ConfigText<'_>) -> Result<Config, ConfigError> {
    Ok(Config {
        dashboard: parse_file(DASHBOARD_FILE, text.dashboard)?,
        repos: parse_file(REPOS_FILE, text.repos)?,
        project: parse_file(PROJECT_FILE, text.project)?,
    })
}

fn parse_file<T: DeserializeOwned>(file: &str, text: &str) -> Result<T, ConfigError> {
    let deserializer = serde_yaml_ng::Deserializer::from_str(text);
    serde_path_to_error::deserialize(deserializer).map_err(|error| {
        let path = error.path().to_string();
        ConfigError {
            file: file.to_owned(),
            path: if path == "." { String::new() } else { path },
            message: error.into_inner().to_string(),
        }
    })
}

// ---------------------------------------------------------------------------
// dashboard/dashboard.yaml

/// `dashboard/dashboard.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Dashboard {
    pub targets: Targets,
    /// Seconds between two refreshes of every source.
    pub refresh_seconds: NonZeroU32,
    pub attention: Attention,
    pub digest: Digest,
}

/// The addresses each build talks to.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Targets {
    pub macos: Endpoints,
    pub web: Endpoints,
}

/// Where the GitHub API, the council and the guardians answer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Endpoints {
    pub github_api: BaseUrl,
    pub council: BaseUrl,
    pub guardians: BaseUrl,
}

/// What makes the overview ask for a person.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Attention {
    /// A failed session counts for this many hours after it started.
    pub session_failure_hours: NonZeroU32,
    /// GitHub API remaining below this share of the limit asks for a person.
    pub rate_limit_percent: Percent,
}

/// How a session's record is reduced to its decision log.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Digest {
    /// Tools whose calls show with their arguments and result.
    pub keep_tools: Vec<Name>,
    /// Tools whose consecutive calls show as one count per tool.
    pub fold_tools: Vec<Name>,
    /// Whether the assistant's text shows.
    pub show_assistant_text: bool,
    /// Records longer than this many bytes are parsed from their tail only.
    pub max_bytes: NonZeroU32,
}

/// An absolute `http` or `https` URL without a trailing slash, to which
/// paths (`/v1/health`) are appended.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct BaseUrl(String);

impl BaseUrl {
    /// The URL of `path` (which starts with `/`) under this one.
    pub fn join(&self, path: &str) -> String {
        format!("{}{path}", self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for BaseUrl {
    type Error = String;

    fn try_from(url: String) -> Result<Self, String> {
        let rest = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .ok_or_else(|| format!("{url:?} is not an absolute http(s) URL"))?;
        if rest.is_empty() || rest.starts_with('/') {
            return Err(format!("{url:?} has no host"));
        }
        if rest.contains(['?', '#']) {
            return Err(format!("{url:?} has a query or fragment"));
        }
        Ok(Self(url.trim_end_matches('/').to_owned()))
    }
}

/// A whole percentage, 0 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "u8")]
pub struct Percent(u8);

impl Percent {
    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Percent {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, String> {
        if value <= 100 {
            Ok(Self(value))
        } else {
            Err(format!("{value} is over 100"))
        }
    }
}

// ---------------------------------------------------------------------------
// shared/repos.yaml

/// `shared/repos.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Repos {
    pub pm: PmRepo,
    pub code: Vec<CodeRepo>,
    pub design: Vec<RepoName>,
    pub omp: Vec<RepoName>,
}

/// The repository of the agenda issues and their Project.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PmRepo {
    pub repo: RepoName,
    pub project: ProjectRef,
}

/// A GitHub Project (v2): its owner's login and its number.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRef {
    pub owner: Name,
    pub number: NonZeroU32,
}

/// A code repository and its working directory on this machine.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CodeRepo {
    pub repo: RepoName,
    pub cwd: Name,
}

/// A repository, `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize)]
#[serde(try_from = "String")]
pub struct RepoName(String);

impl RepoName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for RepoName {
    type Error = String;

    fn try_from(name: String) -> Result<Self, String> {
        let valid = |part: &str| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        match name.split_once('/') {
            Some((owner, repo)) if valid(owner) && valid(repo) => Ok(Self(name)),
            _ => Err(format!("{name:?} is not owner/name")),
        }
    }
}

// ---------------------------------------------------------------------------
// shared/project.yaml

/// `shared/project.yaml`: the Project's field and option names.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub fields: Fields,
}

/// The Project fields PM uses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fields {
    pub status: SelectField<StatusOptions>,
    pub kind: SelectField<KindOptions>,
    pub agenda: TextField,
    pub proposer: SelectField<ProposerOptions>,
}

/// A single-select field: its name and the names of its options.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectField<O> {
    pub name: Name,
    pub options: O,
}

/// A text field: its name.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextField {
    pub name: Name,
}

/// The names of `Status`'s options.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StatusOptions {
    pub todo: Name,
    pub in_progress: Name,
    pub done: Name,
    pub superseded: Name,
}

/// The names of `Kind`'s options.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KindOptions {
    pub human: Name,
    pub agenda: Name,
    pub implementation: Name,
}

/// The names of `Proposer`'s options.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposerOptions {
    pub human: Name,
    pub guardians: Name,
    pub coordination: Name,
}

/// A non-empty name.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Name(String);

impl Name {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for Name {
    type Error = String;

    fn try_from(name: String) -> Result<Self, String> {
        if name.trim().is_empty() {
            Err("is empty".to_owned())
        } else {
            Ok(Self(name))
        }
    }
}
