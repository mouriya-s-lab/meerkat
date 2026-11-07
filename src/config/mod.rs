//! The PM config as each end loads it. macOS reads the directory named by
//! `PM_CONFIG_DIR` at startup; the web build fetches the same four files
//! from the site's `/config/` when it starts, so a config change takes
//! effect on the next deploy and refresh, with no rebuild. Both parse the
//! YAML files with [`schema`] and `quorum/theme.json` with [`theme`], use
//! their own section of `quorum.yaml`'s `targets`, and remember the git
//! blob sha of every file they loaded.

pub mod schema;
pub mod theme;

use std::time::Duration;

pub use schema::{
    Appearance, Attention, BaseUrl, Config, ConfigError, ConfigText, Digest, Endpoints, Fields,
    Filters, HexColor, ProjectRef, RepoName, Repos,
};
pub use theme::{ColorToken, FontToken, LengthToken, Theme, ThemeColor};

/// What the app runs with: this build's endpoints and the shared settings.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub endpoints: Endpoints,
    pub refresh: Duration,
    pub attention: Attention,
    pub digest: Digest,
    pub filters: Filters,
    pub appearance: Appearance,
    pub repos: Repos,
    pub fields: Fields,
    pub theme: Theme,
    /// Every file the config was loaded from, with its git blob sha: what
    /// runs, named so it compares against pm-config's `main`.
    pub loaded: Vec<LoadedFile>,
}

/// One file the config was loaded from: its path relative to the PM
/// config directory (the name every end knows it by) and the git blob
/// sha of its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedFile {
    pub file: String,
    pub sha: String,
}

impl LoadedFile {
    fn of(file: &'static str, bytes: &[u8]) -> Self {
        Self {
            file: file.to_owned(),
            sha: blob_sha(bytes),
        }
    }
}

