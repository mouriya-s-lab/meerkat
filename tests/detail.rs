//! #4's acceptance rows 1–5: the real app on the headless engine (see
//! `tests/support/`) in Chinese, answered from the recordings under
//! `tests/fixtures/responses/` and `tests/fixtures/detail/`.
//!
//! What a page should show is read here from the recordings themselves,
//! by the markers' contract (`Pepgent/pm-new#1` decision 6696709150 §A,
//! §B; decision 6695362696 §2), not from the app's derivations: the
//! Project's items, the council's queue and sessions, and each comment's
//! last line `<!-- pm:<kind> {json} -->`.
//!
//! The tests rewrite this process's `OMOMP_LANG`, `GH_TOKEN` and
//! `PM_CONFIG_DIR`: each takes [`serial`]'s lock, so they run one at a
//! time whatever `--test-threads` says.

mod support;

use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use nsh_testing::StubRoute;
use quorum::{
    Lang, Msg,
    detail::{ImplGroup, Stance},
    gh::markers::Outcome,
    programs::QueueState,
    time::{self, Ago},
};
use serde_json::Value;
use support::{Fixture, Sandbox};

/// How long the window may take to show what an answer holds.
const SHOWN: Duration = Duration::from_secs(10);
/// The PM repository of G25 and of the recorded council queue.
const DESIGN: &str = "oh-my-omp/pm-e2e-design";
/// The PM repository of the human issue that changed an old one, and of
/// the agenda issues implementation asks became.
const AGENDA: &str = "oh-my-omp/pm-e2e-agenda";
/// G25: a panel with a revision round, a veto, a counterexample and a
/// ruling.
const G25: &str = "oh-my-omp/pm-e2e-design#23";
/// The stub network's queue request.
const QUEUE: &str = "GET http://council.test/v1/queue";

/// Holds one test at a time: each rewrites the process's environment.
fn serial() -> MutexGuard<'static, ()> {
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    ONE_AT_A_TIME.lock().unwrap_or_else(PoisonError::into_inner)
}

// -- row 1 -------------------------------------------------------------------

