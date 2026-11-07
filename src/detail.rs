//! Issue detail (C4 「议题详情」, 「实现进度」): pure derivations over one
//! issue's comments, oldest first. An agenda issue gives its panel history
//! and its decision (`Pepgent/pm-new#1` decision 6696709150 §A, §B); an
//! implementation issue gives its ask timeline (decision 6695362696 §2).
//! A comment whose last line starts as a marker but does not read as one,
//! or a marker that breaks the contract's pairing, is an [`Invalid`] item:
//! shown, not dropped.

use std::collections::{BTreeMap, HashMap};

use jiff::{SignedDuration, Timestamp};

use crate::gh::{
    comments::Comment,
    markers::{
        self, Ask, Decision, Draft, Escalated, Impl, Marker, Panel, Replaced, Replaces, Reply,
        Ruling, Superseded, VoteRow,
    },
};
use crate::time;

/// A comment shown as it is, because it is not a marker the contract
/// allows there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid<'a> {
    pub comment_url: &'a str,
    pub reason: String,
}

impl<'a> Invalid<'a> {
    fn new(comment: &'a Comment, reason: String) -> Self {
        Self {
            comment_url: &comment.html_url,
            reason,
        }
    }
}

/// A marker and the comment (or issue body) it ends.
#[derive(Debug, Clone, PartialEq)]
pub struct Posted<'a, T> {
    pub comment: &'a Comment,
    pub marker: T,
}

impl<'a, T> Posted<'a, T> {
    /// The comment's text above its marker line.
    pub fn text(&self) -> &'a str {
        prose(&self.comment.body)
    }
}

/// `body` without its last line when that line is a marker, trimmed.
pub fn prose(body: &str) -> &str {
    let body = body.trim_end();
    let (rest, last) = match body.rfind('\n') {
        Some(end) => (&body[..end], &body[end + 1..]),
        None => ("", body),
    };
    if last.trim_start().starts_with("<!-- pm:") {
        rest.trim()
    } else {
        body.trim()
    }
}

/// Every comment whose last line starts as a marker, with what it reads as.
fn read_all(comments: &[Comment]) -> impl Iterator<Item = (&Comment, Result<Marker, String>)> {
    comments
        .iter()
        .filter_map(|comment| markers::read(&comment.body).map(|read| (comment, read)))
}

// ---------------------------------------------------------------------------
// Panel history (§A)

/// A panel round: the comments from its `pm:panel` (round 1 before any)
/// up to the next round's.
#[derive(Debug, Clone, PartialEq)]
pub struct Round<'a> {
    pub number: u32,
    /// Panel state changes and supersessions, in comment order.
    pub events: Vec<RoundEvent<'a>>,
    /// The drafts posted in this round, by version.
    pub versions: BTreeMap<u32, Version<'a>>,
    pub invalid: Vec<Invalid<'a>>,
}

/// A change of the panel's state.
#[derive(Debug, Clone, PartialEq)]
pub enum RoundEvent<'a> {
    Panel(Posted<'a, Panel>),
    /// A human issue changed this agenda issue.
    Superseded(Posted<'a, Superseded>),
}

/// A draft version with the votes and the ruling on it.
#[derive(Debug, Clone, PartialEq)]
pub struct Version<'a> {
    pub draft: Posted<'a, Draft>,
    /// The vote matrix: one ballot per seat, in vote order.
    pub ballots: Vec<Ballot<'a>>,
    pub ruling: Option<Posted<'a, Ruling>>,
}

impl Version<'_> {
    /// The ruling's category for the row `row`, if a ruling names it.
    pub fn ruled(&self, row: &str) -> Option<&str> {
        let ruling = self.ruling.as_ref()?;
        ruling
            .marker
            .rows
            .iter()
            .find(|ruled| ruled.row == row)
            .map(|ruled| ruled.category.as_str())
    }
}

/// A seat's votes on a version: `cells[i]` is its vote on the draft's row
/// `i`, `None` where it cast none.
#[derive(Debug, Clone, PartialEq)]
pub struct Ballot<'a> {
    pub seat: String,
    pub comment: &'a Comment,
    pub cells: Vec<Option<VoteRow>>,
}

impl<'a> Ballot<'a> {
    /// The seat's reason: its vote comment's text above the marker.
    pub fn reason(&self) -> &'a str {
        prose(&self.comment.body)
    }
}

