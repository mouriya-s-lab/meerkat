//! Runs the dashboard in a native macOS window (`cargo run`), reading the
//! PM config directory `PM_CONFIG_DIR` names.

#[cfg(target_os = "macos")]
fn main() -> impl std::process::Termination {
    use nsh_platform_macos as platform;

    env_logger::init();
    platform::run_app(
        platform::AppSource::Rust(platform::Root::of::<quorum::App>()),
        platform::AppOptions {
            title: "Quorum".to_owned(),
            size: (1200, 820),
            ..Default::default()
        },
    )
}

#[cfg(not(target_os = "macos"))]
compile_error!("Run this binary on macOS; use `nsh package --target web` for the web.");
