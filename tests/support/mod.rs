//! The headless fixture Quorum's integration tests start from (#23): the
//! real [`quorum::App`] on an engine with no window, its council, guardians
//! and GitHub answered from the responses recorded under
//! `tests/fixtures/responses/`, driven the way a person drives it: pressing
//! the sidebar's pages and reading what the window shows.
//!
//! The config it loads is `tests/fixtures/config/` (the example config, its
//! `targets` pointing at the stub network's hosts), handed to the app through
//! `PM_CONFIG_DIR`. A test that answers some requests itself passes its own
//! routes to [`Fixture::start_with`]; they come before the recorded ones.

// Each test crate (`credential`, `i18n`) uses a part of the fixture.
#![allow(dead_code)]

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use nsh_core::{
    element::Root,
    geometry::{Points, Size},
};
use nsh_engine::EngineConfig;
use nsh_testing::{Headless, StubNetwork, StubResponse, StubRoute};
use quorum::{Lang, Msg};

/// The stub network's GitHub host: `targets.*.githubApi` in
/// `tests/fixtures/config/quorum/quorum.yaml`.
pub const GITHUB: &str = "http://github.test";
/// The stub network's council host.
const COUNCIL: &str = "http://council.test";
/// The stub network's guardians host.
const GUARDIANS: &str = "http://guardians.test";
/// The agenda repository the fixture config names (`repos.pm.repo`).
const PM: &str = "oh-my-omp/pm-e2e-agenda";

// What `tests/fixtures/responses/` recorded: the local council
// (`127.0.0.1:7741`), the local guardians (`127.0.0.1:7740`) and GitHub's
// API (`gh api`). The Project has more items than one page holds, so its two
// recorded pages are one answer.
const COUNCIL_HEALTH: &str = include_str!("../fixtures/responses/council-health.json");
const COUNCIL_QUEUE: &str = include_str!("../fixtures/responses/council-queue.json");
const COUNCIL_SESSIONS: &str = include_str!("../fixtures/responses/council-sessions.json");
const COUNCIL_CONFIG: &str = include_str!("../fixtures/responses/council-config.json");
const GUARDIANS_HEALTH: &str = include_str!("../fixtures/responses/guardians-health.json");
const GUARDIANS_BASES: &str = include_str!("../fixtures/responses/guardians-bases.json");
const GUARDIANS_SESSIONS: &str = include_str!("../fixtures/responses/guardians-sessions.json");
const GUARDIANS_CONFIG: &str = include_str!("../fixtures/responses/guardians-config.json");
const PROJECT: &str = include_str!("../fixtures/responses/github-project.json");
const RATE_LIMIT: &str = include_str!("../fixtures/responses/github-rate-limit.json");
const CONFIG_BRANCH: &str = include_str!("../fixtures/responses/github-config-branch.json");
const CONFIG_TREE: &str = include_str!("../fixtures/responses/github-config-tree.json");
// `gh api repos/oh-my-omp/pm-e2e-agenda/contents/README.md?ref=main`, its
// `download_url` without the raw-content token GitHub appends.
const CONFIG_README: &str = include_str!("../fixtures/responses/github-config-readme.json");

/// A JSON answer with `status`.
// `StubResponse` is `#[non_exhaustive]`: it cannot be built as a literal here.
#[allow(clippy::field_reassign_with_default)]
pub fn answer(status: u16, json: &str) -> StubResponse {
    let mut response = StubResponse::default();
    response.status = status;
    response.headers = vec![("Content-Type".to_owned(), "application/json".to_owned())];
    response.body = json.as_bytes().to_vec();
    response
}

/// The Project's recorded answer, as GitHub gave it.
pub fn project() -> StubResponse {
    answer(200, PROJECT)
}

