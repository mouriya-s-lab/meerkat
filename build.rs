//! The web build carries its PM config: for `wasm32`, the four files of the
//! directory `PM_CONFIG_DIR` names are checked with the app's own parsers and
//! copied into `OUT_DIR`, where `src/config/mod.rs` embeds them. A config
//! that does not parse fails the build with its file and key path. Other
//! targets read the directory at startup and need nothing here.

use std::{env, fs, path::PathBuf};

// The app's parsers; the build script uses only `parse` and the file names.
#[allow(dead_code)]
#[path = "src/config/schema.rs"]
mod schema;
#[allow(dead_code)]
#[path = "src/config/theme.rs"]
mod theme;

const CONFIG_DIR_VAR: &str = "PM_CONFIG_DIR";

fn main() {
    println!("cargo::rerun-if-changed=src/config/schema.rs");
    println!("cargo::rerun-if-changed=src/config/theme.rs");
    println!("cargo::rerun-if-env-changed={CONFIG_DIR_VAR}");
    if env::var("CARGO_CFG_TARGET_ARCH").as_deref() != Ok("wasm32") {
        return;
    }
    if let Err(message) = embed() {
        println!("cargo::error={message}");
    }
}

fn embed() -> Result<(), String> {
    let dir = env::var_os(CONFIG_DIR_VAR)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            format!("{CONFIG_DIR_VAR}: not set; the web build embeds the PM config directory it names")
        })?;
    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR is not set")?);
    let read = |file: &str| {
        let path = dir.join(file);
        println!("cargo::rerun-if-changed={}", path.display());
        fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))
    };
    let dashboard = read(schema::DASHBOARD_FILE)?;
    let repos = read(schema::REPOS_FILE)?;
    let project = read(schema::PROJECT_FILE)?;
    let theme = read(theme::THEME_FILE)?;
    schema::parse(schema::ConfigText {
        dashboard: &dashboard,
        repos: &repos,
        project: &project,
    })
    .map_err(|error| format!("invalid PM config in {}: {error}", dir.display()))?;
    theme::parse(&theme)
        .map_err(|error| format!("invalid PM config in {}: {error}", dir.display()))?;
    for (name, text) in [
        ("dashboard.yaml", &dashboard),
        ("repos.yaml", &repos),
        ("project.yaml", &project),
        ("theme.json", &theme),
    ] {
        let path = out.join(name);
        fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}
