//! 「议题详情」(C4 「议题详情」, #9 「详情与实现页的版式」, #4): an agenda
//! issue under 「议题」 — its header, the round track whose chosen step
//! drives the seat × row vote matrix, the draft's change from the version
//! before, its body; beside them its decision, rulings, the council's
//! sessions on it and its relations. Below 1.5 inspectors of room the
//! right column goes under the left one.

use std::rc::Rc;

use nsh_base::{PopoverEvent, button, popover, text_view};
use nsh_core::prelude::*;

use super::{
    icons::{Icon, icon},
    parts::{
        Look, action, empty, error_text, kv, mono, panel, panel_head, pick, small, tag, votes_bar,
    },
    sessions::{self, OpenSession},
    shell::{Context, Nav},
    status::{self, Role},
    text::Msg,
    tokens::{Type, Typeset, color, len, mark},
};
use crate::{
    config::{ColorToken::*, LengthToken::*},
    dag::{Graph, NodeKind},
    detail::{
        self, Ballot, DecisionView, Invalid, Posted, RoundEvent, RowChange, SeatColumn, SeatState,
        Stance, Step, Stop, Version, prose,
    },
    failure::Failure,
    gh::{
        GitHub, IssueRef,
        comments::{Comment, comments},
        markers::{DraftRow, Outcome, Panel},
        project::ProjectItem,
    },
    programs::{Program, QueueState, Session},
    sources::Sources,
    time,
};

/// What is open in detail and what is chosen in it; it outlives the page.
#[derive(Clone, Copy)]
pub struct AgendaState {
    opened: Signal<Option<IssueRef>>,
    /// The round track's chosen step; `None`: the latest.
    step: Signal<Option<Step>>,
    /// The vote matrix cell whose reason shows: seat and row.
    cell: Signal<Option<(String, String)>>,
    /// The council session whose decision log shows in place.
    expanded: Signal<Option<String>>,
}

impl AgendaState {
    pub fn new() -> Self {
        Self {
            opened: signal(None),
            step: signal(None),
            cell: signal(None),
            expanded: signal(None),
        }
    }

    /// Opens `issue`'s detail at its latest step.
    pub fn open(&self, issue: IssueRef) {
        batch(|| {
            self.opened.set(Some(issue));
            self.step.set(None);
            self.cell.set(None);
            self.expanded.set(None);
        });
    }

    pub fn close(&self) {
        self.opened.set(None);
    }

    /// The issue open in detail (tracked).
    pub fn opened(&self) -> Option<IssueRef> {
        self.opened.get()
    }

    /// The issue open in detail, not tracked.
    pub fn opened_untracked(&self) -> Option<IssueRef> {
        self.opened.get_untracked()
    }
}

/// An issue's comments as last read, with the issue they are of.
pub type Comments = Resource<(IssueRef, Rc<Vec<Comment>>), Failure>;

/// The comments of the issue `issue()` names, read once `github` is in and
/// again when the Project's `updatedAt` of it changes (a new comment
/// changes it), not on every refresh: the GitHub API's allowance is shared.
pub fn comments_of(
    sources: Sources,
    github: Memo<Option<Rc<GitHub>>>,
    issue: impl Fn() -> Option<IssueRef> + 'static,
) -> Comments {
    let key = computed(move || {
        let issue = issue()?;
        let updated = sources.project.with(|items| {
            items.and_then(|items| {
                items
                    .data
                    .iter()
                    .find(|item| item.issue == issue)
                    .map(|item| item.updated_at.clone())
            })
        });
        Some((issue, updated))
    });
    resource(
        move || Some((github.get()?, key.get()?)),
        move |(github, (issue, _)): (Rc<GitHub>, (IssueRef, Option<String>)), _| async move {
            let read = comments(&github, &issue).await?;
            Result::Ok((issue, Rc::new(read)))
        },
    )
}

/// `comments`' value when it is `issue`'s (tracked).
pub fn comments_for(comments: Comments, issue: &IssueRef) -> Option<Rc<Vec<Comment>>> {
    comments.with(|read| {
        read.filter(|(of, _)| of == issue)
            .map(|(_, comments)| comments.clone())
    })
}

/// The Project item of `issue` (tracked).
pub fn item_of(sources: Sources, issue: &IssueRef) -> Option<ProjectItem> {
    sources.project.with(|items| {
        items.and_then(|items| items.data.iter().find(|item| item.issue == *issue).cloned())
    })
}

/// A field of a draft's row, as its change shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftField {
    What,
    Where,
    How,
    Tradeoff,
    Category,
}

impl DraftField {
    pub const ALL: [DraftField; 5] = [
        DraftField::What,
        DraftField::Where,
        DraftField::How,
        DraftField::Tradeoff,
        DraftField::Category,
    ];

    fn of(self, row: &DraftRow) -> &str {
        match self {
            DraftField::What => &row.what,
            DraftField::Where => &row.where_,
            DraftField::How => &row.how,
            DraftField::Tradeoff => &row.tradeoff,
            DraftField::Category => &row.category,
        }
    }
}

/// How a stance is colored: approvals `ok`, objections and vetoes
/// `fail`, counterexamples `warn`, no vote the third ink.
pub fn stance_ink(stance: Stance) -> AdaptiveColor {
    match stance {
        Stance::Approve => Role::Ok.text(),
        Stance::Oppose | Stance::Veto => Role::Fail.text(),
        Stance::Counterexample => Role::Warn.text(),
        Stance::Pending => color(Ink3),
    }
}

/// A cell's glyph in its stance's color.
pub fn stance_glyph(stance: Stance) -> Text {
    text(SharedString::from(Msg::StanceGlyph(stance)))
        .ty(Type::BodyStrong)
        .text_color(stance_ink(stance))
}

/// The detail of the agenda issue `issue`.
pub fn page(
    sources: Sources,
    context: Rc<Context>,
    nav: Nav,
    issue: IssueRef,
    comments: Comments,
) -> impl IntoElement {
    let below = context.clone();
    let (a, b) = (issue.clone(), issue);
    scroll().flex_1().child(
        when(super::overview::rail_beside, move || {
            columns(sources, context.clone(), nav, a.clone(), comments, true)
        })
        .otherwise(move || columns(sources, below.clone(), nav, b.clone(), comments, false)),
    )
}

