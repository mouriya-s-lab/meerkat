//! The three PM config files Quorum reads, as types: what they may
//! hold is what parses.
//!
//! Every error names the file and the key path (`targets.web.council`).

use std::{
    fmt,
    num::NonZeroU32,
};

use serde::{Deserialize, de::DeserializeOwned};

/// `quorum/quorum.yaml`, relative to the PM config directory.
pub const QUORUM_FILE: &str = "quorum/quorum.yaml";
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
    pub message: Problem,
}

/// What is wrong with a config file. Quorum's own findings are variants
/// (the UI words them); the parser's and the system's words pass through.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "String")]
pub enum Problem {
    /// The YAML parser's, a reader's or the system's words, as they are.
    Said(String),
    /// Not UTF-8: the decoder's error.
    NotUtf8(String),
    /// The variable naming the PM config directory is not set.
    DirUnset,
    /// Not JSON: the parser's error.
    NotJson(String),
    /// Not a string: the value found, as JSON.
    NotString(String),
    /// Not a list of CSS font families: the text.
    NotFontList(String),
    /// Not a positive `<number>px`: the text.
    NotLength(String),
    /// Not `#rrggbb`: the text.
    NotColor(String),
    /// Not an object: the value found, as JSON.
    NotObject(String),
    /// A token the theme must define is missing.
    MissingToken,
    /// A key that is no token.
    UnknownToken,
    /// The site's `/config/` answered with an error.
    Unreachable(Box<crate::failure::Failure>),
}

impl From<String> for Problem {
    /// Another's words.
    fn from(said: String) -> Self {
        Problem::Said(said)
    }
}

/// The text of the three files.
#[derive(Debug, Clone, Copy)]
pub struct ConfigText<'a> {
    pub quorum: &'a str,
    pub repos: &'a str,
    pub project: &'a str,
}

/// The three files, parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub quorum: Quorum,
    pub repos: Repos,
    pub project: Project,
}

/// Parses the three files; the first error found, with its file and key
/// path.
pub fn parse(text: ConfigText<'_>) -> Result<Config, ConfigError> {
    Ok(Config {
        quorum: parse_file(QUORUM_FILE, text.quorum)?,
        repos: parse_file(REPOS_FILE, text.repos)?,
        project: parse_file(PROJECT_FILE, text.project)?,
    })
}

/// `file` parsed; an error names the key path once (serde_yaml_ng starts
/// the message of a nested key with its path too).
fn parse_file<T: DeserializeOwned>(file: &str, text: &str) -> Result<T, ConfigError> {
    let deserializer = serde_yaml_ng::Deserializer::from_str(text);
    serde_path_to_error::deserialize(deserializer).map_err(|error| {
        let path = error.path().to_string();
        let path = if path == "." { String::new() } else { path };
        let message = error.into_inner().to_string();
        let message = match message.strip_prefix(&format!("{path}: ")) {
            Some(rest) if !path.is_empty() => rest.to_owned(),
            _ => message,
        };
        ConfigError {
            file: file.to_owned(),
            path,
            message: Problem::Said(message),
        }
    })
}

// ---------------------------------------------------------------------------
// quorum/quorum.yaml

/// `quorum/quorum.yaml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Quorum {
    /// The PM config repository the config page shows and saves to.
    pub config: ConfigRepo,
    pub targets: Targets,
    /// Seconds between two refreshes of every source.
    pub refresh_seconds: NonZeroU32,
    pub attention: Attention,
    pub digest: Digest,
    pub filters: Filters,
    pub appearance: Appearance,
}

/// Light or dark: the system's, or one of them whatever the system's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    System,
    Light,
    Dark,
}

/// The addresses each build talks to.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Targets {
    pub macos: Endpoints,
    pub web: Endpoints,
}

/// Where the GitHub API, the council, the guardians and dispatch's
/// launcher check answer.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Endpoints {
    pub github_api: BaseUrl,
    pub council: BaseUrl,
    pub guardians: BaseUrl,
    /// The omp-dispatch launcher's PM config check listener.
    pub dispatch: BaseUrl,
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

/// A color written `#rrggbb`, kept as `0xrrggbb`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct HexColor(u32);

impl HexColor {
    /// `0xrrggbb`.
    pub fn packed(self) -> u32 {
        self.0
    }
}

impl TryFrom<String> for HexColor {
    type Error = String;

    fn try_from(text: String) -> Result<Self, String> {
        text.strip_prefix('#')
            .filter(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
            .and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .map(Self)
            .ok_or_else(|| format!("{text:?} is not #rrggbb"))
    }
}

/// The filters of the 「议题」 page when it opens; the user changes them
/// there. They apply to the list and the DAG alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Filters {
    /// Fold the decided and superseded issues (the list's latest three
    /// and a line for the rest, the DAG's fold); else show them all.
    pub hide_decided: bool,
    /// Show only human issues and the issues they block, transitively.
    pub human_only: bool,
}

