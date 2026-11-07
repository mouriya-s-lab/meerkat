//! #12's acceptance rows 2–4: the real app on the headless engine (see
//! `tests/support/`), on the recorded answers and the fixture config, in
//! Chinese and in English.
//!
//! Every text a page shows must be explained by the union of two sets: the
//! data (what the recorded answers and the config hold, with the forms the
//! window gives it: a line of a text, a path's segments, a short commit) and
//! every [`Msg`] variant worded in the language under test. A text is
//! explained when it splits into pieces that are each a datum, a [`Msg`]'s
//! words, or a run without letters (digits, clock times, punctuation); two
//! pieces may not meet inside a word. A [`Msg`] with fields is worded once
//! per combination of field values: its finite fields (states, programs,
//! readers, booleans, the counts 0, 1 and many, the nested messages the
//! views pass) take each value, its text and number fields take
//! placeholders that become holes, so the variant's words stay fixed and
//! only its holes vary: a number, or data, other messages' fixed words and
//! runs without letters.
//!
//! The tests rewrite this process's `OMOMP_LANG`, `GH_TOKEN` and
//! `PM_CONFIG_DIR`: they take [`serial`]'s lock, so they run one at a time
//! (the issue's commands add `--test-threads=1`).

mod support;

use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use base64::Engine as _;
use quorum::{
    Lang, Msg, QueueMotion,
    config::{self, ConfigError, Problem},
    dag::{self, Closed, Group},
    failure::{self, Failure, NoCredential},
    gh,
    programs::{BaseState, Program, QueueState, SessionState},
    time::{self, Ago},
};
use support::Fixture;

/// How long the window may take to show what a recorded answer holds.
const SHOWN: Duration = Duration::from_secs(10);

/// Holds one test at a time: each rewrites the process's environment.
fn serial() -> MutexGuard<'static, ()> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn every_page_in_chinese() -> Result<(), String> {
    every_page(Lang::Zh)
}

#[test]
fn every_page_in_english() -> Result<(), String> {
    let words = status_words();
    // #9's status word table: each Chinese word has one English word, and
    // no English word stands for two Chinese ones.
    let mut english: HashMap<String, String> = HashMap::new();
    let mut chinese: HashMap<String, String> = HashMap::new();
    for (msg, zh, en) in &words {
        assert_eq!(msg.in_lang(Lang::Zh).to_string(), *zh, "{msg:?} in Chinese");
        assert_eq!(msg.in_lang(Lang::En).to_string(), *en, "{msg:?} in English");
        let had = english.insert((*zh).to_owned(), (*en).to_owned());
        assert!(had.is_none_or(|had| had == *en), "{zh} has two English words");
        let had = chinese.insert((*en).to_owned(), (*zh).to_owned());
        assert!(had.is_none_or(|had| had == *zh), "{en} stands for two Chinese words");
    }
    every_page(Lang::En)
}

#[test]
fn language_follows_locale_change() -> Result<(), String> {
    let _serial = serial();
    support::set_github_token("gho_q12_i18n_token");
    let mut quorum = Fixture::start(Lang::Zh);
    let data = Data::of(&support::config_dir(), &[]);
    wait_for_overview(&mut quorum)?;
    let before = quorum.texts();
    check_page(&Union::new(Lang::Zh, &data), "概览 (zh)", &before)?;
    assert!(
        quorum.shows(&Msg::PageOverview.in_lang(Lang::Zh).to_string()),
        "the overview's title is not in Chinese"
    );

    // The user's language becomes English: the platform reports a new
    // locale, the same engine stays on the same page.
    quorum.change_language(Lang::En, "en-GB")?;
    quorum.wait_for(&quorum.say(&Msg::PageOverview), SHOWN)?;
    quorum.wait_for(&quorum.say(&support::bases_note()), SHOWN)?;
    let after = quorum.texts();
    check_page(&Union::new(Lang::En, &data), "overview (en)", &after)?;
    assert!(
        !quorum.shows(&Msg::PageOverview.in_lang(Lang::Zh).to_string()),
        "the Chinese title is still on screen"
    );
    assert_ne!(before, after, "the overview's texts did not change");
    Ok(())
}