#[test]
fn agenda_with_revision_and_ruling() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("detail-revision-ruling");
    let mut quorum = support::start_detail(&sandbox, DESIGN, Lang::Zh, Vec::new());
    let item = item(G25);
    let comments = comments(G25);
    let versions = versions(&comments);
    open_from_list(&mut quorum, &item.title)?;

    // The header: status, agenda number, kind and drafting seat, when it
    // was opened, the title — as the Project item (and the council's
    // verdict on it) has them.
    let g25 = header(&quorum, DESIGN, &item, Some(queue_state(&recorded_queue(), G25)));
    wait_for_run(&mut quorum, &g25)?;

    // The round track, step by step: the matrix's glyphs are the version's
    // `pm:vote` rows, the veto ⊘ and the counterexample ?; a ruled row has
    // its category; the decided version has the rule in its head.
    let decision = marker_of(&comments, "decision").expect("G25's pm:decision");
    let threshold = decision["rows"][0]["threshold"].as_u64().expect("a threshold") as u32;
    let rule = quorum.say(&Msg::Rule(threshold));
    let steps = steps(&versions);
    assert_eq!(steps.len(), 3, "G25's track: draft v1, revision v1 → v2, draft v2");
    let mut saw = (false, false);
    for (title, shown) in &steps {
        // A press's re-render may come after the press has settled: the
        // step is chosen once the track, matrix and results show it.
        let step = format!("label:{}", quorum.say(title));
        quorum.press(&step)?;
        quorum.wait_for_selected(&step, SHOWN)?;
        let version = versions.iter().find(|v| v.number == *shown).expect("the step's version");
        let cells = cells(version);
        for cell in &cells {
            let label = cell.label(&quorum);
            assert_eq!(quorum.count(&format!("label:{label}"))?, 1, "the matrix has no cell {label}");
            assert_eq!(quorum.count(&cell.selector(&quorum))?, 1, "the cell {label} is not {}", glyph(cell.stance));
            saw.0 |= cell.stance == Stance::Veto;
            saw.1 |= cell.stance == Stance::Counterexample;
        }
        for (row, category) in &version.ruling {
            let ruled = quorum.say(&Msg::Ruled(category.clone()));
            assert!(quorum.shows(&ruled), "v{shown}'s row {row} has no 「{ruled}」");
        }
        if version.ruling.is_empty() {
            let ruled = quorum.say(&Msg::Ruled(String::new()));
            assert!(
                !quorum.texts().iter().any(|t| t.starts_with(ruled.trim_end())),
                "v{shown} has no ruling, yet a row shows one"
            );
        }
        let decided = decision["version"].as_u64() == Some(u64::from(*shown));
        assert_eq!(quorum.shows(&rule), decided, "the rule 「{rule}」 in v{shown}'s head");
    }
    assert_eq!(saw, (true, true), "the recording has a veto and a counterexample");

    // The decision: each row's outcome with its approvals and threshold,
    // and its reason; the implementation issue it created.
    let mut outcomes: BTreeMap<String, usize> = BTreeMap::new();
    for row in decision["rows"].as_array().expect("the decision's rows") {
        let outcome: Outcome = serde_json::from_value(row["outcome"].clone()).expect("an outcome");
        let line = quorum.say(&Msg::OutcomeCount {
            outcome,
            approve: row["approve"].as_u64().expect("approvals") as u32,
            threshold: row["threshold"].as_u64().expect("a threshold") as u32,
        });
        *outcomes.entry(line).or_default() += 1;
        let reason = row["reason"].as_str().expect("a reason");
        assert!(quorum.shows(reason), "the decision's reason {reason:?} is not shown");
    }
    for (line, n) in &outcomes {
        assert_eq!(quorum.count(&format!("text:{line}"))?, *n, "rows that read 「{line}」");
    }
    for created in decision["impl"].as_array().expect("the decision's impl") {
        let created = created.as_str().expect("an issue");
        assert_eq!(quorum.count(&format!("label:{created}"))?, 1, "the decision's implementation {created}");
    }

    // A veto cell on the version it was cast on: its reason pops over —
    // the text above the vote's marker, and the row's counterexample,
    // citation and classification as the marker has them.
    let (version, vote, row) = versions
        .iter()
        .find_map(|version| {
            version.votes.iter().find_map(|vote| {
                let row = vote.rows.iter().find(|r| stance(Some(r)) == Stance::Veto)?;
                Some((version, vote, row))
            })
        })
        .expect("a veto in the recording");
    let draft = steps
        .iter()
        .find(|(title, _)| matches!(title, Msg::StepDraft { version: v, .. } if *v == version.number))
        .expect("the veto's version's step");
    let step = format!("label:{}", quorum.say(&draft.0));
    quorum.press(&step)?;
    quorum.wait_for_selected(&step, SHOWN)?;
    let cell = Cell {
        seat: vote.seat.clone(),
        row: row["row"].as_str().expect("a row").to_owned(),
        stance: Stance::Veto,
    };
    quorum.press(&format!("label:{}", cell.label(&quorum)))?;
    let reason = prose(&vote.body);
    assert!(!reason.is_empty(), "the veto's comment has text above its marker");
    quorum.wait_for(&reason, SHOWN)?;
    for (field, msg) in [
        ("counterexample", Msg::Counterexample),
        ("citation", Msg::Citation),
        ("classification", Msg::Classification),
    ] {
        match row[field].as_str() {
            Some(value) => {
                assert!(quorum.shows(value), "the veto's {field} {value:?} is not in its reason");
                assert!(quorum.shows(&quorum.say(&msg)), "the reason does not name its {field}");
            }
            None => assert!(row[field].is_null(), "the veto's {field} is neither text nor null"),
        }
    }

    // A counterexample cell (oppose with a recorded counterexample) on
    // the version it was cast on: pressing it closes the veto's reason and
    // pops this row's own over — the text above the vote's marker, and the
    // counterexample, citation and classification as the marker has them.
    let (version, vote, row) = versions
        .iter()
        .find_map(|version| {
            version.votes.iter().find_map(|vote| {
                let row = vote.rows.iter().find(|r| stance(Some(r)) == Stance::Counterexample)?;
                Some((version, vote, row))
            })
        })
        .expect("a counterexample in the recording");
    let counterexample = row["counterexample"].as_str().expect("the counterexample");
    let draft = steps
        .iter()
        .find(|(title, _)| matches!(title, Msg::StepDraft { version: v, .. } if *v == version.number))
        .expect("the counterexample's version's step");
    let step = format!("label:{}", quorum.say(&draft.0));
    quorum.press(&step)?;
    quorum.wait_for_selected(&step, SHOWN)?;
    let cell = Cell {
        seat: vote.seat.clone(),
        row: row["row"].as_str().expect("a row").to_owned(),
        stance: Stance::Counterexample,
    };
    quorum.press(&format!("label:{}", cell.label(&quorum)))?;
    let reason = prose(&vote.body);
    assert!(!reason.is_empty(), "the counterexample's comment has text above its marker");
    quorum.wait_for(counterexample, SHOWN)?;
    assert!(quorum.shows(&reason), "the counterexample's reason {reason:?} is not in its popup");
    for (field, msg) in [
        ("citation", Msg::Citation),
        ("classification", Msg::Classification),
    ] {
        match row[field].as_str() {
            Some(value) => {
                assert!(quorum.shows(value), "the counterexample's {field} {value:?} is not in its reason");
                assert!(quorum.shows(&quorum.say(&msg)), "the reason does not name its {field}");
            }
            None => assert!(row[field].is_null(), "the counterexample's {field} is neither text nor null"),
        }
    }
    assert!(
        quorum.shows(&quorum.say(&Msg::Counterexample)),
        "the reason does not name its counterexample"
    );
    Ok(())
}

// -- row 2 -------------------------------------------------------------------

#[test]
fn human_agenda_supersedes() -> Result<(), String> {
    let _serial = serial();
    let queue = support::detail("agenda-queue.json");
    let sandbox = Sandbox::new("detail-supersedes");
    let routes = vec![(QUEUE.to_owned(), StubRoute::Response(support::answer(200, &queue)))];
    let mut quorum = support::start_detail(&sandbox, AGENDA, Lang::Zh, routes);

    // The human issue whose decision changed an old one.
    let (human, superseded) = items()
        .into_iter()
        .filter(|item| item.issue.starts_with(AGENDA) && item.field("Kind") == Some("Human"))
        .find_map(|item| {
            let comments = support::recorded_comments(&item.issue)?;
            let decision = marker_of(&parse_comments(&comments), "decision")?;
            let old = decision["superseded"].as_array()?.first()?.as_str()?.to_owned();
            Some((item, old))
        })
        .expect("a recorded human issue whose decision supersedes an old one");
    let old = item(&superseded);
    open_from_list(&mut quorum, &human.title)?;

    // Its detail lists the old issue it changed; a press opens that one.
    let changed = quorum.say(&Msg::Supersedes);
    quorum.wait_for(&changed, SHOWN)?;
    let link = format!("label:{}", old.title);
    quorum.wait_for_view(&link, SHOWN)?;
    quorum.press(&link)?;

    // The old issue's detail says 已取代, as the council's verdict on it.
    let state = queue_state(&queue, &superseded);
    assert_eq!(quorum.say(&Msg::Queue(state)), "已取代");
    let old_header = header(&quorum, AGENDA, &old, Some(state));
    wait_for_run(&mut quorum, &old_header)?;
    // ... and names the human issue that changed it.
    quorum.wait_for(&quorum.say(&Msg::SupersededBy), SHOWN)?;
    assert_eq!(quorum.count(&format!("label:{}", human.title))?, 1, "the old issue's detail names {}", human.issue);
    Ok(())
}

