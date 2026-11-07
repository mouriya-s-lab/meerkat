//! 「概览」(C4 `overviewView`, #9 「概览的构成」): on the left a sentence
//! on the programs and what waits for you, 「需要你处理」 with the failed
//! sessions grouped by program and kind in the order of their
//! consequences, and the council's queue; on the right the programs, the
//! sessions of the attention window, the guardian bases and the sessions'
//! errors.

use std::rc::Rc;

use nsh_base::button;
use nsh_core::prelude::*;

use super::{
    icons::{Icon, icon},
    issues::{self, deps, progress},
    parts::{
        Look, Span, action, badge, error_text, github_bar, hint, meter, mono, nothing, panel,
        panel_head, press, row, section_head, sentence, small, source_bar,
    },
    runtime::{base_name, rewarm_button},
    sessions::{self, OpenSession, histogram},
    shell::{Context, Nav, Page},
    status::{self, Role},
    text::{Msg, QueueMotion},
    tokens::{Type, Typeset, color, len},
};
use crate::{
    attention::{Cause, Concern, Need, SessionGroup, first_line},
    config::{ColorToken::*, LengthToken::*},
    gh::IssueRef,
    programs::{BaseState, Program, QueueState, SessionState},
    sources::{ProgramSources, Sources},
    time,
};

/// The overview: the rail beside the main column where it fits, under it
/// where it does not.
pub fn page(sources: Sources, context: Rc<Context>, nav: Nav) -> impl IntoElement {
    let below = context.clone();
    scroll().flex_1().child(
        when(rail_beside, move || columns(sources, context.clone(), nav, true))
            .otherwise(move || columns(sources, below.clone(), nav, false)),
    )
}

/// Whether the rail fits beside the main column: the sidebar, a main
/// column one and a half inspectors wide, the rail and the page's gaps.
fn rail_beside() -> bool {
    window().width >= len(SidebarWidth) + len(InspectorWidth) * 2.5 + len(Space8) * 3.0
}

fn columns(sources: Sources, context: Rc<Context>, nav: Nav, beside: bool) -> View {
    let main = view()
        .flex_1()
        .min_w(Points::ZERO)
        .gap(len(Space6))
        .child(status_sentence(sources, context.clone()))
        .child(needs_section(sources, context.clone(), nav))
        .child(queue_section(sources, context.clone(), nav));
    let rail = view()
        .gap(len(Space4))
        .child(programs_panel(sources, context.clone()))
        .child(sessions_panel(sources, context.clone(), nav))
        .child(bases_panel(sources, context.clone(), nav))
        .child(errors_panel(sources, context));
    let page = view().gap(len(Space6)).px(len(Space6)).py(len(Space5));
    if beside {
        page.flex_row()
            .items_start()
            .child(main)
            .child(rail.w(len(InspectorWidth) + len(Space8)).flex_shrink_0())
    } else {
        page.child(main).child(rail)
    }
}

// ---------------------------------------------------------------------------
// The sentence

