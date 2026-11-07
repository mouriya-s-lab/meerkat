//! #26's acceptance rows 1–2: the real app on the headless engine (see
//! `tests/support/`) in Chinese, its council queue answered from the
//! recording `tests/fixtures/responses/council-queue.json` or from
//! `council-queue-closed.json`, the same recording with one issue
//! (`oh-my-omp/pm-e2e-design#20`) turned `"state":"closed","closed":true`.
//!
//! The recorded queue lists the issues of `oh-my-omp/pm-e2e-design`, which
//! the recorded Project holds too; the tests load a copy of the fixture
//! config whose PM repository is that one, so each issue on the 「议题」
//! page shows the queue's verdict.
//!
//! The tests rewrite this process's `OMOMP_LANG`, `GH_TOKEN` and
//! `PM_CONFIG_DIR`: they take [`serial`]'s lock, so they run one at a time
//! (the issue's commands add `--test-threads=1`).

mod support;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use nsh_testing::StubRoute;
use quorum::{
    Lang, Msg, QueueMotion,
    dag::{Closed, Group},
    programs::QueueState,
};
use support::{Fixture, Sandbox};

/// How long the window may take to show what an answer holds.
const SHOWN: Duration = Duration::from_secs(10);
/// The council's queue, as recorded.
const RECORDED: &str = include_str!("fixtures/responses/council-queue.json");
/// The recording with one issue closed undecided.
const CLOSED: &str = include_str!("fixtures/responses/council-queue-closed.json");
/// The recorded Project.
const PROJECT: &str = include_str!("fixtures/responses/github-project.json");
/// The stub network's queue request (`targets.*.council` in the fixture
/// config).
const QUEUE: &str = "GET http://council.test/v1/queue";
/// The repository the recorded queue lists.
const DESIGN: &str = "oh-my-omp/pm-e2e-design";

/// Holds one test at a time: each rewrites the process's environment.
fn serial() -> MutexGuard<'static, ()> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn closed_is_settled() -> Result<(), String> {
    let _serial = serial();
    let queue = entries(CLOSED);
    let closed: Vec<&Entry> = queue.iter().filter(|e| e.state == "closed").collect();
    let [closed] = closed[..] else {
        return Err(format!("the answer closes {} issues, not one", closed.len()));
    };
    // The verdicts as `src/dag.rs` groups them now: closed with the
    // decided and superseded issues.
    let expected = Expected::of(&queue, |state| match state {
        "closed" => Group::Closed,
        other => before(other),
    });
    assert!(expected.groups[&Group::Closed].contains(&closed.title));

    let sandbox = Sandbox::new("queue-state-closed");
    let mut quorum = start(&sandbox, CLOSED);

    // 概览: the queue parses (the status sentence says how it moves), and
    // the in-review and unsettled counts are the expected ones, which leave
    // the closed issue out; the queue panel lists the unsettled rest as it
    // does.
    overview(&mut quorum, &expected)?;

    // 议题, the list: the closed issue in the settled group, saying 已关闭.
    let list = issues_list(&mut quorum, &expected)?;
    let word = quorum.say(&Msg::Queue(QueueState::Closed));
    assert_eq!(word, "已关闭");
    assert_eq!(
        word_after(&quorum, &list, &closed.title),
        Some(word.clone()),
        "the list's row of {} does not say {word}",
        closed.reference
    );

    // 议题, the graph, the closed issue selected from the list: the
    // inspector names it 已关闭; folded, the fold counts it with the other
    // settled issues and no column holds it.
    quorum.press(&format!("label:{}", closed.title))?;
    let graph = graph_shown(&mut quorum)?;
    assert!(
        graph.windows(3).any(|w| w[0] == word && w[2] == closed.title),
        "the graph's inspector does not name {} {word}: {graph:?}",
        closed.reference
    );
    graph_folded(&mut quorum, &expected)?;

    // 运行状况: the council's work counts it neither.
    runtime(&mut quorum, &expected)
}

