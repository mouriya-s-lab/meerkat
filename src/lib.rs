//! The Pepgent PM dashboard (`nsh.json` `"root"` names [`App`]): one Rust
//! app for macOS (`cargo run`) and the web (`nsh package --target web`).
//! It owns no PM state: it reads GitHub and the council and guardians
//! programs, and shows what it read.

pub mod attention;
pub mod config;
pub mod gh;
mod http;
pub mod programs;
pub mod sources;
pub mod time;
mod ui;

use nsh_base::theme;
use nsh_core::prelude::*;

/// The app's UI set (`nsh.json` `"rust"`).
#[native_module("pm-dashboard/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The app's root: the overview for the config this build reads, or what
/// is wrong with that config.
#[component]
pub fn App() -> impl IntoElement {
    let content = match config::load() {
        Ok(config) => ui::shell(config),
        Err(error) => ui::config_error(&error),
    };
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(content)
}