// -- row 3 -------------------------------------------------------------------

#[test]
fn sessions_by_round() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("detail-sessions");
    let mut quorum = support::start_detail(&sandbox, DESIGN, Lang::Zh, Vec::new());
    let item = item(G25);
    let comments = comments(G25);
    open_from_list(&mut quorum, &item.title)?;

    // The council's recorded sessions whose `ref` is G25, by the round
    // each started in: the last round whose panel started at or before it.
    let starts = round_starts(&comments);
    let sessions: Vec<Session> = council_sessions()
        .into_iter()
        .filter(|s| s.reference == G25)
        .collect();
    assert!(!sessions.is_empty(), "the recording has sessions on G25");
    let mut by_round: BTreeMap<u32, Vec<&Session>> = BTreeMap::new();
    for session in &sessions {
        let at = time::parse_ms(&session.started_at).expect("a start");
        let round = starts.iter().rev().find(|(_, start)| *start <= at).map_or(1, |(round, _)| *round);
        by_round.entry(round).or_default().push(session);
    }

    let head = quorum.say(&Msg::CouncilSessions);
    let end = quorum.say(&Msg::Relations);
    {
        let expected = sessions.len().to_string();
        let (head, end) = (head.clone(), end.clone());
        quorum.wait_until("G25's sessions panel", SHOWN, move |texts| {
            panel_of(texts, &head, &end).is_some_and(|panel| panel.first().copied() == Some(expected.as_str()))
        })?;
    }
    let texts = quorum.texts();
    let panel = panel_of(&texts.iter().map(String::as_str).collect::<Vec<_>>(), &head, &end).expect("the panel");

    // Each round's header, then its sessions — the same ones the recording
    // has in that round, one line each, each kind's together.
    let headers: Vec<(u32, usize)> = by_round
        .keys()
        .map(|round| {
            let name = quorum.say(&Msg::RoundNumber(*round));
            let at = panel.iter().position(|t| *t == name).unwrap_or_else(|| panic!("no 「{name}」 in {panel:?}"));
            (*round, at)
        })
        .collect();
    for (round, at) in &headers {
        let until = headers.iter().map(|(_, h)| *h).filter(|h| h > at).min().unwrap_or(panel.len());
        let shown = &panel[at + 1..until];
        let expected = &by_round[round];
        let titles: Vec<String> = expected.iter().map(|s| s.title(&quorum)).collect();
        let lines: Vec<&str> = shown.iter().copied().filter(|t| titles.iter().any(|title| title == t)).collect();
        assert_eq!(lines.len(), expected.len(), "round {round}'s sessions: {shown:?}");
        for title in &titles {
            let want = titles.iter().filter(|t| *t == title).count();
            let got = lines.iter().filter(|t| **t == title.as_str()).count();
            assert_eq!(got, want, "round {round} shows 「{title}」 {got} times");
        }
        let kinds: Vec<&str> = lines
            .iter()
            .map(|line| expected.iter().find(|s| s.title(&quorum) == *line).expect("a session").kind.as_str())
            .collect();
        for (i, kind) in kinds.iter().enumerate() {
            assert!(
                !kinds[..i].contains(kind) || kinds[i - 1] == *kind,
                "round {round}'s {kind} sessions are not together: {kinds:?}"
            );
        }
    }

    // The vote session whose log was recorded, expanded in place: its
    // decision log is what 「会话」 shows for the same session.
    let logged = sessions
        .iter()
        .find(|s| s.id == support::VOTE_LOG_SESSION)
        .expect("the logged session is G25's");
    assert_eq!(logged.kind, "vote");
    let round = by_round.iter().find(|(_, s)| s.iter().any(|s| s.id == logged.id)).map(|(r, _)| *r).expect("its round");
    let title = logged.title(&quorum);
    // Rounds show newest first: the same line of later rounds comes before.
    let before = by_round
        .range(round + 1..)
        .flat_map(|(_, s)| s)
        .filter(|s| s.title(&quorum) == title)
        .count();
    assert_eq!(by_round[&round].iter().filter(|s| s.title(&quorum) == title).count(), 1);
    let collapsed = panel.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
    quorum.press(&format!("label:{title} >> nth={before}"))?;
    let open = quorum.say(&Msg::OpenSession);
    let parsing = quorum.say(&Msg::Parsing);
    {
        let (open, parsing) = (open.clone(), parsing.clone());
        quorum.wait_until("the session's log in place", SHOWN, move |texts| {
            texts.contains(&open.as_str()) && !texts.contains(&parsing.as_str())
        })?;
    }
    let texts = quorum.texts();
    let expanded: Vec<String> = panel_of(&texts.iter().map(String::as_str).collect::<Vec<_>>(), &head, &end)
        .expect("the panel")
        .iter()
        .map(|t| (*t).to_owned())
        .collect();
    let mut log = inserted(&collapsed, &expanded);
    assert_eq!(log.pop().as_deref(), Some(open.as_str()), "the log ends with 「{open}」");
    assert!(log.len() > 1, "the expanded log is empty: {log:?}");
    // The log is the recorded one: its entries' clock times are among it.
    let clocks: Vec<String> = support::detail("design-23-vote-log.jsonl")
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|entry| entry["timestamp"].as_str().and_then(time::parse_ms).map(time::clock))
        .collect();
    assert!(log.iter().any(|t| clocks.contains(t)), "the expanded log is not the recorded one: {log:?}");

    quorum.press(&format!("label:{open}"))?;
    let want = log.clone();
    quorum.wait_until("the same log on 「会话」", SHOWN, move |texts| {
        texts.windows(want.len()).any(|w| w.iter().zip(&want).all(|(a, b)| *a == b))
    })?;
    assert!(quorum.shows(&logged.id), "「会话」 does not show the session {}", logged.id);
    Ok(())
}