/// The issue's panel rounds, in round order.
pub fn panel_history(comments: &[Comment]) -> Vec<Round<'_>> {
    let mut rounds: Vec<Round> = Vec::new();
    let mut current = 1;
    // Which round holds each draft version.
    let mut owner: HashMap<u32, usize> = HashMap::new();
    for (comment, read) in read_all(comments) {
        let here = round_index(&mut rounds, current);
        let marker = match read {
            Ok(marker) => marker,
            Err(reason) => {
                rounds[here].invalid.push(Invalid::new(comment, reason));
                continue;
            }
        };
        match marker {
            Marker::Panel(panel) => {
                current = panel.round;
                let at = round_index(&mut rounds, current);
                rounds[at].events.push(RoundEvent::Panel(Posted {
                    comment,
                    marker: panel,
                }));
            }
            Marker::Superseded(superseded) => {
                rounds[here].events.push(RoundEvent::Superseded(Posted {
                    comment,
                    marker: superseded,
                }));
            }
            Marker::Draft(draft) => {
                let version = draft.version;
                if let Some(&first) = owner.get(&version) {
                    let reason = format!(
                        "second pm:draft of version {version} (first: {})",
                        rounds[first].versions[&version].draft.comment.html_url
                    );
                    rounds[here].invalid.push(Invalid::new(comment, reason));
                } else {
                    owner.insert(version, here);
                    rounds[here].versions.insert(
                        version,
                        Version {
                            draft: Posted {
                                comment,
                                marker: draft,
                            },
                            ballots: Vec::new(),
                            ruling: None,
                        },
                    );
                }
            }
            Marker::Vote(vote) => {
                let Some(&at) = owner.get(&vote.version) else {
                    let reason = format!(
                        "pm:vote of seat {} on version {}, which has no draft",
                        vote.seat, vote.version
                    );
                    rounds[here].invalid.push(Invalid::new(comment, reason));
                    continue;
                };
                let round = &mut rounds[at];
                let version = round
                    .versions
                    .get_mut(&vote.version)
                    .expect("owner names the round holding the version");
                if let Some(first) = version.ballots.iter().find(|b| b.seat == vote.seat) {
                    let reason = format!(
                        "second pm:vote of seat {} on version {} (first: {})",
                        vote.seat, vote.version, first.comment.html_url
                    );
                    round.invalid.push(Invalid::new(comment, reason));
                    continue;
                }
                let rows = &version.draft.marker.rows;
                let mut cells = vec![None; rows.len()];
                for cast in vote.rows {
                    match rows.iter().position(|row| row.id == cast.row) {
                        Some(i) if cells[i].is_none() => cells[i] = Some(cast),
                        Some(_) => round.invalid.push(Invalid::new(
                            comment,
                            format!("pm:vote of seat {} votes twice on row {}", vote.seat, cast.row),
                        )),
                        None => round.invalid.push(Invalid::new(
                            comment,
                            format!(
                                "pm:vote of seat {} names row {}, not in draft version {}",
                                vote.seat, cast.row, vote.version
                            ),
                        )),
                    }
                }
                version.ballots.push(Ballot {
                    seat: vote.seat,
                    comment,
                    cells,
                });
            }
            Marker::Ruling(ruling) => {
                let Some(&at) = owner.get(&ruling.version) else {
                    let reason = format!("pm:ruling on version {}, which has no draft", ruling.version);
                    rounds[here].invalid.push(Invalid::new(comment, reason));
                    continue;
                };
                let round = &mut rounds[at];
                let version = round
                    .versions
                    .get_mut(&ruling.version)
                    .expect("owner names the round holding the version");
                match &version.ruling {
                    Some(first) => {
                        let reason = format!(
                            "second pm:ruling on version {} (first: {})",
                            ruling.version, first.comment.html_url
                        );
                        round.invalid.push(Invalid::new(comment, reason));
                    }
                    None => {
                        version.ruling = Some(Posted {
                            comment,
                            marker: ruling,
                        });
                    }
                }
            }
            // The decision has its own view; the rest belong to
            // implementation issues and an agenda issue's body.
            Marker::Decision(_)
            | Marker::Escalation(_)
            | Marker::Impl(_)
            | Marker::Ask(_)
            | Marker::Reply(_)
            | Marker::Replaced(_)
            | Marker::Replaces(_)
            | Marker::Escalated(_) => {}
        }
    }
    rounds.sort_by_key(|round| round.number);
    rounds
}