/// Whether the programs answer and the queue moves, then what waits for
/// you within the window.
fn status_sentence(sources: Sources, context: Rc<Context>) -> View {
    let hours = context.hours();
    view()
        .flex_row()
        .items_start()
        .gap(len(Space2))
        .child(dynamic(move || {
            let council = !sources.council.health.with_error(|e| e.is_some());
            let guardians = !sources.guardians.health.with_error(|e| e.is_some());
            let state = status::program(council && guardians);
            status::glyph(state.glyph, state.role).mt(len(Space1) / 2.0).into_element()
        }))
        .child(dynamic(move || {
            let council = !sources.council.health.with_error(|e| e.is_some());
            let guardians = !sources.guardians.health.with_error(|e| e.is_some());
            // The queue's motion only while the council and its queue
            // answer: a kept copy from before a failure is not how it
            // moves now.
            let queue_failed = sources.queue.with_error(|e| e.is_some());
            let queue = sources.queue.with(|q| match q {
                Some(q) if council && !queue_failed => {
                    Some(if q.data.iter().any(|e| e.state == QueueState::Deliberating) {
                        QueueMotion::Moving
                    } else {
                        QueueMotion::Idle
                    })
                }
                _ => None,
            });
            let needs = context.needs.get();
            let failures = needs.iter().filter(|n| n.failure()).count();
            let asks = needs
                .iter()
                .filter(|n| matches!(n, Need::One(Concern::EscalationOpen(_))))
                .count();
            let stale = needs
                .iter()
                .filter(|n| matches!(n, Need::One(Concern::BaseStale(_))))
                .count();
            let low = needs
                .iter()
                .filter(|n| matches!(n, Need::One(Concern::RateLow { .. })))
                .count();
            let mut spans = vec![if council && guardians {
                Span::Strong(Msg::RunningWell.to_string())
            } else {
                Span::Fail(Msg::ProgramsDown { council, guardians }.to_string())
            }];
            spans.push(Span::Plain(
                Msg::ProgramsLine {
                    council,
                    guardians,
                    queue,
                }
                .to_string(),
            ));
            let mut items: Vec<Span> = Vec::new();
            if failures > 0 {
                items.push(Span::Fail(Msg::FailureKinds(failures).to_string()));
            }
            if asks > 0 {
                items.push(Span::Plain(Msg::AsksWaiting(asks).to_string()));
            }
            if stale > 0 {
                items.push(Span::Plain(Msg::StaleBases(stale).to_string()));
            }
            if low > 0 {
                items.push(Span::Plain(Msg::RateLow.to_string()));
            }
            if items.is_empty() {
                spans.push(Span::Plain(Msg::NothingNeedsYou.to_string()));
            } else {
                spans.push(Span::Plain(Msg::WithinHours(hours).to_string()));
                let n = items.len();
                for (i, item) in items.into_iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::Plain(Msg::ListSep { last: i + 1 == n }.to_string()));
                    }
                    spans.push(item);
                }
                spans.push(Span::Plain(Msg::WaitForYou.to_string()));
            }
            sentence(spans).flex_1().into_element()
        }))
}

// ---------------------------------------------------------------------------
// 需要你处理

fn needs_section(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let count = {
        let context = context.clone();
        move || {
            let n = context.needs.get().len();
            SharedString::from(if n > 0 { n.to_string() } else { String::new() })
        }
    };
    view()
        .gap(len(Space2))
        .child(section_head(Msg::NeedsYou, count, Vec::new()))
        .child(dynamic(move || {
            let needs = context.needs.get();
            let context = context.clone();
            untracked(move || {
                if needs.is_empty() {
                    return small(SharedString::from(Msg::NothingNeedsYouLine)).into_element();
                }
                panel()
                    .children(
                        needs
                            .iter()
                            .map(|need| need_row(need.clone(), sources, context.clone(), nav)),
                    )
                    .into_element()
            })
        }))
}

/// The issue's agenda number, else `#n`.
fn agenda(context: &Context, issue: &IssueRef) -> String {
    issues::agenda_of(context, issue)
}

/// The text of one need's row: its glyph and title, a count for a group,
/// the line under the title, when it happened.
struct Line {
    status: status::Status,
    title: String,
    count: Option<usize>,
    meta: Vec<AnyElement>,
    at: Option<f64>,
}