/// `config`: the PM config repository, the branch the page reads, compares
/// with and commits to, and which programs read the files under each
/// directory.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigRepo {
    pub repo: RepoName,
    pub branch: Name,
    pub readers: Readers,
}

/// A program that reads PM config files and checks candidates of them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Reader {
    Council,
    Guardians,
    Quorum,
    Dispatch,
}

impl Reader {
    pub const ALL: [Reader; 4] = [Reader::Council, Reader::Guardians, Reader::Quorum, Reader::Dispatch];
}

/// Directory prefixes (`council/`) and the readers of the files under
/// each, in the file's order.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "serde_yaml_ng::Mapping")]
pub struct Readers(Vec<(String, Vec<Reader>)>);

impl Readers {
    /// The readers of the file at `path`: those of every prefix it starts
    /// with, each once, in [`Reader::ALL`]'s order.
    pub fn of(&self, path: &str) -> Vec<Reader> {
        let mut found: Vec<Reader> = self
            .0
            .iter()
            .filter(|(prefix, _)| path.starts_with(prefix.as_str()))
            .flat_map(|(_, readers)| readers.iter().copied())
            .collect();
        found.sort();
        found.dedup();
        found
    }
}

impl TryFrom<serde_yaml_ng::Mapping> for Readers {
    type Error = String;

    fn try_from(map: serde_yaml_ng::Mapping) -> Result<Self, String> {
        map.into_iter()
            .map(|(prefix, readers)| {
                let prefix = match prefix {
                    serde_yaml_ng::Value::String(prefix) => prefix,
                    other => return Err(format!("{other:?} is not a directory prefix")),
                };
                if prefix.is_empty() || prefix.starts_with('/') || !prefix.ends_with('/') {
                    return Err(format!("{prefix:?} is not a directory prefix like `council/`"));
                }
                let readers: Vec<Reader> = serde_yaml_ng::from_value(readers)
                    .map_err(|error| format!("{prefix}: {error}"))?;
                if readers.is_empty() {
                    return Err(format!("{prefix}: no reader"));
                }
                let mut unique = readers.clone();
                unique.sort();
                unique.dedup();
                if unique.len() != readers.len() {
                    return Err(format!("{prefix}: a reader is named twice"));
                }
                Ok((prefix, readers))
            })
            .collect::<Result<_, _>>()
            .map(Self)
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

#[cfg(test)]
mod tests {
    use super::*;

    const QUORUM: &str = include_str!("../../config.example/quorum/quorum.yaml");
    const REPOS: &str = include_str!("../../config.example/shared/repos.yaml");
    const PROJECT: &str = include_str!("../../config.example/shared/project.yaml");

    /// The example config with its `appearance` line replaced by `line`.
    fn with_appearance(line: &str) -> Result<Config, ConfigError> {
        let quorum = QUORUM
            .lines()
            .map(|l| if l.starts_with("appearance:") { line } else { l })
            .collect::<Vec<_>>()
            .join("\n");
        parse(ConfigText {
            quorum: &quorum,
            repos: REPOS,
            project: PROJECT,
        })
    }

    #[test]
    fn the_example_config_parses() {
        let config = parse(ConfigText {
            quorum: QUORUM,
            repos: REPOS,
            project: PROJECT,
        })
        .unwrap();
        assert_eq!(config.quorum.appearance, Appearance::System);
    }

    #[test]
    fn appearance_is_system_light_or_dark() {
        let of = |value: &str| {
            with_appearance(&format!("appearance: {value}")).map(|c| c.quorum.appearance)
        };
        assert_eq!(of("system"), Ok(Appearance::System));
        assert_eq!(of("light"), Ok(Appearance::Light));
        assert_eq!(of("dark"), Ok(Appearance::Dark));
        let error = of("dim").unwrap_err();
        assert_eq!(error.file, QUORUM_FILE);
        assert_eq!(error.path, "appearance");
        assert!(
            matches!(&error.message, Problem::Said(said) if said.starts_with("unknown variant `dim`")),
            "{:?}",
            error.message
        );
        assert_eq!(of("Dark").unwrap_err().path, "appearance");
    }

    #[test]
    fn appearance_is_required() {
        let error = with_appearance("").unwrap_err();
        assert_eq!(error.file, QUORUM_FILE);
        assert!(
            matches!(&error.message, Problem::Said(said) if said.contains("missing field `appearance`")),
            "{:?}",
            error.message
        );
    }
}