/// Opens every page in `lang` and checks each text on it, then starts on
/// an invalid config and checks the configuration error page, then starts
/// with no GitHub credential and checks the panels that read GitHub.
fn every_page(lang: Lang) -> Result<(), String> {
    let _serial = serial();
    // The credential comes from `GH_TOKEN`: no `gh` runs.
    support::set_github_token("gho_q12_i18n_token");
    let mut quorum = Fixture::start(lang);
    let data = Data::of(&support::config_dir(), &[]);
    let union = Union::new(lang, &data);
    let mut pages: Vec<(&str, Vec<String>)> = Vec::new();

    wait_for_overview(&mut quorum)?;
    let app = quorum.say(&Msg::AppName);
    assert_eq!(app, "Quorum");
    assert!(quorum.shows(&app), "the sidebar does not name the app Quorum");
    pages.push(("overview", quorum.texts()));

    quorum.open(Msg::PageIssues)?;
    quorum.wait_for_any(&support::project_titles(), SHOWN)?;
    let graph = quorum.say(&Msg::ViewGraph);
    quorum.press(&format!("label:{graph}"))?;
    quorum.wait_for(&quorum.say(&Msg::Fit), SHOWN)?;
    pages.push(("issues: graph", quorum.texts()));
    let list = quorum.say(&Msg::ViewList);
    quorum.press(&format!("label:{list}"))?;
    quorum.wait_for(&quorum.say(&Msg::Agenda), SHOWN)?;
    pages.push(("issues: list", quorum.texts()));

    quorum.open(Msg::PageRuntime)?;
    quorum.wait_for(&quorum.say(&Msg::OldestWarm), SHOWN)?;
    quorum.wait_for(&quorum.say(&Msg::Limits), SHOWN)?;
    pages.push(("runtime", quorum.texts()));

    quorum.open(Msg::PageSessions)?;
    quorum.wait_for(&quorum.say(&Msg::SelectSession), SHOWN)?;
    quorum.wait_for(&quorum.say(&Msg::Kind("vote".to_owned())), SHOWN)?;
    pages.push(("sessions", quorum.texts()));

    quorum.open(Msg::PageConfig)?;
    quorum.wait_for(&quorum.say(&Msg::VersionsTitle), SHOWN)?;
    quorum.wait_for_any(&support::readme_lines(), SHOWN)?;
    pages.push(("config", quorum.texts()));

    let failures: Vec<String> = pages
        .iter()
        .filter_map(|(page, texts)| check_page(&union, page, texts).err())
        .collect();
    drop(quorum);

    // A config the parser rejects: the configuration error page. Its detail
    // is the parser's words about the config, as they are.
    let invalid = support::fixtures_dir().join("config-invalid");
    let mut broken = Fixture::start_in(lang, &invalid, Vec::new());
    broken.wait_for(&broken.say(&Msg::ConfigError), SHOWN)?;
    let error = quorum::config::load().err().ok_or("the invalid config loads")?;
    let Problem::Said(said) = error.message else {
        return Err(format!("the invalid config's problem is not the parser's: {:?}", error.message));
    };
    let data = Data::of(&invalid, &[error.file, error.path, said]);
    let errors = check_page(&Union::new(lang, &data), "configuration error", &broken.texts()).err();
    drop(broken);

    let missing = without_credential(lang)?;

    let failures: Vec<String> = failures.into_iter().chain(errors).chain(missing).collect();
    if failures.is_empty() { Ok(()) } else { Err(failures.join("\n")) }
}

/// #23's `gh_missing`: no `GH_TOKEN` and no `gh` on `PATH`. The overview's
/// and the issues page's GitHub panels say why there is no credential; each
/// text on them is checked. The reason holds the system's error running
/// `gh`, which is data: the test asks the system for it the same way.
fn without_credential(lang: Lang) -> Result<Option<String>, String> {
    let path = std::env::var_os("PATH");
    let sandbox = support::Sandbox::new("i18n-gh-missing");
    support::path_without_gh(sandbox.dir());
    support::clear_github_token();
    let system = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .err()
        .ok_or("`gh` runs though PATH has none")?
        .to_string();
    let checked = (|| {
        let mut quorum = Fixture::start(lang);
        let reason = quorum.say(&Msg::Failure(Failure::NoCredential(NoCredential::GhCannotRun(system.clone()))));
        let data = Data::of(&support::config_dir(), std::slice::from_ref(&system));
        let union = Union::new(lang, &data);
        quorum.wait_for_containing(&reason, SHOWN)?;
        let overview = quorum.texts();
        quorum.open(Msg::PageIssues)?;
        quorum.wait_for_containing(&reason, SHOWN)?;
        let issues = quorum.texts();
        let failures: Vec<String> = [("overview, no credential", overview), ("issues, no credential", issues)]
            .iter()
            .filter_map(|(page, texts)| check_page(&union, page, texts).err())
            .collect();
        Ok::<_, String>((!failures.is_empty()).then(|| failures.join("\n")))
    })();
    if let Some(path) = path {
        // SAFETY: the tests run one at a time ([`serial`]).
        unsafe { std::env::set_var("PATH", path) };
    }
    checked
}

/// Waits for the overview's council, guardians and GitHub data.
fn wait_for_overview(quorum: &mut Fixture) -> Result<(), String> {
    let mut texts = vec![quorum.say(&Msg::CouncilQueue), quorum.say(&support::bases_note())];
    texts.extend(support::base_repos());
    texts.extend(support::health_versions());
    quorum.wait_for_all(&texts, SHOWN)?;
    quorum.wait_for_any(&support::project_titles(), SHOWN)
}