// -- row 4 -------------------------------------------------------------------

#[test]
fn implementation_timelines() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("detail-implementations");
    let mut quorum = support::start_detail(&sandbox, AGENDA, Lang::Zh, Vec::new());
    let started = time::now_ms();
    let implementations: Vec<Item> = items()
        .into_iter()
        .filter(|item| item.field("Kind") == Some("Implementation"))
        .collect();
    let mut groups: BTreeMap<ImplGroup, Vec<&Item>> = BTreeMap::new();
    for item in &implementations {
        groups.entry(expected_group(item)).or_default().push(item);
    }
    assert_eq!(groups.len(), 4, "the recordings fill every group: {groups:?}");
    let titles: Vec<&str> = implementations.iter().map(|i| i.title.as_str()).collect();

    // 「实现」: the groups in order 等答复, 进行中, 待办, 完成, each with its
    // count and its issues; 完成 unfolds on a press.
    quorum.open(Msg::PageImplementations)?;
    let done = quorum.say(&Msg::ImplGroup(ImplGroup::Done));
    quorum.wait_for_view(&format!("label:{done}"), SHOWN)?;
    quorum.press(&format!("label:{done}"))?;
    {
        let titles: Vec<String> = titles.iter().map(|t| (*t).to_owned()).collect();
        quorum.wait_until("every implementation issue", SHOWN, move |texts| {
            titles.iter().all(|t| texts.contains(&t.as_str()))
        })?;
    }
    let texts = quorum.texts();
    let headers: Vec<(ImplGroup, usize)> = ImplGroup::ALL
        .into_iter()
        .map(|group| {
            let name = quorum.say(&Msg::ImplGroup(group));
            let count = groups[&group].len().to_string();
            let at = texts
                .windows(2)
                .position(|w| w[0] == name && w[1] == count)
                .unwrap_or_else(|| panic!("no group 「{name}」 of {count}: {texts:?}"));
            (group, at)
        })
        .collect();
    assert!(headers.windows(2).all(|w| w[0].1 < w[1].1), "the groups are out of order: {headers:?}");
    // Each row names its issue first (「pm-e2e-code#N · 来自 …」): titles
    // repeat in the recording, issue numbers do not.
    for (group, at) in &headers {
        let until = headers.iter().map(|(_, h)| *h).filter(|h| h > at).min().unwrap_or(texts.len());
        for item in &groups[group] {
            let issue = short(&item.issue);
            let names = |t: &String| *t == issue || t.starts_with(&format!("{issue} · "));
            let rows: Vec<usize> = texts.iter().enumerate().filter(|(_, t)| names(t)).map(|(i, _)| i).collect();
            assert!(
                matches!(rows[..], [row] if *at < row && row < until),
                "{} is not once under {group:?}: {rows:?}",
                item.issue
            );
        }
    }

    // Three timelines: resumed after a reply, a reply from the council
    // after an escalation, and a replaced ask. Every settlement is in its
    // ask's unit.
    let resumed = implementations
        .iter()
        .find(|item| {
            let markers = markers(&comments(&item.issue));
            markers.iter().filter(|(k, _)| k == "impl").count() > 1 && markers.iter().any(|(k, _)| k == "reply")
        })
        .expect("an issue resumed after a reply");
    let council = implementations
        .iter()
        .find(|item| {
            let markers = markers(&comments(&item.issue));
            markers.iter().any(|(k, m)| k == "reply" && m["decision"].is_string())
                && markers.iter().any(|(k, _)| k == "escalated")
        })
        .expect("an issue the council replied to after an escalation");
    let replaced = implementations
        .iter()
        .find(|item| markers(&comments(&item.issue)).iter().any(|(k, _)| k == "replaced"))
        .expect("an issue whose ask was replaced");
    let waiting = groups[&ImplGroup::Waiting][0];
    for item in [resumed, council, replaced, waiting] {
        let same = titles.iter().filter(|t| **t == item.title).count();
        assert_eq!(same, 1, "{}'s title is not unique among the implementations", item.issue);
        quorum.press(&format!("label:{}", item.title))?;
        let asks = asks(&comments(&item.issue));
        quorum.wait_for_view(&format!("label:{}", asks[0].head(&quorum)), SHOWN)?;
        for ask in &asks {
            ask.check(&mut quorum, started)?;
        }
    }

    // The resumed issue: one start, then each later `impl` a resume.
    quorum.press(&format!("label:{}", resumed.title))?;
    let impls = markers(&comments(&resumed.issue)).iter().filter(|(k, _)| k == "impl").count();
    quorum.wait_for(&quorum.say(&Msg::ImplStarted), SHOWN)?;
    assert_eq!(quorum.count(&format!("text:{}", quorum.say(&Msg::ImplStarted)))?, 1);
    assert_eq!(quorum.count(&format!("text:{}", quorum.say(&Msg::ImplResumed)))?, impls - 1);

    // The replaced issue points at the new one; the new one's `replaces`
    // points back.
    quorum.press(&format!("label:{}", replaced.title))?;
    let (_, settled) = markers(&comments(&replaced.issue))
        .into_iter()
        .find(|(k, _)| k == "replaced")
        .expect("its pm:replaced");
    let new = item(settled["by"].as_str().expect("replaced by"));
    let forward = format!("label:{}", quorum.say(&Msg::ReplacedBy(short(&new.issue))));
    quorum.wait_for_view(&forward, SHOWN)?;
    quorum.press(&forward)?;
    let (kind, replaces) = marker(&new.body).expect("the new issue's body ends with a marker");
    assert_eq!(kind, "replaces");
    assert_eq!(replaces["old"].as_str(), Some(replaced.issue.as_str()));
    let back = format!("label:{}", quorum.say(&Msg::Replaces(short(&replaced.issue))));
    quorum.wait_for_view(&back, SHOWN)?;
    quorum.wait_for(&new.title, SHOWN)?;
    quorum.press(&back)?;
    let asks = asks(&comments(&replaced.issue));
    quorum.wait_for_view(&format!("label:{}", asks[0].head(&quorum)), SHOWN)?;
    Ok(())
}

