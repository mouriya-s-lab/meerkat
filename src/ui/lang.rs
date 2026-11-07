//! Which language the UI speaks (#12): the oh-my-omp convention for
//! person-facing text, with the step graphical apps add. `OMOMP_LANG`,
//! then the first of `LC_ALL`, `LC_MESSAGES`, `LANG` that is set, then the
//! platform's preferred languages (macOS: the system's, web: the
//! browser's), then English. The web has no environment.

use std::cell::Cell;

use nsh_core::prelude::*;

/// A language the UI is written in. Any `zh*` reads Simplified Chinese.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    Zh,
    #[default]
    En,
}

/// The environment variables the convention reads, in order after
/// `OMOMP_LANG`.
const LOCALE_VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len()).is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

impl Lang {
    /// The language for the environment `var` reads and the platform's
    /// preferred `languages`, most preferred first.
    pub fn resolve(var: impl Fn(&str) -> Option<String>, languages: &[impl AsRef<str>]) -> Lang {
        if let Some(explicit) = var("OMOMP_LANG") {
            match explicit.trim().to_ascii_lowercase().as_str() {
                "zh" => return Lang::Zh,
                "en" => return Lang::En,
                _ => {}
            }
        }
        let locale = LOCALE_VARIABLES
            .into_iter()
            .filter_map(&var)
            .find(|value| !value.trim().is_empty());
        if let Some(locale) = locale {
            return if starts_with_ignore_case(locale.trim(), "zh") { Lang::Zh } else { Lang::En };
        }
        let first = |prefix: &str| {
            languages
                .iter()
                .position(|language| starts_with_ignore_case(language.as_ref().trim(), prefix))
        };
        match (first("zh"), first("en")) {
            (Some(zh), Some(en)) if zh < en => Lang::Zh,
            (Some(_), None) => Lang::Zh,
            _ => Lang::En,
        }
    }

    /// The UI's language now, the memo [`install`] made: reading it
    /// subscribes.
    pub fn current() -> Lang {
        CURRENT.get().expect("the root installs the UI's language first").get()
    }
}

thread_local! {
    static CURRENT: Cell<Option<Memo<Lang>>> = const { Cell::new(None) };
}

/// Makes the UI's language a memo of the environment and the platform's
/// preferred languages, read again whenever NSH reports a new locale (the
/// system's or the browser's language changed). Called once by the root.
pub fn install() {
    let memo = computed(|| {
        let _ = locale();
        Lang::resolve(env_var, &preferred_languages())
    });
    CURRENT.set(Some(memo));
}

/// Elements made by `f`, made again when the UI's language changes; what
/// `f` reads besides is not tracked.
pub fn localized<E: IntoElements + 'static>(f: impl Fn() -> E + 'static) -> AnyElement {
    dynamic(move || {
        let _ = Lang::current();
        untracked(&f)
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// The web has no environment.
#[cfg(target_arch = "wasm32")]
fn env_var(_: &str) -> Option<String> {
    None
}

/// The system's preferred languages (System Settings → Language & Region).
#[cfg(target_os = "macos")]
fn preferred_languages() -> Vec<String> {
    objc2_foundation::NSLocale::preferredLanguages().iter().map(|language| language.to_string()).collect()
}

/// The browser's preferred languages (`navigator.languages`).
#[cfg(target_arch = "wasm32")]
fn preferred_languages() -> Vec<String> {
    let Some(window) = web_sys::window() else {
        return Vec::new();
    };
    window.navigator().languages().iter().filter_map(|language| language.as_string()).collect()
}

/// Other platforms: NSH's locale stands for the list.
#[cfg(not(any(target_os = "macos", target_arch = "wasm32")))]
fn preferred_languages() -> Vec<String> {
    vec![locale().to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(env: &[(&str, &str)], languages: &[&str]) -> Lang {
        Lang::resolve(
            |name| env.iter().find(|(key, _)| *key == name).map(|(_, value)| (*value).to_owned()),
            languages,
        )
    }

    /// The convention's environment vectors.
    #[test]
    fn lang_environment_vectors() {
        assert_eq!(resolve(&[("OMOMP_LANG", " ZH ")], &[]), Lang::Zh);
        assert_eq!(resolve(&[("OMOMP_LANG", "zh-CN"), ("LANG", "en_US.UTF-8")], &[]), Lang::En);
        assert_eq!(resolve(&[("LC_ALL", ""), ("LC_MESSAGES", "zh_CN.UTF-8")], &[]), Lang::Zh);
        assert_eq!(resolve(&[("LC_ALL", "C"), ("LANG", "zh_CN.UTF-8")], &[]), Lang::En);
        assert_eq!(resolve(&[], &[]), Lang::En);
        // An `omomp.yml` naming zh is never read: only the environment counts.
        assert_eq!(resolve(&[("LANG", "en_US.UTF-8")], &[]), Lang::En);
    }

    /// The convention's vectors for graphical apps.
    #[test]
    fn lang_graphical_app_vectors() {
        assert_eq!(resolve(&[], &["zh-Hans-CN", "en-US"]), Lang::Zh);
        assert_eq!(resolve(&[], &["ja-JP", "zh-CN"]), Lang::Zh);
        assert_eq!(resolve(&[], &["en-US", "zh-CN"]), Lang::En);
        assert_eq!(resolve(&[("LANG", "en_US.UTF-8")], &["zh-Hans-CN"]), Lang::En);
        // The web: the browser's languages alone.
        assert_eq!(resolve(&[], &["zh-CN"]), Lang::Zh);
        assert_eq!(resolve(&[], &["fr-FR"]), Lang::En);
    }

    #[test]
    fn lang_traditional_chinese_reads_chinese() {
        assert_eq!(resolve(&[], &["zh-Hant-JP", "ja-JP"]), Lang::Zh);
    }
}