/// Every text of `page` the union explains; else each one it does not.
fn check_page(union: &Union, page: &str, texts: &[String]) -> Result<(), String> {
    let unexplained: Vec<String> = texts
        .iter()
        .filter(|text| !union.explains(text.trim()))
        .map(|text| format!("{page} ({:?}): {text:?} is neither data nor a Msg", union.lang))
        .collect();
    if unexplained.is_empty() { Ok(()) } else { Err(unexplained.join("\n")) }
}

/// #9's status words, the message, its Chinese and its English.
fn status_words() -> Vec<(Msg, &'static str, &'static str)> {
    vec![
        (Msg::Queue(QueueState::Decided), "已定案", "Decided"),
        (Msg::Queue(QueueState::Superseded), "已取代", "Superseded"),
        (Msg::Queue(QueueState::Blocked), "等前置", "Blocked"),
        (Msg::Queue(QueueState::Held), "被人类议题挂起", "Held by a human issue"),
        (Msg::Queue(QueueState::Deliberating), "在审", "Deliberating"),
        (Msg::Queue(QueueState::Interrupted), "中断", "Interrupted"),
        (Msg::Queue(QueueState::Failed), "失败", "Failed"),
        (Msg::Queue(QueueState::Ready), "可召集", "Ready to convene"),
        (Msg::Session(SessionState::Running), "运行中", "Running"),
        (Msg::Session(SessionState::Done), "完成", "Done"),
        (Msg::Session(SessionState::Failed), "失败", "Failed"),
        (Msg::Base(BaseState::Warming), "预热中", "Warming"),
        (Msg::Base(BaseState::Ready), "就绪", "Ready"),
        (Msg::Base(BaseState::Failed), "失败", "Failed"),
        (Msg::Stale, "过期", "Stale"),
        (Msg::Online, "在线", "Online"),
        (Msg::Unreachable, "不可达", "Unreachable"),
        (Msg::Escalated, "已提为议题", "Escalated"),
        (Msg::RateLow, "API 余量低", "API allowance low"),
    ]
}

// -- the data --------------------------------------------------------------

/// What the recorded answers and a config directory hold, in the forms the
/// window gives it.
struct Data(BTreeSet<String>);

impl Data {
    /// The recorded answers, the config in `config`, and `extra`.
    fn of(config: &Path, extra: &[String]) -> Data {
        let mut raw: Vec<String> = extra.to_vec();
        let responses = support::fixtures_dir().join("responses");
        for file in files(&responses) {
            let text = std::fs::read_to_string(&file).expect("a recorded answer");
            let json: serde_json::Value = serde_json::from_str(&text).expect("a JSON answer");
            json_strings(&json, &mut raw);
        }
        for file in files(config) {
            // The config's files by their paths in the directory.
            let path = file.strip_prefix(config).expect("a file in the config directory");
            raw.push(path.to_string_lossy().into_owned());
            let text = std::fs::read_to_string(&file).expect("a config file");
            let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).expect("a config file");
            yaml_strings(&yaml, &mut raw);
        }
        // What the window derives from the answers with the app's own code.
        raw.extend(support::health_versions());
        raw.extend(support::base_repos());
        let mut data = BTreeSet::new();
        for text in raw {
            derive(&text, &mut data);
        }
        data.retain(|datum| datum.chars().any(char::is_alphabetic));
        Data(data)
    }
}

/// `text`, each of its lines, its `/`- and `#`-separated segments and what
/// follows each separator, and a commit sha's short forms.
fn derive(text: &str, out: &mut BTreeSet<String>) {
    for line in std::iter::once(text).chain(text.lines()) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        out.insert(line.to_owned());
        for (i, c) in line.char_indices() {
            if c == '/' || c == '#' {
                out.insert(line[i + 1..].to_owned());
            }
        }
        out.extend(line.split(['/', '#']).map(str::to_owned));
        if line.len() == 40 && line.bytes().all(|b| b.is_ascii_hexdigit()) {
            out.insert(line[..7].to_owned());
        }
    }
}

fn json_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => items.iter().for_each(|item| json_strings(item, out)),
        serde_json::Value::Object(fields) => {
            // A file's contents (`GET /repos/…/contents/…`) are base64.
            if fields.get("encoding").and_then(|e| e.as_str()) == Some("base64")
                && let Some(content) = fields.get("content").and_then(|c| c.as_str())
            {
                let packed: String = content.split_whitespace().collect();
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(packed)
                    .expect("base64 contents");
                let text = String::from_utf8(bytes).expect("UTF-8 contents");
                // The config page reads a file as YAML: its keys too.
                if let Ok(yaml) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) {
                    yaml_strings(&yaml, out);
                }
                out.push(text);
            }
            fields.values().for_each(|field| json_strings(field, out));
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

/// A config file's keys and scalar values (the config page shows both).
fn yaml_strings(value: &serde_yaml_ng::Value, out: &mut Vec<String>) {
    use serde_yaml_ng::Value;
    match value {
        Value::String(text) => out.push(text.clone()),
        Value::Bool(b) => out.push(b.to_string()),
        Value::Sequence(items) => items.iter().for_each(|item| yaml_strings(item, out)),
        Value::Mapping(fields) => {
            for (key, field) in fields {
                yaml_strings(key, out);
                yaml_strings(field, out);
            }
        }
        Value::Tagged(tagged) => yaml_strings(&tagged.value, out),
        Value::Null | Value::Number(_) => {}
    }
}

