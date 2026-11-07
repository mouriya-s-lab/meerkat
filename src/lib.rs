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

/// The app's root: the overview for the config it loads, drawn with its
/// theme's tokens in its appearance, or what is wrong with that config.
/// macOS's load reads `PM_CONFIG_DIR` and resolves almost at once; the
/// web's fetches `/config/` from the site, so the first frame says it is
/// loading (nothing else can be drawn until the theme is known: every
/// page reads its tokens).
#[component]
pub fn App() -> impl IntoElement {
    let loaded = resource(|| Some(()), |(), _| load_config());
    let content = dynamic(move || {
        if let Some(error) = loaded.error() {
            return ui::config_error(&error).into_elements();
        }
        loaded.with(|config| match config {
            Some(config) => {
                set_color_scheme(appearance(config.appearance));
                let tokens = ui::tokens(&config.theme);
                if *nsh_core::element::tokens() != tokens {
                    set_tokens(tokens.clone());
                }
                ui::app(config.clone(), tokens).into_elements()
            }
            None => config_loading().into_elements(),
        })
    });
    view()
        .flex_1()
        .bg(move || theme().background)
        .text_color(move || theme().foreground)
        .child(content)
        .into_element()
}

/// What shows while the config loads, before its theme is known: NSH's
/// own theme, since every page reads the config's tokens.
fn config_loading() -> impl IntoElement {
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p_6()
        .child(
            text("正在读取配置…")
                .text_sm()
                .text_color(move || theme().muted_foreground),
        )
        .into_element()
}

/// The config this build loads: the site's `/config/` on the web, the
/// `PM_CONFIG_DIR` directory elsewhere.
#[cfg(target_arch = "wasm32")]
async fn load_config() -> Result<config::AppConfig, config::ConfigError> {
    config::load().await
}

/// The config this build loads: the `PM_CONFIG_DIR` directory.
#[cfg(not(target_arch = "wasm32"))]
async fn load_config() -> Result<config::AppConfig, config::ConfigError> {
    config::load()
}

/// NSH's appearance for the config's.
pub fn appearance(appearance: config::Appearance) -> nsh_core::platform::Appearance {
    match appearance {
        config::Appearance::System => nsh_core::platform::Appearance::System,
        config::Appearance::Light => nsh_core::platform::Appearance::Light,
        config::Appearance::Dark => nsh_core::platform::Appearance::Dark,
    }
}