/// The git blob sha of `bytes` (`sha1("blob <len>\0" + bytes)`, lowercase
/// hex): the value `git hash-object` gives the same file.
fn blob_sha(bytes: &[u8]) -> String {
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(format!("blob {}\0", bytes.len()).as_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

impl AppConfig {
    fn of(config: Config, theme: Theme, loaded: Vec<LoadedFile>) -> Self {
        let Config {
            quorum,
            repos,
            project,
        } = config;
        #[cfg(target_arch = "wasm32")]
        let endpoints = quorum.targets.web;
        #[cfg(not(target_arch = "wasm32"))]
        let endpoints = quorum.targets.macos;
        Self {
            endpoints,
            refresh: Duration::from_secs(u64::from(quorum.refresh_seconds.get())),
            attention: quorum.attention,
            digest: quorum.digest,
            filters: quorum.filters,
            appearance: quorum.appearance,
            repos,
            fields: project.fields,
            theme,
            loaded,
        }
    }
}

/// `bytes` as text; a file that is not UTF-8 is a config error.
fn text_of<'a>(file: &'static str, bytes: &'a [u8]) -> Result<&'a str, ConfigError> {
    std::str::from_utf8(bytes).map_err(|error| ConfigError {
        file: file.to_owned(),
        path: String::new(),
        message: format!("不是 UTF-8：{error}"),
    })
}

/// The config of the four files' bytes: each parsed by the same parsers
/// every end uses, and each remembered by its git blob sha.
fn assemble(
    quorum: &[u8],
    theme: &[u8],
    repos: &[u8],
    project: &[u8],
) -> Result<AppConfig, ConfigError> {
    let loaded = [
        LoadedFile::of(schema::QUORUM_FILE, quorum),
        LoadedFile::of(theme::THEME_FILE, theme),
        LoadedFile::of(schema::REPOS_FILE, repos),
        LoadedFile::of(schema::PROJECT_FILE, project),
    ]
    .into_iter()
    .collect();
    let config = schema::parse(ConfigText {
        quorum: text_of(schema::QUORUM_FILE, quorum)?,
        repos: text_of(schema::REPOS_FILE, repos)?,
        project: text_of(schema::PROJECT_FILE, project)?,
    })?;
    Ok(AppConfig::of(
        config,
        theme::parse(text_of(theme::THEME_FILE, theme)?)?,
        loaded,
    ))
}

/// The environment variable naming the PM config directory.
pub const CONFIG_DIR_VAR: &str = "PM_CONFIG_DIR";

/// The config the site serves under `/config/`: the four files, fetched
/// when the app starts. A request or status that fails names the file it
/// was for.
#[cfg(target_arch = "wasm32")]
pub async fn load() -> Result<AppConfig, ConfigError> {
    let quorum = fetch(schema::QUORUM_FILE).await?;
    let theme = fetch(theme::THEME_FILE).await?;
    let repos = fetch(schema::REPOS_FILE).await?;
    let project = fetch(schema::PROJECT_FILE).await?;
    assemble(&quorum, &theme, &repos, &project)
}

/// `/config/<file>` of the site the page came from, fetched in full.
#[cfg(target_arch = "wasm32")]
async fn fetch(file: &'static str) -> Result<Vec<u8>, ConfigError> {
    use nsh_core::prelude::HttpRequest;

    let url = format!("{}/config/{file}", origin());
    crate::http::bytes(HttpRequest::get(url))
        .await
        .map_err(|message| ConfigError {
            file: file.to_owned(),
            path: String::new(),
            message,
        })
}

/// The site's own origin, to which `/config/<file>` is appended: NSH's
/// `fetch` takes absolute URLs only.
#[cfg(target_arch = "wasm32")]
fn origin() -> String {
    web_sys::window()
        .and_then(|window| window.location().origin().ok())
        .unwrap_or_default()
}

/// The config in the directory `PM_CONFIG_DIR` names.
#[cfg(not(target_arch = "wasm32"))]
pub fn load() -> Result<AppConfig, ConfigError> {
    use std::path::PathBuf;

    let dir = std::env::var_os(CONFIG_DIR_VAR)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| ConfigError {
            file: CONFIG_DIR_VAR.to_owned(),
            path: String::new(),
            message: "未设置：应指向 PM 配置目录".to_owned(),
        })?;
    let read = |file: &'static str| {
        let path = dir.join(file);
        std::fs::read(&path).map_err(|error| ConfigError {
            file: file.to_owned(),
            path: String::new(),
            message: format!("{}: {error}", path.display()),
        })
    };
    let quorum = read(schema::QUORUM_FILE)?;
    let theme = read(theme::THEME_FILE)?;
    let repos = read(schema::REPOS_FILE)?;
    let project = read(schema::PROJECT_FILE)?;
    assemble(&quorum, &theme, &repos, &project)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example config directory, read whole.
    fn example() -> [Vec<u8>; 4] {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example");
        let read = |file: &str| {
            std::fs::read(dir.join(file))
                .unwrap_or_else(|error| panic!("config.example/{file}: {error}"))
        };
        [
            read(schema::QUORUM_FILE),
            read(theme::THEME_FILE),
            read(schema::REPOS_FILE),
            read(schema::PROJECT_FILE),
        ]
    }

    /// `blob_sha` gives what `git hash-object --stdin` gives the same
    /// bytes: these are its answers for the empty file and `hello\n`.
    #[test]
    fn blob_sha_matches_git_hash_object() {
        assert_eq!(
            blob_sha(b""),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
        assert_eq!(
            blob_sha(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    /// Loading the example configuration remembers each of the four files
    /// by the name the PM config directory gives it and the sha of its
    /// bytes.
    #[test]
    fn loading_records_each_files_blob_sha() {
        let [quorum, theme, repos, project] = example();
        let config = assemble(&quorum, &theme, &repos, &project).unwrap();
        let expected = [
            (schema::QUORUM_FILE, &quorum),
            (theme::THEME_FILE, &theme),
            (schema::REPOS_FILE, &repos),
            (schema::PROJECT_FILE, &project),
        ];
        assert_eq!(config.loaded.len(), expected.len());
        for (loaded, (file, bytes)) in config.loaded.iter().zip(expected) {
            assert_eq!(loaded.file, file);
            assert_eq!(loaded.sha, blob_sha(bytes), "{file}");
            assert_eq!(loaded.sha.len(), 40, "{file}");
        }
    }

    /// A file that does not parse is reported under its own name, with the
    /// key path that is wrong.
    #[test]
    fn a_bad_file_names_itself_and_its_key() {
        let [quorum, theme, repos, project] = example();
        let unknown = String::from_utf8(quorum).unwrap() + "unexpectedKey: 1\n";
        let error = assemble(unknown.as_bytes(), &theme, &repos, &project).unwrap_err();
        assert_eq!(error.file, schema::QUORUM_FILE);
        assert_eq!(error.path, "unexpectedKey");
        assert!(error.message.contains("unknown field"), "{}", error.message);
    }

    /// A file that is not UTF-8 is a config error naming the file.
    #[test]
    fn a_file_that_is_not_utf8_is_an_error() {
        let [_, theme, repos, project] = example();
        let error = assemble(&[0xff, 0xfe], &theme, &repos, &project).unwrap_err();
        assert_eq!(error.file, schema::QUORUM_FILE);
        assert!(error.message.starts_with("不是 UTF-8"), "{}", error.message);
    }
}