/// The files under `dir`, at any depth.
fn files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).expect("a fixture directory") {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            found.extend(files(&path));
        } else {
            found.push(path);
        }
    }
    found
}

// -- the messages ----------------------------------------------------------

/// A text field's placeholder.
const S: &str = "\u{E000}";
/// A list element's placeholder.
const L: &str = "\u{E001}";
/// A number field's placeholder ("many": not 0, not 1).
const N: u64 = 9_000_001;
/// Where a number's placeholder was, while a text is cut into parts.
const NUMBER: char = '\u{E002}';

/// A worded message: fixed words and holes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Part {
    Words(String),
    /// Data, fixed words and runs without letters, one after another.
    Text,
    Number,
    /// One or more [`Part::Text`]s, `separator` between two.
    List { separator: String },
}

/// `msg`'s words in `lang` with its placeholders turned into holes.
fn template(msg: &Msg, lang: Lang) -> Vec<Part> {
    let worded = msg
        .in_lang(lang)
        .to_string()
        // A placeholder written with `{:?}`.
        .replace("\\u{e000}", S)
        .replace("9,000,001", &NUMBER.to_string())
        .replace(&N.to_string(), &NUMBER.to_string());
    #[derive(PartialEq)]
    enum Token {
        Char(char),
        Text,
        Element,
        Number,
    }
    let tokens: Vec<Token> = worded
        .chars()
        .map(|c| match c {
            '\u{E000}' => Token::Text,
            '\u{E001}' => Token::Element,
            NUMBER => Token::Number,
            c => Token::Char(c),
        })
        .collect();
    let mut parts: Vec<Part> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i] {
            Token::Char(c) => match parts.last_mut() {
                Some(Part::Words(words)) => words.push(*c),
                _ => parts.push(Part::Words(c.to_string())),
            },
            Token::Text => parts.push(Part::Text),
            Token::Number => parts.push(Part::Number),
            Token::Element => {
                // Two elements and what stands between them.
                let mut separator = String::new();
                let mut j = i + 1;
                while let Some(Token::Char(c)) = tokens.get(j) {
                    separator.push(*c);
                    j += 1;
                }
                if tokens.get(j) == Some(&Token::Element) {
                    i = j;
                }
                parts.push(Part::List { separator });
            }
        }
        i += 1;
    }
    parts
}

/// What is wrong with a config file, each of Quorum's findings and others'
/// words.
fn problems() -> Vec<Problem> {
    let mut problems = vec![
        Problem::Said(s()),
        Problem::NotUtf8(s()),
        Problem::DirUnset,
        Problem::NotJson(s()),
        Problem::NotString(s()),
        Problem::NotFontList(s()),
        Problem::NotLength(s()),
        Problem::NotColor(s()),
        Problem::NotObject(s()),
        Problem::MissingToken,
        Problem::UnknownToken,
    ];
    problems.extend(direct_failures().into_iter().map(|why| Problem::Unreachable(Box::new(why))));
    problems
}

/// A config error of each problem, with a key path and without.
fn config_errors() -> Vec<ConfigError> {
    problems()
        .into_iter()
        .flat_map(|message| {
            [String::new(), s()].map(|path| ConfigError { file: s(), path, message: message.clone() })
        })
        .collect()
}

/// Why something did not come, but for a config error.
fn direct_failures() -> Vec<Failure> {
    vec![
        Failure::Said(s()),
        Failure::Unparsable { url: s(), error: s() },
        Failure::CredentialPending,
        Failure::NoCredential(NoCredential::GhFailed(s())),
        Failure::NoCredential(NoCredential::GhSilent),
        Failure::NoCredential(NoCredential::GhCannotRun(s())),
        Failure::GraphQlNoData,
        Failure::ProjectNotFound { owner: s(), number: N },
        Failure::TreeTruncated { repo: s(), head: s() },
        Failure::Encoding { path: s(), encoding: s() },
        Failure::NotUtf8 { path: s() },
        Failure::ParseStopped(s()),
    ]
}

/// Why something did not come: each [`failure::Failure`].
fn failures() -> Vec<Failure> {
    let mut all = direct_failures();
    all.extend(config_errors().into_iter().map(failure::Failure::Config));
    all
}

fn s() -> String {
    S.to_owned()
}

/// The pages, as the command bar's 「前往」 names them.
fn pages() -> [Msg; 5] {
    [Msg::PageOverview, Msg::PageIssues, Msg::PageSessions, Msg::PageRuntime, Msg::PageConfig]
}

