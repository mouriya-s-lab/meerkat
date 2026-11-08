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
    IssueRef,
    comments::Comment,
    markers::{
        self, Ask, Choice, Decision, Draft, DraftRow, Escalated, Impl, Marker, Panel, PanelEvent,
        Replaced, Replaces, Reply, Ruling, Superseded, VoteRow,
    },
};
use crate::{
    programs::{Session, SessionState},
    time,
};

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

/// An issue's own text: its page, its body and when it was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IssueText<'a> {
    pub url: &'a str,
    pub body: &'a str,
    pub created_at: &'a str,
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
                            format!(
                                "pm:vote of seat {} votes twice on row {}",
                                vote.seat, cast.row
                            ),
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
                    let reason = format!(
                        "pm:ruling on version {}, which has no draft",
                        ruling.version
                    );
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
    Replaces {
        issue: IssueText<'a>,
        marker: Replaces,
    },
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
/// `escalated`, the council's `reply` or `replaced` of the same ask.
pub fn ask_timeline<'a>(
    issue: IssueText<'a>,
    comments: &'a [Comment],
    now: Timestamp,
) -> Vec<TimelineItem<'a>> {
    let mut entries: Vec<Entry> = Vec::new();
    match markers::read(issue.body) {
        Some(Ok(Marker::Replaces(marker))) => {
            entries.push(Entry::Done(TimelineItem::Replaces { issue, marker }));
        }
        Some(Err(reason)) => {
            entries.push(Entry::Done(TimelineItem::Invalid(Invalid {
                comment_url: issue.url,
                reason,
            })));
        }
        Some(Ok(_)) | None => {}
    }
    let mut asks: HashMap<u64, usize> = HashMap::new();
    for (comment, read) in read_all(comments) {
        let marker = match read {
            Ok(marker) => marker,
            Err(reason) => {
                entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(
                    comment, reason,
                ))));
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
                    entries.push(Entry::Done(TimelineItem::Replaced(Posted {
                        comment,
                        marker,
                    })));
                }
            },
            Marker::Escalated(marker) => {
                escalate(&mut entries, &asks, Posted { comment, marker });
            }
            Marker::Replaces(_) => {
                let reason = "pm:replaces belongs in the issue body, not a comment".to_owned();
                entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(
                    comment, reason,
                ))));
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
                    format!(
                        "open pm:ask created at {:?}, not an RFC 3339 time",
                        ask.comment.created_at
                    ),
                )),
            },
        })
        .collect()
}

/// Settles the ask `ask` with `by`, or records why `by` settles nothing.
fn settle<'a>(
    entries: &mut Vec<Entry<'a>>,
    asks: &HashMap<u64, usize>,
    ask: u64,
    by: Settlement<'a>,
) {
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
    entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(
        comment, reason,
    ))));
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
    entries.push(Entry::Done(TimelineItem::Invalid(Invalid::new(
        comment, reason,
    ))));
}

fn no_ask(ask: u64) -> String {
    format!("settles ask {ask}, which is no pm:ask on this issue")
}

/// An escalated ask still waiting for the council.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenEscalation {
    /// The implementation issue.
    pub issue: IssueRef,
    /// The `ask` comment's id.
    pub ask: u64,
    /// The agenda issue it became.
    pub agenda: IssueRef,
    /// The `escalated` comment.
    pub url: String,
    pub created_at: String,
}

/// The asks of `issue`'s timeline that wait on the council.
pub fn open_escalations(issue: &IssueRef, timeline: &[TimelineItem<'_>]) -> Vec<OpenEscalation> {
    timeline
        .iter()
        .filter_map(|item| match item {
            TimelineItem::Ask(AskThread {
                progress:
                    AskProgress::Open {
                        waiting_on: WaitingOn::Council(escalated),
                        ..
                    },
                ..
            }) => Some(OpenEscalation {
                issue: issue.clone(),
                ask: escalated.marker.ask,
                agenda: escalated.marker.agenda.clone(),
                url: escalated.comment.html_url.clone(),
                created_at: escalated.comment.created_at.clone(),
            }),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// What the pages show (#9 「详情与实现页的版式」)

/// A cell of the vote matrix, as its glyph tells it: a veto, an objection
/// with a counterexample, one without, an approval, or no vote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stance {
    Approve,
    Oppose,
    Veto,
    Counterexample,
    Pending,
}

impl Stance {
    pub fn of(cell: Option<&VoteRow>) -> Self {
        match cell {
            None => Stance::Pending,
            Some(VoteRow {
                choice: Choice::Veto,
                ..
            }) => Stance::Veto,
            Some(VoteRow {
                choice: Choice::Oppose,
                counterexample: Some(_),
                ..
            }) => Stance::Counterexample,
            Some(VoteRow {
                choice: Choice::Oppose,
                counterexample: None,
                ..
            }) => Stance::Oppose,
            Some(VoteRow {
                choice: Choice::Approve,
                ..
            }) => Stance::Approve,
        }
    }
}

/// The votes on one row of a version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowTally {
    pub approve: usize,
    pub oppose: usize,
    pub veto: usize,
    pub counterexample: usize,
    /// The seats that voted on it.
    pub voted: usize,
}

impl Version<'_> {
    /// The votes on the draft's row `at`.
    pub fn tally(&self, at: usize) -> RowTally {
        let mut tally = RowTally::default();
        for ballot in &self.ballots {
            match Stance::of(ballot.cells.get(at).and_then(Option::as_ref)) {
                Stance::Approve => tally.approve += 1,
                Stance::Oppose => tally.oppose += 1,
                Stance::Veto => tally.veto += 1,
                Stance::Counterexample => tally.counterexample += 1,
                Stance::Pending => continue,
            }
            tally.voted += 1;
        }
        tally
    }

    /// The seats that objected to row `id` (an objection, a
    /// counterexample or a veto), in vote order.
    pub fn objectors(&self, id: &str) -> Vec<&str> {
        let Some(at) = self.draft.marker.rows.iter().position(|row| row.id == id) else {
            return Vec::new();
        };
        self.ballots
            .iter()
            .filter(|ballot| {
                !matches!(
                    Stance::of(ballot.cells.get(at).and_then(Option::as_ref)),
                    Stance::Approve | Stance::Pending
                )
            })
            .map(|ballot| ballot.seat.as_str())
            .collect()
    }
}

/// The versions of every round in version order, with their round.
pub fn versions<'r, 'a>(rounds: &'r [Round<'a>]) -> Vec<(u32, &'r Version<'a>)> {
    let mut all: Vec<(u32, &Version)> = rounds
        .iter()
        .flat_map(|round| {
            round
                .versions
                .values()
                .map(move |version| (round.number, version))
        })
        .collect();
    all.sort_by_key(|(_, version)| version.draft.marker.version);
    all
}