/// Every request the overview makes at startup, answered from the
/// recordings: the programs' `/v1/…` endpoints and the GitHub API.
fn recorded() -> Vec<(String, StubRoute)> {
    let answers = [
        (format!("GET {COUNCIL}/v1/health"), COUNCIL_HEALTH),
        (format!("GET {COUNCIL}/v1/queue"), COUNCIL_QUEUE),
        (format!("GET {COUNCIL}/v1/sessions?limit=200"), COUNCIL_SESSIONS),
        (format!("GET {COUNCIL}/v1/config"), COUNCIL_CONFIG),
        (format!("GET {GUARDIANS}/v1/health"), GUARDIANS_HEALTH),
        (format!("GET {GUARDIANS}/v1/bases"), GUARDIANS_BASES),
        (format!("GET {GUARDIANS}/v1/sessions?limit=200"), GUARDIANS_SESSIONS),
        (format!("GET {GUARDIANS}/v1/config"), GUARDIANS_CONFIG),
        (format!("POST {GITHUB}/graphql"), PROJECT),
        (format!("GET {GITHUB}/rate_limit"), RATE_LIMIT),
        (format!("GET {GITHUB}/repos/{PM}/branches/main"), CONFIG_BRANCH),
        (format!("GET {GITHUB}/repos/{PM}/git/trees/*"), CONFIG_TREE),
        (format!("GET {GITHUB}/repos/{PM}/contents/README.md?ref=main"), CONFIG_README),
    ];
    answers
        .into_iter()
        .map(|(route, json)| (route, StubRoute::Response(answer(200, json))))
        .collect()
}

/// Quorum running on the headless engine in one language: the window's
/// contents, and what it asked the stub network.
pub struct Fixture {
    app: Headless,
    network: Arc<StubNetwork>,
    lang: Lang,
}

impl Fixture {
    /// Starts Quorum in `lang` on the recorded answers, in the fixture
    /// config directory, and waits for its first frame.
    pub fn start(lang: Lang) -> Fixture {
        Fixture::start_with(lang, Vec::new())
    }

    /// Starts Quorum in `lang` with `routes` ahead of the recorded ones, so a
    /// test answers some requests itself.
    pub fn start_with(lang: Lang, routes: Vec<(String, StubRoute)>) -> Fixture {
        Fixture::start_in(lang, &config_dir(), routes)
    }

    /// Starts Quorum in `lang` on the config directory `dir`, with `routes`
    /// ahead of the recorded ones.
    pub fn start_in(lang: Lang, dir: &Path, routes: Vec<(String, StubRoute)>) -> Fixture {
        set_language(lang);
        // SAFETY: the tests that write the environment run one at a time
        // (`--test-threads=1`), so nothing else reads it while it is written.
        unsafe { std::env::set_var(quorum::config::CONFIG_DIR_VAR, dir) };
        let mut routes = routes;
        routes.extend(recorded());
        let network = Arc::new(StubNetwork::new(routes));
        let mut config = EngineConfig::new(Root::of::<quorum::App>());
        // The sidebar shows at 900 points and up: the tests press its pages.
        config.viewport = Size::new(Points(1200.0), Points(900.0));
        config.environment.window = config.viewport;
        config.network = Some(network.clone());
        let mut app = Headless::start(config).expect("the app starts headless");
        app.first_render()
            .unwrap_or_else(|error| panic!("Quorum's first frame: {error}"));
        Fixture { app, network, lang }
    }

    /// `msg` in the language the fixture set.
    pub fn say(&self, msg: &Msg) -> String {
        msg.in_lang(self.lang).to_string()
    }

    /// Presses the sidebar's button for `page`, as a person does.
    pub fn open(&mut self, page: Msg) -> Result<(), String> {
        let label = self.say(&page);
        self.app.press(&format!("label:{label}"))
    }

    /// Presses the one view `selector` names (`label:…`, `text:…`).
    pub fn press(&mut self, selector: &str) -> Result<(), String> {
        self.app.press(selector)
    }

    /// Sets the language variable to `lang` and reports a locale change, as
    /// the platform does when the user's language changes: the same engine
    /// goes on running.
    pub fn change_language(&mut self, lang: Lang, locale: &str) -> Result<(), String> {
        set_language(lang);
        self.lang = lang;
        let locale = nsh_core::platform::Locale::parse(locale).ok_or("a locale identifier")?;
        self.app.set_environment("locale", |environment| environment.locale = locale)
    }