/// What the programs panels say a program is working on.
fn works() -> Vec<Msg> {
    let n = counts().map(|k| k as usize);
    let mut works: Vec<Msg> = maybe(&n).into_iter().map(Msg::Deliberating).collect();
    works.extend(maybe(&n).into_iter().map(Msg::AsksRunning));
    for open in n {
        works.extend(n.map(|deliberating| Msg::QueueWork { open, deliberating }));
    }
    works
}

fn counts() -> [u64; 3] {
    [0, 1, N]
}

fn agos() -> Vec<Ago> {
    vec![Ago::Now, Ago::Seconds(N), Ago::Minutes(N), Ago::Hours(N), Ago::Days(N)]
}

fn maybe<T: Clone>(values: &[T]) -> Vec<Option<T>> {
    std::iter::once(None).chain(values.iter().cloned().map(Some)).collect()
}

fn programs() -> [Program; 2] {
    [Program::Council, Program::Guardians]
}

/// The kinds the UI names, and one it does not.
fn kinds() -> Vec<String> {
    ["vote", "draft", "base", "proxy", "query"].iter().map(|k| (*k).to_owned()).chain([s()]).collect()
}

/// An empty list, and a list of two placeholders.
fn lists() -> Vec<Vec<String>> {
    vec![Vec::new(), vec![L.to_owned(), L.to_owned()]]
}