fn need_row(need: Need, sources: Sources, context: Rc<Context>, nav: Nav) -> AnyElement {
    let line = line_of(&need, &context);
    let action = need_action(&need, sources, context.clone(), nav);
    let group = match &need {
        Need::Sessions(group) if group.sessions.len() > 1 => Some(group.clone()),
        _ => None,
    };
    let open = signal(false);
    let now = context.now;
    let spark = group.as_ref().map(|group| {
        let times: Vec<f64> = group
            .sessions
            .iter()
            .filter_map(|s| time::parse_ms(s.ended_at.as_deref().unwrap_or(&s.started_at)))
            .collect();
        spark(time::hourly(times, time::now_ms(), 24))
    });
    let head = view()
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .min_h(len(RowGroup))
        .px(len(Space4))
        .py(len(Space2))
        .child(match &group {
            Some(_) => button()
                .on_press(move || open.update(|o| *o = !*o))
                .accessibility_label(SharedString::from(Msg::Expand))
                .selected(move || open.get())
                .size(len(Space4))
                .items_center()
                .justify_center()
                .cursor_pointer()
                .child(dynamic(move || {
                    icon(
                        if open.get() { Icon::ChevronDown } else { Icon::ChevronRight },
                        || color(Ink3),
                    )
                    .into_element()
                }))
                .into_element(),
            None => view().w(len(Space4)).into_element(),
        })
        .child(status::glyph(line.status.glyph, line.status.role))
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .overflow_hidden()
                .gap(len(Space1) / 2.0)
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .child(text(line.title.clone()).ty(Type::BodyStrong).number_of_lines(1))
                        .children(line.count.map(|n| badge(n, Role::Fail))),
                )
                .child(
                    // Details wrap to further lines rather than being cut;
                    // one longer than the row ends in an ellipsis.
                    view()
                        .flex_row()
                        .flex_wrap()
                        .items_center()
                        .gap(len(Space2))
                        .min_w(Points::ZERO)
                        .children(line.meta.into_iter().map(|item| {
                            view().flex_shrink(1.0).min_w(Points::ZERO).overflow_hidden().child(item)
                        })),
                ),
        )
        .children(spark)
        .child(small(move || {
            line.at
                .map(|at| SharedString::from(Msg::Ago(time::ago(at, now.get()))))
                .unwrap_or_default()
        }).w(len(Space8) + len(Space6)).text_right())
        .child(action);
    view()
        .border_b_1()
        .border_b_color(color(Border))
        .child(head)
        .child(dynamic(move || match (&group, open.get()) {
            (Some(group), true) => causes(group, &context, now).into_element(),
            _ => nothing(),
        }))
        .into_element()
}

/// A group's 24 hours, one bar an hour.
fn spark(counts: Vec<u32>) -> View {
    let most = counts.iter().copied().max().unwrap_or(0).max(1) as f32;
    view()
        .flex_row()
        .items_end()
        .gap(len(Space1) / 4.0)
        .h(len(Space5))
        .flex_shrink_0()
        .children(counts.into_iter().map(move |n| {
            view()
                .w(len(Space1) / 2.0)
                .h(relative(if n == 0 { 0.08 } else { n as f32 / most }))
                .bg(if n == 0 { color(Ink4) } else { Role::Fail.ink() })
        }))
}

/// A group's causes: each error's first line, how often, about which
/// issues, the latest.
fn causes(group: &SessionGroup, context: &Context, now: Signal<f64>) -> View {
    view()
        .pl(len(Space8) + len(Space4))
        .pr(len(Space4))
        .pb(len(Space3))
        .children(group.causes.iter().map(|cause| cause_row(cause, context, now)))
}

fn cause_row(cause: &Cause, context: &Context, now: Signal<f64>) -> View {
    let about = cause
        .issues
        .iter()
        .map(|(issue, n)| Msg::IssueTimes(agenda(context, issue), *n).to_string())
        .collect::<Vec<_>>()
        .join(" · ");
    let at = cause.last_ms;
    view()
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .h(len(RowLog))
        .border_t_1()
        .border_t_color(color(Border))
        .child(mono(cause.line.clone()).text_color(color(Foreground)).flex_1().min_w(Points::ZERO).number_of_lines(1))
        .child(mono(Msg::Times(cause.count).to_string()).w(len(Space8)))
        .child(small(about).w(len(InspectorWidth) / 2.0).number_of_lines(1))
        .child(small(move || {
            at.map(|at| SharedString::from(Msg::Ago(time::ago(at, now.get()))))
                .unwrap_or_default()
        }))
}