#[test]
fn eight_states_unchanged() -> Result<(), String> {
    let _serial = serial();
    let queue = entries(RECORDED);
    assert!(
        queue.iter().all(|e| EIGHT.contains(&e.state.as_str())),
        "the recording holds a state the queue had not before #26"
    );
    let expected = Expected::of(&queue, before);

    let sandbox = Sandbox::new("queue-state-eight");
    let mut quorum = start(&sandbox, RECORDED);

    let mut screens = Vec::new();
    overview(&mut quorum, &expected)?;
    assert_eq!(quorum.say(&Msg::OpenCount(expected.unsettled)), format!("{} 个未了结", expected.unsettled));
    screens.push(quorum.texts());
    let list = issues_list(&mut quorum, &expected)?;
    assert_eq!(quorum.say(&Msg::GroupName(Group::Closed)), "已了结");
    screens.push(list);
    screens.push(graph_shown(&mut quorum)?);
    graph_folded(&mut quorum, &expected)?;
    screens.push(quorum.texts());
    runtime(&mut quorum, &expected)?;
    screens.push(quorum.texts());

    // No group name, count or empty line says 已定案 or 未定案 any more; the
    // issues' titles are data and may.
    let titles = project_titles();
    for texts in &screens {
        for (i, text) in texts.iter().enumerate() {
            if titles.contains(text) {
                continue;
            }
            assert!(!text.contains("未定案"), "{text:?} says 未定案");
            assert!(!text.contains("已定案议题"), "{text:?} folds 已定案 issues");
            let header = texts.get(i + 1).is_some_and(|next| next.parse::<usize>().is_ok());
            assert!(!(text == "已定案" && header), "a group is named 已定案");
        }
    }
    Ok(())
}

// -- the expectations --------------------------------------------------------

/// The queue's eight states before #26.
const EIGHT: [&str; 8] = [
    "decided",
    "superseded",
    "blocked",
    "held",
    "deliberating",
    "interrupted",
    "failed",
    "ready",
];

/// A queue state's group as `src/dag.rs` mapped it before #26.
fn before(state: &str) -> Group {
    match state {
        "failed" | "interrupted" => Group::Attention,
        "deliberating" => Group::Deliberating,
        "ready" => Group::Ready,
        "blocked" | "held" => Group::Waiting,
        "decided" | "superseded" => Group::Closed,
        other => panic!("{other:?} was not a queue state before #26"),
    }
}

/// A row of the queue answer, with its issue's title in the Project.
struct Entry {
    reference: String,
    state: String,
    title: String,
}

/// The rows of a queue answer.
fn entries(json: &str) -> Vec<Entry> {
    let titles = titles_by_issue();
    let queue: serde_json::Value = serde_json::from_str(json).expect("a queue answer");
    queue
        .as_array()
        .expect("an array of rows")
        .iter()
        .map(|row| {
            let reference = row["ref"].as_str().expect("a row's ref").to_owned();
            let title = titles
                .get(&reference)
                .unwrap_or_else(|| panic!("the Project holds no {reference}"))
                .clone();
            Entry {
                reference,
                state: row["state"].as_str().expect("a row's state").to_owned(),
                title,
            }
        })
        .collect()
}

/// What the pages should show for a queue answer, its rows grouped by
/// `group`.
struct Expected {
    /// Each group's titles.
    groups: HashMap<Group, Vec<String>>,
    /// Rows the overview counts as unsettled.
    unsettled: usize,
    /// Rows deliberating.
    deliberating: usize,
}

impl Expected {
    fn of(queue: &[Entry], group: impl Fn(&str) -> Group) -> Expected {
        let mut groups: HashMap<Group, Vec<String>> = Group::ALL.into_iter().map(|g| (g, Vec::new())).collect();
        for entry in queue {
            groups.get_mut(&group(&entry.state)).expect("a group").push(entry.title.clone());
        }
        Expected {
            unsettled: queue.len() - groups[&Group::Closed].len(),
            deliberating: queue.iter().filter(|e| e.state == "deliberating").count(),
            groups,
        }
    }

    fn settled(&self) -> usize {
        self.groups[&Group::Closed].len()
    }
}

/// The Project's issues by `owner/name#number`, their titles.
fn titles_by_issue() -> HashMap<String, String> {
    project_issues()
        .iter()
        .filter_map(|content| {
            let repo = content["repository"]["nameWithOwner"].as_str()?;
            let number = content["number"].as_u64()?;
            Some((format!("{repo}#{number}"), content["title"].as_str()?.to_owned()))
        })
        .collect()
}

/// Every title in the Project.
fn project_titles() -> HashSet<String> {
    project_issues()
        .iter()
        .filter_map(|content| Some(content["title"].as_str()?.to_owned()))
        .collect()
}

/// The contents of the recorded Project's items.
fn project_issues() -> Vec<serde_json::Value> {
    let mut project: serde_json::Value = serde_json::from_str(PROJECT).expect("github-project.json");
    let nodes = project["data"]["repositoryOwner"]["projectV2"]["items"]["nodes"].take();
    let serde_json::Value::Array(nodes) = nodes else {
        panic!("the Project's items are not a list");
    };
    nodes.into_iter().map(|mut node| node["content"].take()).collect()
}

// -- the app -----------------------------------------------------------------