fn columns(
    sources: Sources,
    context: Rc<Context>,
    nav: Nav,
    issue: IssueRef,
    comments: Comments,
    beside: bool,
) -> View {
    let state = context.agenda;
    // Each refresh loads the Project and the council's sessions again, with
    // a new load time: the columns read them through memos that compare
    // what they hold, so a column is made again only when its data changed,
    // not every `refreshSeconds` — a press on a view made again meanwhile
    // would land on one no longer there.
    let item = {
        let issue = issue.clone();
        computed(move || item_of(sources, &issue))
    };
    let council = computed(move || {
        sources
            .council
            .sessions
            .with(|loaded| loaded.map(|loaded| Rc::<[Session]>::from(loaded.data.as_slice())))
    });
    let left = {
        let (context, issue) = (context.clone(), issue.clone());
        view()
            .flex_1()
            .min_w(Points::ZERO)
            .gap(len(Space4))
            .child(dynamic(move || {
                let read = comments_for(comments, &issue);
                let error = comments.error();
                let graph = context.all_issues.get();
                let item = item.get();
                let council = council.get();
                let step = state.step.get();
                let (context, issue) = (context.clone(), issue.clone());
                untracked(move || {
                    left_column(
                        &context,
                        nav,
                        &issue,
                        Loaded {
                            graph: graph.as_deref(),
                            item: item.as_ref(),
                            comments: read.as_deref(),
                            council: council.as_deref().unwrap_or_default(),
                        },
                        error.map(|error| (error, comments)),
                        step,
                        state,
                    )
                    .into_element()
                })
            }))
    };
    let right = {
        let issue = issue.clone();
        view().gap(len(Space4)).child(dynamic(move || {
            let read = comments_for(comments, &issue);
            let graph = context.all_issues.get();
            let council = council.get();
            let (context, issue) = (context.clone(), issue.clone());
            untracked(move || {
                right_column(
                    sources,
                    &context,
                    nav,
                    &issue,
                    graph.as_deref(),
                    read.as_deref(),
                    council.as_deref().unwrap_or_default(),
                    state,
                )
                .into_element()
            })
        }))
    };
    let page = view().gap(len(Space6)).px(len(Space6)).py(len(Space5));
    if beside {
        page.flex_row()
            .items_start()
            .child(left)
            .child(right.w(len(InspectorWidth) + len(Space8)).flex_shrink_0())
    } else {
        page.child(left).child(right)
    }
}

// ---------------------------------------------------------------------------
// The left column

/// What the detail draws from, as last loaded.
struct Loaded<'l> {
    graph: Option<&'l Graph>,
    item: Option<&'l ProjectItem>,
    comments: Option<&'l Vec<Comment>>,
    council: &'l [Session],
}

fn left_column(
    context: &Rc<Context>,
    nav: Nav,
    issue: &IssueRef,
    loaded: Loaded<'_>,
    failed: Option<(Failure, Comments)>,
    step: Option<Step>,
    state: AgendaState,
) -> View {
    let Loaded {
        graph,
        item,
        comments,
        council,
    } = loaded;
    let mut parts: Vec<AnyElement> = vec![header(context, issue, graph, item).into_element()];
    match (comments, failed) {
        (_, Some((error, resource))) => parts.push(
            panel()
                .child(super::parts::error_bar(error, None, move || {
                    resource.refetch()
                }))
                .into_element(),
        ),
        (None, None) => parts.push(panel().child(super::parts::skeleton(4)).into_element()),
        (Some(_), None) => {}
    }
    if let Some(comments) = comments {
        let rounds = detail::panel_history(comments);
        let decision = detail::decision_view(comments);
        let steps = detail::steps(&rounds);
        let chosen = step
            .filter(|step| steps.contains(step))
            .or(steps.last().copied());
        let pending = graph
            .and_then(|graph| graph.nodes.iter().find(|node| node.issue == *issue))
            .and_then(|node| node.panel.as_ref())
            .map(|panel| (panel.version, panel.pending.clone()));
        let by_round = detail::round_sessions(&rounds, issue, council);
        match chosen {
            None => parts.push(panel().child(empty(Msg::NoDraftYet)).into_element()),
            Some(chosen) => {
                let versions = detail::versions(&rounds);
                let queue = pending
                    .as_ref()
                    .and_then(|(version, pending)| version.map(|v| (v, pending.len())));
                let stop = rounds.last().and_then(|round| {
                    detail::stopped(round).map(|(stop, panel)| RoundStop {
                        round: round.number,
                        stop,
                        at: hm_of(&panel.comment.created_at),
                    })
                });
                parts.push(
                    track(
                        &steps,
                        chosen,
                        &versions,
                        decision.as_ref(),
                        queue,
                        stop,
                        state,
                    )
                    .into_element(),
                );
                if let Some(&(round, version)) = versions
                    .iter()
                    .find(|(_, version)| version.draft.marker.version == chosen.version())
                {
                    let sessions = by_round.get(&round).cloned().unwrap_or_default();
                    let queue_pending: &[String] = match &pending {
                        Some((Some(current), pending))
                            if *current == version.draft.marker.version =>
                        {
                            pending
                        }
                        _ => &[],
                    };
                    let columns = detail::seat_columns(version, &sessions, queue_pending);
                    parts.push(
                        matrix(round, version, &columns, decision.as_ref(), state, nav)
                            .into_element(),
                    );
                }
                if let Some(change) = draft_change(&versions, chosen.changed()) {
                    parts.push(change.into_element());
                }
            }
        }
        let invalid: Vec<&Invalid> = rounds
            .iter()
            .flat_map(|round| &round.invalid)
            .chain(decision.iter().flat_map(|view| &view.invalid))
            .collect();
        if !invalid.is_empty() {
            parts.push(invalid_panel(&invalid).into_element());
        }
    }
    parts.push(body_panel(item).into_element());
    view().gap(len(Space4)).children(parts)
}

/// Status, agenda number, kind and drafting seat, when it was opened; the
/// title.
fn header(
    context: &Context,
    issue: &IssueRef,
    graph: Option<&Graph>,
    item: Option<&ProjectItem>,
) -> View {
    let node = graph.and_then(|graph| graph.nodes.iter().find(|node| node.issue == *issue));
    let mut meta: Vec<AnyElement> = Vec::new();
    if let Some(node) = node {
        let st = status::node(&node.state, &context.issues_settings.fields);
        meta.push(status::label(&st, Type::Body).into_element());
    }
    meta.push(small(super::issues::agenda_of(context, issue)).into_element());
    if let Some(node) = node {
        meta.push(
            small(
                Msg::IssueKind {
                    human: node.kind == NodeKind::Human,
                    proposer: node
                        .proposer
                        .as_ref()
                        .map(|p| super::issues::proposer(p, &context.issues_settings.fields)),
                }
                .to_string(),
            )
            .into_element(),
        );
    }
    if let Some(item) = item {
        meta.push(small(Msg::CreatedAt(time::short(&item.created_at)).to_string()).into_element());
    }
    let title_text = node
        .map(|node| node.title.clone())
        .or_else(|| item.map(|item| item.title.clone()))
        .unwrap_or_else(|| issue.to_string());
    let title = text(title_text)
        .ty(Type::Title)
        .number_of_lines(3)
        .selectable(true);
    let superseded = node.is_some_and(|node| node.state.superseded());
    view()
        .gap(len(Space2))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(len(Space3))
                .children(meta),
        )
        .child(if superseded {
            title.line_through()
        } else {
            title
        })
}