fn line_of(need: &Need, context: &Context) -> Line {
    let at = need.at_ms();
    let sep = || small("·").into_element();
    match need {
        Need::One(Concern::AgendaFailed(entry)) => {
            let note = entry.notes.first().map(|n| first_line(n).to_owned());
            let title = context
                .all_issues
                .get_untracked()
                .and_then(|g| g.nodes.iter().find(|n| n.issue == entry.reference).map(|n| n.title.clone()));
            Line {
                status: status::queue(entry.state),
                title: Msg::AgendaFailed(agenda(context, &entry.reference)).to_string(),
                count: None,
                meta: [title.map(|t| small(t).number_of_lines(1).into_element()), note.map(|n| mono(n).number_of_lines(1).into_element())]
                    .into_iter()
                    .flatten()
                    .collect(),
                at,
            }
        }
        Need::One(Concern::Unreachable {
            program,
            error,
            last_success,
        }) => Line {
            status: status::program(false),
            title: Msg::ProgramUnreachable(*program).to_string(),
            count: None,
            meta: vec![
                error_text(first_line(error).to_owned()).number_of_lines(1).into_element(),
                sep(),
                small(Msg::LastLoaded(last_success.map(time::clock)).to_string()).into_element(),
            ],
            at,
        },
        Need::One(Concern::BaseFailed(base)) => Line {
            status: status::base(BaseState::Failed),
            title: Msg::BaseFailedTitle(base_name(base)).to_string(),
            count: None,
            meta: vec![error_text(first_line(base.error.as_deref().unwrap_or_default()).to_owned())
                .number_of_lines(1)
                .into_element()],
            at,
        },
        Need::Sessions(group) if group.sessions.len() == 1 => {
            let session = &group.sessions[0];
            let about = session.reference.as_ref().map(|r| agenda(context, r));
            Line {
                status: status::session(SessionState::Failed),
                title: Msg::SessionFailedOne {
                    program: group.program,
                    kind: group.kind.clone(),
                    about,
                }
                .to_string(),
                count: None,
                meta: vec![
                    error_text(crate::attention::error_line(session).to_owned())
                        .number_of_lines(1)
                        .into_element(),
                    sep(),
                    mono(session.model.clone()).number_of_lines(1).into_element(),
                ],
                at,
            }
        }
        Need::Sessions(group) => {
            let issues = group
                .issues()
                .iter()
                .map(|(issue, n)| Msg::IssueTimes(agenda(context, issue), *n).to_string())
                .collect::<Vec<_>>()
                .join(" ");
            Line {
                status: status::session(SessionState::Failed),
                title: Msg::SessionsFailed {
                    program: group.program,
                    kind: group.kind.clone(),
                }
                .to_string(),
                count: Some(group.sessions.len()),
                meta: vec![
                    small(issues).into_element(),
                    sep(),
                    mono(group.models().join(", ")).number_of_lines(1).into_element(),
                ],
                at,
            }
        }
        Need::One(Concern::SessionFailed { session, .. }) => Line {
            status: status::session(SessionState::Failed),
            title: session.label.clone(),
            count: None,
            meta: Vec::new(),
            at,
        },
        Need::One(Concern::EscalationOpen(escalation)) => Line {
            status: status::escalated(),
            title: Msg::EscalationTitle(agenda(context, &escalation.agenda)).to_string(),
            count: None,
            meta: vec![small(Msg::EscalationFrom(escalation.issue.to_string(), escalation.ask).to_string()).into_element()],
            at,
        },
        Need::One(Concern::BaseStale(base)) => Line {
            status: status::stale(),
            title: Msg::BaseStaleTitle(base_name(base)).to_string(),
            count: None,
            meta: vec![
                mono(base.stale_reason.clone()).number_of_lines(1).into_element(),
                sep(),
                small(
                    base.warmed_at
                        .as_deref()
                        .and_then(time::parse_ms)
                        .map(|at| Msg::WarmedAgo(time::ago(at, time::now_ms())).to_string())
                        .unwrap_or_default(),
                )
                .into_element(),
            ],
            at,
        },
        Need::One(Concern::RateLow { api, bucket }) => Line {
            status: status::rate_low(),
            title: Msg::RateLowTitle(crate::attention::api_label(*api)).to_string(),
            count: None,
            meta: vec![small(
                Msg::RateLeft {
                    remaining: bucket.remaining,
                    limit: bucket.limit,
                    reset: time::hm(bucket.reset as f64 * 1000.0),
                }
                .to_string(),
            )
            .into_element()],
            at,
        },
    }
}