/// Every variant of [`Msg`], once per combination of its field values.
fn instances() -> Vec<Msg> {
    use Msg::*;
    let mut all = vec![
        PageOverview, PageIssues, PageSessions, PageRuntime, PageConfig, GroupCouncil, GroupRun,
        SearchHint, KeyPalette, Checking, Unknown, Refresh, Back, ConfigError, ConfigHint,
        LoadingConfig, AppName, AppMark, GitHubApi, Llm429, Stale, Online, Unreachable,
        Escalated, RateLow, RefreshNow, ShowList, ShowGraph, KeyRefresh, GroupIssues,
        GroupSessions, GroupCommands, NoResults, KeyUpDown, KeyEnter, KeyEscape, HintMove,
        HintOpen, HintClose, PaletteHint, Retry, FetchingCredential, RunningWell,
        NothingNeedsYou, WaitForYou, NeedsYou, NothingNeedsYouLine, Expand, OpenIssue,
        ViewSession, AllIssues, CouncilQueue, QueueEmpty, View, Programs, Total, Bases, NoBases,
        SessionErrors, NoErrors, RewarmSending, RewarmStarted, RewarmBusy, RewarmUnknown,
        Rewarm, RewarmConfirm, Cancel, OldestWarm, Model, State, WarmedAt, SelectBase, No,
        Error, BaseId, SessionId, RewarmNote, Limits, LimitedNote, Now, All, FilterProgram,
        FilterKind, SessionsFilterHint, Started, SessionColumn, Duration, NoSessions,
        SelectSession, Parsing, NotListed, Ended, ForkedFrom, SessionEnded, DecisionLog,
        OnlyErrors, Compact, Json, NothingAbnormal, Opening, ProgramPrompt, Continuation,
        Assistant, NoResult, Arguments, ToolFailed, Result, NotRetried, Human, ViewList,
        ViewGraph, FilterState, FilterClosed, FilterHuman, IssuesFilterHint, Agenda, Title,
        Progress, Deps, UpdatedColumn, NoIssues, ZoomOut, ZoomIn, Fit, WaitingForData,
        SelectIssue, ThisRound, Voted, Pending, WaitsNothingOpen, WhyNotStarted, None,
        Prerequisites, Blocking, IgnoredBlockers, NotCounted, CouncilNotes, OpenOnGitHub,
        OpenDetail, FilterFiles, TopFiles, NoReaders, ReadersLabel, OlderMark, EditedMark,
        ModePreview, ModeYaml, ModeChanges, SameAsBranch, GateChecking, GatePass, GateBlocked,
        Discard, Saving, OpenCommit, ChecksTitle, Unchanged, CheckRunning, Passed, NoInput,
        VersionsTitle, QuorumHere, Checkout, Pull, Pulling,
    ];
    let n = counts();
    let pairs: Vec<(u64, u64)> = n.iter().flat_map(|a| n.iter().map(move |b| (*a, *b))).collect();
    let triples: Vec<(u64, u64, u64)> =
        pairs.iter().flat_map(|(a, b)| n.iter().map(move |c| (*a, *b, *c))).collect();
    let bools = [false, true];

    // -- shell
    all.extend(programs().map(Program));
    all.extend(pairs.iter().map(|(a, b)| Allowance(*a, *b)));
    all.push(Resets(s()));
    all.extend(agos().into_iter().map(Updated));
    all.extend(n.map(|h| LastHours(h as u32)));
    let apis = [gh::rate::Api::Rest, gh::rate::Api::GraphQl];
    all.extend(apis.map(GitHubBucket));
    let mut tooks: Vec<time::Took> = (0..=10).map(time::Took::Tenths).collect();
    tooks.extend([time::Took::Seconds(N), time::Took::Minutes(N, N), time::Took::Hours(N, N)]);
    all.extend(tooks.into_iter().map(Took));

    // -- status words
    all.extend(
        [
            QueueState::Decided,
            QueueState::Superseded,
            QueueState::Blocked,
            QueueState::Held,
            QueueState::Deliberating,
            QueueState::Interrupted,
            QueueState::Failed,
            QueueState::Ready,
        ]
        .map(Queue),
    );
    all.extend([SessionState::Running, SessionState::Done, SessionState::Failed].map(Session));
    let base_states = [BaseState::Warming, BaseState::Ready, BaseState::Failed];
    all.extend(base_states.map(Base));
    all.push(StatusUnset(s()));
    all.extend([dag::Fallback::Unreachable, dag::Fallback::Unlisted].map(|why| Msg::Fallback { word: s(), why }));

    // -- command bar
    all.extend(pages().map(|page| GoTo(Box::new(page))));
    all.extend(n.map(|k| KeyPage(k as usize)));

    // -- parts
    all.push(TokenKey(s()));
    all.extend(failures().into_iter().map(Msg::Failure));
    all.extend(config_errors().into_iter().map(ConfigFailure));
    all.extend(problems().into_iter().map(Msg::Problem));
    all.extend(maybe(&[s()]).into_iter().map(LastLoaded));

    // -- overview
    for council in bools {
        for guardians in bools {
            all.push(ProgramsDown { council, guardians });
            for queue in maybe(&[QueueMotion::Moving, QueueMotion::Idle]) {
                all.push(ProgramsLine { council, guardians, queue });
            }
        }
    }
    all.extend(n.map(|k| FailureKinds(k as usize)));
    all.extend(n.map(|k| AsksWaiting(k as usize)));
    all.extend(n.map(|k| StaleBases(k as usize)));
    all.extend(n.map(|h| WithinHours(h as u32)));
    all.extend(bools.map(|last| ListSep { last }));
    all.extend(agos().into_iter().map(Ago));
    all.extend(n.map(|k| IssueTimes(s(), k as usize)));
    all.extend(n.map(|k| Times(k as usize)));
    all.push(AgendaFailed(s()));
    all.extend(programs().map(ProgramUnreachable));
    all.push(BaseFailedTitle(s()));
    for program in programs() {
        for kind in kinds() {
            for about in maybe(&[s()]) {
                all.push(SessionFailedOne { program, kind: kind.clone(), about });
            }
            all.push(SessionsFailed { program, kind });
        }
    }
    all.push(EscalationTitle(s()));
    all.extend(n.map(|k| EscalationFrom(s(), k)));
    all.push(BaseStaleTitle(s()));
    all.extend(agos().into_iter().map(WarmedAgo));
    all.extend(apis.map(RateLowTitle));
    all.extend(pairs.iter().map(|(remaining, limit)| RateLeft { remaining: *remaining, limit: *limit, reset: s() }));
    all.extend(n.map(|k| ViewSessions(k as usize)));
    all.push(OpenAgenda(s()));
    all.extend(n.map(|k| OpenCount(k as usize)));
    all.extend(agos().into_iter().map(Span));
    all.extend(maybe(&n.map(|k| k as usize)).into_iter().map(Deliberating));
    all.extend(maybe(&n.map(|k| k as usize)).into_iter().map(AsksRunning));
    for online in bools {
        for up in maybe(&agos()) {
            all.extend(works().into_iter().map(|work| ProgramLine { online, up, work: Box::new(work) }));
        }
    }
    all.extend(n.map(|h| Hours(h as u32)));
    all.extend(triples.iter().map(|(ready, total, stale)| BasesNote {
        ready: *ready as usize,
        total: *total as usize,
        stale: *stale as usize,
    }));
    all.extend(bools.map(|stale| ModelNote { model: s(), stale }));
    all.extend(pairs.iter().map(|(hours, sessions)| ErrorsNote { hours: *hours as u32, sessions: *sessions as usize }));

    // -- runtime
    all.extend(failures().into_iter().map(RewarmFailed));
    all.push(RewarmDetail { guardian: s(), repo: s(), model: s() });
    all.extend(pairs.iter().map(|(a, b)| Fraction(*a as usize, *b as usize)));
    all.extend(n.map(|h| ForksInHours(h as u32)));
    all.extend(n.map(|h| ForksShort(h as u32)));
    all.extend(base_states.map(StateAndStale));
    all.extend(maybe(&agos()).into_iter().map(|ago| AtAndAgo(s(), ago)));
    all.extend(pairs.iter().map(|(forks, failed)| ForksNote { forks: *forks as usize, failed: *failed as usize }));
    all.extend(n.map(|k| AllForks(k as usize)));
    all.extend(agos().into_iter().map(Running));
    all.extend(pairs.iter().map(|(open, deliberating)| QueueWork { open: *open as usize, deliberating: *deliberating as usize }));
    for work in maybe(&works()) {
        for checked in maybe(&agos()) {
            all.push(WorkAndChecked(work.clone().map(Box::new), checked));
        }
    }
    all.extend(pairs.iter().map(|(hours, sessions)| LimitedSessions { hours: *hours as u32, sessions: *sessions as usize }));

    // -- sessions
    all.extend(kinds().into_iter().map(Kind));
    all.extend(failures().into_iter().map(RecordFailed));
    for kind in kinds() {
        all.extend(programs().map(|program| ProgramKind(kind.clone(), program)));
    }
    all.extend(pairs.iter().map(|(entries, lines)| LogCounts { entries: *entries as usize, lines: *lines as usize }));
    all.extend(pairs.iter().map(|(total, kept)| Truncated { total: *total as usize, kept: *kept as usize }));
    all.extend(n.map(|k| Unreadable(k as usize)));
    for attempt in n {
        all.extend([String::new(), s()].map(|error| RetryAttempt(attempt as u32, error)));
    }
    all.extend(n.map(|k| Retries(k as usize)));
    all.extend(n.map(|k| Lines(k as usize)));
    all.extend(n.map(|k| Folded(s(), k as usize)));
    all.extend(n.map(|k| Items(k as usize)));

    // -- issues
    all.extend(triples.iter().map(|(round, voted, seats)| RoundVoted {
        round: *round as u32,
        voted: *voted as usize,
        seats: *seats as usize,
    }));
    all.extend(pairs.iter().map(|(done, total)| Implemented(*done as u32, *total as u32)));
    all.push(WaitsFor(s()));
    all.push(Blocks(s()));
    all.push(WaitsDecided(s()));
    all.extend(Group::ALL.map(GroupName));
    all.extend([Closed::Fold, Closed::Show, Closed::Hide].map(ClosedMode));
    all.extend(bools.map(OnOff));
    all.extend(pairs.iter().map(|(open, total)| OpenOfTotal { open: *open as usize, total: *total as usize }));
    all.extend(pairs.iter().map(|(decided, superseded)| MoreClosed {
        decided: *decided as usize,
        superseded: *superseded as usize,
    }));
    all.extend(n.map(|k| CanMove(k as usize)));
    all.extend(n.map(|k| WaitsColumn(k as usize)));
    for closed in n {
        all.extend(lists().into_iter().map(|chains| FoldedClosed { closed: closed as usize, chains }));
    }
    for (round, voted, seats) in &triples {
        for version in maybe(&n.map(|v| v as u32)) {
            all.push(RoundSummary {
                round: *round as u32,
                version,
                voted: *voted as usize,
                seats: *seats as usize,
            });
        }
    }
    all.extend(lists().into_iter().map(Names));
    all.push(WaitsOn(s(), s()));
    for human in bools {
        for proposer in maybe(&[s()]) {
            for updated in maybe(&agos()) {
                all.push(IssueMeta { human, proposer: proposer.clone(), updated });
            }
        }
    }

    // -- config
    all.extend(config::Reader::ALL.map(Msg::Reader));
    all.extend((0..1 << config::Reader::ALL.len()).map(|mask: usize| {
        Readers(config::Reader::ALL.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, r)| *r).collect())
    }));
    all.push(OpenedFrom { branch: s(), sha: s() });
    all.extend(n.map(|k| SectionLines(k as usize)));
    for shown in 0..=7 {
        all.extend(n.map(|removed| ChangedLines { added: vec![N as usize; shown], removed: removed as usize }));
    }
    all.push(SaveTo(s()));
    all.extend(failures().into_iter().map(SaveFailed));
    all.push(Committed(s()));
    all.push(Conflict(s()));
    all.extend(n.map(|k| ReaderCount(k as usize)));
    all.extend(n.map(|k| Errors(k as usize)));
    all.extend(maybe(&[s()]).into_iter().map(|head| BranchAt { branch: s(), head }));
    all.push(SameAs(s()));
    all.extend(lists().into_iter().map(RunningOld));
    all.push(UpToDate(s()));
    all.extend(n.map(|commits| Behind { branch: s(), commits: commits as u32 }));
    all
}