/// The index of round `number`, added when missing.
fn round_index(rounds: &mut Vec<Round<'_>>, number: u32) -> usize {
    if let Some(at) = rounds.iter().position(|round| round.number == number) {
        return at;
    }
    rounds.push(Round {
        number,
        events: Vec::new(),
        versions: BTreeMap::new(),
        invalid: Vec::new(),
    });
    rounds.len() - 1
}

// ---------------------------------------------------------------------------
// Decision (§A)

/// The issue's decision: each row's outcome with its approvals and
/// threshold, the old issues it superseded and the implementation issues
/// it created.
#[derive(Debug, Clone, PartialEq)]
pub struct DecisionView<'a> {
    pub decision: Posted<'a, Decision>,
    /// Later `pm:decision` comments: an issue is decided once.
    pub invalid: Vec<Invalid<'a>>,
}

/// The issue's first `pm:decision`, if it has one.
pub fn decision_view(comments: &[Comment]) -> Option<DecisionView<'_>> {
    let mut view: Option<DecisionView> = None;
    for (comment, read) in read_all(comments) {
        let Ok(Marker::Decision(decision)) = read else {
            continue;
        };
        match &mut view {
            None => {
                view = Some(DecisionView {
                    decision: Posted {
                        comment,
                        marker: decision,
                    },
                    invalid: Vec::new(),
                });
            }
            Some(view) => {
                let reason = format!(
                    "second pm:decision (first: {})",
                    view.decision.comment.html_url
                );
                view.invalid.push(Invalid::new(comment, reason));
            }
        }
    }
    view
}

// ---------------------------------------------------------------------------
// Ask timeline (§2)