    /// Every text the window shows now, in tree order.
    pub fn texts(&self) -> Vec<String> {
        self.app.screen().texts().into_iter().map(str::to_owned).collect()
    }

    /// Whether the window shows `text` (one text, whole, as read).
    pub fn shows(&self, text: &str) -> bool {
        self.app.screen().shows(text)
    }

    /// Waits for the window to show `text`, `timeout` at most.
    pub fn wait_for(&mut self, text: &str, timeout: Duration) -> Result<(), String> {
        self.app
            .wait_until(&format!("`{text}` showing"), timeout, |screen| screen.shows(text))
    }

    /// Waits for the window to show every text of `texts`.
    pub fn wait_for_all(&mut self, texts: &[String], timeout: Duration) -> Result<(), String> {
        self.app
            .wait_until(&format!("{texts:?} showing"), timeout, |screen| {
                texts.iter().all(|text| screen.shows(text))
            })
    }

    /// Waits for the window to show one of `texts`.
    pub fn wait_for_any(&mut self, texts: &[String], timeout: Duration) -> Result<(), String> {
        self.app
            .wait_until(&format!("one of {texts:?} showing"), timeout, |screen| {
                texts.iter().any(|text| screen.shows(text))
            })
    }

    /// Waits for a text the regular expression `pattern` matches (Rust's
    /// `regex` syntax).
    pub fn wait_for_matching(&mut self, pattern: &str, timeout: Duration) -> Result<(), String> {
        self.app.wait_for_within(&format!("text:/{pattern}/"), timeout)
    }

    /// How many `method` requests for `url` Quorum made.
    pub fn requests(&self, method: &str, url: &str) -> usize {
        self.network
            .requests()
            .iter()
            .filter(|request| request.method == method && request.url == url)
            .count()
    }

    /// Waits until `method` `url` was requested `at_least` times.
    pub fn wait_for_requests(
        &mut self,
        method: &str,
        url: &str,
        at_least: usize,
        timeout: Duration,
    ) -> Result<(), String> {
        let network = self.network.clone();
        let method = method.to_owned();
        let url = url.to_owned();
        self.app
            .wait_until(&format!("{at_least} × {method} {url}"), timeout, move |_| {
                network
                    .requests()
                    .iter()
                    .filter(|request| request.method == method && request.url == url)
                    .count()
                    >= at_least
            })
    }
}

/// Sets `OMOMP_LANG`, the variable the oh-my-omp convention reads first.
pub fn set_language(lang: Lang) {
    let code = match lang {
        Lang::Zh => "zh",
        Lang::En => "en",
    };
    // SAFETY: see [`path_with`].
    unsafe { std::env::set_var("OMOMP_LANG", code) };
}

/// The bases panel's note, from the recorded guardians answer's counts.
pub fn bases_note() -> Msg {
    let bases = bases();
    let ready = bases
        .iter()
        .filter(|base| base["state"] == "ready")
        .count();
    let stale = bases
        .iter()
        .filter(|base| base["stale"].as_bool().unwrap_or(false))
        .count();
    Msg::BasesNote {
        ready,
        total: bases.len(),
        stale,
    }
}

/// The bases in the recorded guardians answer.
fn bases() -> Vec<serde_json::Value> {
    let bases: serde_json::Value =
        serde_json::from_str(GUARDIANS_BASES).expect("guardians-bases.json");
    bases
        .as_array()
        .expect("an array of bases")
        .clone()
}

/// The versions the programs answered with (`GET /v1/health`), as the
/// programs panel writes them.
pub fn health_versions() -> Vec<String> {
    [COUNCIL_HEALTH, GUARDIANS_HEALTH]
        .iter()
        .map(|json| {
            let health: quorum::programs::Health =
                serde_json::from_str(json).expect("a program's health");
            health.short_version().to_owned()
        })
        .collect()
}