/// When a version's draft was posted, as `HH:MM`.
fn hm_of(at: &str) -> Option<String> {
    time::parse_ms(at).map(time::hm)
}

/// The latest round's panel, stopped and not resumed: which round, how,
/// and when (`HH:MM`).
#[derive(Debug, Clone)]
struct RoundStop {
    round: u32,
    stop: Stop,
    at: Option<String>,
}

/// The council's verdict word for a stopped panel (#9 「状态词表」).
fn stop_state(stop: Stop) -> QueueState {
    match stop {
        Stop::Interrupted => QueueState::Interrupted,
        Stop::Blocked => QueueState::Blocked,
        Stop::Failed => QueueState::Failed,
    }
}

/// The round track: each version in its round and each revision; a press
/// chooses the step the matrix shows. `queue` is the version the council
/// collects votes on and how many seats it still waits for; `stop` ends
/// the latest version's step when its round's panel stopped.
fn track(
    steps: &[Step],
    chosen: Step,
    versions: &[(u32, &Version<'_>)],
    decision: Option<&DecisionView<'_>>,
    queue: Option<(u32, usize)>,
    stop: Option<RoundStop>,
    state: AgendaState,
) -> View {
    let version = |n: u32| {
        versions
            .iter()
            .find(|(_, v)| v.draft.marker.version == n)
            .map(|(_, v)| *v)
    };
    let decided = decision.map(|view| view.decision.marker.version);
    let cards: Vec<AnyElement> = steps
        .iter()
        .map(|&step| {
            let (title, line) = match step {
                Step::Draft { round, version: n } => {
                    let this = version(n);
                    let from = this
                        .and_then(|v| hm_of(&v.draft.comment.created_at))
                        .unwrap_or_default();
                    let next = version(n + 1).and_then(|v| hm_of(&v.draft.comment.created_at));
                    let stopped = stop.as_ref().filter(|stop| {
                        stop.round == round && version(n + 1).is_none() && decided != Some(n)
                    });
                    let to = match (decision, stopped) {
                        (Some(view), _) if decided == Some(n) => {
                            hm_of(&view.decision.comment.created_at)
                        }
                        (_, Some(stopped)) => stopped.at.clone(),
                        _ => next,
                    };
                    let objecting = this.map_or(0, |v| {
                        v.draft
                            .marker
                            .rows
                            .iter()
                            .filter(|row| !v.objectors(&row.id).is_empty())
                            .count()
                    });
                    let result = if decided == Some(n) {
                        Msg::StepDecided
                    } else if let Some(stopped) = stopped {
                        Msg::Queue(stop_state(stopped.stop))
                    } else if objecting > 0 {
                        Msg::StepObjections(objecting)
                    } else {
                        Msg::StepVoted {
                            voted: this.map_or(0, |v| v.ballots.len()),
                            seats: this.map_or(0, |v| v.ballots.len())
                                + queue
                                    .filter(|(v, _)| *v == n)
                                    .map_or(0, |(_, pending)| pending),
                        }
                    };
                    (
                        Msg::StepDraft { round, version: n },
                        format!("{} · {}", Msg::StepSpan { from, to }, result),
                    )
                }
                Step::Revision { from, to } => {
                    let changed: Vec<String> = match (version(from), version(to)) {
                        (Some(old), Some(new)) => {
                            detail::draft_changes(&old.draft.marker, &new.draft.marker)
                                .into_iter()
                                .filter_map(|change| match change {
                                    RowChange::Same(_) => None,
                                    RowChange::Added(row) | RowChange::Removed(row) => {
                                        Some(row.id.clone())
                                    }
                                    RowChange::Changed { new, .. } => Some(new.id.clone()),
                                })
                                .collect()
                        }
                        _ => Vec::new(),
                    };
                    let at = version(to)
                        .and_then(|v| hm_of(&v.draft.comment.created_at))
                        .unwrap_or_default();
                    (
                        Msg::StepRevision { from, to },
                        format!("{at} · {}", Msg::StepChanged(changed)),
                    )
                }
            };
            step_card(title, line, step == chosen, move || {
                state.step.set(Some(step))
            })
            .into_element()
        })
        .collect();
    panel().flex_row().flex_wrap().children(cards)
}

fn step_card(
    title: Msg,
    line: String,
    chosen: bool,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let label = SharedString::from(title);
    button()
        .on_press(on_press)
        .accessibility_label(label.clone())
        .selected(move || chosen)
        .flex_1()
        .min_w(len(InspectorWidth) / 2.0)
        .gap(len(Space1))
        .px(len(Space4) - mark())
        .py(len(Space3))
        .border_l(mark())
        .border_l_color(if chosen {
            color(Selection)
        } else {
            super::parts::clear()
        })
        .border_r_1()
        .border_r_color(color(Border))
        .bg(if chosen {
            color(Selected)
        } else {
            super::parts::clear()
        })
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(text(label).ty(Type::BodyStrong).number_of_lines(1))
        .child(small(line).number_of_lines(1))
}

/// The width of a seat's column in the matrix.
fn seat_width() -> Points {
    len(Space8) * 2.0
}

/// The narrowest the matrix gets before it scrolls: the row's padding, a
/// least width for the row's text and result, and each seat's column with
/// the gap before it; a card at least this wide shows every seat.
fn matrix_min_width(seats: usize) -> Points {
    len(Space4) * 2.0 + len(InspectorWidth) / 2.0 + (seat_width() + len(Space2)) * seats as f32
}

/// The seat × row matrix of `version`: a glyph per cell, a press on one
/// shows the seat's reason; each row's result under its text, the rule in
/// the head once the decision gives it.
fn matrix(
    round: u32,
    version: &Version<'_>,
    columns: &[SeatColumn<'_, '_>],
    decision: Option<&DecisionView<'_>>,
    state: AgendaState,
    nav: Nav,
) -> View {
    let number = version.draft.marker.version;
    let decided = decision.filter(|view| view.decision.marker.version == number);
    let legend: Vec<AnyElement> = [
        Stance::Approve,
        Stance::Oppose,
        Stance::Veto,
        Stance::Counterexample,
        Stance::Pending,
    ]
    .into_iter()
    .map(|stance| {
        view()
            .flex_row()
            .items_center()
            .gap(len(Space1))
            .child(stance_glyph(stance).ty(Type::Small))
            .child(small(SharedString::from(Msg::Stance(stance))))
            .into_element()
    })
    .chain(
        decided
            .and_then(|view| view.decision.marker.rows.first())
            .map(|row| small(Msg::Rule(row.threshold).to_string()).into_element()),
    )
    .collect();
    let head = view()
        .flex_row()
        .items_end()
        .gap(len(Space2))
        .min_h(len(Row))
        .px(len(Space4))
        .py(len(Space1))
        .border_b_1()
        .border_b_color(color(Border))
        .child(super::parts::th(SharedString::from(Msg::DecisionRow)).flex_1())
        .children(columns.iter().map(|column| {
            let under = match column.state {
                SeatState::Running => Some((
                    Msg::Session(crate::programs::SessionState::Running),
                    Role::Run,
                )),
                SeatState::Failed => Some((Msg::SeatSessionFailed, Role::Fail)),
                SeatState::Voted | SeatState::Pending => None,
            };
            view()
                .w(seat_width())
                .flex_shrink_0()
                .items_center()
                .child(super::parts::th(column.seat.to_owned()))
                .children(under.map(|(word, role)| {
                    text(SharedString::from(word))
                        .ty(Type::Small)
                        .text_color(role.text())
                        .number_of_lines(1)
                }))
                .into_element()
        }));
    let rows: Vec<AnyElement> = version
        .draft
        .marker
        .rows
        .iter()
        .enumerate()
        .map(|(at, row)| {
            matrix_row(round, version, columns, at, row, decided, state, nav).into_element()
        })
        .collect();
    panel()
        .child(panel_head(
            SharedString::from(Msg::Votes),
            SharedString::from(Msg::VotesNote {
                round,
                version: number,
            }),
            vec![
                view()
                    .flex_row()
                    .flex_wrap()
                    .items_center()
                    .gap(len(Space3))
                    .children(legend)
                    .into_element(),
            ],
        ))
        .child(
            scroll().horizontal().child(
                view()
                    .w_full()
                    .min_w(matrix_min_width(columns.len()))
                    .child(head)
                    .children(rows),
            ),
        )
}

#[allow(clippy::too_many_arguments)]
fn matrix_row(
    round: u32,
    version: &Version<'_>,
    columns: &[SeatColumn<'_, '_>],
    at: usize,
    row: &DraftRow,
    decided: Option<&DecisionView<'_>>,
    state: AgendaState,
    nav: Nav,
) -> View {
    let ruled = version.ruled(&row.id).map(str::to_owned);
    let cells: Vec<AnyElement> = columns
        .iter()
        .map(|column| cell(round, row, column, at, state, nav).into_element())
        .collect();
    let tally = version.tally(at);
    let seats = columns.len();
    let outcome =
        decided.and_then(|view| view.decision.marker.rows.iter().find(|r| r.row == row.id));
    let (word, ink) = match outcome {
        Some(outcome) => (
            Msg::Outcome(outcome.outcome),
            match outcome.outcome {
                Outcome::Passed => Role::Ok.text(),
                Outcome::Rejected => color(Ink2),
                Outcome::Withdrawn => color(Ink3),
            },
        ),
        None if tally.veto > 0 => (Msg::HasVeto, Role::Fail.text()),
        None if tally.counterexample > 0 => (Msg::HasCounterexample, Role::Warn.text()),
        None => (
            Msg::Approvals {
                approve: tally.approve,
                threshold: None,
                waiting: seats.saturating_sub(tally.voted),
            },
            color(Ink2),
        ),
    };
    view()
        .flex_row()
        .items_stretch()
        .gap(len(Space2))
        .min_h(len(Row) + len(Space2))
        .px(len(Space4))
        .border_b_1()
        .border_b_color(color(Border))
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .py(len(Space2))
                .gap(len(Space1))
                .child(
                    view()
                        .flex_row()
                        .items_start()
                        .gap(len(Space2))
                        .child(small(row.id.clone()).flex_shrink_0())
                        .child(
                            text(row.what.clone())
                                .ty(Type::Body)
                                .flex_1()
                                .min_w(Points::ZERO)
                                .number_of_lines(2),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(len(Space2))
                        .child(votes_bar(tally.approve, seats.max(tally.voted)))
                        .child(
                            text(SharedString::from(word))
                                .ty(Type::Small)
                                .text_color(ink)
                                .number_of_lines(1),
                        )
                        .children(
                            ruled
                                .map(|category| tag(Msg::Ruled(category).to_string(), Role::Human)),
                        ),
                ),
        )
        .children(cells)
}

/// A matrix cell: the glyph, on stripes under a seat whose session failed;
/// a cast vote opens the seat's reason.
fn cell(
    round: u32,
    row: &DraftRow,
    column: &SeatColumn<'_, '_>,
    at: usize,
    state: AgendaState,
    nav: Nav,
) -> View {
    let vote = column.cell(at);
    let stance = Stance::of(vote);
    let frame = view()
        .w(seat_width())
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .overflow_hidden();
    let frame = if column.state == SeatState::Failed {
        frame.child(hatch())
    } else {
        frame
    };
    let label = SharedString::from(Msg::Cell {
        seat: column.seat.to_owned(),
        row: row.id.clone(),
        stance,
    });
    let (Some(vote), Some(ballot)) = (vote, column.ballot) else {
        return frame.child(
            view()
                .accessible(true)
                .label(label)
                .child(stance_glyph(stance)),
        );
    };
    let key = (column.seat.to_owned(), row.id.clone());
    let open = {
        let key = key.clone();
        move || state.cell.with(|cell| cell.as_ref() == Some(&key))
    };
    let trigger = {
        let open = open.clone();
        button()
            .accessibility_label(label)
            .size(len(Control))
            .items_center()
            .justify_center()
            .rounded(len(RadiusControl))
            .border_1()
            .border_color(move || {
                if open() {
                    color(Selection)
                } else {
                    super::parts::clear()
                }
            })
            .hover(|s| s.bg(color(Accent)))
            .cursor_pointer()
            .child(stance_glyph(stance))
    };
    let card = reason(round, row, vote, ballot, column.session, stance, nav);
    frame.child(
        popover()
            .open(move || Some(open()))
            .on_open_change(move |event: &PopoverEvent| {
                if let PopoverEvent::OpenChange(next, _) = event {
                    state.cell.set(next.then(|| key.clone()));
                }
            })
            .placement(Placement::Bottom)
            .align(Align::Center)
            .offset(len(Space1))
            .trigger(trigger)
            .child(card),
    )
}

/// Diagonal stripes in `fail-subtle` over a cell's frame: the seat's
/// session failed.
fn hatch() -> CanvasView {
    let (w, h) = (seat_width(), len(Row) * 3.0);
    canvas(move || {
        let ink = pick(Role::Fail.subtle());
        let step = len(Space2).0;
        let mut painter = Painter::new()
            .stroke_color(ink)
            .line_width(mark())
            .begin_path();
        let mut x = -h.0;
        while x < w.0 {
            painter = painter.move_to(px(x), h).line_to(px(x + h.0), Points::ZERO);
            x += step;
        }
        painter.stroke()
    })
    .absolute()
    .top(Points::ZERO)
    .left(Points::ZERO)
    .w(w)
    .h(h)
}

/// A seat's reason for its vote on a row: stance, seat, row and time, the
/// text above its vote's marker, the row's counterexample, citation and
/// classification, and its session and comment.
fn reason(
    round: u32,
    row: &DraftRow,
    vote: &crate::gh::markers::VoteRow,
    ballot: &Ballot<'_>,
    session: Option<&Session>,
    stance: Stance,
    nav: Nav,
) -> View {
    let text_above = ballot.reason();
    let comment_url = ballot.comment.html_url.clone();
    let at = time::short(&ballot.comment.created_at);
    let mut facts: Vec<AnyElement> = Vec::new();
    if let Some(counter) = &vote.counterexample {
        facts.push(kv(Msg::Counterexample, mono(counter.clone()).selectable(true)).into_element());
    }
    if let Some(citation) = &vote.citation {
        facts.push(kv(Msg::Citation, mono(citation.clone()).selectable(true)).into_element());
    }
    if let Some(classification) = &vote.classification {
        facts.push(
            kv(
                Msg::Classification,
                text(classification.clone()).ty(Type::Body),
            )
            .into_element(),
        );
    }
    let open_session = session.map(|session| {
        let open = OpenSession {
            program: Program::Council,
            id: session.id.clone(),
        };
        action(
            Msg::OpenSession,
            Some(Icon::ArrowRight),
            Look::Link,
            || false,
            move || nav.open_session(open.clone()),
        )
        .into_element()
    });
    view()
        .w(len(InspectorWidth))
        .gap(len(Space2))
        .p(len(Space4))
        .rounded(len(RadiusPanel))
        .border_1()
        .border_color(color(Border))
        .bg(color(Popover))
        .text_color(color(PopoverForeground))
        .shadow_lg()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap(len(Space2))
                .child(stance_glyph(stance))
                .child(
                    text(SharedString::from(Msg::Stance(stance)))
                        .ty(Type::Label)
                        .text_color(stance_ink(stance)),
                )
                .child(text(ballot.seat.clone()).ty(Type::Label))
                .child(small(format!(
                    "{} · {} · {at}",
                    row.id,
                    Msg::RoundNumber(round)
                ))),
        )
        .child(if text_above.is_empty() {
            small(SharedString::from(Msg::NoReason)).into_element()
        } else {
            text(text_above.to_owned())
                .ty(Type::Body)
                .selectable(true)
                .into_element()
        })
        .children(facts)
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .children(open_session)
                .child(view().flex_1())
                .child(action(
                    Msg::Comment,
                    Some(Icon::External),
                    Look::Link,
                    || false,
                    move || drop(open_url(comment_url.clone())),
                )),
        )
}