// -- row 5 -------------------------------------------------------------------

#[test]
fn click_paths() -> Result<(), String> {
    let _serial = serial();
    let sandbox = Sandbox::new("detail-click-paths");
    let mut quorum = support::start_detail(&sandbox, DESIGN, Lang::Zh, Vec::new());
    let g25_item = item(G25);
    let g25 = header(&quorum, DESIGN, &g25_item, Some(queue_state(&recorded_queue(), G25)));
    let latest = versions(&comments(G25)).pop().expect("G25's latest version");
    let cells = cells(&latest);
    let node = format!("label:{}", g25_item.title);
    let open_detail = format!("label:{}", quorum.say(&Msg::OpenDetail));
    let issues = quorum.say(&Msg::PageIssues);

    // The graph: a press selects the node, the inspector shows the latest
    // version's seat × row matrix; 「打开详情」 opens the detail, whose
    // matrix is the same.
    quorum.open(Msg::PageIssues)?;
    quorum.press(&format!("label:{}", quorum.say(&Msg::ViewGraph)))?;
    // Fit the canvas to the window: every column's node is then on screen.
    let fit = format!("label:{}", quorum.say(&Msg::Fit));
    quorum.wait_for_view(&fit, SHOWN)?;
    quorum.press(&fit)?;
    quorum.wait_for_view(&node, SHOWN)?;
    quorum.press(&node)?;
    quorum.wait_for_view(&cells[0].selector(&quorum), SHOWN)?;
    assert!(quorum.selected(&node)?, "the graph's node is not selected");
    same_cells(&quorum, &cells, "the inspector's matrix")?;
    quorum.press(&open_detail)?;
    wait_for_run(&mut quorum, &g25)?;
    quorum.wait_for_view(&cells[0].selector(&quorum), SHOWN)?;
    same_cells(&quorum, &cells, "the detail's matrix")?;

    // The list: the same through a row.
    quorum.press(&format!("label:{issues} >> nth=1"))?;
    quorum.press(&format!("label:{}", quorum.say(&Msg::ViewList)))?;
    quorum.wait_for_view(&node, SHOWN)?;
    quorum.press(&node)?;
    quorum.wait_for_view(&cells[0].selector(&quorum), SHOWN)?;
    same_cells(&quorum, &cells, "the list inspector's matrix")?;
    quorum.press(&open_detail)?;
    wait_for_run(&mut quorum, &g25)?;

    // 概览 「需要你处理」: the open escalation's button opens the agenda
    // issue it waits on.
    let (_, escalated) = items()
        .into_iter()
        .filter(|item| item.field("Kind") == Some("Implementation") && expected_group(item) == ImplGroup::Waiting)
        .flat_map(|item| markers(&comments(&item.issue)))
        .find(|(k, _)| k == "escalated")
        .expect("an open escalation");
    let agenda = item(escalated["agenda"].as_str().expect("an agenda issue"));
    quorum.open(Msg::PageOverview)?;
    let button = format!("label:{}", quorum.say(&Msg::OpenAgenda(agenda_label(DESIGN, &agenda))));
    quorum.wait_for_view(&button, SHOWN)?;
    quorum.press(&button)?;
    let opened = header(&quorum, DESIGN, &agenda, None);
    wait_for_run(&mut quorum, &opened)?;
    Ok(())
}

// -- the recordings ----------------------------------------------------------

/// A Project item as the recording has it.
#[derive(Debug, Clone)]
struct Item {
    issue: String,
    title: String,
    body: String,
    created_at: String,
    closed: bool,
    fields: BTreeMap<String, String>,
}

impl Item {
    fn field(&self, name: &str) -> Option<&str> {
        self.fields.get(name).map(String::as_str)
    }
}

/// The recorded Project's issues.
fn items() -> Vec<Item> {
    let project: Value = serde_json::from_str(&support::detail_project()).expect("the recorded Project");
    project["data"]["repositoryOwner"]["projectV2"]["items"]["nodes"]
        .as_array()
        .expect("the Project's items")
        .iter()
        .filter_map(|node| {
            let content = &node["content"];
            let repo = content["repository"]["nameWithOwner"].as_str()?;
            let number = content["number"].as_u64()?;
            let fields = node["fieldValues"]["nodes"]
                .as_array()?
                .iter()
                .filter_map(|value| {
                    let field = value["field"]["name"].as_str()?;
                    let value = value["name"].as_str().or(value["text"].as_str())?;
                    Some((field.to_owned(), value.to_owned()))
                })
                .collect();
            Some(Item {
                issue: format!("{repo}#{number}"),
                title: content["title"].as_str()?.to_owned(),
                body: content["body"].as_str().unwrap_or_default().to_owned(),
                created_at: content["createdAt"].as_str()?.to_owned(),
                closed: content["state"] == "CLOSED",
                fields,
            })
        })
        .collect()
}

fn item(issue: &str) -> Item {
    items()
        .into_iter()
        .find(|item| item.issue == issue)
        .unwrap_or_else(|| panic!("the Project holds no {issue}"))
}

/// The council's recorded queue answer.
fn recorded_queue() -> String {
    std::fs::read_to_string(support::fixtures_dir().join("responses/council-queue.json")).expect("the queue")
}

/// The verdict a queue answer gives `issue`.
fn queue_state(queue: &str, issue: &str) -> QueueState {
    let rows: Vec<Value> = serde_json::from_str(queue).expect("a queue answer");
    let row = rows
        .iter()
        .find(|row| row["ref"] == issue)
        .unwrap_or_else(|| panic!("the queue lists no {issue}"));
    serde_json::from_value(row["state"].clone()).expect("a queue state")
}

