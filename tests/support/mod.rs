//! The headless fixture Meerkat's integration tests start from (#23): the
//! real [`meerkat::App`] on an engine with no window, its council, guardians
//! and GitHub answered from the responses recorded under
//! `tests/fixtures/responses/`, driven the way a person drives it: pressing
//! the sidebar's pages and reading what the window shows.
//!
//! The config it loads is `tests/fixtures/config/` (the example config, its
//! `targets` pointing at the stub network's hosts), handed to the app through
//! `MEERKAT_CONFIG_DIR`. A test that answers some requests itself passes its own
//! routes to [`Fixture::start_with`]; they come before the recorded ones.
//! The detail pages' recordings, and what was written by hand where the
//! records lack a case, are under `tests/fixtures/detail/`; each hand-written
//! file says in its leading `//` lines what it supplements and from what,
//! and [`detail`] reads a file without them.

// Each test crate (`credential`, `i18n`, `queue_state`, `detail`) uses a
// part of the fixture.
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
use meerkat::{Lang, Msg};

/// The stub network's GitHub host: `targets.*.githubApi` in
/// `tests/fixtures/config/meerkat/meerkat.yaml`.
pub const GITHUB: &str = "http://github.test";
/// The stub network's council host.
pub const COUNCIL: &str = "http://council.test";
/// The stub network's guardians host.
const GUARDIANS: &str = "http://guardians.test";
/// The agenda repository the fixture config names (`repos.agenda.repo`).
const AGENDA: &str = "mouriya-s-lab/meerkat-e2e-agenda";

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
// `gh api repos/mouriya-s-lab/meerkat-e2e-agenda/contents/README.md?ref=main`, its
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
        (format!("GET {GITHUB}/repos/{AGENDA}/branches/main"), CONFIG_BRANCH),
        (format!("GET {GITHUB}/repos/{AGENDA}/git/trees/*"), CONFIG_TREE),
        (format!("GET {GITHUB}/repos/{AGENDA}/contents/README.md?ref=main"), CONFIG_README),
    ];
    answers
        .into_iter()
        .map(|(route, json)| (route, StubRoute::Response(answer(200, json))))
        .collect()
}

/// Meerkat running on the headless engine in one language: the window's
/// contents, and what it asked the stub network.
pub struct Fixture {
    app: Headless,
    network: Arc<StubNetwork>,
    lang: Lang,
}

impl Fixture {
    /// Starts Meerkat in `lang` on the recorded answers, in the fixture
    /// config directory, and waits for its first frame.
    pub fn start(lang: Lang) -> Fixture {
        Fixture::start_with(lang, Vec::new())
    }

    /// Starts Meerkat in `lang` with `routes` ahead of the recorded ones, so a
    /// test answers some requests itself.
    pub fn start_with(lang: Lang, routes: Vec<(String, StubRoute)>) -> Fixture {
        Fixture::start_in(lang, &config_dir(), routes)
    }

