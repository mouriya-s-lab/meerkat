//! Runs Meerkat in a native macOS window (`relax run --allow-c meerkat`
//! while developing), reading the Meerkat config directory
//! `MEERKAT_CONFIG_DIR` names.

#[cfg(target_os = "macos")]
fn main() -> impl std::process::Termination {
    use nsh_platform_macos as platform;

    env_logger::init();
    // The window, title bar included, opens in the config's appearance
    // (NSH sets `NSWindow.appearance` only from `AppOptions`; the app's
    // `set_color_scheme` reaches the views). A config that does not load
    // shows its error in the system's.
    let appearance = meerkat::config::load()
        .map(|config| meerkat::appearance(config.appearance))
        .unwrap_or(platform::Appearance::System);
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<meerkat::App>()),
        platform::AppOptions {
            title: "Meerkat".to_owned(),
            size: (1200, 820),
            appearance,
            ..Default::default()
        },
    )
}

#[cfg(not(target_os = "macos"))]
compile_error!("Run this binary on macOS; use `nsh package --target web` for the web.");
