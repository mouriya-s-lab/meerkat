//! The local Meerkat config checkout the macOS build reads (`MEERKAT_CONFIG_DIR`):
//! how far it is behind the branch on GitHub, and a fast-forward pull.
//! git runs on the runtime's blocking threads.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use nsh_core::prelude::blocking;

use crate::failure::Failure;

/// `git -C dir args…`: its standard output, or what it printed when it
/// failed.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|error| format!("git {}: {error}", args.join(" ")))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git {}: {}", args.join(" "), stderr.trim()))
    }
}

/// How many commits of `origin/<branch>` the checkout lacks, after a
/// `git fetch`.
pub async fn behind(dir: PathBuf, branch: String) -> Result<u32, Failure> {
    blocking(move || {
        git(&dir, &["fetch", "origin"])?;
        let count = git(&dir, &["rev-list", "--count", &format!("HEAD..origin/{branch}")])?;
        count
            .parse()
            .map_err(|_| format!("git rev-list --count: {count:?}"))
    })
    .await
    .map_err(|error| Failure::Said(error.to_string()))?
    .map_err(Failure::Said)
}

/// `git pull --ff-only` in the checkout.
pub async fn pull(dir: PathBuf) -> Result<(), Failure> {
    blocking(move || git(&dir, &["pull", "--ff-only"]).map(drop))
        .await
        .map_err(|error| Failure::Said(error.to_string()))?
        .map_err(Failure::Said)
}