/// A comment and the marker it ends with.
#[derive(Debug, Clone)]
struct Posted {
    id: u64,
    at: String,
    body: String,
    marker: Option<(String, Value)>,
}

fn comments(issue: &str) -> Vec<Posted> {
    let json = support::recorded_comments(issue).unwrap_or_else(|| panic!("no recording of {issue}'s comments"));
    parse_comments(&json)
}

fn parse_comments(json: &str) -> Vec<Posted> {
    let list: Vec<Value> = serde_json::from_str(json).expect("a comments answer");
    list.iter()
        .map(|comment| {
            let body = comment["body"].as_str().unwrap_or_default().to_owned();
            Posted {
                id: comment["id"].as_u64().expect("a comment id"),
                at: comment["created_at"].as_str().expect("a comment's time").to_owned(),
                marker: marker(&body),
                body,
            }
        })
        .collect()
}

/// The marker `body`'s last line holds: `<!-- pm:<kind> {json} -->`.
fn marker(body: &str) -> Option<(String, Value)> {
    let last = body.trim_end().lines().last()?.trim();
    let inner = last.strip_prefix("<!-- pm:")?.strip_suffix("-->")?.trim();
    let (kind, json) = inner.split_once(' ')?;
    Some((kind.to_owned(), serde_json::from_str(json).ok()?))
}

/// The text above a comment's marker.
fn prose(body: &str) -> String {
    let body = body.trim_end();
    match body.rsplit_once('\n') {
        Some((rest, last)) if last.trim_start().starts_with("<!-- pm:") => rest.trim().to_owned(),
        None if body.trim_start().starts_with("<!-- pm:") => String::new(),
        _ => body.trim().to_owned(),
    }
}

fn markers(comments: &[Posted]) -> Vec<(String, Value)> {
    comments.iter().filter_map(|c| c.marker.clone()).collect()
}

/// The first marker of `kind`.
fn marker_of(comments: &[Posted], kind: &str) -> Option<Value> {
    markers(comments).into_iter().find(|(k, _)| k == kind).map(|(_, m)| m)
}

// -- panel history -----------------------------------------------------------

/// A draft version: its round, its rows, the votes on it and its ruling.
#[derive(Debug)]
struct Version {
    round: u32,
    number: u32,
    rows: Vec<String>,
    votes: Vec<Vote>,
    ruling: Vec<(String, String)>,
}

/// A seat's `pm:vote` on a version.
#[derive(Debug)]
struct Vote {
    seat: String,
    body: String,
    rows: Vec<Value>,
}

/// The versions, in comment order, each in the round of the last
/// `pm:panel` before its draft.
fn versions(comments: &[Posted]) -> Vec<Version> {
    let mut round = 1;
    let mut out: Vec<Version> = Vec::new();
    for comment in comments {
        let Some((kind, marker)) = &comment.marker else {
            continue;
        };
        let number = || marker["version"].as_u64().expect("a version") as u32;
        let rows = || marker["rows"].as_array().expect("rows").clone();
        match kind.as_str() {
            "panel" => round = marker["round"].as_u64().map_or(round, |r| r as u32),
            "draft" => out.push(Version {
                round,
                number: number(),
                rows: rows().iter().map(|r| r["id"].as_str().expect("a row id").to_owned()).collect(),
                votes: Vec::new(),
                ruling: Vec::new(),
            }),
            "vote" => {
                let version = out.iter_mut().find(|v| v.number == number()).expect("a vote's draft");
                version.votes.push(Vote {
                    seat: marker["seat"].as_str().expect("a seat").to_owned(),
                    body: comment.body.clone(),
                    rows: rows(),
                });
            }
            "ruling" => {
                let version = out.iter_mut().find(|v| v.number == number()).expect("a ruling's draft");
                for row in rows() {
                    version.ruling.push((
                        row["row"].as_str().expect("a row").to_owned(),
                        row["category"].as_str().expect("a category").to_owned(),
                    ));
                }
            }
            _ => {}
        }
    }
    out
}

/// When each round's panel started (Unix ms), in comment order.
fn round_starts(comments: &[Posted]) -> Vec<(u32, f64)> {
    comments
        .iter()
        .filter_map(|c| {
            let (kind, marker) = c.marker.as_ref()?;
            let started = matches!(marker["event"].as_str(), Some("started" | "resumed"));
            (kind == "panel" && started).then(|| {
                (marker["round"].as_u64().expect("a round") as u32, time::parse_ms(&c.at).expect("a time"))
            })
        })
        .collect()
}

/// The round track: each version's step, a revision before every version
/// after the first; with the version whose votes each step shows (a
/// revision shows the votes that called for it).
fn steps(versions: &[Version]) -> Vec<(Msg, u32)> {
    let mut steps = Vec::new();
    for (i, version) in versions.iter().enumerate() {
        if let Some(before) = i.checked_sub(1).map(|i| &versions[i]) {
            steps.push((Msg::StepRevision { from: before.number, to: version.number }, before.number));
        }
        steps.push((Msg::StepDraft { round: version.round, version: version.number }, version.number));
    }
    steps
}

/// A vote row's stance, as the contract reads it: a veto, an objection
/// with a counterexample, one without, an approval, or no vote.
fn stance(row: Option<&Value>) -> Stance {
    let Some(row) = row else {
        return Stance::Pending;
    };
    match row["choice"].as_str() {
        Some("veto") => Stance::Veto,
        Some("oppose") if !row["counterexample"].is_null() => Stance::Counterexample,
        Some("oppose") => Stance::Oppose,
        Some("approve") => Stance::Approve,
        other => panic!("a vote's choice {other:?}"),
    }
}