/// An item of an implementation issue's timeline.
#[derive(Debug, Clone, PartialEq)]
pub enum TimelineItem<'a> {
    /// The issue body's `replaces`: this issue replaced `old`.
    Replaces(Posted<'a, Replaces>),
    /// A worker session started or resumed.
    Impl(Posted<'a, Impl>),
    /// An `ask` with what settled it, or what it waits on.
    Ask(AskThread<'a>),
    /// A `replaced` that settles no ask.
    Replaced(Posted<'a, Replaced>),
    Invalid(Invalid<'a>),
}

/// An `ask` and its progress.
#[derive(Debug, Clone, PartialEq)]
pub struct AskThread<'a> {
    pub ask: Posted<'a, Ask>,
    pub progress: AskProgress<'a>,
}

/// Where an ask stands.
#[derive(Debug, Clone, PartialEq)]
pub enum AskProgress<'a> {
    /// No settlement yet; `age` is the time since the ask.
    Open {
        waiting_on: WaitingOn<'a>,
        age: SignedDuration,
    },
    /// Settled; `escalated` when the council's decision settled it.
    Settled {
        escalated: Option<Posted<'a, Escalated>>,
        settlement: Settlement<'a>,
    },
}

/// Who owes an open ask its settlement.
#[derive(Debug, Clone, PartialEq)]
pub enum WaitingOn<'a> {
    Guardians,
    /// The guardians raised it as an agenda issue.
    Council(Posted<'a, Escalated>),
}

/// The marker that settled an ask.
#[derive(Debug, Clone, PartialEq)]
pub enum Settlement<'a> {
    Reply(Posted<'a, Reply>),
    Replaced(Posted<'a, Replaced>),
}

impl<'a> Settlement<'a> {
    pub fn comment(&self) -> &'a Comment {
        match self {
            Settlement::Reply(posted) => posted.comment,
            Settlement::Replaced(posted) => posted.comment,
        }
    }
}

/// A timeline entry while the comments are read: an ask stays open to
/// its settlement.
enum Entry<'a> {
    Done(TimelineItem<'a>),
    Ask {
        ask: Posted<'a, Ask>,
        escalated: Option<Posted<'a, Escalated>>,
        settlement: Option<Settlement<'a>>,
    },
}

/// An implementation issue's timeline as of `now`: the body's `replaces`,
/// then its markers in comment order, each `ask` paired by comment id with
/// the one `reply`, `replaced` or `escalated` that settles it and, after
/// `escalated`, the council's `reply` or `replaced` of the same ask. `issue`
/// is the issue itself read as a comment (its body ends with `replaces`).
pub fn ask_timeline<'a>(
    issue: &'a Comment,
    comments: &'a [Comment],
    now: Timestamp,
) -> Vec<TimelineItem<'a>> {
    let mut entries: Vec<Entry> = Vec::new();
    match markers::read(&issue.body) {
        Some(Ok(Marker::Replaces(replaces))) => {
            entries.push(Entry::Done(TimelineItem::Replaces(Posted {
                comment: issue,
                marker: replaces,
            })));
        }
        Some(Err(reason)) => {
            entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(issue, reason))));
        }
        Some(Ok(_)) | None => {}
    }
    let mut asks: HashMap<u64, usize> = HashMap::new();
    for (comment, read) in read_all(comments) {
        let marker = match read {
            Ok(marker) => marker,
            Err(reason) => {
                entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(comment, reason))));
                continue;
            }
        };
        match marker {
            Marker::Impl(marker) => {
                entries.push(Entry::Done(TimelineItem::Impl(Posted { comment, marker })));
            }
            Marker::Ask(marker) => {
                asks.insert(comment.id, entries.len());
                entries.push(Entry::Ask {
                    ask: Posted { comment, marker },
                    escalated: None,
                    settlement: None,
                });
            }
            Marker::Reply(marker) => {
                let ask = marker.ask;
                let by = Settlement::Reply(Posted { comment, marker });
                settle(&mut entries, &asks, ask, by);
            }
            Marker::Replaced(marker) => match marker.ask {
                Some(ask) => {
                    let by = Settlement::Replaced(Posted { comment, marker });
                    settle(&mut entries, &asks, ask, by);
                }
                None => {
                    entries.push(Entry::Done(TimelineItem::Replaced(Posted { comment, marker })));
                }
            },
            Marker::Escalated(marker) => {
                escalate(&mut entries, &asks, Posted { comment, marker });
            }
            Marker::Replaces(_) => {
                let reason = "pm:replaces belongs in the issue body, not a comment".to_owned();
                entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(comment, reason))));
            }
            // An agenda issue's markers.
            Marker::Escalation(_)
            | Marker::Panel(_)
            | Marker::Draft(_)
            | Marker::Vote(_)
            | Marker::Ruling(_)
            | Marker::Decision(_)
            | Marker::Superseded(_) => {}
        }
    }
    entries
        .into_iter()
        .map(|entry| match entry {
            Entry::Done(item) => item,
            Entry::Ask {
                ask,
                escalated,
                settlement: Some(settlement),
            } => TimelineItem::Ask(AskThread {
                ask,
                progress: AskProgress::Settled {
                    escalated,
                    settlement,
                },
            }),
            Entry::Ask {
                ask,
                escalated,
                settlement: None,
            } => match time::parse(&ask.comment.created_at) {
                Some(asked) => TimelineItem::Ask(AskThread {
                    progress: AskProgress::Open {
                        waiting_on: escalated.map_or(WaitingOn::Guardians, WaitingOn::Council),
                        age: now.duration_since(asked),
                    },
                    ask,
                }),
                None => TimelineItem::Invalid(Invalid::new(
                    ask.comment,
                    format!("open pm:ask created at {:?}, not an RFC 3339 time", ask.comment.created_at),
                )),
            },
        })
        .collect()
}

/// Settles the ask `ask` with `by`, or records why `by` settles nothing.
fn settle<'a>(entries: &mut Vec<Entry<'a>>, asks: &HashMap<u64, usize>, ask: u64, by: Settlement<'a>) {
    let reason = match asks.get(&ask).map(|&at| &mut entries[at]) {
        Some(Entry::Ask {
            settlement: settlement @ None,
            ..
        }) => {
            *settlement = Some(by);
            return;
        }
        Some(Entry::Ask {
            settlement: Some(first),
            ..
        }) => format!(
            "second settlement of ask {ask} (first: {})",
            first.comment().html_url
        ),
        Some(Entry::Done(_)) | None => no_ask(ask),
    };
    let comment = by.comment();
    entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(comment, reason))));
}

/// Hands the ask to the council, or records why `escalated` settles nothing.
fn escalate<'a>(
    entries: &mut Vec<Entry<'a>>,
    asks: &HashMap<u64, usize>,
    escalated: Posted<'a, Escalated>,
) {
    let ask = escalated.marker.ask;
    let reason = match asks.get(&ask).map(|&at| &mut entries[at]) {
        Some(Entry::Ask {
            settlement: Some(first),
            ..
        }) => format!(
            "second settlement of ask {ask} (first: {})",
            first.comment().html_url
        ),
        Some(Entry::Ask {
            escalated: Some(first),
            ..
        }) => format!(
            "second settlement of ask {ask} (first: {})",
            first.comment.html_url
        ),
        Some(Entry::Ask {
            escalated: slot @ None,
            settlement: None,
            ..
        }) => {
            *slot = Some(escalated);
            return;
        }
        Some(Entry::Done(_)) | None => no_ask(ask),
    };
    let comment = escalated.comment;
    entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(comment, reason))));
}