// -- the union -------------------------------------------------------------

/// The data and every message worded in `lang`.
struct Union {
    lang: Lang,
    /// Fixed texts (data, and messages without holes) by their first
    /// character.
    fixed: HashMap<char, Vec<String>>,
    /// Messages with holes.
    templates: Vec<Vec<Part>>,
}

impl Union {
    fn new(lang: Lang, data: &Data) -> Union {
        let mut fixed: BTreeSet<String> = data.0.clone();
        let mut templates: BTreeSet<Vec<Part>> = BTreeSet::new();
        for msg in instances() {
            let parts = template(&msg, lang);
            match parts.as_slice() {
                [] => {}
                [Part::Words(words)] => {
                    fixed.insert(words.clone());
                }
                // A kind the UI does not name passes through: a lone hole.
                [Part::Text] => {}
                _ => {
                    templates.insert(parts);
                }
            }
        }
        let mut by_first: HashMap<char, Vec<String>> = HashMap::new();
        for text in fixed {
            if let Some(first) = text.chars().next() {
                by_first.entry(first).or_default().push(text);
            }
        }
        Union { lang, fixed: by_first, templates: templates.into_iter().collect() }
    }

    /// Whether `text` splits into pieces that are each a fixed text, a run
    /// without letters, or a message with holes whose holes hold fixed texts
    /// and runs; no two pieces meet inside a word.
    fn explains(&self, text: &str) -> bool {
        let text = Text::new(self, text);
        let candidates: Vec<&Vec<Part>> = self
            .templates
            .iter()
            .filter(|parts| {
                parts.iter().all(|part| match part {
                    Part::Words(words) => text.text.contains(words.as_str()),
                    _ => true,
                })
            })
            .collect();
        let len = text.text.len();
        let mut reached = vec![false; len + 1];
        reached[0] = true;
        for at in 0..len {
            if !reached[at] || !text.text.is_char_boundary(at) || !piece_may_start(text.text, at) {
                continue;
            }
            for end in &text.single[at] {
                reached[*end] = true;
            }
            for parts in &candidates {
                let mut ends = Vec::new();
                text.match_parts(parts, at, &mut ends);
                for end in ends {
                    reached[end] = true;
                }
            }
        }
        len == 0 || reached[len]
    }
}

