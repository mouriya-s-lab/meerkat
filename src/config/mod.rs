//! The PM config as each build gets it. macOS reads the directory named by
//! `PM_CONFIG_DIR` at startup; the web build carries the files `build.rs`
//! checked and embedded from the same variable at build time. Both parse
//! the YAML files with [`schema`] and `dashboard/theme.json` with
//! [`theme`], and use their own section of `dashboard.yaml`'s `targets`.

pub mod schema;
pub mod theme;

use std::time::Duration;

pub use schema::{
    Attention, BaseUrl, Config, ConfigError, ConfigText, Digest, Endpoints, Fields, Filters,
    HexColor, List, ListColumn, ListSort, ProjectRef, RepoName, Repos,
};
pub use theme::{ColorToken, FontToken, LengthToken, Theme, ThemeColor};

/// What the app runs with: this build's endpoints and the shared settings.
#[derive(Debug, Clone, PartialEq)]
pub struct AppConfig {
    pub endpoints: Endpoints,
    pub refresh: Duration,
    pub attention: Attention,
    pub digest: Digest,
    pub list: List,
    pub filters: Filters,
    pub repos: Repos,
    pub fields: Fields,
    pub theme: Theme,
}

impl AppConfig {
    fn of(config: Config, theme: Theme) -> Self {
        let Config {
            dashboard,
            repos,
            project,
        } = config;
        #[cfg(target_arch = "wasm32")]
        let endpoints = dashboard.targets.web;
        #[cfg(not(target_arch = "wasm32"))]
        let endpoints = dashboard.targets.macos;
        Self {
            endpoints,
            refresh: Duration::from_secs(u64::from(dashboard.refresh_seconds.get())),
            attention: dashboard.attention,
            digest: dashboard.digest,
            list: dashboard.list,
            filters: dashboard.filters,
            repos,
            fields: project.fields,
            theme,
        }
    }
}

/// The environment variable naming the PM config directory.
pub const CONFIG_DIR_VAR: &str = "PM_CONFIG_DIR";

/// The config embedded at build time (`build.rs` checked it).
#[cfg(target_arch = "wasm32")]
pub fn load() -> Result<AppConfig, ConfigError> {
    let config = schema::parse(ConfigText {
        dashboard: include_str!(concat!(env!("OUT_DIR"), "/dashboard.yaml")),
        repos: include_str!(concat!(env!("OUT_DIR"), "/repos.yaml")),
        project: include_str!(concat!(env!("OUT_DIR"), "/project.yaml")),
    })?;
    let theme = theme::parse(include_str!(concat!(env!("OUT_DIR"), "/theme.json")))?;
    Ok(AppConfig::of(config, theme))
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
    let read = |file: &str| {
        let path = dir.join(file);
        std::fs::read_to_string(&path).map_err(|error| ConfigError {
            file: path.display().to_string(),
            path: String::new(),
            message: error.to_string(),
        })
    };
    let dashboard = read(schema::DASHBOARD_FILE)?;
    let repos = read(schema::REPOS_FILE)?;
    let project = read(schema::PROJECT_FILE)?;
    let theme = read(theme::THEME_FILE)?;
    let config = schema::parse(ConfigText {
        dashboard: &dashboard,
        repos: &repos,
        project: &project,
    })?;
    Ok(AppConfig::of(config, theme::parse(&theme)?))
}