/// Quorum in Chinese on a copy of the fixture config whose PM repository is
/// the one the queue lists, its queue answered with `queue`.
fn start(sandbox: &Sandbox, queue: &str) -> Fixture {
    support::set_github_token("gho_q26_queue_state_token");
    let dir = sandbox.dir().join("config");
    copy_dir(&support::config_dir(), &dir);
    let repos = dir.join("shared/repos.yaml");
    let text = std::fs::read_to_string(&repos).expect("repos.yaml");
    let pm = "pm:\n  repo: oh-my-omp/pm-e2e-agenda\n";
    assert!(text.contains(pm), "the fixture's repos.yaml names another PM repository");
    std::fs::write(&repos, text.replace(pm, &format!("pm:\n  repo: {DESIGN}\n"))).expect("repos.yaml written");
    let answer = StubRoute::Response(support::answer(200, queue));
    Fixture::start_in(Lang::Zh, &dir, vec![(QUEUE.to_owned(), answer)])
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("a directory");
    for entry in std::fs::read_dir(from).expect("a fixture directory") {
        let path: PathBuf = entry.expect("an entry").path();
        let target = to.join(path.file_name().expect("a name"));
        if path.is_dir() {
            copy_dir(&path, &target);
        } else {
            std::fs::copy(&path, &target).expect("a copied file");
        }
    }
}

/// 概览: the queue answered and parsed, its counts, and the queue panel's
/// rows (the unsettled issues) or its empty line.
fn overview(quorum: &mut Fixture, expected: &Expected) -> Result<(), String> {
    let idle = quorum.say(&Msg::ProgramsLine {
        council: true,
        guardians: true,
        queue: Some(if expected.deliberating > 0 { QueueMotion::Moving } else { QueueMotion::Idle }),
    });
    quorum.wait_for_containing(&idle, SHOWN)?;
    quorum.wait_for(&quorum.say(&Msg::OpenCount(expected.unsettled)), SHOWN)?;
    let deliberating = format!(" · {}", quorum.say(&Msg::Deliberating(Some(expected.deliberating))));
    quorum.wait_for_matching(&format!("{}$", escape(&deliberating)), SHOWN)?;
    let unsettled: Vec<String> = Group::ALL
        .into_iter()
        .filter(|g| *g != Group::Closed)
        .flat_map(|g| expected.groups[&g].clone())
        .collect();
    if unsettled.is_empty() {
        quorum.wait_for(&quorum.say(&Msg::QueueEmpty), SHOWN)?;
    } else {
        quorum.wait_for_all(&unsettled, SHOWN)?;
    }
    for settled in &expected.groups[&Group::Closed] {
        assert!(!quorum.shows(settled), "the queue panel lists the settled {settled:?}");
    }
    assert!(!quorum.shows(&quorum.say(&Msg::Retry)), "a panel shows an error");
    Ok(())
}

/// 议题 as a list: the filter bar's 「N 个未了结 · 共 M」, then each group's
/// header, count and rows as `expected` says, empty groups absent. Returns
/// the list's texts: the rows draw before the column heads, which stick on
/// top of them.
fn issues_list(quorum: &mut Fixture, expected: &Expected) -> Result<Vec<String>, String> {
    quorum.open(Msg::PageIssues)?;
    quorum.press(&format!("label:{}", quorum.say(&Msg::ViewList)))?;
    let titles: Vec<String> = expected.groups.values().flatten().cloned().collect();
    let bounds = List {
        note: quorum.say(&Msg::OpenOfTotal { open: expected.unsettled, total: titles.len() }),
        heads: [quorum.say(&Msg::Agenda), quorum.say(&Msg::Title)],
    };
    assert_eq!(bounds.note, format!("{} 个未了结 · 共 {}", expected.unsettled, titles.len()));
    {
        let bounds = bounds.clone();
        quorum.wait_until("every issue in the list", SHOWN, move |texts| {
            bounds.of(texts).is_some_and(|(a, b)| titles.iter().all(|t| texts[a..b].contains(&t.as_str())))
        })?;
    }
    let screen = quorum.texts();
    let (a, b) = bounds.of(&screen.iter().map(String::as_str).collect::<Vec<_>>()).ok_or("the list went")?;
    let texts = screen[a..b].to_vec();
    let headers: Vec<(Group, usize)> = Group::ALL
        .into_iter()
        .filter_map(|g| {
            let name = quorum.say(&Msg::GroupName(g));
            texts.iter().position(|t| *t == name).map(|i| (g, i))
        })
        .collect();
    for group in Group::ALL {
        let members = &expected.groups[&group];
        let name = quorum.say(&Msg::GroupName(group));
        let Some(&(_, at)) = headers.iter().find(|(g, _)| *g == group) else {
            assert!(members.is_empty(), "the list has no group {name} for {members:?}: {texts:?}");
            continue;
        };
        assert!(!members.is_empty(), "the list shows the empty group {name}");
        assert_eq!(texts.get(at + 1), Some(&members.len().to_string()), "the count of {name}");
        let end = headers.iter().map(|(_, h)| *h).filter(|h| *h > at).min().unwrap_or(texts.len());
        for member in members {
            let row = texts.iter().position(|t| t == member);
            assert!(
                row.is_some_and(|row| at < row && row < end),
                "{member:?} is not under {name}"
            );
        }
    }
    Ok(texts)
}