/// What you do about `need`, as a button.
fn need_action(need: &Need, sources: Sources, context: Rc<Context>, nav: Nav) -> AnyElement {
    let open_issue = |issue: IssueRef, look: Look, label: Msg| {
        let context = context.clone();
        press(label, look, move || context.open_issue(nav, issue.clone())).into_element()
    };
    match need {
        Need::One(Concern::AgendaFailed(entry)) => {
            open_issue(entry.reference.clone(), Look::Primary, Msg::OpenIssue)
        }
        Need::One(Concern::Unreachable { program, .. }) => {
            let program = match program {
                Program::Council => sources.council,
                Program::Guardians => sources.guardians,
            };
            press(Msg::Retry, Look::Plain, move || program.health.refetch()).into_element()
        }
        Need::One(Concern::BaseFailed(base)) | Need::One(Concern::BaseStale(base)) => {
            rewarm_button(base.clone(), sources, context.clone()).into_element()
        }
        Need::Sessions(group) if group.sessions.len() == 1 => {
            let open = OpenSession {
                program: group.program,
                id: group.sessions[0].id.clone(),
            };
            press(Msg::ViewSession, Look::Plain, move || nav.open_session(open.clone())).into_element()
        }
        Need::Sessions(group) => {
            let (program, kind) = (group.program, group.kind.clone());
            let sessions = context.sessions;
            press(Msg::ViewSessions(group.sessions.len()), Look::Plain, move || {
                batch(|| {
                    sessions.focus_failed(program, kind.clone());
                    nav.page.set(Page::Sessions);
                })
            })
            .into_element()
        }
        Need::One(Concern::SessionFailed { program, session }) => {
            let open = OpenSession {
                program: *program,
                id: session.id.clone(),
            };
            press(Msg::ViewSession, Look::Plain, move || nav.open_session(open.clone())).into_element()
        }
        Need::One(Concern::EscalationOpen(escalation)) => open_issue(
            escalation.agenda.clone(),
            Look::Plain,
            Msg::OpenAgenda(agenda(&context, &escalation.agenda)),
        ),
        Need::One(Concern::RateLow { .. }) => nothing(),
    }
}

// ---------------------------------------------------------------------------
// 议会队列

fn queue_section(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let note = {
        let context = context.clone();
        move || {
            context
                .all_issues
                .get()
                .map(|g| {
                    SharedString::from(Msg::OpenCount(g.nodes.iter().filter(|n| !n.state.closed()).count()))
                })
                .unwrap_or_default()
        }
    };
    let link = action(Msg::AllIssues, Some(Icon::ArrowRight), Look::Link, || false, move || {
        nav.page.set(Page::Issues)
    })
    .into_element();
    view()
        .gap(len(Space2))
        .child(section_head(Msg::CouncilQueue, note, vec![link]))
        .child(
            panel()
                .child(source_bar(sources.queue, 4))
                .child(github_bar(sources.credential, sources.project, 0))
                .child(dynamic(move || {
                    let Some(graph) = context.all_issues.get() else {
                        return nothing();
                    };
                    let now = context.now;
                    let context = context.clone();
                    untracked(move || {
                        let open: Vec<usize> =
                            (0..graph.nodes.len()).filter(|&i| !graph.nodes[i].state.closed()).collect();
                        if open.is_empty() {
                            return super::parts::empty(Msg::QueueEmpty).into_element();
                        }
                        view()
                            .children(open.into_iter().map(|i| {
                                let node = graph.nodes[i].clone();
                                let state = status::node(&node.state, &context.issues_settings.fields);
                                let issue = node.issue.clone();
                                let context2 = context.clone();
                                let updated = time::parse_ms(&node.updated_at);
                                row(|| false, node.title.clone(), move || {
                                    context2.open_issue(nav, issue.clone())
                                })
                                .child(status::glyph(state.glyph, state.role))
                                .child(
                                    small(node.agenda.clone().unwrap_or_default())
                                        .w(len(Space8) + len(Space2)),
                                )
                                .child(
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .gap(len(Space2))
                                        .flex_1()
                                        .min_w(Points::ZERO)
                                        .overflow_hidden()
                                        .child(text(node.title.clone()).ty(Type::Body).flex_shrink(1.0).number_of_lines(1))
                                        .children(issues::human_tag(&node)),
                                )
                                .child(view().w(len(Space8) * 4.0).flex_shrink_0().overflow_hidden().child(progress(&graph, i)))
                                .child(view().w(len(Space8) * 2.0).flex_shrink_0().overflow_hidden().child(deps(&graph, i)))
                                .child(small(move || {
                                    updated
                                        .map(|at| SharedString::from(Msg::Span(time::ago(at, now.get()))))
                                        .unwrap_or_default()
                                })
                                .w(len(Space8) + len(Space4)).text_right())
                                .into_element()
                            }))
                            .into_element()
                    })
                })),
        )
}

