//! Runs Quorum in a native macOS window (`cargo run`), reading the
//! PM config directory `PM_CONFIG_DIR` names.

#[cfg(target_os = "macos")]
fn main() -> impl std::process::Termination {
    use nsh_platform_macos as platform;

    env_logger::init();
    // The window, title bar included, opens in the config's appearance
    // (NSH sets `NSWindow.appearance` only from `AppOptions`; the app's
    // `set_color_scheme` reaches the views). A config that does not load
    // shows its error in the system's.
    let appearance = quorum::config::load()
        .map(|config| quorum::appearance(config.appearance))
        .unwrap_or(platform::Appearance::System);
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<quorum::App>()),
        platform::AppOptions {
            title: "Quorum".to_owned(),
            size: (1200, 820),
            appearance,
            ..Default::default()
        },
    )
}

#[cfg(not(target_os = "macos"))]
compile_error!("Run this binary on macOS; use `nsh package --target web` for the web.");