/// A step of the round track: a draft version in its round, or the
/// revision from one version to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Draft { round: u32, version: u32 },
    Revision { from: u32, to: u32 },
}

impl Step {
    /// The version whose votes the matrix shows for this step: a
    /// revision shows the votes that called for it.
    pub fn version(self) -> u32 {
        match self {
            Step::Draft { version, .. } => version,
            Step::Revision { from, .. } => from,
        }
    }

    /// The version whose change from the one before it shows under the
    /// matrix.
    pub fn changed(self) -> u32 {
        match self {
            Step::Draft { version, .. } => version,
            Step::Revision { to, .. } => to,
        }
    }
}

/// The round track: each version, a revision step before every version
/// after the first.
pub fn steps(rounds: &[Round<'_>]) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut last: Option<u32> = None;
    for (round, version) in versions(rounds) {
        let version = version.draft.marker.version;
        if let Some(from) = last {
            steps.push(Step::Revision { from, to: version });
        }
        steps.push(Step::Draft { round, version });
        last = Some(version);
    }
    steps
}

/// How a draft's row differs from the version before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RowChange<'a> {
    Same(&'a DraftRow),
    Added(&'a DraftRow),
    Removed(&'a DraftRow),
    Changed {
        old: &'a DraftRow,
        new: &'a DraftRow,
    },
}

/// The rows of `new` against `old`, by row id: `new`'s rows in its order,
/// then the rows only `old` had.
pub fn draft_changes<'a>(old: &'a Draft, new: &'a Draft) -> Vec<RowChange<'a>> {
    let mut changes: Vec<RowChange> = new
        .rows
        .iter()
        .map(|row| match old.rows.iter().find(|o| o.id == row.id) {
            Some(o) if o == row => RowChange::Same(row),
            Some(o) => RowChange::Changed { old: o, new: row },
            None => RowChange::Added(row),
        })
        .collect();
    changes.extend(
        old.rows
            .iter()
            .filter(|o| new.rows.iter().all(|row| row.id != o.id))
            .map(RowChange::Removed),
    );
    changes
}

/// When each round began: its first `started` or `resumed` panel event,
/// in Unix milliseconds.
fn round_starts(rounds: &[Round<'_>]) -> Vec<(u32, f64)> {
    rounds
        .iter()
        .filter_map(|round| {
            let start = round.events.iter().find_map(|event| match event {
                RoundEvent::Panel(panel)
                    if matches!(
                        panel.marker.event,
                        PanelEvent::Started | PanelEvent::Resumed
                    ) =>
                {
                    time::parse_ms(&panel.comment.created_at)
                }
                RoundEvent::Panel(_) | RoundEvent::Superseded(_) => None,
            })?;
            Some((round.number, start))
        })
        .collect()
}

/// The round a session started in, `started_ms` its start: the last round
/// that began at or before it; the first round for one before every
/// round's start, and when no round has begun.
pub fn round_of(rounds: &[Round<'_>], started_ms: f64) -> u32 {
    let starts = round_starts(rounds);
    starts
        .iter()
        .rev()
        .find(|(_, start)| *start <= started_ms)
        .or(starts.first())
        .map_or(1, |(round, _)| *round)
}

/// How a round's panel stopped short of a decision (§A `panel` events
/// other than `started` and `resumed`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Interrupted,
    Blocked,
    Failed,
}

/// A round whose panel stopped and has not resumed since: its last
/// `pm:panel` is `interrupted`, `blocked` or `failed`.
pub fn stopped<'r, 'a>(round: &'r Round<'a>) -> Option<(Stop, &'r Posted<'a, Panel>)> {
    let panel = round.events.iter().rev().find_map(|event| match event {
        RoundEvent::Panel(panel) => Some(panel),
        RoundEvent::Superseded(_) => None,
    })?;
    let stop = match panel.marker.event {
        PanelEvent::Started | PanelEvent::Resumed => return None,
        PanelEvent::Interrupted => Stop::Interrupted,
        PanelEvent::Blocked => Stop::Blocked,
        PanelEvent::Failed => Stop::Failed,
    };
    Some((stop, panel))
}

/// What an implementation issue waits for (#9 「实现」 groups, in order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ImplGroup {
    /// An ask has no settlement yet.
    Waiting,
    /// A worker started on it.
    Running,
    /// Nobody started on it.
    Todo,
    /// Done, or superseded.
    Done,
}

impl ImplGroup {
    pub const ALL: [ImplGroup; 4] = [
        ImplGroup::Waiting,
        ImplGroup::Running,
        ImplGroup::Todo,
        ImplGroup::Done,
    ];
}

/// An implementation issue as the list shows it: its group, its latest
/// worker session and its oldest open ask.
#[derive(Debug, Clone, PartialEq)]
pub struct ImplSummary {
    pub group: ImplGroup,
    /// The latest `impl`: its session and when it was posted.
    pub last_impl: Option<(String, String)>,
    /// The oldest open ask: who it waits on (the agenda issue for the
    /// council) and since when.
    pub open_ask: Option<OpenAsk>,
}

/// An open ask as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAsk {
    pub council: Option<IssueRef>,
    pub since: String,
}

/// The summary of an implementation issue: `closed` when it is done or
/// superseded; `timeline` when its comments are read.
pub fn impl_summary(
    closed: bool,
    in_progress: bool,
    timeline: Option<&[TimelineItem<'_>]>,
) -> ImplSummary {
    let items = timeline.unwrap_or_default();
    let last_impl = items.iter().rev().find_map(|item| match item {
        TimelineItem::Impl(posted) => Some((
            posted.marker.session.clone(),
            posted.comment.created_at.clone(),
        )),
        _ => None,
    });
    let open_ask = items.iter().find_map(|item| match item {
        TimelineItem::Ask(AskThread {
            ask,
            progress: AskProgress::Open { waiting_on, .. },
        }) => Some(OpenAsk {
            council: match waiting_on {
                WaitingOn::Guardians => None,
                WaitingOn::Council(escalated) => Some(escalated.marker.agenda.clone()),
            },
            since: ask.comment.created_at.clone(),
        }),
        _ => None,
    });
    let group = if closed {
        ImplGroup::Done
    } else if open_ask.is_some() {
        ImplGroup::Waiting
    } else if in_progress || last_impl.is_some() {
        ImplGroup::Running
    } else {
        ImplGroup::Todo
    };
    ImplSummary {
        group,
        last_impl,
        open_ask,
    }
}

// ---------------------------------------------------------------------------
// The council's sessions on an issue (#4 「审议会话与议题的关联」)

/// The seat a council session sat for: its label is `<kind> <seat> <ref>`
/// (meerkat-council `internal/sessions` `Summary.Label`); none for a label
/// without a seat.
pub fn council_seat(session: &Session) -> Option<&str> {
    let mut words = session.label.split_whitespace();
    if words.next()? != session.kind {
        return None;
    }
    let seat = words.next()?;
    let is_ref = session
        .reference
        .as_ref()
        .is_some_and(|issue| issue.to_string() == seat);
    (!is_ref).then_some(seat)
}

/// The council's sessions on `issue` by the round each started in (see
/// [`round_of`]), each round's drafts before its votes, then by start.
pub fn round_sessions<'s>(
    rounds: &[Round<'_>],
    issue: &IssueRef,
    sessions: &'s [Session],
) -> BTreeMap<u32, Vec<&'s Session>> {
    let mut by_round: BTreeMap<u32, Vec<&Session>> = BTreeMap::new();
    for session in sessions
        .iter()
        .filter(|session| session.reference.as_ref() == Some(issue))
    {
        // A start the council wrote unreadably counts as the latest.
        let start = time::parse_ms(&session.started_at).unwrap_or(f64::INFINITY);
        by_round
            .entry(round_of(rounds, start))
            .or_default()
            .push(session);
    }
    for round in by_round.values_mut() {
        round.sort_by(|a, b| {
            (a.kind != "draft", &a.kind, &a.started_at).cmp(&(
                b.kind != "draft",
                &b.kind,
                &b.started_at,
            ))
        });
    }
    by_round
}

/// What a seat's column of the vote matrix says under its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeatState {
    /// Its ballot is in.
    Voted,
    /// Its vote session runs.
    Running,
    /// Its last vote session failed and no ballot came.
    Failed,
    /// No ballot yet, nothing running.
    Pending,
}

/// A seat's column of a version's vote matrix.
#[derive(Debug, Clone, PartialEq)]
pub struct SeatColumn<'r, 'a> {
    pub seat: &'r str,
    pub ballot: Option<&'r Ballot<'a>>,
    /// The seat's latest vote session in the version's round.
    pub session: Option<&'r Session>,
    pub state: SeatState,
}

/// The seats of `version`'s matrix: those that voted, in vote order; then
/// those with a vote session in its round (`sessions`, as
/// [`round_sessions`] gives a round's), then the `pending` ones the
/// council's queue lists for it, each once.
pub fn seat_columns<'r, 'a>(
    version: &'r Version<'a>,
    sessions: &[&'r Session],
    pending: &'r [String],
) -> Vec<SeatColumn<'r, 'a>> {
    let mut seats: Vec<&'r str> = version
        .ballots
        .iter()
        .map(|ballot| ballot.seat.as_str())
        .collect();
    let votes = sessions
        .iter()
        .copied()
        .filter(|session| session.kind == "vote");
    for seat in votes
        .clone()
        .filter_map(council_seat)
        .chain(pending.iter().map(String::as_str))
    {
        if !seats.contains(&seat) {
            seats.push(seat);
        }
    }
    seats
        .into_iter()
        .map(|seat| {
            let ballot = version.ballots.iter().find(|ballot| ballot.seat == seat);
            let session = votes
                .clone()
                .filter(|session| council_seat(session) == Some(seat))
                .max_by(|a, b| a.started_at.cmp(&b.started_at));
            let state = match (ballot, session.map(|session| session.state)) {
                (Some(_), _) => SeatState::Voted,
                (None, Some(SessionState::Running)) => SeatState::Running,
                (None, Some(SessionState::Failed)) => SeatState::Failed,
                (None, Some(SessionState::Done) | None) => SeatState::Pending,
            };
            SeatColumn {
                seat,
                ballot,
                session,
                state,
            }
        })
        .collect()
}

impl SeatColumn<'_, '_> {
    /// The seat's vote on the draft's row `at`.
    pub fn cell(&self, at: usize) -> Option<&VoteRow> {
        self.ballot?.cells.get(at)?.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gh::markers::Outcome;

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

    /// An issue whose body carries no marker.
    fn plain_issue(url: &str) -> IssueText<'_> {
        IssueText {
            url,
            body: "实现说明。",
            created_at: "2026-11-06T00:00:00Z",
        }
    }

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    /// A hand-built comment posted `minute` minutes after 01:00.
    fn posted(id: u64, minute: u32, body: &str) -> Comment {
        Comment {
            id,
            body: body.to_owned(),
            html_url: format!("u{id}"),
            created_at: format!("2026-11-07T01:{minute:02}:00Z"),
        }
    }

    fn ask(id: u64, minute: u32) -> Comment {
        posted(
            id,
            minute,
            "问题？\n\n<!-- pm:ask {\"v\":1,\"tier\":\"simple\",\"session\":\"s\"} -->",
        )
    }

    fn open_asks(comments: &[Comment]) -> Vec<u64> {
        let issue = issue_ref("o/code#3");
        let timeline = ask_timeline(
            plain_issue("https://github.com/o/code/issues/3"),
            comments,
            at("2026-11-07T02:00:00Z"),
        );
        open_escalations(&issue, &timeline)
            .iter()
            .map(|open| open.ask)
            .collect()
    }

    // The cases of the removed `gh::escalations::unsettled`, now through
    // the one derivation the overview and the 实现 page share.
    #[test]
    fn a_later_council_reply_settles_its_escalated_ask_only() {
        let comments = [
            ask(10, 0),
            ask(11, 1),
            posted(12, 2, "<!-- pm:escalated {\"v\":1,\"ask\":10,\"agenda\":\"o/pm#4\"} -->"),
            posted(13, 3, "<!-- pm:escalated {\"v\":1,\"ask\":11,\"agenda\":\"o/pm#5\"} -->"),
            posted(14, 4, "<!-- pm:reply {\"v\":1,\"ask\":10,\"decision\":\"o/pm#4\"} -->"),
        ];
        assert_eq!(open_asks(&comments), vec![11]);
        let issue = issue_ref("o/code#3");
        let timeline = ask_timeline(
            plain_issue("https://github.com/o/code/issues/3"),
            &comments,
            at("2026-11-07T02:00:00Z"),
        );
        let open = open_escalations(&issue, &timeline);
        assert_eq!(open[0].url, "u13");
        assert_eq!(open[0].agenda, issue_ref("o/pm#5"));
    }

    #[test]
    fn a_later_replaced_settles_its_escalated_ask() {
        let comments = [
            ask(10, 0),
            posted(12, 2, "<!-- pm:escalated {\"v\":1,\"ask\":10,\"agenda\":\"o/pm#4\"} -->"),
            posted(14, 4, "<!-- pm:replaced {\"v\":1,\"ask\":10,\"by\":\"o/code#9\"} -->"),
        ];
        assert!(open_asks(&comments).is_empty());
    }

    // Where the shared derivation is stricter than `unsettled` was: an
    // `escalated` naming no `pm:ask` of the issue (§2) waits on nothing; it
    // is an invalid marker on the timeline instead.
    #[test]
    fn an_escalated_without_its_ask_is_invalid_not_open() {
        let comments = [posted(
            12,
            2,
            "<!-- pm:escalated {\"v\":1,\"ask\":10,\"agenda\":\"o/pm#4\"} -->",
        )];
        assert!(open_asks(&comments).is_empty());
        let timeline = ask_timeline(
            plain_issue("https://github.com/o/code/issues/3"),
            &comments,
            at("2026-11-07T02:00:00Z"),
        );
        assert!(
            timeline
                .iter()
                .any(|item| matches!(item, TimelineItem::Invalid(_)))
        );
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
        assert_eq!(
            seats(v1),
            ["Coordination", "ClaudeHigh", "Argon", "LeadMentor"]
        );
        let opposed = cell(v1, "ClaudeHigh", "r1").unwrap();
        assert_eq!(opposed.choice, Choice::Oppose);
        assert!(
            opposed
                .counterexample
                .as_deref()
                .unwrap()
                .starts_with("notes list --limit 0")
        );
        assert_eq!(cell(v1, "Argon", "r1").unwrap().choice, Choice::Approve);
        assert!(v1.ruling.is_none());

        let v2 = &rounds[1].versions[&2];
        assert_eq!(rounds[1].versions.len(), 1);
        assert_eq!(
            seats(v2),
            ["Argon", "ClaudeHigh", "Coordination", "LeadMentor"]
        );
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
        assert_eq!(
            decision.implementations,
            [issue_ref("mouriya-s-lab/meerkat-e2e-code#35")]
        );
        assert!(decision.superseded.is_empty());
        assert!(view.decision.text().starts_with("决策表第 2 版定案。"));
        assert!(
            view.decision
                .text()
                .ends_with("实现 issue：mouriya-s-lab/meerkat-e2e-code#35")
        );
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
        assert_eq!(veto.citation.as_deref(), Some("mouriya-s-lab/meerkat-e2e-agenda#18"));
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
        let old = issue_ref("mouriya-s-lab/meerkat-e2e-agenda#39");
        let rounds = panel_history(&comments);
        assert_eq!(
            rounds[0].versions[&1].draft.marker.supersedes,
            std::slice::from_ref(&old)
        );
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
                assert_eq!(
                    superseded.marker.by,
                    issue_ref("mouriya-s-lab/meerkat-e2e-agenda#40")
                );
            }
            other => panic!("{other:?}"),
        }
        assert!(rounds[0].versions.is_empty());
        assert_eq!(decision_view(&comments), None);
    }

    fn panel(id: u64, minute: u32, event: &str, round: u32, error: &str) -> Comment {
        posted(
            id,
            minute,
            &format!(
                "<!-- pm:panel {{\"v\":1,\"event\":\"{event}\",\"round\":{round},\"sessions\":[],\"prereq\":null,\"error\":{error}}} -->"
            ),
        )
    }

    #[test]
    fn a_round_whose_last_panel_event_stops_it_is_stopped() {
        // Round 2 fails on publishing as meerkat-e2e-design#14's did; round 1
        // fails and resumes.
        let comments = [
            panel(1, 0, "started", 1, "null"),
            panel(
                2,
                1,
                "failed",
                1,
                "\"vote sessions ended without a goal tool\"",
            ),
            panel(3, 2, "resumed", 1, "null"),
            panel(4, 3, "started", 2, "null"),
            panel(
                5,
                4,
                "failed",
                2,
                "\"create issue in mouriya-s-lab/meerkat-e2e-code: GraphQL: Title can't be blank (createIssue)\"",
            ),
        ];
        let rounds = panel_history(&comments);
        assert_eq!(rounds.len(), 2);
        assert!(stopped(&rounds[0]).is_none());
        let (stop, posted) = stopped(&rounds[1]).expect("round 2 stopped");
        assert_eq!(stop, Stop::Failed);
        assert_eq!(posted.comment.id, 5);
        assert!(
            posted
                .marker
                .error
                .as_deref()
                .unwrap()
                .contains("Title can't be blank")
        );
    }

    #[test]
    fn interrupted_and_blocked_rounds_are_stopped_until_resumed() {
        let interrupted = [
            panel(1, 0, "started", 1, "null"),
            panel(2, 1, "interrupted", 1, "null"),
        ];
        let rounds = panel_history(&interrupted);
        assert_eq!(
            stopped(&rounds[0]).map(|(stop, _)| stop),
            Some(Stop::Interrupted)
        );
        let blocked = [panel(1, 0, "blocked", 1, "null")];
        let rounds = panel_history(&blocked);
        assert_eq!(
            stopped(&rounds[0]).map(|(stop, _)| stop),
            Some(Stop::Blocked)
        );
        let running = [panel(1, 0, "started", 1, "null")];
        assert!(stopped(&panel_history(&running)[0]).is_none());
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
        assert!(invalid[0].starts_with("second pm:vote of seat Coordination on version 1 (first: https://github.com/mouriya-s-lab/meerkat-e2e-agenda/issues/44#issuecomment-6703721691)"));
        assert!(invalid[1].starts_with("pm:vote: unknown variant `abstain`"));
        assert_eq!(
            invalid[2],
            "pm:vote of seat Argon on version 7, which has no draft"
        );
        assert!(invalid[3].starts_with("pm:tally: unknown kind"));
        assert_eq!(
            rounds[0].invalid[0].comment_url,
            "https://github.com/mouriya-s-lab/meerkat-e2e-agenda/issues/44#issuecomment-6703721700"
        );
        let v1 = &rounds[0].versions[&1];
        assert_eq!(
            seats(v1),
            ["Coordination", "ClaudeHigh", "Argon", "LeadMentor"]
        );
        assert_eq!(
            cell(v1, "Coordination", "r1").unwrap().choice,
            Choice::Approve
        );

        let view = decision_view(&comments).unwrap();
        assert_eq!(
            view.decision.comment.html_url,
            "https://github.com/mouriya-s-lab/meerkat-e2e-agenda/issues/44#issuecomment-6703758516"
        );
        assert_eq!(
            reasons(&view.invalid),
            [
                "second pm:decision (first: https://github.com/mouriya-s-lab/meerkat-e2e-agenda/issues/44#issuecomment-6703758516)"
            ]
        );
    }

    fn settled<'t>(
        item: &'t TimelineItem,
    ) -> (&'t AskThread<'t>, Option<&'t Escalated>, &'t Settlement<'t>) {
        match item {
            TimelineItem::Ask(thread) => match &thread.progress {
                AskProgress::Settled {
                    escalated,
                    settlement,
                } => (thread, escalated.as_ref().map(|e| &e.marker), settlement),
                open => panic!("{open:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    // code #20: impl → ask → reply → impl → ask → reply → impl.
    #[test]
    fn replies_pair_with_their_asks_between_impls() {
        let comments = comments!("code-20-comments.json");
        let issue = plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/20");
        let items = ask_timeline(issue, &comments, at("2026-11-07T01:00:00Z"));
        assert_eq!(items.len(), 5, "{items:?}");
        for (i, item) in items.iter().enumerate() {
            match (i % 2, item) {
                (0, TimelineItem::Impl(posted)) => {
                    assert_eq!(posted.comment.id, comments[i + i / 2].id)
                }
                (1, item) => {
                    let (thread, escalated, settlement) = settled(item);
                    assert!(escalated.is_none());
                    let Settlement::Reply(reply) = settlement else {
                        panic!("{settlement:?}")
                    };
                    assert_eq!(reply.marker.ask, thread.ask.comment.id);
                    assert_eq!(reply.marker.decision, None);
                }
                (_, other) => panic!("{i}: {other:?}"),
            }
        }
        let TimelineItem::Impl(last) = &items[4] else {
            unreachable!()
        };
        assert_eq!(last.marker.session, "01a1b372-a9db-70fb-aae8-98da66ed931f");
        assert_eq!(last.marker.pid, 35853);
    }

    // code #3: two guardians replies, then three asks escalated and
    // settled by the council's reply.
    #[test]
    fn escalated_asks_carry_the_council_settlement() {
        let comments = comments!("code-3-comments.json");
        let issue = plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/3");
        let items = ask_timeline(issue, &comments, at("2026-11-07T08:00:00Z"));
        assert_eq!(items.len(), 5, "{items:?}");
        for item in &items[..2] {
            assert!(settled(item).1.is_none());
        }
        for (item, agenda) in items[2..].iter().zip([31, 36, 43]) {
            let agenda = issue_ref(&format!("mouriya-s-lab/meerkat-e2e-agenda#{agenda}"));
            let (thread, escalated, settlement) = settled(item);
            let escalated = escalated.unwrap();
            assert_eq!(
                (escalated.ask, &escalated.agenda),
                (thread.ask.comment.id, &agenda)
            );
            let Settlement::Reply(reply) = settlement else {
                panic!("{settlement:?}")
            };
            assert_eq!(reply.marker.ask, thread.ask.comment.id);
            assert_eq!(reply.marker.decision.as_ref(), Some(&agenda));
        }
    }

    // code #16: escalated, the council has not settled it.
    #[test]
    fn escalated_open_ask_waits_on_the_council() {
        let comments = comments!("code-16-comments.json");
        let issue = plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/16");
        let items = ask_timeline(issue, &comments, at("2026-11-07T01:17:25Z"));
        let [TimelineItem::Ask(thread)] = items.as_slice() else {
            panic!("{items:?}")
        };
        let AskProgress::Open {
            waiting_on: WaitingOn::Council(escalated),
            age,
        } = &thread.progress
        else {
            panic!("{thread:?}")
        };
        assert_eq!(
            escalated.marker.agenda,
            issue_ref("mouriya-s-lab/meerkat-e2e-agenda#13")
        );
        assert_eq!(escalated.marker.ask, thread.ask.comment.id);
        assert_eq!(*age, SignedDuration::from_mins(90));
    }

    // code #23: an ask nobody settled.
    #[test]
    fn unsettled_ask_waits_on_the_guardians() {
        let comments = comments!("code-23-comments.json");
        let issue = plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/23");
        let asked = at(&comments[0].created_at);
        let now = asked.checked_add(SignedDuration::from_secs(42)).unwrap();
        let items = ask_timeline(issue, &comments, now);
        let [TimelineItem::Ask(thread)] = items.as_slice() else {
            panic!("{items:?}")
        };
        assert_eq!(
            thread.progress,
            AskProgress::Open {
                waiting_on: WaitingOn::Guardians,
                age: SignedDuration::from_secs(42)
            }
        );
    }

    // code #12 replaced by #13; #13's body replaces #12.
    #[test]
    fn replaced_and_replaces_point_at_each_other() {
        let old_comments = comments!("code-12-comments.json");
        let old_issue = plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/12");
        let new_json: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/code-13-issue.json")).unwrap();
        let new_issue = IssueText {
            url: new_json["html_url"].as_str().unwrap(),
            body: new_json["body"].as_str().unwrap(),
            created_at: new_json["created_at"].as_str().unwrap(),
        };
        let now = at("2026-11-07T08:00:00Z");

        let old = ask_timeline(old_issue, &old_comments, now);
        let [item] = old.as_slice() else {
            panic!("{old:?}")
        };
        let (thread, escalated, settlement) = settled(item);
        assert!(escalated.is_none());
        let Settlement::Replaced(replaced) = settlement else {
            panic!("{settlement:?}")
        };
        assert_eq!(replaced.marker.by, issue_ref("mouriya-s-lab/meerkat-e2e-code#13"));
        assert_eq!(replaced.marker.ask, Some(thread.ask.comment.id));

        let new = ask_timeline(new_issue, &[], now);
        let [TimelineItem::Replaces { issue, marker }] = new.as_slice() else {
            panic!("{new:?}")
        };
        assert_eq!(marker.old, issue_ref("mouriya-s-lab/meerkat-e2e-code#12"));
        assert_eq!(marker.ask, Some(thread.ask.comment.id));
        assert_eq!(
            issue.url,
            "https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/13"
        );
    }

    // code #14 (ask, reply) followed by a second reply, an escalation after
    // the reply, a reply to no ask, a bad field, an unknown kind and a line
    // that is no marker JSON.
    #[test]
    fn timeline_invalid_markers_are_kept_with_their_reason() {
        let comments = comments!("code-14-handbuilt-invalid-comments.json");
        let issue = plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/14");
        let items = ask_timeline(issue, &comments, at("2026-11-07T08:00:00Z"));
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
        let first = "https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/14#issuecomment-6698655483";
        let second = format!("second settlement of ask 6698636994 (first: {first})");
        assert_eq!(invalid[0].reason, second);
        assert_eq!(invalid[1].reason, second);
        assert_eq!(
            invalid[2].reason,
            "settles ask 1234, which is no pm:ask on this issue"
        );
        assert!(
            invalid[3]
                .reason
                .starts_with("pm:reply: invalid type: string"),
            "{}",
            invalid[3].reason
        );
        assert!(invalid[4].reason.starts_with("pm:nudge: unknown kind"));
        assert!(invalid[5].reason.starts_with("pm:ask: "));
        for (invalid, comment) in invalid.iter().zip(&comments[2..]) {
            assert_eq!(invalid.comment_url, comment.html_url);
        }
    }

    #[test]
    fn replaces_in_a_comment_and_a_bad_body_marker_are_invalid() {
        let issue = IssueText {
            body: "正文\n\n<!-- pm:replaces {\"v\":1,\"old\":\"bad\",\"ask\":null} -->",
            ..plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/13")
        };
        let comments = [Comment {
            id: 2,
            body: "<!-- pm:replaces {\"v\":1,\"old\":\"o/c#1\",\"ask\":null} -->".to_owned(),
            html_url: "https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/13#issuecomment-2"
                .to_owned(),
            created_at: "2026-11-06T00:00:00Z".to_owned(),
        }];
        let items = ask_timeline(issue, &comments, at("2026-11-07T08:00:00Z"));
        let [TimelineItem::Invalid(body), TimelineItem::Invalid(comment)] = items.as_slice() else {
            panic!("{items:?}")
        };
        assert!(body.reason.starts_with("pm:replaces: "));
        assert_eq!(
            comment.reason,
            "pm:replaces belongs in the issue body, not a comment"
        );
    }

    fn draft_row(id: &str, what: &str) -> DraftRow {
        DraftRow {
            id: id.to_owned(),
            what: what.to_owned(),
            where_: "cmd/notes list".to_owned(),
            how: "how".to_owned(),
            tradeoff: "tradeoff".to_owned(),
            category: "技术".to_owned(),
            missing_design: false,
            deliver: None,
        }
    }

    fn draft(version: u32, rows: Vec<DraftRow>) -> Draft {
        Draft {
            version,
            rows,
            supersedes: Vec::new(),
            settle: None,
            withdrawn: None,
        }
    }

    fn test_session(
        kind: &str,
        label: &str,
        issue: Option<&str>,
        started_at: &str,
        state: SessionState,
    ) -> Session {
        Session {
            id: format!("{kind}-{started_at}"),
            kind: kind.to_owned(),
            label: label.to_owned(),
            model: "model".to_owned(),
            started_at: started_at.to_owned(),
            ended_at: None,
            state,
            error: None,
            reference: issue.map(issue_ref),
            parent: None,
        }
    }

    // Hand-built cells: every glyph maps to its stance, veto first.
    #[test]
    fn every_cell_glyph_maps_to_its_stance() {
        let vote = |choice, counterexample: Option<&str>| {
            Some(VoteRow {
                row: "r1".to_owned(),
                choice,
                counterexample: counterexample.map(str::to_owned),
                citation: None,
                classification: None,
            })
        };
        assert_eq!(Stance::of(None), Stance::Pending);
        assert_eq!(
            Stance::of(vote(Choice::Approve, None).as_ref()),
            Stance::Approve
        );
        assert_eq!(
            Stance::of(vote(Choice::Oppose, None).as_ref()),
            Stance::Oppose
        );
        assert_eq!(
            Stance::of(vote(Choice::Oppose, Some("counter")).as_ref()),
            Stance::Counterexample
        );
        assert_eq!(Stance::of(vote(Choice::Veto, None).as_ref()), Stance::Veto);
        assert_eq!(
            Stance::of(vote(Choice::Veto, Some("counter")).as_ref()),
            Stance::Veto
        );
    }

    // agenda #38: v1 has three approvals and a counterexample; v2 is unanimous.
    #[test]
    fn tally_counts_each_glyph_once_and_skips_missing_cells() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(
            rounds[0].versions[&1].tally(0),
            RowTally {
                approve: 3,
                oppose: 0,
                veto: 0,
                counterexample: 1,
                voted: 4
            }
        );
        assert_eq!(
            rounds[1].versions[&2].tally(0),
            RowTally {
                approve: 4,
                voted: 4,
                ..RowTally::default()
            }
        );
        assert_eq!(rounds[0].versions[&1].tally(7), RowTally::default());
    }

    // agenda #38 v1 and agenda #44 v1: only objecting seats are listed.
    #[test]
    fn objectors_lists_only_the_seats_that_pushed_back() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        assert_eq!(rounds[0].versions[&1].objectors("r1"), ["ClaudeHigh"]);
        assert!(rounds[0].versions[&1].objectors("r9").is_empty());
        assert!(rounds[1].versions[&2].objectors("r1").is_empty());

        let vetoed = comments!("agenda-44-comments.json");
        let vetoed_rounds = panel_history(&vetoed);
        assert_eq!(vetoed_rounds[0].versions[&1].objectors("r1"), ["Argon"]);
    }

    // agenda #38: one version per round, in version order with their rounds.
    #[test]
    fn versions_lists_each_rounds_version_with_its_round() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        let ordered: Vec<(u32, u32)> = versions(&rounds)
            .iter()
            .map(|(round, version)| (*round, version.draft.marker.version))
            .collect();
        assert_eq!(ordered, [(1, 1), (2, 2)]);
    }

    // agenda #38: a revision step sits between the two drafts; agenda #44 has none.
    #[test]
    fn the_track_revises_each_version_after_the_first() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        let track = steps(&rounds);
        assert_eq!(
            track,
            [
                Step::Draft {
                    round: 1,
                    version: 1
                },
                Step::Revision { from: 1, to: 2 },
                Step::Draft {
                    round: 2,
                    version: 2
                },
            ]
        );
        assert_eq!(track[1].version(), 1);
        assert_eq!(track[1].changed(), 2);
        assert_eq!(track[0].version(), track[0].changed());

        let single = comments!("agenda-44-comments.json");
        let single_rounds = panel_history(&single);
        assert_eq!(
            steps(&single_rounds),
            [Step::Draft {
                round: 1,
                version: 1
            }]
        );
    }

    // agenda #38: v2 revises v1's row in place.
    #[test]
    fn a_revision_marks_the_row_it_rewrote() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        let old = &rounds[0].versions[&1].draft.marker;
        let new = &rounds[1].versions[&2].draft.marker;
        let changes = draft_changes(old, new);
        let [
            RowChange::Changed {
                old: before,
                new: after,
            },
        ] = changes.as_slice()
        else {
            panic!("{changes:?}")
        };
        assert_eq!(before.id, "r1");
        assert_ne!(before.what, after.what);
    }

    // Hand-built drafts: same, changed, added, then removed rows.
    #[test]
    fn hand_built_drafts_show_same_changed_added_and_removed() {
        let old = draft(
            1,
            vec![
                draft_row("r1", "same"),
                draft_row("r2", "was"),
                draft_row("r3", "gone"),
            ],
        );
        let new = draft(
            2,
            vec![
                draft_row("r1", "same"),
                draft_row("r2", "is"),
                draft_row("r4", "new"),
            ],
        );
        let changes = draft_changes(&old, &new);
        assert_eq!(changes.len(), 4);
        assert!(matches!(&changes[0], RowChange::Same(row) if row.id == "r1"));
        assert!(matches!(&changes[1], RowChange::Changed { old, new }
                if old.id == "r2" && new.id == "r2" && old.what != new.what));
        assert!(matches!(&changes[2], RowChange::Added(row) if row.id == "r4"));
        assert!(matches!(&changes[3], RowChange::Removed(row) if row.id == "r3"));
    }

    // agenda #38: a session starting exactly when round 2 starts is round 2's.
    #[test]
    fn a_session_starting_on_a_rounds_start_belongs_to_it() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        let ms = |text: &str| time::parse_ms(text).unwrap();
        assert_eq!(round_of(&rounds, ms("2026-11-07T06:44:38Z")), 2);
        assert_eq!(round_of(&rounds, ms("2026-11-07T06:39:31Z")), 1);
        assert_eq!(round_of(&rounds, ms("2026-11-07T06:40:00Z")), 1);
        assert_eq!(round_of(&rounds, ms("2026-11-07T07:00:00Z")), 2);
        assert_eq!(round_of(&[], ms("2026-11-07T07:00:00Z")), 1);
    }

    // code #16 (open council ask) and code #23 (open guardians ask): waiting beats running.
    #[test]
    fn an_open_ask_outranks_a_running_worker() {
        let waiting = comments!("code-16-comments.json");
        let timeline = ask_timeline(
            plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/16"),
            &waiting,
            at("2026-11-07T08:00:00Z"),
        );
        let summary = impl_summary(false, true, Some(&timeline));
        assert_eq!(summary.group, ImplGroup::Waiting);
        assert_eq!(summary.last_impl, None);
        assert_eq!(
            summary.open_ask,
            Some(OpenAsk {
                council: Some(issue_ref("mouriya-s-lab/meerkat-e2e-agenda#13")),
                since: "2026-11-06T23:47:25Z".to_owned(),
            })
        );

        let guardians = comments!("code-23-comments.json");
        let gtimeline = ask_timeline(
            plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/23"),
            &guardians,
            at("2026-11-07T08:00:00Z"),
        );
        let gsummary = impl_summary(false, false, Some(&gtimeline));
        assert_eq!(gsummary.group, ImplGroup::Waiting);
        assert_eq!(
            gsummary.open_ask,
            Some(OpenAsk {
                council: None,
                since: "2026-11-07T03:58:55Z".to_owned()
            })
        );
    }

    // code #20: settled asks with worker sessions are running until closed.
    #[test]
    fn settled_asks_with_a_worker_session_are_running_until_closed() {
        let comments = comments!("code-20-comments.json");
        let timeline = ask_timeline(
            plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/20"),
            &comments,
            at("2026-11-07T01:00:00Z"),
        );
        let running = impl_summary(false, false, Some(&timeline));
        assert_eq!(running.group, ImplGroup::Running);
        assert_eq!(
            running.last_impl,
            Some((
                "01a1b372-a9db-70fb-aae8-98da66ed931f".to_owned(),
                "2026-11-07T00:39:59Z".to_owned()
            ))
        );
        assert_eq!(running.open_ask, None);
        assert_eq!(
            impl_summary(true, false, Some(&timeline)).group,
            ImplGroup::Done
        );
    }

    // Hand-built: no timeline and no worker means todo; the flag alone means running.
    #[test]
    fn no_timeline_and_no_worker_is_todo() {
        let todo = impl_summary(false, false, None);
        assert_eq!(
            todo,
            ImplSummary {
                group: ImplGroup::Todo,
                last_impl: None,
                open_ask: None
            }
        );
        assert_eq!(impl_summary(false, true, None).group, ImplGroup::Running);
        let empty: Vec<TimelineItem<'_>> = Vec::new();
        assert_eq!(
            impl_summary(false, false, Some(&empty)).group,
            ImplGroup::Todo
        );
    }

    // code #16: the escalated ask the council has not settled stays open.
    #[test]
    fn an_unsettled_escalation_waits_on_its_agenda_issue() {
        let comments = comments!("code-16-comments.json");
        let issue = issue_ref("mouriya-s-lab/meerkat-e2e-code#16");
        let timeline = ask_timeline(
            plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/16"),
            &comments,
            at("2026-11-07T01:17:25Z"),
        );
        let open = open_escalations(&issue, &timeline);
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].issue, issue);
        assert_eq!(open[0].ask, 6698892101);
        assert_eq!(open[0].agenda, issue_ref("mouriya-s-lab/meerkat-e2e-agenda#13"));
        assert_eq!(open[0].created_at, "2026-11-06T23:48:15Z");
        assert!(open[0].url.ends_with("#issuecomment-6698906382"));
    }

    // code #3 (council settled) and code #23 (never escalated): nothing waits.
    #[test]
    fn settled_and_never_escalated_asks_leave_nothing_open() {
        let settled = comments!("code-3-comments.json");
        let issue = issue_ref("mouriya-s-lab/meerkat-e2e-code#3");
        let timeline = ask_timeline(
            plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/3"),
            &settled,
            at("2026-11-07T08:00:00Z"),
        );
        assert!(open_escalations(&issue, &timeline).is_empty());

        let plain = comments!("code-23-comments.json");
        let pissue = issue_ref("mouriya-s-lab/meerkat-e2e-code#23");
        let ptimeline = ask_timeline(
            plain_issue("https://github.com/mouriya-s-lab/meerkat-e2e-code/issues/23"),
            &plain,
            at("2026-11-07T08:00:00Z"),
        );
        assert!(open_escalations(&pissue, &ptimeline).is_empty());
    }

    // Hand-built sessions: the seat sits between the kind and the ref.
    #[test]
    fn the_seat_is_the_label_word_after_the_kind() {
        let issue = "mouriya-s-lab/meerkat-e2e-agenda#38";
        let vote = test_session(
            "vote",
            "vote Argon mouriya-s-lab/meerkat-e2e-agenda#38",
            Some(issue),
            "2026-11-07T06:49:00Z",
            SessionState::Done,
        );
        assert_eq!(council_seat(&vote), Some("Argon"));
        let kind_mismatch = test_session(
            "draft",
            "vote Argon mouriya-s-lab/meerkat-e2e-agenda#38",
            Some(issue),
            "2026-11-07T06:50:00Z",
            SessionState::Done,
        );
        assert_eq!(council_seat(&kind_mismatch), None);
        let seatless = test_session(
            "vote",
            "vote",
            Some(issue),
            "2026-11-07T06:50:00Z",
            SessionState::Done,
        );
        assert_eq!(council_seat(&seatless), None);
        let ref_as_seat = test_session(
            "vote",
            "vote mouriya-s-lab/meerkat-e2e-agenda#38 extra",
            Some(issue),
            "2026-11-07T06:50:00Z",
            SessionState::Done,
        );
        assert_eq!(council_seat(&ref_as_seat), None);
        let unlinked = test_session(
            "vote",
            "vote Argon mouriya-s-lab/meerkat-e2e-agenda#38",
            None,
            "2026-11-07T06:50:00Z",
            SessionState::Done,
        );
        assert_eq!(council_seat(&unlinked), Some("Argon"));
    }

    // agenda #38 with hand-built sessions: drafts sort before votes in a round.
    #[test]
    fn sessions_group_by_round_with_drafts_before_votes() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        let issue = issue_ref("mouriya-s-lab/meerkat-e2e-agenda#38");
        let sessions = [
            test_session(
                "vote",
                "vote Argon mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:49:00Z",
                SessionState::Done,
            ),
            test_session(
                "draft",
                "draft Argon mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:50:00Z",
                SessionState::Done,
            ),
            test_session(
                "vote",
                "vote ClaudeHigh mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:40:00Z",
                SessionState::Done,
            ),
            test_session(
                "vote",
                "vote Argon mouriya-s-lab/meerkat-e2e-agenda#44",
                Some("mouriya-s-lab/meerkat-e2e-agenda#44"),
                "2026-11-07T06:49:00Z",
                SessionState::Done,
            ),
            test_session(
                "vote",
                "vote LeadMentor mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "soon",
                SessionState::Running,
            ),
        ];
        let by_round = round_sessions(&rounds, &issue, &sessions);
        assert_eq!(by_round.keys().copied().collect::<Vec<_>>(), [1, 2]);
        assert_eq!(by_round[&1].len(), 1);
        assert_eq!(council_seat(by_round[&1][0]), Some("ClaudeHigh"));
        let round2: Vec<&str> = by_round[&2]
            .iter()
            .map(|s| council_seat(s).unwrap())
            .collect();
        assert_eq!(round2, ["Argon", "Argon", "LeadMentor"]);
        assert_eq!(by_round[&2][0].kind, "draft");
    }

    // agenda #38 v1 with hand-built sessions: voted, running, failed, pending.
    #[test]
    fn each_seat_shows_its_ballot_or_its_latest_vote_session() {
        let comments = comments!("agenda-38-comments.json");
        let rounds = panel_history(&comments);
        let version = &rounds[0].versions[&1];
        let sessions = [
            test_session(
                "vote",
                "vote Argon mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:40:00Z",
                SessionState::Done,
            ),
            test_session(
                "vote",
                "vote Argon mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:41:00Z",
                SessionState::Done,
            ),
            test_session(
                "vote",
                "vote Scout mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:41:00Z",
                SessionState::Running,
            ),
            test_session(
                "vote",
                "vote Rookie mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:41:00Z",
                SessionState::Failed,
            ),
            test_session(
                "vote",
                "vote Observer mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:41:00Z",
                SessionState::Done,
            ),
            test_session(
                "draft",
                "draft Drafty mouriya-s-lab/meerkat-e2e-agenda#38",
                Some("mouriya-s-lab/meerkat-e2e-agenda#38"),
                "2026-11-07T06:41:00Z",
                SessionState::Running,
            ),
        ];
        let refs: Vec<&Session> = sessions.iter().collect();
        let pending = ["Observer".to_owned(), "Queued".to_owned()];
        let columns = seat_columns(version, &refs, &pending);
        let states: Vec<(&str, SeatState)> = columns.iter().map(|c| (c.seat, c.state)).collect();
        assert_eq!(
            states,
            [
                ("Coordination", SeatState::Voted),
                ("ClaudeHigh", SeatState::Voted),
                ("Argon", SeatState::Voted),
                ("LeadMentor", SeatState::Voted),
                ("Scout", SeatState::Running),
                ("Rookie", SeatState::Failed),
                ("Observer", SeatState::Pending),
                ("Queued", SeatState::Pending),
            ]
        );
        let argon = columns.iter().find(|c| c.seat == "Argon").unwrap();
        assert_eq!(argon.session.unwrap().started_at, "2026-11-07T06:41:00Z");
        assert!(argon.ballot.is_some());
        let queued = columns.iter().find(|c| c.seat == "Queued").unwrap();
        assert!(queued.ballot.is_none() && queued.session.is_none());
        assert!(queued.cell(0).is_none());
        assert!(columns.iter().all(|c| c.seat != "Drafty"));
    }
}