// ---------------------------------------------------------------------------
// The rail

fn rail_link(nav: Nav, page: Page) -> AnyElement {
    action(Msg::View, Some(Icon::ArrowRight), Look::Link, || false, move || nav.page.set(page))
        .into_element()
}

fn programs_panel(sources: Sources, context: Rc<Context>) -> View {
    panel()
        .child(panel_head(Msg::Programs, "", Vec::new()))
        .child(source_bar(sources.council.health, 0))
        .child(source_bar(sources.guardians.health, 0))
        .child(program_row(sources, sources.council, context.clone()))
        .child(program_row(sources, sources.guardians, context))
}

/// A program: its state, how long it runs, its work, its short version
/// (the full one is its tooltip).
fn program_row(sources: Sources, program: ProgramSources, context: Rc<Context>) -> View {
    let now = context.now;
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Row) + len(Space2))
        .px(len(Space4))
        .child(dynamic(move || {
            let failed = program.health.with_error(|e| e.is_some());
            let state = status::program(!failed);
            status::glyph(state.glyph, state.role).into_element()
        }))
        .child(text(SharedString::from(Msg::Program(program.program))).ty(Type::BodyStrong).flex_shrink_0())
        .child(small(move || {
            let failed = program.health.with_error(|e| e.is_some());
            let up = program
                .health
                .with(|l| l.and_then(|l| time::parse_ms(&l.data.started_at)))
                .map(|at| time::ago(at, now.get()));
            let work = match program.program {
                Program::Council => Msg::Deliberating(sources.queue.with(|q| {
                    q.map(|q| q.data.iter().filter(|e| e.state == QueueState::Deliberating).count())
                })),
                Program::Guardians => Msg::AsksRunning(running_asks(program)),
            };
            SharedString::from(Msg::ProgramLine {
                online: !failed,
                up,
                work: Box::new(work),
            })
        }).flex_1().min_w(Points::ZERO).number_of_lines(1))
        .child(dynamic(move || {
            match program.health.with(|l| l.map(|l| (l.data.short_version().to_owned(), l.data.version.clone()))) {
                Some((short, full)) => small(short).truncate().tooltip(hint(full)).into_element(),
                None => nothing(),
            }
        }))
}

/// The guardians' ask sessions under way (`query` sessions running).
pub fn running_asks(guardians: ProgramSources) -> Option<usize> {
    guardians.sessions.with(|l| {
        l.map(|l| {
            l.data
                .iter()
                .filter(|s| s.kind == "query" && s.state == SessionState::Running)
                .count()
        })
    })
}

fn sessions_panel(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let hours = context.hours();
    panel()
        .child(panel_head(Msg::PageSessions, SharedString::from(Msg::Hours(hours)), vec![rail_link(nav, Page::Sessions)]))
        .child(source_bar(sources.council.sessions, 0))
        .child(source_bar(sources.guardians.sessions, 0))
        .child(dynamic(move || {
            let since = time::now_ms() - f64::from(hours) * 3_600_000.0;
            let recent: Vec<_> = sessions::all(sources)
                .into_iter()
                .filter(|(_, s)| time::parse_ms(&s.started_at).is_some_and(|at| at >= since))
                .collect();
            untracked(move || {
                let count = |state| recent.iter().filter(|(_, s)| s.state == state).count();
                let figure = |n: usize, label: Msg, role: Option<Role>| {
                    view()
                        .flex_1()
                        .child(
                            text(n.to_string())
                                .ty(Type::Num)
                                .text_color(role.map_or(color(Foreground), Role::text)),
                        )
                        .child(small(SharedString::from(label)))
                };
                let failed = count(SessionState::Failed);
                view()
                    .px(len(Space4))
                    .py(len(Space3))
                    .gap(len(Space3))
                    .child(
                        view()
                            .flex_row()
                            .child(figure(recent.len(), Msg::Total, None))
                            .child(figure(count(SessionState::Running), Msg::Session(SessionState::Running), None))
                            .child(figure(failed, Msg::Session(SessionState::Failed), (failed > 0).then_some(Role::Fail))),
                    )
                    .child(histogram(&recent, hours as usize, len(Space8) + len(Space4)))
                    .into_element()
            })
        }))
}