/// The draft of `changed` against the version before it, row by row, with
/// why it was revised; none for the first version.
fn draft_change(versions: &[(u32, &Version<'_>)], changed: u32) -> Option<View> {
    let at = versions
        .iter()
        .position(|(_, v)| v.draft.marker.version == changed)?;
    let (old_round, old) = *versions.get(at.checked_sub(1)?)?;
    let (_, new) = versions[at];
    let changes = detail::draft_changes(&old.draft.marker, &new.draft.marker);
    let same: Vec<String> = changes
        .iter()
        .filter_map(|change| match change {
            RowChange::Same(row) => Some(row.id.clone()),
            _ => None,
        })
        .collect();
    let mut objectors: Vec<String> = Vec::new();
    for change in &changes {
        let id = match change {
            RowChange::Same(_) | RowChange::Added(_) => continue,
            RowChange::Removed(row) | RowChange::Changed { old: row, .. } => &row.id,
        };
        for seat in old.objectors(id) {
            if !objectors.iter().any(|s| s == seat) {
                objectors.push(seat.to_owned());
            }
        }
    }
    let ruler = old.ruling.as_ref().map(|ruling| ruling.marker.seat.clone());
    let rows: Vec<AnyElement> = changes
        .iter()
        .filter(|change| !matches!(change, RowChange::Same(_)))
        .map(|change| change_row(change).into_element())
        .collect();
    let right = if same.is_empty() {
        Vec::new()
    } else {
        vec![small(Msg::DraftUnchanged(same).to_string()).into_element()]
    };
    Some(
        panel()
            .child(panel_head(
                SharedString::from(Msg::DraftChange),
                SharedString::from(Msg::VersionArrow {
                    from: old.draft.marker.version,
                    to: changed,
                }),
                right,
            ))
            .children(rows)
            .child(
                small(
                    Msg::RevisionWhy {
                        round: old_round,
                        seats: objectors,
                        ruler,
                    }
                    .to_string(),
                )
                .px(len(Space4))
                .py(len(Space3)),
            ),
    )
}

/// A line struck out (`del`) or added.
fn diff_line(field: DraftField, line: &str, added: bool) -> View {
    let (bg, ink) = if added {
        (color(DiffAdd), color(DiffAddText))
    } else {
        (color(DiffDel), color(DiffDelText))
    };
    let words = text(line.to_owned())
        .ty(Type::Body)
        .text_color(ink)
        .selectable(true)
        .flex_1()
        .min_w(Points::ZERO);
    view()
        .flex_row()
        .items_start()
        .gap(len(Space2))
        .px(len(Space3))
        .py(len(Space1))
        .bg(bg)
        .children((field != DraftField::What).then(|| {
            small(SharedString::from(Msg::DraftField(field)))
                .text_color(ink)
                .flex_shrink_0()
        }))
        .child(if added { words } else { words.line_through() })
}

fn change_row(change: &RowChange<'_>) -> View {
    let (id, lines, mark_word): (&str, Vec<View>, Option<Msg>) = match change {
        RowChange::Same(row) => (&row.id, Vec::new(), None),
        RowChange::Added(row) => (
            &row.id,
            vec![diff_line(DraftField::What, &row.what, true)],
            Some(Msg::RowAdded),
        ),
        RowChange::Removed(row) => (
            &row.id,
            vec![diff_line(DraftField::What, &row.what, false)],
            Some(Msg::RowRemoved),
        ),
        RowChange::Changed { old, new } => (
            &new.id,
            DraftField::ALL
                .into_iter()
                .filter(|field| field.of(old) != field.of(new))
                .flat_map(|field| {
                    [
                        diff_line(field, field.of(old), false),
                        diff_line(field, field.of(new), true),
                    ]
                })
                .collect(),
            None,
        ),
    };
    view()
        .flex_row()
        .items_start()
        .gap(len(Space3))
        .px(len(Space4))
        .py(len(Space2))
        .border_b_1()
        .border_b_color(color(Border))
        .child(
            view()
                .w(len(Space8))
                .flex_shrink_0()
                .gap(len(Space1))
                .child(small(id.to_owned()))
                .children(mark_word.map(|word| small(SharedString::from(word)))),
        )
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .gap(len(Space1) / 2.0)
                .children(lines),
        )
}

fn invalid_panel(invalid: &[&Invalid<'_>]) -> View {
    panel()
        .child(panel_head(
            SharedString::from(Msg::InvalidMarkers),
            invalid.len().to_string(),
            Vec::new(),
        ))
        .children(invalid.iter().map(|item| {
            let url = item.comment_url.to_owned();
            view()
                .flex_row()
                .items_start()
                .gap(len(Space2))
                .px(len(Space4))
                .py(len(Space2))
                .border_b_1()
                .border_b_color(color(Border))
                .child(status::glyph(status::Glyph::Triangle, Role::Warn))
                .child(
                    error_text(item.reason.clone())
                        .selectable(true)
                        .flex_1()
                        .min_w(Points::ZERO),
                )
                .child(action(
                    Msg::Comment,
                    Some(Icon::External),
                    Look::Link,
                    || false,
                    move || drop(open_url(url.clone())),
                ))
                .into_element()
        }))
}

/// The issue's body as Markdown, its marker line left out.
fn body_panel(item: Option<&ProjectItem>) -> View {
    let body = item
        .map(|item| prose(&item.body).to_owned())
        .unwrap_or_default();
    panel()
        .child(panel_head(SharedString::from(Msg::Body), "", Vec::new()))
        .child(if body.is_empty() {
            empty(Msg::NoBody).into_element()
        } else {
            view()
                .px(len(Space4))
                .py(len(Space3))
                .child(
                    text_view()
                        .markdown(SharedString::from(body))
                        .selectable(true),
                )
                .into_element()
        })
}

// ---------------------------------------------------------------------------
// The right column

#[allow(clippy::too_many_arguments)]
fn right_column(
    sources: Sources,
    context: &Rc<Context>,
    nav: Nav,
    issue: &IssueRef,
    graph: Option<&Graph>,
    comments: Option<&Vec<Comment>>,
    council: &[Session],
    state: AgendaState,
) -> View {
    let Some(comments) = comments else {
        return view().child(relations(sources, context, nav, issue, graph, None, &[]));
    };
    let rounds = detail::panel_history(comments);
    let decision = detail::decision_view(comments);
    let by_round = detail::round_sessions(&rounds, issue, council);
    let current = rounds.last().map_or(1, |round| round.number);
    let blocking: Vec<&Session> = by_round
        .get(&current)
        .map(|sessions| {
            sessions
                .iter()
                .copied()
                .filter(|s| s.state == crate::programs::SessionState::Failed)
                .collect()
        })
        .unwrap_or_default();
    let superseded_by: Vec<IssueRef> = rounds
        .iter()
        .flat_map(|round| &round.events)
        .filter_map(|event| match event {
            RoundEvent::Superseded(posted) => Some(posted.marker.by.clone()),
            RoundEvent::Panel(_) => None,
        })
        .collect();
    let stopped = rounds.last().and_then(|round| {
        detail::stopped(round)
            .map(|(stop, panel)| stop_line(sources, context, nav, graph, round.number, stop, panel))
    });
    view()
        .gap(len(Space4))
        .child(decision_panel(
            context,
            nav,
            issue,
            graph,
            decision.as_ref(),
            stopped,
            &blocking,
        ))
        .child(rulings_panel(&rounds))
        .child(sessions_panel(context, nav, &by_round, state))
        .child(relations(
            sources,
            context,
            nav,
            issue,
            graph,
            decision.as_ref(),
            &superseded_by,
        ))
}

/// The decision: each row's outcome with its approvals, threshold and
/// reason; before it, the issue's state, why (`stopped`: the latest
/// round's panel stopped), and the sessions that failed in its round.
fn decision_panel(
    context: &Rc<Context>,
    nav: Nav,
    issue: &IssueRef,
    graph: Option<&Graph>,
    decision: Option<&DecisionView<'_>>,
    stopped: Option<AnyElement>,
    blocking: &[&Session],
) -> View {
    let Some(view_) = decision else {
        let summary = || {
            let node = graph?.nodes.iter().find(|node| node.issue == *issue)?;
            node.notes
                .first()
                .map(|note| crate::attention::first_line(note).to_owned())
                .or_else(|| {
                    node.panel.as_ref().map(|panel| {
                        Msg::RoundSummary {
                            round: panel.round,
                            version: panel.version,
                            voted: panel.votes.len(),
                            seats: panel.votes.len() + panel.pending.len(),
                        }
                        .to_string()
                    })
                })
        };
        let why = stopped.or_else(|| {
            summary().map(|summary| {
                text(summary)
                    .ty(Type::Body)
                    .text_color(color(Ink2))
                    .selectable(true)
                    .px(len(Space4))
                    .py(len(Space3))
                    .into_element()
            })
        });
        return panel()
            .child(panel_head(
                SharedString::from(Msg::Decision),
                SharedString::from(Msg::NotDecided),
                Vec::new(),
            ))
            .children(why)
            .children(
                blocking
                    .iter()
                    .map(|session| session_line(context, nav, session, None)),
            );
    };
    let decision = &view_.decision;
    panel()
        .child(panel_head(
            SharedString::from(Msg::Decision),
            Msg::DecidedAt(time::short(&decision.comment.created_at)).to_string(),
            vec![
                action(Msg::Comment, Some(Icon::External), Look::Link, || false, {
                    let url = decision.comment.html_url.clone();
                    move || drop(open_url(url.clone()))
                })
                .into_element(),
            ],
        ))
        .children(decision.marker.rows.iter().map(|row| {
            let ink = match row.outcome {
                Outcome::Passed => Role::Ok.text(),
                Outcome::Rejected => color(Ink2),
                Outcome::Withdrawn => color(Ink3),
            };
            view()
                .gap(len(Space1))
                .px(len(Space4))
                .py(len(Space2))
                .border_b_1()
                .border_b_color(color(Border))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .child(small(row.row.clone()))
                        .child(
                            text(
                                Msg::OutcomeCount {
                                    outcome: row.outcome,
                                    approve: row.approve,
                                    threshold: row.threshold,
                                }
                                .to_string(),
                            )
                            .ty(Type::Label)
                            .text_color(ink),
                        ),
                )
                .children((!row.reason.is_empty()).then(|| {
                    text(row.reason.clone())
                        .ty(Type::Small)
                        .text_color(color(Ink2))
                        .selectable(true)
                }))
                .into_element()
        }))
}

/// A stopped panel in the decision card: the round, the council's verdict
/// word and when, the error it posted, the prerequisite it waits for, and
/// its `meerkat:panel` comment.
fn stop_line(
    sources: Sources,
    context: &Rc<Context>,
    nav: Nav,
    graph: Option<&Graph>,
    round: u32,
    stop: Stop,
    panel: &Posted<'_, Panel>,
) -> AnyElement {
    let state = stop_state(stop);
    let st = status::queue(state);
    let url = panel.comment.html_url.clone();
    view()
        .gap(len(Space1))
        .px(len(Space4))
        .py(len(Space3))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .child(status::glyph(st.glyph, st.role))
                .child(
                    text(
                        Msg::RoundStopped {
                            round,
                            state,
                            at: time::short(&panel.comment.created_at),
                        }
                        .to_string(),
                    )
                    .ty(Type::Label)
                    .text_color(st.role.text())
                    .flex_1()
                    .min_w(Points::ZERO),
                )
                .child(action(
                    Msg::Comment,
                    Some(Icon::External),
                    Look::Link,
                    || false,
                    move || drop(open_url(url.clone())),
                )),
        )
        .children(
            panel
                .marker
                .error
                .clone()
                .map(|error| error_text(error).selectable(true)),
        )
        .children(
            panel
                .marker
                .prereq
                .as_ref()
                .map(|prereq| issue_link(sources, context, nav, graph, prereq)),
        )
        .into_element()
}