/// One text being explained: where each fixed piece or run from each
/// position ends, and where a hole's content from each position can end.
struct Text<'a> {
    text: &'a str,
    /// Ends of one fixed text or run starting at each position.
    single: Vec<Vec<usize>>,
    /// Ends of hole contents (one or more fixed texts and runs) starting
    /// at each position.
    filled: Vec<BTreeSet<usize>>,
}

impl<'a> Text<'a> {
    fn new(union: &Union, text: &'a str) -> Text<'a> {
        let len = text.len();
        let mut single = vec![Vec::new(); len + 1];
        for (at, first) in text.char_indices() {
            if !first.is_alphabetic() {
                let mut end = at;
                for c in text[at..].chars().take_while(|c| !c.is_alphabetic()) {
                    end += c.len_utf8();
                    single[at].push(end);
                }
            }
            for fixed in union.fixed.get(&first).into_iter().flatten() {
                if text[at..].starts_with(fixed.as_str()) {
                    single[at].push(at + fixed.len());
                }
            }
        }
        let mut filled = vec![BTreeSet::new(); len + 1];
        for start in (0..len).rev().filter(|start| text.is_char_boundary(*start)) {
            let mut ends = BTreeSet::new();
            for end in &single[start] {
                ends.insert(*end);
                if *end < len && piece_may_start(text, *end) {
                    ends.extend(filled[*end].iter().copied());
                }
            }
            filled[start] = ends;
        }
        Text { text, single, filled }
    }

    /// Where `parts` matched from `at` can end.
    fn match_parts(&self, parts: &[Part], at: usize, ends: &mut Vec<usize>) {
        let Some((part, rest)) = parts.split_first() else {
            ends.push(at);
            return;
        };
        let next_words = match rest.first() {
            Some(Part::Words(words)) => Some(words.as_str()),
            _ => Option::None,
        };
        match part {
            Part::Words(words) => {
                if self.text[at..].starts_with(words.as_str()) {
                    self.match_parts(rest, at + words.len(), ends);
                }
            }
            Part::Number => {
                if !self.text[at..].starts_with(|c: char| c.is_ascii_digit()) {
                    return;
                }
                let run: Vec<usize> = self.text[at..]
                    .char_indices()
                    .take_while(|(_, c)| c.is_ascii_digit() || *c == ',' || *c == '.')
                    .filter(|(_, c)| c.is_ascii_digit())
                    .map(|(i, c)| at + i + c.len_utf8())
                    .collect();
                for end in run {
                    self.match_parts(rest, end, ends);
                }
            }
            Part::Text => {
                for end in &self.filled[at] {
                    if next_words.is_none_or(|words| self.text[*end..].starts_with(words)) {
                        self.match_parts(rest, *end, ends);
                    }
                }
            }
            Part::List { separator } => self.match_list(separator, rest, next_words, at, ends),
        }
    }

    /// One list element from `at`, then the rest of the list or of the parts.
    fn match_list(&self, separator: &str, rest: &[Part], next_words: Option<&str>, at: usize, ends: &mut Vec<usize>) {
        for end in &self.filled[at] {
            let tail = &self.text[*end..];
            if next_words.is_none_or(|words| tail.starts_with(words)) {
                self.match_parts(rest, *end, ends);
            }
            if !separator.is_empty() && tail.starts_with(separator) {
                self.match_list(separator, rest, next_words, end + separator.len(), ends);
            }
        }
    }
}

/// Whether a piece may start at `at`: not inside a word, where the
/// characters on both sides are letters or digits.
fn piece_may_start(text: &str, at: usize) -> bool {
    let before = text[..at].chars().next_back();
    let after = text[at..].chars().next();
    !matches!((before, after), (Some(b), Some(a)) if b.is_alphanumeric() && a.is_alphanumeric())
}