    /// Starts Meerkat in `lang` on the config directory `dir`, with `routes`
    /// ahead of the recorded ones.
    pub fn start_in(lang: Lang, dir: &Path, routes: Vec<(String, StubRoute)>) -> Fixture {
        set_language(lang);
        // SAFETY: the tests that write the environment run one at a time
        // (`--test-threads=1`), so nothing else reads it while it is written.
        unsafe { std::env::set_var(meerkat::config::CONFIG_DIR_VAR, dir) };
        let mut routes = routes;
        routes.extend(recorded());
        let network = Arc::new(StubNetwork::new(routes));
        let mut config = EngineConfig::new(Root::of::<meerkat::App>());
        // The sidebar shows at 900 points and up: the tests press its pages.
        config.viewport = Size::new(Points(1200.0), Points(900.0));
        config.environment.window = config.viewport;
        config.network = Some(network.clone());
        let mut app = Headless::start(config).expect("the app starts headless");
        app.first_render()
            .unwrap_or_else(|error| panic!("Meerkat's first frame: {error}"));
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

    /// How many views `selector` names on screen now.
    pub fn count(&self, selector: &str) -> Result<usize, String> {
        self.app.screen().count(selector)
    }

    /// Whether the one view `selector` names is selected.
    pub fn selected(&self, selector: &str) -> Result<bool, String> {
        self.app.screen().selected(selector)
    }

    /// Waits for the window to show `text`, `timeout` at most.
    pub fn wait_for(&mut self, text: &str, timeout: Duration) -> Result<(), String> {
        self.app
            .wait_until(&format!("`{text}` showing"), timeout, |screen| screen.shows(text))
    }

    /// Waits for a text that contains `part`.
    pub fn wait_for_containing(&mut self, part: &str, timeout: Duration) -> Result<(), String> {
        self.app.wait_until(&format!("a text containing `{part}`"), timeout, |screen| {
            screen.texts().iter().any(|text| text.contains(part))
        })
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

    /// Waits for a view `selector` names (`label:…`, `… >> text:…`).
    pub fn wait_for_view(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        self.app.wait_for_within(selector, timeout)
    }

    /// Waits for the one view `selector` names to be selected.
    pub fn wait_for_selected(&mut self, selector: &str, timeout: Duration) -> Result<(), String> {
        self.app.wait_until(&format!("`{selector}` selected"), timeout, |screen| {
            screen.selected(selector) == Ok(true)
        })
    }

    /// Waits until `done` holds of the texts the window shows, in tree
    /// order; `what` names the wait in its error.
    pub fn wait_until(
        &mut self,
        what: &str,
        timeout: Duration,
        done: impl Fn(&[&str]) -> bool + 'static,
    ) -> Result<(), String> {
        self.app.wait_until(what, timeout, move |screen| done(&screen.texts()))
    }

    /// How many `method` requests for `url` Meerkat made.
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
            let health: meerkat::programs::Health =
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

/// The titles of the Project's items in the agenda repository, as the recorded
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
            if content["__typename"] != "Issue" || content["repository"]["nameWithOwner"] != AGENDA {
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

/// The Meerkat config directory the app loads: the fixture's copy of
/// `config.example/`, its endpoints pointing at the stub network's hosts.
pub fn config_dir() -> PathBuf {
    fixtures_dir().join("config")
}

/// A file under `tests/fixtures/detail/` without its `//` lines, which say
/// what in it was written by hand and from what.
pub fn detail(name: &str) -> String {
    let path = fixtures_dir().join("detail").join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines().filter(|line| !line.starts_with("//")).collect::<Vec<_>>().join("\n")
}

/// The council's recorded `/v1/sessions` answer with the sessions of the
/// JSON array `newer` ahead of them.
pub fn sessions_with(newer: &str) -> String {
    let mut sessions: Vec<serde_json::Value> = serde_json::from_str(newer).expect("the sessions are a JSON array");
    let recorded: Vec<serde_json::Value> =
        serde_json::from_str(COUNCIL_SESSIONS).expect("the recorded sessions are a JSON array");
    sessions.extend(recorded);
    serde_json::Value::Array(sessions).to_string()
}

/// The route of the comments of `issue` (`owner/repo#n`), as Meerkat asks
/// GitHub for them.
pub fn comments_route(issue: &str) -> String {
    let (repo, number) = issue.split_once('#').expect("an issue is owner/repo#n");
    format!("GET {GITHUB}/repos/{repo}/issues/{number}/comments?per_page=100&page=1")
}

/// The council session whose log `tests/fixtures/detail/` recorded: the
/// round 1 Coordination vote on G25 (`design-23-vote-log.jsonl`).
pub const VOTE_LOG_SESSION: &str = "01a1b625-d001-7000-8000-000000000002";

/// The Project the detail tests read: the recording with the three
/// implementation issues still open (`github-project-open-implementations.json`).
pub fn detail_project() -> String {
    detail("github-project-open-implementations.json")
}

/// The recorded comments of `issue` (`owner/repo#n`) the detail tests
/// answer with, if any were recorded: under `detail/`, else the earlier
/// recordings under `tests/fixtures/`.
pub fn recorded_comments(issue: &str) -> Option<String> {
    let (repo, number) = issue.split_once('#').expect("an issue is owner/repo#n");
    let prefix = match repo {
        "mouriya-s-lab/meerkat-e2e-design" => "design",
        "mouriya-s-lab/meerkat-e2e-code" => "code",
        "mouriya-s-lab/meerkat-e2e-agenda" => "agenda",
        _ => return None,
    };
    let name = format!("{prefix}-{number}-comments.json");
    if fixtures_dir().join("detail").join(&name).is_file() {
        return Some(detail(&name));
    }
    std::fs::read_to_string(fixtures_dir().join(&name)).ok()
}

/// The issues whose comments [`recorded_comments`] holds.
pub fn recorded_issues() -> Vec<String> {
    let mut issues = Vec::new();
    for dir in [fixtures_dir().join("detail"), fixtures_dir()] {
        for entry in std::fs::read_dir(&dir).expect("a fixture directory") {
            let name = entry.expect("a directory entry").file_name().to_string_lossy().into_owned();
            let Some((prefix, number)) = name.strip_suffix("-comments.json").and_then(|stem| stem.split_once('-')) else {
                continue;
            };
            if number.parse::<u64>().is_err() {
                continue;
            }
            let repo = match prefix {
                "design" => "mouriya-s-lab/meerkat-e2e-design",
                "code" => "mouriya-s-lab/meerkat-e2e-code",
                "agenda" => "mouriya-s-lab/meerkat-e2e-agenda",
                _ => continue,
            };
            let issue = format!("{repo}#{number}");
            if !issues.contains(&issue) {
                issues.push(issue);
            }
        }
    }
    issues
}

/// What the detail pages read beyond the overview's recordings: the
/// Project with open implementation issues, the council's sessions with
/// G25's ahead of them, the recorded vote log, and every recorded issue's
/// comments.
pub fn detail_routes() -> Vec<(String, StubRoute)> {
    let mut routes = vec![
        (format!("POST {GITHUB}/graphql"), detail_project()),
        (
            format!("GET {COUNCIL}/v1/sessions?limit=200"),
            sessions_with(&detail("design-23-sessions.json")),
        ),
        (
            format!("GET {COUNCIL}/v1/sessions/{VOTE_LOG_SESSION}/log"),
            detail("design-23-vote-log.jsonl"),
        ),
    ];
    for issue in recorded_issues() {
        let json = recorded_comments(&issue).expect("a recorded issue's comments");
        routes.push((comments_route(&issue), json));
    }
    routes
        .into_iter()
        .map(|(route, body)| (route, StubRoute::Response(answer(200, &body))))
        .collect()
}

/// A copy of the fixture config in `sandbox` whose agenda repository is
/// `repo`.
pub fn config_with_agenda(sandbox: &Sandbox, repo: &str) -> PathBuf {
    let dir = sandbox.dir().join("config");
    copy_dir(&config_dir(), &dir);
    let repos = dir.join("shared/repos.yaml");
    let text = std::fs::read_to_string(&repos).expect("repos.yaml");
    let block = format!("agenda:\n  repo: {AGENDA}\n");
    assert!(text.contains(&block), "the fixture's repos.yaml names another agenda repository");
    std::fs::write(&repos, text.replace(&block, &format!("agenda:\n  repo: {repo}\n"))).expect("repos.yaml written");
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a fixture directory") {
        let path = entry.expect("an entry").path();
        let target = to.join(path.file_name().expect("a name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("a copied file");
        }
    }
}

/// Meerkat in `lang` on a copy of the fixture config whose agenda repository is
/// `repo`, answered from the detail recordings, `routes` ahead of them.
pub fn start_detail(sandbox: &Sandbox, repo: &str, lang: Lang, routes: Vec<(String, StubRoute)>) -> Fixture {
    set_github_token("gho_q4_detail_token");
    let dir = config_with_agenda(sandbox, repo);
    let mut routes = routes;
    routes.extend(detail_routes());
    Fixture::start_in(lang, &dir, routes)
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
            "meerkat-credential-{name}-{}",
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