/// A stance's glyph, as #4 writes them.
fn glyph(stance: Stance) -> &'static str {
    match stance {
        Stance::Approve => "✓",
        Stance::Oppose => "✕",
        Stance::Veto => "⊘",
        Stance::Counterexample => "?",
        Stance::Pending => "·",
    }
}

/// A matrix cell: its seat, row and stance.
#[derive(Debug)]
struct Cell {
    seat: String,
    row: String,
    stance: Stance,
}

impl Cell {
    fn label(&self, quorum: &Fixture) -> String {
        quorum.say(&Msg::Cell {
            seat: self.seat.clone(),
            row: self.row.clone(),
            stance: self.stance,
        })
    }

    /// The cell showing its glyph.
    fn selector(&self, quorum: &Fixture) -> String {
        format!("label:{} >> text:{}", self.label(quorum), glyph(self.stance))
    }
}

/// Every seat × row cell of `version`.
fn cells(version: &Version) -> Vec<Cell> {
    version
        .votes
        .iter()
        .flat_map(|vote| {
            version.rows.iter().map(|row| Cell {
                seat: vote.seat.clone(),
                row: row.clone(),
                stance: stance(vote.rows.iter().find(|r| r["row"] == row.as_str())),
            })
        })
        .collect()
}

/// Each of `cells` shows once with its glyph.
fn same_cells(quorum: &Fixture, cells: &[Cell], what: &str) -> Result<(), String> {
    for cell in cells {
        let n = quorum.count(&cell.selector(quorum))?;
        if n != 1 {
            return Err(format!("{what}: {} shows {n} times", cell.selector(quorum)));
        }
    }
    Ok(())
}

// -- council sessions --------------------------------------------------------

/// A council session as the recorded `/v1/sessions` has it.
#[derive(Debug)]
struct Session {
    id: String,
    kind: String,
    label: String,
    reference: String,
    started_at: String,
}

impl Session {
    /// Its line's title: kind and the seat its label names.
    fn title(&self, quorum: &Fixture) -> String {
        let seat = self.label.split_whitespace().nth(1).filter(|seat| *seat != self.reference);
        quorum.say(&Msg::SessionSeat {
            kind: self.kind.clone(),
            seat: seat.map(str::to_owned),
        })
    }
}

fn council_sessions() -> Vec<Session> {
    let sessions: Vec<Value> =
        serde_json::from_str(&support::sessions_with(&support::detail("design-23-sessions.json"))).expect("sessions");
    sessions
        .iter()
        .map(|s| Session {
            id: s["id"].as_str().expect("an id").to_owned(),
            kind: s["kind"].as_str().expect("a kind").to_owned(),
            label: s["label"].as_str().unwrap_or_default().to_owned(),
            reference: s["ref"].as_str().unwrap_or_default().to_owned(),
            started_at: s["startedAt"].as_str().expect("a start").to_owned(),
        })
        .collect()
}

/// The texts of the panel headed `head`, up to the next panel's `end`.
fn panel_of<'t>(texts: &[&'t str], head: &str, end: &str) -> Option<Vec<&'t str>> {
    let start = texts.iter().position(|t| *t == head)? + 1;
    let stop = start + texts[start..].iter().position(|t| *t == end)?;
    Some(texts[start..stop].to_vec())
}