/// The guardians' bases in the recorded answer, by the name the window gives
/// each: the repository without its owner (`short_repo`).
pub fn base_repos() -> Vec<String> {
    bases()
        .iter()
        .map(|base| {
            let repo = base["repo"].as_str().expect("a base's repo");
            repo.rsplit('/').next().unwrap_or(repo).to_owned()
        })
        .collect()
}

/// The titles of the Project's items in the PM repository, as the recorded
/// answer has them and the 「议题」 page lists them: human and agenda issues.
pub fn project_titles() -> Vec<String> {
    let project: serde_json::Value =
        serde_json::from_str(PROJECT).expect("github-project.json");
    project["data"]["repositoryOwner"]["projectV2"]["items"]["nodes"]
        .as_array()
        .expect("the Project's items")
        .iter()
        .filter_map(|node| {
            let content = &node["content"];
            if content["__typename"] != "Issue" || content["repository"]["nameWithOwner"] != PM {
                return None;
            }
            let kind = node["fieldValues"]["nodes"]
                .as_array()?
                .iter()
                .find(|value| value["field"]["name"] == "Kind")?["name"]
                .as_str()?;
            matches!(kind, "Human" | "Agenda").then(|| {
                content["title"]
                    .as_str()
                    .expect("an issue's title")
                    .to_owned()
            })
        })
        .collect()
}

/// Puts `dir` first on `PATH`, ahead of this process's own.
pub fn path_with(dir: &Path) {
    let mut path = OsString::from(dir);
    if let Some(existing) = std::env::var_os("PATH") {
        path.push(":");
        path.push(existing);
    }
    // SAFETY: the credential tests run one at a time (`--test-threads=1`),
    // so nothing else reads these while they are written.
    unsafe { std::env::set_var("PATH", path) };
}

/// Sets `PATH` to `dir` and the system's `/usr/bin:/bin`, where no `gh` is.
pub fn path_without_gh(dir: &Path) {
    // SAFETY: see [`path_with`].
    unsafe { std::env::set_var("PATH", format!("{}:/usr/bin:/bin", dir.display())) };
}

/// Sets `GH_TOKEN`, which the credential reads before `gh`.
pub fn set_github_token(token: &str) {
    // SAFETY: see [`path_with`].
    unsafe { std::env::set_var("GH_TOKEN", token) };
}

/// Removes `GH_TOKEN`, so the credential asks `gh`.
pub fn clear_github_token() {
    // SAFETY: see [`path_with`].
    unsafe { std::env::remove_var("GH_TOKEN") };
}

/// The PM config directory the app loads: the fixture's copy of
/// `config.example/`, its endpoints pointing at the stub network's hosts.
pub fn config_dir() -> PathBuf {
    fixtures_dir().join("config")
}

/// `tests/fixtures/`.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The lines of the recorded `README.md`, the file the config page opens
/// first.
pub fn readme_lines() -> Vec<String> {
    use base64::Engine as _;

    let answer: serde_json::Value = serde_json::from_str(CONFIG_README).expect("the README's answer");
    let packed: String = answer["content"].as_str().expect("its content").split_whitespace().collect();
    let bytes = base64::engine::general_purpose::STANDARD.decode(packed).expect("base64 content");
    String::from_utf8(bytes)
        .expect("a UTF-8 README")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// A temporary directory of a test's own, removed when dropped.
pub struct Sandbox {
    dir: PathBuf,
}

impl Sandbox {
    /// The directory named after `name`, empty.
    pub fn new(name: &str) -> Sandbox {
        let dir = std::env::temp_dir().join(format!(
            "quorum-credential-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        Sandbox { dir }
    }

    /// The directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Writes the executable script `name` into the directory.
    pub fn script(&self, name: &str, contents: &str) {
        use std::os::unix::fs::PermissionsExt;

        let path = self.dir.join(name);
        std::fs::write(&path, contents).expect("the script's text");
        let mut permissions = std::fs::metadata(&path).expect("the script").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("an executable script");
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