fn no_ask(ask: u64) -> String {
    format!("settles ask {ask}, which is no pm:ask on this issue")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gh::IssueRef;
    use crate::gh::markers::{Choice, Outcome};

    /// Real comment JSON (`gh api repos/<repo>/issues/<n>/comments`), and
    /// hand-built fixtures (`*-handbuilt-*`) that copy a real comment's
    /// envelope.
    fn fixture(json: &str) -> Vec<Comment> {
        serde_json::from_str(json).expect("fixture is comment JSON")
    }

    macro_rules! comments {
        ($name:literal) => {
            fixture(include_str!(concat!("../tests/fixtures/", $name)))
        };
    }

    fn issue_ref(text: &str) -> IssueRef {
        IssueRef::try_from(text.to_owned()).unwrap()
    }

    /// An issue whose body carries no marker, read as a comment.
    fn plain_issue(url: &str) -> Comment {
        Comment {
            id: 1,
            body: "实现说明。".to_owned(),
            html_url: url.to_owned(),
            created_at: "2026-11-06T00:00:00Z".to_owned(),
        }
    }

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn seats<'v>(version: &'v Version<'_>) -> Vec<&'v str> {
        version.ballots.iter().map(|b| b.seat.as_str()).collect()
    }

    fn cell<'v>(version: &'v Version, seat: &str, row: &str) -> Option<&'v VoteRow> {
        let at = version.draft.marker.rows.iter().position(|r| r.id == row)?;
        let ballot = version.ballots.iter().find(|b| b.seat == seat)?;
        ballot.cells[at].as_ref()
    }

    fn reasons<'r>(invalid: &'r [Invalid]) -> Vec<&'r str> {
        invalid.iter().map(|i| i.reason.as_str()).collect()
    }

    #[test]
    fn prose_drops_the_marker_line() {
        assert_eq!(prose("答复。\n\n<!-- pm:reply {} -->\n"), "答复。");
        assert_eq!(prose("<!-- pm:reply {} -->"), "");
        assert_eq!(prose(" plain\ntext \n"), "plain\ntext");
    }

    // agenda #38: draft v1 opposed with a counterexample, round 2 revises.
    #[test]
    fn revision_round_splits_versions_by_round() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(rounds.iter().map(|r| r.number).collect::<Vec<_>>(), [1, 2]);
        for round in &rounds {
            assert!(round.invalid.is_empty(), "{:?}", round.invalid);
            assert!(matches!(
                round.events.as_slice(),
                [RoundEvent::Panel(p)] if p.marker.round == round.number
            ));
        }

        let v1 = &rounds[0].versions[&1];
        assert_eq!(rounds[0].versions.len(), 1);
        assert_eq!(seats(v1), ["Coordination", "ClaudeHigh", "Argon", "LeadMentor"]);
        let opposed = cell(v1, "ClaudeHigh", "r1").unwrap();
        assert_eq!(opposed.choice, Choice::Oppose);
        assert!(opposed.counterexample.as_deref().unwrap().starts_with("notes list --limit 0"));
        assert_eq!(cell(v1, "Argon", "r1").unwrap().choice, Choice::Approve);
        assert!(v1.ruling.is_none());

        let v2 = &rounds[1].versions[&2];
        assert_eq!(rounds[1].versions.len(), 1);
        assert_eq!(seats(v2), ["Argon", "ClaudeHigh", "Coordination", "LeadMentor"]);
        for ballot in &v2.ballots {
            let vote = ballot.cells[0].as_ref().unwrap();
            assert_eq!(vote.choice, Choice::Approve);
            assert!(vote.classification.as_deref().unwrap().starts_with("技术"));
            assert!(!ballot.reason().is_empty() && !ballot.reason().contains("<!-- pm:"));
        }
    }

    #[test]
    fn decision_of_a_revised_issue_is_the_last_version() {
        let comments = comments!("agenda-38-comments.json");
        let view = decision_view(&comments).unwrap();
        assert!(view.invalid.is_empty());
        let decision = &view.decision.marker;
        assert_eq!(decision.version, 2);
        assert_eq!(decision.rows.len(), 1);
        let row = &decision.rows[0];
        assert_eq!((row.row.as_str(), row.outcome), ("r1", Outcome::Passed));
        assert_eq!((row.approve, row.threshold), (4, 3));
        assert_eq!(decision.implementations, [issue_ref("oh-my-omp/pm-e2e-code#35")]);
        assert!(decision.superseded.is_empty());
        assert!(view.decision.text().starts_with("决策表第 2 版定案。"));
        assert!(view.decision.text().ends_with("实现 issue：oh-my-omp/pm-e2e-code#35"));
    }

    // agenda #44: a veto citing a human issue, then a ruling on the category.
    #[test]
    fn ruling_marks_its_version_and_the_veto_keeps_its_citation() {
        let comments = comments!("agenda-44-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(rounds.len(), 1);
        assert!(rounds[0].invalid.is_empty());
        let v1 = &rounds[0].versions[&1];
        let veto = cell(v1, "Argon", "r1").unwrap();
        assert_eq!(veto.choice, Choice::Veto);
        assert_eq!(veto.citation.as_deref(), Some("oh-my-omp/pm-e2e-agenda#18"));
        assert_eq!(veto.classification.as_deref(), Some("意图"));
        assert_eq!(v1.ruling.as_ref().unwrap().marker.seat, "LeadMentor");
        assert_eq!(v1.ruled("r1"), Some("意图"));
        assert_eq!(v1.ruled("r2"), None);

        let decision = decision_view(&comments).unwrap().decision.marker;
        let row = &decision.rows[0];
        assert_eq!(row.outcome, Outcome::Rejected);
        assert_eq!((row.approve, row.threshold), (3, 3));
        assert!(decision.implementations.is_empty());
    }

    // No real issue has both a revision round and a ruling yet: #38 with a
    // ruling on version 2 copied from #44's ruling envelope.
    #[test]
    fn ruling_in_a_revision_round_lands_on_that_round() {
        let comments = comments!("agenda-38-handbuilt-revision-ruling-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(rounds.len(), 2);
        assert!(rounds[0].versions[&1].ruling.is_none());
        assert_eq!(rounds[1].versions[&2].ruled("r1"), Some("技术"));
        assert!(rounds.iter().all(|r| r.invalid.is_empty()));
    }

    // agenda #40 (human issue H) supersedes #39 (old issue O).
    #[test]
    fn human_issue_lists_the_issue_it_supersedes() {
        let comments = comments!("agenda-40-comments.json");
        let old = issue_ref("oh-my-omp/pm-e2e-agenda#39");
        let rounds = panel_history(&comments);
        assert_eq!(rounds[0].versions[&1].draft.marker.supersedes, std::slice::from_ref(&old));
        let decision = decision_view(&comments).unwrap().decision.marker;
        assert_eq!(decision.superseded, [old]);
        assert!(decision.implementations.is_empty());
    }

    #[test]
    fn superseded_issue_shows_who_changed_it() {
        let comments = comments!("agenda-39-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(rounds.len(), 1);
        match rounds[0].events.as_slice() {
            [RoundEvent::Superseded(superseded), RoundEvent::Panel(_)] => {
                assert_eq!(superseded.marker.by, issue_ref("oh-my-omp/pm-e2e-agenda#40"));
            }
            other => panic!("{other:?}"),
        }
        assert!(rounds[0].versions.is_empty());
        assert_eq!(decision_view(&comments), None);
    }

    // #44 with a second vote of one seat, an unknown choice, a vote on a
    // version without a draft, an unknown kind and a second decision.
    #[test]
    fn panel_invalid_markers_are_kept_with_their_reason() {
        let comments = comments!("agenda-44-handbuilt-invalid-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(rounds.len(), 1);
        let invalid = reasons(&rounds[0].invalid);
        assert_eq!(invalid.len(), 4, "{invalid:?}");
        assert!(invalid[0].starts_with("second pm:vote of seat Coordination on version 1 (first: https://github.com/oh-my-omp/pm-e2e-agenda/issues/44#issuecomment-6703721691)"));
        assert!(invalid[1].starts_with("pm:vote: unknown variant `abstain`"));
        assert_eq!(invalid[2], "pm:vote of seat Argon on version 7, which has no draft");
        assert!(invalid[3].starts_with("pm:tally: unknown kind"));
        assert_eq!(
            rounds[0].invalid[0].comment_url,
            "https://github.com/oh-my-omp/pm-e2e-agenda/issues/44#issuecomment-6703721700"
        );
        let v1 = &rounds[0].versions[&1];
        assert_eq!(seats(v1), ["Coordination", "ClaudeHigh", "Argon", "LeadMentor"]);
        assert_eq!(cell(v1, "Coordination", "r1").unwrap().choice, Choice::Approve);

        let view = decision_view(&comments).unwrap();
        assert_eq!(
            view.decision.comment.html_url,
            "https://github.com/oh-my-omp/pm-e2e-agenda/issues/44#issuecomment-6703758516"
        );
        assert_eq!(reasons(&view.invalid), ["second pm:decision (first: https://github.com/oh-my-omp/pm-e2e-agenda/issues/44#issuecomment-6703758516)"]);
    }

    fn settled<'t>(item: &'t TimelineItem) -> (&'t AskThread<'t>, Option<&'t Escalated>, &'t Settlement<'t>) {
        match item {
            TimelineItem::Ask(thread) => match &thread.progress {
                AskProgress::Settled { escalated, settlement } => {
                    (thread, escalated.as_ref().map(|e| &e.marker), settlement)
                }
                open => panic!("{open:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    // code #20: impl → ask → reply → impl → ask → reply → impl.
    #[test]
    fn replies_pair_with_their_asks_between_impls() {
        let comments = comments!("code-20-comments.json");
        let issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/20");
        let items = ask_timeline(&issue, &comments, at("2026-11-07T01:00:00Z"));
        assert_eq!(items.len(), 5, "{items:?}");
        for (i, item) in items.iter().enumerate() {
            match (i % 2, item) {
                (0, TimelineItem::Impl(posted)) => assert_eq!(posted.comment.id, comments[i + i / 2].id),
                (1, item) => {
                    let (thread, escalated, settlement) = settled(item);
                    assert!(escalated.is_none());
                    let Settlement::Reply(reply) = settlement else { panic!("{settlement:?}") };
                    assert_eq!(reply.marker.ask, thread.ask.comment.id);
                    assert_eq!(reply.marker.decision, None);
                }
                (_, other) => panic!("{i}: {other:?}"),
            }
        }
        let TimelineItem::Impl(last) = &items[4] else { unreachable!() };
        assert_eq!(last.marker.session, "01a1b372-a9db-70fb-aae8-98da66ed931f");
        assert_eq!(last.marker.pid, 35853);
    }

    // code #3: two guardians replies, then three asks escalated and
    // settled by the council's reply.
    #[test]
    fn escalated_asks_carry_the_council_settlement() {
        let comments = comments!("code-3-comments.json");
        let issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/3");
        let items = ask_timeline(&issue, &comments, at("2026-11-07T08:00:00Z"));
        assert_eq!(items.len(), 5, "{items:?}");
        for item in &items[..2] {
            assert!(settled(item).1.is_none());
        }
        for (item, agenda) in items[2..].iter().zip([31, 36, 43]) {
            let agenda = issue_ref(&format!("oh-my-omp/pm-e2e-agenda#{agenda}"));
            let (thread, escalated, settlement) = settled(item);
            let escalated = escalated.unwrap();
            assert_eq!((escalated.ask, &escalated.agenda), (thread.ask.comment.id, &agenda));
            let Settlement::Reply(reply) = settlement else { panic!("{settlement:?}") };
            assert_eq!(reply.marker.ask, thread.ask.comment.id);
            assert_eq!(reply.marker.decision.as_ref(), Some(&agenda));
        }
    }

    // code #16: escalated, the council has not settled it.
    #[test]
    fn escalated_open_ask_waits_on_the_council() {
        let comments = comments!("code-16-comments.json");
        let issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/16");
        let items = ask_timeline(&issue, &comments, at("2026-11-07T01:17:25Z"));
        let [TimelineItem::Ask(thread)] = items.as_slice() else { panic!("{items:?}") };
        let AskProgress::Open { waiting_on: WaitingOn::Council(escalated), age } = &thread.progress else {
            panic!("{thread:?}")
        };
        assert_eq!(escalated.marker.agenda, issue_ref("oh-my-omp/pm-e2e-agenda#13"));
        assert_eq!(escalated.marker.ask, thread.ask.comment.id);
        assert_eq!(*age, SignedDuration::from_mins(90));
    }

    // code #23: an ask nobody settled.
    #[test]
    fn unsettled_ask_waits_on_the_guardians() {
        let comments = comments!("code-23-comments.json");
        let issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/23");
        let asked = at(&comments[0].created_at);
        let now = asked.checked_add(SignedDuration::from_secs(42)).unwrap();
        let items = ask_timeline(&issue, &comments, now);
        let [TimelineItem::Ask(thread)] = items.as_slice() else { panic!("{items:?}") };
        assert_eq!(
            thread.progress,
            AskProgress::Open { waiting_on: WaitingOn::Guardians, age: SignedDuration::from_secs(42) }
        );
    }

    // code #12 replaced by #13; #13's body replaces #12.
    #[test]
    fn replaced_and_replaces_point_at_each_other() {
        let old_comments = comments!("code-12-comments.json");
        let old_issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/12");
        let new_issue: Comment =
            serde_json::from_str(include_str!("../tests/fixtures/code-13-issue.json")).unwrap();
        let now = at("2026-11-07T08:00:00Z");

        let old = ask_timeline(&old_issue, &old_comments, now);
        let [item] = old.as_slice() else { panic!("{old:?}") };
        let (thread, escalated, settlement) = settled(item);
        assert!(escalated.is_none());
        let Settlement::Replaced(replaced) = settlement else { panic!("{settlement:?}") };
        assert_eq!(replaced.marker.by, issue_ref("oh-my-omp/pm-e2e-code#13"));
        assert_eq!(replaced.marker.ask, Some(thread.ask.comment.id));

        let new = ask_timeline(&new_issue, &[], now);
        let [TimelineItem::Replaces(replaces)] = new.as_slice() else { panic!("{new:?}") };
        assert_eq!(replaces.marker.old, issue_ref("oh-my-omp/pm-e2e-code#12"));
        assert_eq!(replaces.marker.ask, Some(thread.ask.comment.id));
        assert_eq!(replaces.comment.html_url, "https://github.com/oh-my-omp/pm-e2e-code/issues/13");
    }

    // code #14 (ask, reply) followed by a second reply, an escalation after
    // the reply, a reply to no ask, a bad field, an unknown kind and a line
    // that is no marker JSON.
    #[test]
    fn timeline_invalid_markers_are_kept_with_their_reason() {
        let comments = comments!("code-14-handbuilt-invalid-comments.json");
        let issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/14");
        let items = ask_timeline(&issue, &comments, at("2026-11-07T08:00:00Z"));
        assert_eq!(items.len(), 7, "{items:?}");
        let (_, _, settlement) = settled(&items[0]);
        assert_eq!(settlement.comment().id, comments[1].id);
        let invalid: Vec<&Invalid> = items[1..]
            .iter()
            .map(|item| match item {
                TimelineItem::Invalid(invalid) => invalid,
                other => panic!("{other:?}"),
            })
            .collect();
        let first = "https://github.com/oh-my-omp/pm-e2e-code/issues/14#issuecomment-6698655483";
        let second = format!("second settlement of ask 6698636994 (first: {first})");
        assert_eq!(invalid[0].reason, second);
        assert_eq!(invalid[1].reason, second);
        assert_eq!(invalid[2].reason, "settles ask 1234, which is no pm:ask on this issue");
        assert!(invalid[3].reason.starts_with("pm:reply: invalid type: string"), "{}", invalid[3].reason);
        assert!(invalid[4].reason.starts_with("pm:nudge: unknown kind"));
        assert!(invalid[5].reason.starts_with("pm:ask: "));
        for (invalid, comment) in invalid.iter().zip(&comments[2..]) {
            assert_eq!(invalid.comment_url, comment.html_url);
        }
    }

    #[test]
    fn replaces_in_a_comment_and_a_bad_body_marker_are_invalid() {
        let mut issue = plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/13");
        issue.body = "正文\n\n<!-- pm:replaces {\"v\":1,\"old\":\"bad\",\"ask\":null} -->".to_owned();
        let comments = [Comment {
            body: "<!-- pm:replaces {\"v\":1,\"old\":\"o/c#1\",\"ask\":null} -->".to_owned(),
            ..plain_issue("https://github.com/oh-my-omp/pm-e2e-code/issues/13#issuecomment-2")
        }];
        let items = ask_timeline(&issue, &comments, at("2026-11-07T08:00:00Z"));
        let [TimelineItem::Invalid(body), TimelineItem::Invalid(comment)] = items.as_slice() else {
            panic!("{items:?}")
        };
        assert!(body.reason.starts_with("pm:replaces: "));
        assert_eq!(comment.reason, "pm:replaces belongs in the issue body, not a comment");
    }
}
