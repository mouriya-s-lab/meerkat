//! Quorum (`nsh.json` `"root"` names [`App`]): one Rust
//! app for macOS (`cargo run`) and the web (`nsh package --target web`).
//! It owns no PM state: it reads GitHub and the council and guardians
//! programs, and shows what it read.

pub mod attention;
pub mod config;
pub mod digest;
pub mod dag;
pub mod gh;
mod http;
pub mod programs;
pub mod sources;
pub mod time;
mod ui;

use nsh_base::theme;
use nsh_core::prelude::*;

/// The app's UI set (`nsh.json` `"rust"`).
#[native_module("quorum/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// The app's root: the overview for the config this build reads, drawn
/// with its theme's tokens, or what is wrong with that config.
#[component]
pub fn App() -> impl IntoElement {
    let content = match config::load() {
        Ok(config) => {
            let tokens = ui::tokens(&config.theme);
            set_tokens(tokens.clone());
            ui::app(config, tokens)
        }
        Err(error) => ui::config_error(&error),
    };
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(content)
}