/// Where the list stands among the window's texts: after the filter bar's
/// note, before the column heads 「议程」「标题」.
#[derive(Clone)]
struct List {
    note: String,
    heads: [String; 2],
}

impl List {
    fn of(&self, texts: &[&str]) -> Option<(usize, usize)> {
        let start = texts.iter().position(|t| *t == self.note)? + 1;
        let end = start + texts[start..].windows(2).position(|w| w[0] == self.heads[0] && w[1] == self.heads[1])?;
        Some((start, end))
    }
}

/// The word a list row shows after its title (and its 「人类」 tag).
fn word_after(quorum: &Fixture, texts: &[String], title: &str) -> Option<String> {
    let human = quorum.say(&Msg::Human);
    let at = texts.iter().position(|t| t == title)?;
    texts[at + 1..].iter().find(|t| **t != human).cloned()
}

/// 「可推进 · 」, the head of the graph's first column without its count.
fn can_move(quorum: &Fixture) -> String {
    let label = quorum.say(&Msg::CanMove(0));
    label.strip_suffix('0').expect("a count last").to_owned()
}

/// 议题 as a graph, the settled issues shown in its columns (the fixture
/// config's `hideDecided: false`). Returns the page's texts.
fn graph_shown(quorum: &mut Fixture) -> Result<Vec<String>, String> {
    quorum.press(&format!("label:{}", quorum.say(&Msg::ViewGraph)))?;
    let heads = [quorum.say(&Msg::Agenda), quorum.say(&Msg::Title)];
    let fit = quorum.say(&Msg::Fit);
    let column = can_move(quorum);
    quorum.wait_until("the graph's columns", SHOWN, move |texts| {
        !texts.windows(2).any(|w| w[0] == heads[0] && w[1] == heads[1])
            && texts.contains(&fit.as_str())
            && texts.iter().any(|t| t.starts_with(&column))
    })?;
    Ok(quorum.texts())
}

/// The graph with its 「已了结」 token stepped from show through hide to
/// fold: the fold's hint counts every settled issue, the columns hold
/// the unsettled ones.
fn graph_folded(quorum: &mut Fixture, expected: &Expected) -> Result<(), String> {
    let token = format!("label:{}", quorum.say(&Msg::FilterClosed));
    assert_eq!(quorum.say(&Msg::FilterClosed), "已了结");
    quorum.press(&token)?;
    quorum.wait_for(&quorum.say(&Msg::ClosedMode(Closed::Hide)), SHOWN)?;
    quorum.press(&token)?;
    quorum.wait_for(&quorum.say(&Msg::ClosedMode(Closed::Fold)), SHOWN)?;
    let fold = quorum.say(&Msg::FoldedClosed { closed: expected.settled(), chains: Vec::new() });
    assert_eq!(fold, format!("{} 个已了结议题已折叠", expected.settled()));
    quorum.wait_for_containing(&fold, SHOWN)?;
    let heads = [can_move(quorum), {
        let label = quorum.say(&Msg::WaitsColumn(0));
        label.strip_suffix('0').expect("a count last").to_owned()
    }];
    let placed: usize = quorum
        .texts()
        .iter()
        .filter_map(|t| heads.iter().find_map(|h| t.strip_prefix(h.as_str())))
        .map(|n| n.parse::<usize>().expect("a column's count"))
        .sum();
    assert_eq!(placed, expected.unsettled, "the folded graph's columns");
    Ok(())
}

/// 运行状况: the council's line counts the unsettled and deliberating rows.
fn runtime(quorum: &mut Fixture, expected: &Expected) -> Result<(), String> {
    quorum.open(Msg::PageRuntime)?;
    let work = quorum.say(&Msg::QueueWork {
        open: expected.unsettled,
        deliberating: expected.deliberating,
    });
    quorum.wait_for_containing(&work, SHOWN)
}

/// `text` as a regular expression that matches it alone.
fn escape(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            let special = "\\.+*?()|[]{}^$#&-~".contains(c);
            special.then_some('\\').into_iter().chain([c])
        })
        .collect()
}