/// What `after` has that `before` has not, between their common ends.
fn inserted(before: &[String], after: &[String]) -> Vec<String> {
    let head = before.iter().zip(after).take_while(|(a, b)| a == b).count();
    let tail = before[head..]
        .iter()
        .rev()
        .zip(after[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    after[head..after.len() - tail].to_vec()
}

// -- implementation timelines ------------------------------------------------

/// An ask and what the issue's markers say of it (§2: one settlement per
/// ask, by comment id; `escalated` hands it to the council).
#[derive(Debug)]
struct Ask {
    n: usize,
    ask: Posted,
    escalated: Option<Posted>,
    settled: Option<Posted>,
}

/// The issue's asks in comment order, each with its escalation and
/// settlement.
fn asks(comments: &[Posted]) -> Vec<Ask> {
    let mut asks: Vec<Ask> = Vec::new();
    for comment in comments {
        let Some((kind, marker)) = &comment.marker else {
            continue;
        };
        let of = |asks: &mut Vec<Ask>| -> Option<usize> {
            let id = marker["ask"].as_u64()?;
            asks.iter().position(|a| a.ask.id == id)
        };
        match kind.as_str() {
            "ask" => asks.push(Ask {
                n: asks.len() + 1,
                ask: comment.clone(),
                escalated: None,
                settled: None,
            }),
            "escalated" => {
                let at = of(&mut asks).expect("an escalation of an ask");
                asks[at].escalated = Some(comment.clone());
            }
            "reply" | "replaced" => {
                let at = of(&mut asks).expect("a settlement of an ask");
                asks[at].settled = Some(comment.clone());
            }
            _ => {}
        }
    }
    asks
}

fn ms(at: &str) -> f64 {
    time::parse_ms(at).expect("an RFC 3339 time")
}

impl Ask {
    /// The unit's head: number, time, how long until answered or escalated.
    fn head(&self, quorum: &Fixture) -> String {
        let asked = ms(&self.ask.at);
        let answered = match (&self.escalated, &self.settled) {
            (None, Some(settled)) => Some(time::ago(asked, ms(&settled.at))),
            _ => None,
        };
        quorum.say(&Msg::AskHead {
            n: self.n,
            at: time::hm(asked),
            answered,
            escalated: self.escalated.as_ref().map(|e| time::ago(asked, ms(&e.at))),
        })
    }

    /// The unit holds the question, its escalation and its settlement, or
    /// what it waits on and for how long.
    fn check(&self, quorum: &mut Fixture, started: f64) -> Result<(), String> {
        let unit = format!("label:{}", self.head(quorum));
        let inside = |quorum: &Fixture, text: &str| -> Result<(), String> {
            match quorum.count(&format!("{unit} >> text:{text}"))? {
                1 => Ok(()),
                n => Err(format!("{unit} holds 「{text}」 {n} times")),
            }
        };
        if quorum.count(&unit)? != 1 {
            return Err(format!("no single unit {unit}"));
        }
        let question = prose(&self.ask.body);
        if !question.is_empty() {
            inside(quorum, &question)?;
        }
        if let Some(escalated) = &self.escalated {
            let (_, marker) = escalated.marker.as_ref().expect("a marker");
            let agenda = marker["agenda"].as_str().expect("an agenda issue");
            inside(quorum, &quorum.say(&Msg::Escalate(agenda_label(AGENDA, &item(agenda)))))?;
        }
        match &self.settled {
            Some(settled) => {
                let (kind, marker) = settled.marker.as_ref().expect("a marker");
                let title = match (kind.as_str(), marker["decision"].as_str(), marker["by"].as_str()) {
                    ("reply", Some(agenda), _) => Msg::CouncilReply(agenda_label(AGENDA, &item(agenda))),
                    ("reply", None, _) => Msg::GuardianReply,
                    ("replaced", _, Some(by)) => Msg::ReplacedBy(short(by)),
                    other => panic!("a settlement {other:?}"),
                };
                inside(quorum, &quorum.say(&title))?;
                let quoted = prose(&settled.body);
                if !quoted.is_empty() {
                    inside(quorum, &quoted)?;
                }
            }
            None => {
                let waiting = match &self.escalated {
                    Some(escalated) => {
                        let (_, marker) = escalated.marker.as_ref().expect("a marker");
                        let agenda = item(marker["agenda"].as_str().expect("an agenda issue"));
                        Msg::WaitDecision(agenda_label(AGENDA, &agenda))
                    }
                    None => Msg::WaitGuardians,
                };
                inside(quorum, &quorum.say(&waiting))?;
                // How long it has waited, as of when it was drawn.
                let asked = ms(&self.ask.at);
                let then: Ago = time::ago(asked, started);
                let now: Ago = time::ago(asked, time::now_ms());
                let words: Vec<String> = [then, now].iter().map(|ago| quorum.say(&Msg::Waited(*ago))).collect();
                let shown = words
                    .iter()
                    .map(|w| quorum.count(&format!("{unit} >> text:{w}")))
                    .collect::<Result<Vec<_>, _>>()?;
                if !shown.contains(&1) {
                    return Err(format!("{unit} does not say how long it waited: {words:?}"));
                }
            }
        }
        Ok(())
    }
}

/// The group an implementation issue belongs in, by #4 「实现」: done when
/// closed, done or superseded; waiting while an ask has no settlement;
/// in progress once a worker started (or its Status says so); else to do.
fn expected_group(item: &Item) -> ImplGroup {
    if item.closed || matches!(item.field("Status"), Some("Done" | "Superseded")) {
        return ImplGroup::Done;
    }
    let comments = comments(&item.issue);
    if asks(&comments).iter().any(|ask| ask.settled.is_none()) {
        ImplGroup::Waiting
    } else if item.field("Status") == Some("In Progress") || markers(&comments).iter().any(|(k, _)| k == "impl") {
        ImplGroup::Running
    } else {
        ImplGroup::Todo
    }
}

/// `owner/repo#n` without its owner.
fn short(issue: &str) -> String {
    issue.split_once('/').map_or(issue, |(_, rest)| rest).to_owned()
}

// -- the window --------------------------------------------------------------

/// How an issue is named where the PM repository is `pm`: its agenda
/// number when it is one of the repository's agenda or human issues, else
/// `#n`.
fn agenda_label(pm: &str, item: &Item) -> String {
    let (repo, number) = item.issue.split_once('#').expect("owner/repo#n");
    let listed = repo == pm && matches!(item.field("Kind"), Some("Human" | "Agenda"));
    match item.field("Agenda") {
        Some(agenda) if listed => agenda.to_owned(),
        _ => format!("#{number}"),
    }
}

/// The detail's header for `item`: the status word (the council's verdict
/// `state`), agenda number, kind and drafting seat, when it was opened,
/// the title.
fn header(quorum: &Fixture, pm: &str, item: &Item, state: Option<QueueState>) -> Vec<String> {
    let mut run = Vec::new();
    if let Some(state) = state {
        run.push(quorum.say(&Msg::Queue(state)));
    }
    run.push(agenda_label(pm, item));
    if state.is_some() {
        run.push(quorum.say(&Msg::IssueKind {
            human: item.field("Kind") == Some("Human"),
            proposer: item.field("Proposer").map(str::to_owned),
        }));
    }
    run.push(quorum.say(&Msg::CreatedAt(time::short(&item.created_at))));
    run.push(item.title.clone());
    run
}

/// Waits for `run` as consecutive texts.
fn wait_for_run(quorum: &mut Fixture, run: &[String]) -> Result<(), String> {
    let run = run.to_vec();
    quorum.wait_until(&format!("{run:?} in a row"), SHOWN, move |texts| {
        texts.windows(run.len()).any(|w| w.iter().zip(&run).all(|(a, b)| a.trim() == b))
    })
}

/// 议题 as a list: a press on `title`'s row selects it, 「打开详情」 in the
/// inspector opens its detail (#9: a press selects).
fn open_from_list(quorum: &mut Fixture, title: &str) -> Result<(), String> {
    quorum.open(Msg::PageIssues)?;
    quorum.press(&format!("label:{}", quorum.say(&Msg::ViewList)))?;
    let row = format!("label:{title}");
    quorum.wait_for_view(&row, SHOWN)?;
    quorum.press(&row)?;
    let open = format!("label:{}", quorum.say(&Msg::OpenDetail));
    quorum.wait_for_view(&open, SHOWN)?;
    quorum.press(&open)
}