/// Every ruling: which row became which category, by whom, in which
/// round and when.
fn rulings_panel(rounds: &[detail::Round<'_>]) -> View {
    let rulings: Vec<(u32, &detail::Posted<'_, crate::gh::markers::Ruling>)> = rounds
        .iter()
        .flat_map(|round| {
            round.versions.values().filter_map(move |version| {
                version.ruling.as_ref().map(|ruling| (round.number, ruling))
            })
        })
        .collect();
    let lines: Vec<AnyElement> = rulings
        .iter()
        .flat_map(|(round, ruling)| {
            let meta = Msg::RulingMeta {
                seat: ruling.marker.seat.clone(),
                round: *round,
                at: hm_of(&ruling.comment.created_at).unwrap_or_default(),
            }
            .to_string();
            ruling.marker.rows.iter().map(move |row| {
                view()
                    .flex_row()
                    .items_start()
                    .gap(len(Space2))
                    .px(len(Space4))
                    .py(len(Space2))
                    .child(icon(Icon::Gavel, || color(Ink2)))
                    .child(
                        view()
                            .flex_1()
                            .min_w(Points::ZERO)
                            .child(
                                text(
                                    Msg::RulingLine {
                                        row: row.row.clone(),
                                        category: row.category.clone(),
                                    }
                                    .to_string(),
                                )
                                .ty(Type::Body),
                            )
                            .child(small(meta.clone())),
                    )
                    .into_element()
            })
        })
        .collect();
    let count = lines.len();
    panel()
        .child(panel_head(
            SharedString::from(Msg::Rulings),
            count.to_string(),
            Vec::new(),
        ))
        .child(if lines.is_empty() {
            empty(Msg::NoRulings).into_element()
        } else {
            view().py(len(Space1)).children(lines).into_element()
        })
}

/// The council's sessions on the issue by round, drafts first; a press
/// shows a session's decision log in place, as 「会话」 reduces it.
fn sessions_panel(
    context: &Rc<Context>,
    nav: Nav,
    by_round: &std::collections::BTreeMap<u32, Vec<&Session>>,
    state: AgendaState,
) -> View {
    let count: usize = by_round.values().map(Vec::len).sum();
    let mut parts: Vec<AnyElement> = Vec::new();
    for (round, sessions) in by_round.iter().rev() {
        parts.push(
            small(SharedString::from(Msg::RoundNumber(*round)))
                .px(len(Space4))
                .pt(len(Space3))
                .pb(len(Space1))
                .into_element(),
        );
        for session in sessions {
            parts.push(session_line(context, nav, session, Some(state)).into_element());
        }
    }
    panel()
        .child(panel_head(
            SharedString::from(Msg::CouncilSessions),
            count.to_string(),
            Vec::new(),
        ))
        .child(if parts.is_empty() {
            empty(Msg::NoCouncilSessions).into_element()
        } else {
            view().pb(len(Space2)).children(parts).into_element()
        })
}

/// A council session in a line: glyph, kind and seat, its error's first
/// line, how long; with `state`, a press shows its log under it, else it
/// opens on 「会话」.
fn session_line(
    context: &Rc<Context>,
    nav: Nav,
    session: &Session,
    state: Option<AgendaState>,
) -> View {
    let st = status::session(session.state);
    let id = session.id.clone();
    let title = Msg::SessionSeat {
        kind: session.kind.clone(),
        seat: detail::council_seat(session).map(str::to_owned),
    }
    .to_string();
    let line = {
        let id = id.clone();
        button()
            .on_press(move || match state {
                Some(state) => state.expanded.update(|open| {
                    *open = if open.as_deref() == Some(id.as_str()) {
                        None
                    } else {
                        Some(id.clone())
                    }
                }),
                None => nav.open_session(OpenSession {
                    program: Program::Council,
                    id: id.clone(),
                }),
            })
            .accessibility_label(title.clone())
            .flex_row()
            .items_center()
            .gap(len(Space2))
            .min_h(len(RowLog))
            .px(len(Space4))
            .hover(|s| s.bg(color(Accent)))
            .cursor_pointer()
            .child(status::glyph(st.glyph, st.role))
            .child(
                text(title)
                    .ty(Type::Body)
                    .flex_shrink_0()
                    .number_of_lines(1),
            )
            .child(
                error_text(
                    session
                        .error
                        .as_deref()
                        .map(crate::attention::first_line)
                        .unwrap_or_default()
                        .to_owned(),
                )
                .flex_1()
                .min_w(Points::ZERO)
                .number_of_lines(1),
            )
            .child(small(sessions::duration(session)).flex_shrink_0())
    };
    let Some(state) = state else {
        return view().child(line);
    };
    let context = context.clone();
    let open = OpenSession {
        program: Program::Council,
        id: id.clone(),
    };
    view().child(line).child(when(
        move || {
            state
                .expanded
                .with(|open| open.as_deref() == Some(id.as_str()))
        },
        move || {
            let to_page = open.clone();
            view()
                .mx(len(Space4))
                .mb(len(Space2))
                .px(len(Space3))
                .py(len(Space2))
                .rounded(len(RadiusControl))
                .bg(color(Sunken))
                .child(sessions::session_log(context.clone(), open.clone()))
                .child(view().flex_row().child(action(
                    Msg::OpenSession,
                    Some(Icon::ArrowRight),
                    Look::Link,
                    || false,
                    move || nav.open_session(to_page.clone()),
                )))
        },
    ))
}

/// Prerequisites, what it blocks, the implementation issues the decision
/// created, the old issues it changed and the human issues that changed
/// it; each opens its detail.
fn relations(
    sources: Sources,
    context: &Rc<Context>,
    nav: Nav,
    issue: &IssueRef,
    graph: Option<&Graph>,
    decision: Option<&DecisionView<'_>>,
    superseded_by: &[IssueRef],
) -> View {
    let at = graph.and_then(|graph| graph.nodes.iter().position(|node| node.issue == *issue));
    let issues_of = |nodes: Vec<usize>| -> Vec<IssueRef> {
        graph.map_or_else(Vec::new, |graph| {
            nodes
                .into_iter()
                .map(|i| graph.nodes[i].issue.clone())
                .collect()
        })
    };
    let (ups, downs) = match (graph, at) {
        (Some(graph), Some(at)) => (
            issues_of(graph.into_node(at).map(|edge| edge.from).collect()),
            issues_of(graph.out_of(at).map(|edge| edge.to).collect()),
        ),
        _ => (Vec::new(), Vec::new()),
    };
    let links = |issues: &[IssueRef]| -> AnyElement {
        if issues.is_empty() {
            return text(SharedString::from(Msg::None))
                .ty(Type::Body)
                .into_element();
        }
        view()
            .gap(len(Space1) / 2.0)
            .children(
                issues
                    .iter()
                    .map(|issue| issue_link(sources, context, nav, graph, issue)),
            )
            .into_element()
    };
    let mut lines = vec![
        kv(Msg::Prerequisites, links(&ups)).into_element(),
        kv(Msg::Blocking, links(&downs)).into_element(),
    ];
    let implementations = match decision {
        Some(view) => links(&view.decision.marker.implementations),
        None => text(SharedString::from(Msg::AfterDecision))
            .ty(Type::Body)
            .into_element(),
    };
    lines.push(kv(Msg::Implementations, implementations).into_element());
    if let Some(view) = decision.filter(|view| !view.decision.marker.superseded.is_empty()) {
        lines.push(kv(Msg::Supersedes, links(&view.decision.marker.superseded)).into_element());
    }
    if !superseded_by.is_empty() {
        lines.push(kv(Msg::SupersededBy, links(superseded_by)).into_element());
    }
    panel()
        .child(panel_head(
            SharedString::from(Msg::Relations),
            "",
            Vec::new(),
        ))
        .child(
            view()
                .px(len(Space4))
                .py(len(Space3))
                .gap(len(Space1))
                .children(lines),
        )
}

/// An issue in a line that opens its detail: an agenda issue's under
/// 「议题」, any other's under 「实现」.
fn issue_link(
    sources: Sources,
    context: &Rc<Context>,
    nav: Nav,
    graph: Option<&Graph>,
    issue: &IssueRef,
) -> AnyElement {
    let node = graph.and_then(|graph| graph.nodes.iter().find(|node| node.issue == *issue));
    let target = issue.clone();
    let context_ = context.clone();
    let line = button()
        .on_press(move || context_.open_any(nav, target.clone()))
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(RowLog))
        .rounded(len(RadiusControl))
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer();
    match node {
        Some(node) => {
            let st = status::node(&node.state, &context.issues_settings.fields);
            let title = text(node.title.clone())
                .ty(Type::Body)
                .flex_1()
                .min_w(Points::ZERO)
                .number_of_lines(1);
            line.accessibility_label(node.title.clone())
                .child(status::glyph(st.glyph, st.role))
                .child(small(super::issues::agenda_of(context, issue)))
                .child(if node.state.superseded() {
                    title.line_through()
                } else {
                    title
                })
                .into_element()
        }
        None => {
            let title = item_of(sources, issue)
                .map(|item| item.title)
                .unwrap_or_default();
            line.accessibility_label(issue.to_string())
                .child(icon(Icon::Code, || color(Ink2)))
                .child(small(issue.to_string()))
                .child(
                    text(title)
                        .ty(Type::Body)
                        .flex_1()
                        .min_w(Points::ZERO)
                        .number_of_lines(1),
                )
                .into_element()
        }
    }
}

/// The latest version's seat × row glyphs, small: the 「议题」 list
/// inspector's matrix (#4), with the version it shows.
pub fn mini_matrix(comments: &[Comment]) -> Option<View> {
    let rounds = detail::panel_history(comments);
    let versions = detail::versions(&rounds);
    let &(round, version) = versions.last()?;
    let rows = &version.draft.marker.rows;
    let width = len(Space8) + len(Space4);
    let head = view()
        .flex_row()
        .gap(len(Space1))
        .child(view().w(len(Space8)).flex_shrink_0())
        .children(version.ballots.iter().map(|ballot| {
            small(ballot.seat.clone())
                .w(width)
                .flex_shrink_0()
                .number_of_lines(1)
                .into_element()
        }));
    Some(
        view()
            .gap(len(Space1) / 2.0)
            .child(small(
                Msg::VotesNote {
                    round,
                    version: version.draft.marker.version,
                }
                .to_string(),
            ))
            .child(head)
            .children(rows.iter().enumerate().map(|(at, row)| {
                view()
                    .flex_row()
                    .items_center()
                    .gap(len(Space1))
                    .child(
                        small(row.id.clone())
                            .w(len(Space8))
                            .flex_shrink_0()
                            .number_of_lines(1),
                    )
                    .children(version.ballots.iter().map(|ballot| {
                        let stance = Stance::of(ballot.cells.get(at).and_then(Option::as_ref));
                        view()
                            .w(width)
                            .flex_shrink_0()
                            .accessible(true)
                            .label(SharedString::from(Msg::Cell {
                                seat: ballot.seat.clone(),
                                row: row.id.clone(),
                                stance,
                            }))
                            .child(stance_glyph(stance).ty(Type::Label))
                            .into_element()
                    }))
                    .into_element()
            })),
    )
}