fn bases_panel(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let note = move || {
        sources
            .bases
            .with(|b| {
                b.map(|b| {
                    let ready = b.data.iter().filter(|b| b.state == BaseState::Ready).count();
                    let stale = b.data.iter().filter(|b| b.stale).count();
                    SharedString::from(Msg::BasesNote {
                        ready,
                        total: b.data.len(),
                        stale,
                    })
                })
            })
            .unwrap_or_default()
    };
    panel()
        .child(panel_head(Msg::Bases, note, vec![rail_link(nav, Page::Runtime)]))
        .child(source_bar(sources.bases, 3))
        .child(dynamic(move || {
            let Some(bases) = sources.bases.get().map(|l| l.data) else {
                return nothing();
            };
            let now = context.now;
            untracked(move || {
                if bases.is_empty() {
                    return super::parts::empty(Msg::NoBases).into_element();
                }
                view()
                    .children(bases.into_iter().map(|base| {
                        let state = if base.stale { status::stale() } else { status::base(base.state) };
                        let warmed = base.warmed_at.as_deref().and_then(time::parse_ms);
                        view()
                            .flex_row()
                            .items_center()
                            .gap(len(Space2))
                            .h(len(Row) + len(Space1))
                            .px(len(Space4))
                            .border_t_1()
                            .border_t_color(color(Border))
                            .child(status::glyph(state.glyph, state.role))
                            .child(
                                text(super::runtime::short_repo(&base.repo))
                                    .ty(Type::Body)
                                    .number_of_lines(1),
                            )
                            .child(
                                small(Msg::ModelNote {
                                    model: super::runtime::short_model(&base.model),
                                    stale: base.stale,
                                }
                                .to_string())
                                .flex_1()
                                .number_of_lines(1),
                            )
                            .child(small(move || {
                                warmed
                                    .map(|at| SharedString::from(Msg::Ago(time::ago(at, now.get()))))
                                    .unwrap_or_default()
                            }))
                            .into_element()
                    }))
                    .into_element()
            })
        }))
}

/// The sessions with an error in the window, counted by the error's first
/// line, the most first.
fn errors_panel(sources: Sources, context: Rc<Context>) -> View {
    let hours = context.hours();
    let errors = move || {
        let since = time::now_ms() - f64::from(hours) * 3_600_000.0;
        let mut counts: Vec<(String, usize)> = Vec::new();
        let mut total = 0;
        for (_, session) in sessions::all(sources) {
            let Some(error) = session.error.as_deref() else {
                continue;
            };
            if !time::parse_ms(&session.started_at).is_some_and(|at| at >= since) {
                continue;
            }
            total += 1;
            let line = first_line(error).to_owned();
            match counts.iter_mut().find(|(l, _)| *l == line) {
                Some((_, n)) => *n += 1,
                None => counts.push((line, 1)),
            }
        }
        counts.sort_by_key(|c| std::cmp::Reverse(c.1));
        (counts, total)
    };
    panel()
        .child(panel_head(
            Msg::SessionErrors,
            move || SharedString::from(Msg::ErrorsNote { hours, sessions: errors().1 }),
            Vec::new(),
        ))
        .child(dynamic(move || {
            let (counts, _) = errors();
            untracked(move || {
                if counts.is_empty() {
                    return super::parts::empty(Msg::NoErrors).into_element();
                }
                let most = counts[0].1.max(1) as f32;
                view()
                    .py(len(Space2))
                    .children(counts.into_iter().take(8).map(|(line, n)| {
                        view()
                            .flex_row()
                            .items_center()
                            .gap(len(Space3))
                            .h(len(RowLog))
                            .px(len(Space4))
                            .child(mono(line).text_color(color(Foreground)).flex_1().min_w(Points::ZERO).number_of_lines(1))
                            .child(view().w(len(Space8) * 2.0).child(meter(n as f32 / most, color(Ink3))))
                            .child(small(n.to_string()).w(len(Space5)).text_right())
                            .into_element()
                    }))
                    .into_element()
            })
        }))
}
