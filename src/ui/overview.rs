//! The overview (C4 `overviewView`): what needs a person, then a card per
//! part of PM. Each card shows its own sources' state and data; a failing
//! source leaves the others as they are.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::{
    parts::{badge, card, count_row, maybe_link, note, source_status},
    theme::{DONE, FAILED, HUMAN, IN_PROGRESS, SUPERSEDED, TODO, WAITING},
};
use crate::{
    attention::{self, Concern, Inputs, Links, ProgramView, first_line},
    config::{Attention, Digest, Fields},
    gh::project::{Kind, ProjectItem, Status},
    programs::{AgendaKind, Base, BaseState, ProgramClient, QueueEntry, QueueState, Session},
    sources::{ProgramSources, Sources},
    time,
};

/// What the pages need besides their sources.
pub struct Context {
    pub council: ProgramClient,
    pub guardians: ProgramClient,
    pub attention: Attention,
    pub fields: Fields,
    pub digest: Digest,
}

/// The overview page's content.
pub fn overview(sources: Sources, context: Rc<Context>) -> View {
    view()
        .gap_4()
        .p_4()
        .child(attention_card(sources, context.clone()))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_4()
                .children([
                    agenda_card(sources),
                    implementation_card(sources, context),
                    bases_card(sources),
                    errors_card(sources),
                    health_card(sources),
                ]
                .map(|card| card.flex_grow(1.0).basis(px(340.0)).min_w(px(280.0)))),
        )
}

fn empty() -> AnyElement {
    view().into_element()
}

// ---------------------------------------------------------------------------
// 需要人处理

fn program_view<'a>(
    program: &ProgramSources,
    health_error: &'a Option<String>,
    health_at: Option<f64>,
    sessions: &'a Option<Vec<Session>>,
) -> ProgramView<'a> {
    ProgramView {
        program: program.program,
        unreachable: health_error.as_deref().map(|error| (error, health_at)),
        sessions: sessions.as_deref(),
    }
}

fn attention_card(sources: Sources, context: Rc<Context>) -> View {
    let rows = dynamic(move || {
        let data = |s: &crate::sources::Source<Vec<Session>>| s.get().map(|l| l.data);
        let bases = sources.bases.get().map(|l| l.data);
        let queue = sources.queue.get().map(|l| l.data);
        let escalations = sources.escalations.get().map(|l| l.data);
        let rate = sources.rate.get().map(|l| l.data);
        let project = sources.project.get().map(|l| l.data);
        let council_sessions = data(&sources.council.sessions);
        let guardians_sessions = data(&sources.guardians.sessions);
        let council_error = sources.council.health.error();
        let guardians_error = sources.guardians.health.error();
        let council_at = sources.council.health.with(|l| l.map(|l| l.at));
        let guardians_at = sources.guardians.health.with(|l| l.map(|l| l.at));
        let concerns = attention::concerns(
            Inputs {
                bases: bases.as_deref(),
                queue: queue.as_deref(),
                escalations: escalations.as_deref(),
                rate: rate.as_ref(),
                council: program_view(
                    &sources.council,
                    &council_error,
                    council_at,
                    &council_sessions,
                ),
                guardians: program_view(
                    &sources.guardians,
                    &guardians_error,
                    guardians_at,
                    &guardians_sessions,
                ),
                now_ms: time::now_ms(),
            },
            &context.attention,
        );
        let links = Links {
            council: &context.council,
            guardians: &context.guardians,
            project: project.as_deref(),
        };
        let rows: Vec<_> = concerns
            .iter()
            .map(|concern| (concern_color(concern), attention::describe(concern, &links)))
            .collect();
        untracked(move || {
            if rows.is_empty() {
                return text("没有需要处理的事")
                    .text_sm()
                    .text_color(DONE)
                    .into_element();
            }
            view()
                .gap_1()
                .children(rows.into_iter().map(|(color, row)| {
                    maybe_link(
                        row.link,
                        view()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_1()
                            .py_1()
                            .child(badge(row.category, color))
                            .child(text(row.text).text_sm().flex_1().number_of_lines(2)),
                    )
                }))
                .into_element()
        })
    });
    card("需要人处理", Vec::new(), rows)
}

fn concern_color(concern: &Concern) -> AdaptiveColor {
    match concern {
        Concern::BaseFailed(_)
        | Concern::SessionFailed { .. }
        | Concern::AgendaFailed(_)
        | Concern::Unreachable { .. } => FAILED,
        Concern::BaseStale(_) | Concern::EscalationOpen(_) | Concern::RateLow { .. } => WAITING,
    }
}

// ---------------------------------------------------------------------------
// 议题

/// The queue's states, in the order the card lists them.
const QUEUE_STATES: [(QueueState, &str, AdaptiveColor); 8] = [
    (QueueState::Deliberating, "在审", IN_PROGRESS),
    (QueueState::Blocked, "等前置", WAITING),
    (QueueState::Held, "被人类议题挂起", WAITING),
    (QueueState::Ready, "可召集", TODO),
    (QueueState::Interrupted, "中断", WAITING),
    (QueueState::Failed, "失败", FAILED),
    (QueueState::Decided, "已定案", DONE),
    (QueueState::Superseded, "已取代", SUPERSEDED),
];

fn agenda_card(sources: Sources) -> View {
    let body = dynamic(move || {
        let Some(queue) = sources.queue.get().map(|l| l.data) else {
            return empty();
        };
        untracked(move || agenda_counts(&queue))
    });
    card(
        "议题",
        vec![source_status("议会队列", sources.queue)],
        body,
    )
}

fn agenda_counts(queue: &[QueueEntry]) -> AnyElement {
    let count = |state: QueueState| queue.iter().filter(|e| e.state == state).count();
    let human_open = queue
        .iter()
        .filter(|e| {
            e.kind == AgendaKind::Human
                && !matches!(e.state, QueueState::Decided | QueueState::Superseded)
        })
        .count();
    view()
        .gap_1()
        .children(
            QUEUE_STATES.map(|(state, label, color)| count_row(color, label, count(state))),
        )
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .pt_1()
                .child(badge("人类", HUMAN))
                .child(text(format!("人类议题未定案 {human_open} 个")).text_sm()),
        )
        .child(note(format!("共 {} 个议题", queue.len())))
        .into_element()
}

// ---------------------------------------------------------------------------
// 实现

fn implementation_card(sources: Sources, context: Rc<Context>) -> View {
    let body = dynamic(move || {
        let Some(items) = sources.project.get().map(|l| l.data) else {
            return empty();
        };
        let escalations = sources.escalations.with(|l| l.map(|l| l.data.len()));
        let context = context.clone();
        untracked(move || implementation_counts(&items, escalations, &context.fields))
    });
    card(
        "实现",
        vec![
            source_status("Project", sources.project),
            source_status("提为议题", sources.escalations),
        ],
        body,
    )
}

fn implementation_counts(
    items: &[ProjectItem],
    escalations: Option<usize>,
    fields: &Fields,
) -> AnyElement {
    let implementations: Vec<_> = items
        .iter()
        .filter(|item| item.kind == Some(Kind::Implementation))
        .collect();
    let count = |want: fn(&Option<Status>) -> bool| {
        implementations
            .iter()
            .filter(|item| want(&item.status))
            .count()
    };
    let options = &fields.status.options;
    view()
        .gap_1()
        .child(count_row(
            TODO,
            options.todo.to_string(),
            count(|s| *s == Some(Status::Todo)),
        ))
        .child(count_row(
            IN_PROGRESS,
            options.in_progress.to_string(),
            count(|s| *s == Some(Status::InProgress)),
        ))
        .child(count_row(
            DONE,
            options.done.to_string(),
            count(|s| *s == Some(Status::Done)),
        ))
        .child(count_row(
            SUPERSEDED,
            options.superseded.to_string(),
            count(|s| *s == Some(Status::Superseded)),
        ))
        .child(count_row(
            TODO,
            format!("{} 未设置或为其他选项", fields.status.name),
            count(|s| matches!(s, None | Some(Status::Other(_)))),
        ))
        .child(count_row(
            WAITING,
            "提为议题未了结",
            escalations.unwrap_or_default(),
        ))
        .child(note(format!(
            "{}={} 共 {} 个（Project 共 {} 个 issue）",
            fields.kind.name,
            fields.kind.options.implementation,
            implementations.len(),
            items.len()
        )))
        .into_element()
}

// ---------------------------------------------------------------------------
// 守护基座

fn bases_card(sources: Sources) -> View {
    let body = dynamic(move || {
        let Some(bases) = sources.bases.get().map(|l| l.data) else {
            return empty();
        };
        untracked(move || bases_view(&bases))
    });
    card("守护基座", vec![source_status("基座", sources.bases)], body)
}

pub(super) fn base_state(state: BaseState) -> (&'static str, AdaptiveColor) {
    match state {
        BaseState::Warming => ("预热中", IN_PROGRESS),
        BaseState::Ready => ("就绪", DONE),
        BaseState::Failed => ("失败", FAILED),
    }
}

fn bases_view(bases: &[Base]) -> AnyElement {
    let count = |state: BaseState| bases.iter().filter(|b| b.state == state).count();
    view()
        .gap_1()
        .children(
            [BaseState::Ready, BaseState::Warming, BaseState::Failed].map(|state| {
                let (label, color) = base_state(state);
                count_row(color, label, count(state))
            }),
        )
        .child(count_row(
            WAITING,
            "过期",
            bases.iter().filter(|b| b.stale).count(),
        ))
        .child(view().h(px(1.0)).my_1().bg(move || theme().border))
        .children(bases.iter().map(|base| {
            let (label, color) = base_state(base.state);
            view()
                .gap(px(2.0))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(badge(label, color))
                        .when(base.stale, |row| row.child(badge("过期", WAITING)))
                        .child(
                            text(format!("{} · {}", base.guardian, base.repo))
                                .text_sm()
                                .flex_1()
                                .number_of_lines(1),
                        ),
                )
                .child(note(format!(
                    "{}{}",
                    base.model,
                    base.warmed_at
                        .as_deref()
                        .map(|at| format!(" · 预热于 {}", time::short(at)))
                        .unwrap_or_default()
                )))
                .when(base.stale, |row| {
                    row.child(note(format!("过期原因：{}", base.stale_reason)))
                })
                .when_some(base.error.clone(), |row, error| {
                    row.child(
                        text(first_line(&error).to_owned())
                            .text_xs()
                            .text_color(FAILED)
                            .number_of_lines(2),
                    )
                })
        }))
        .into_element()
}

// ---------------------------------------------------------------------------
// 最近 LLM 异常

/// How many errored sessions the card lists.
const ERRORS_SHOWN: usize = 8;

fn errors_card(sources: Sources) -> View {
    let body = dynamic(move || {
        let council = sources.council.sessions.get().map(|l| l.data);
        let guardians = sources.guardians.sessions.get().map(|l| l.data);
        untracked(move || {
            let mut errored: Vec<(&'static str, Session)> = [
                (sources.council.program.label(), council),
                (sources.guardians.program.label(), guardians),
            ]
            .into_iter()
            .flat_map(|(label, sessions)| {
                sessions
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|session| session.error.is_some())
                    .map(move |session| (label, session))
            })
            .collect();
            errored.sort_by(|a, b| {
                time::parse_ms(&b.1.started_at)
                    .partial_cmp(&time::parse_ms(&a.1.started_at))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            if errored.is_empty() {
                return note("没有带异常的会话").into_element();
            }
            view()
                .gap_2()
                .children(errored.into_iter().take(ERRORS_SHOWN).map(|(label, session)| {
                    view()
                        .gap(px(2.0))
                        .child(
                            view()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(badge(label, TODO))
                                .child(
                                    text(format!("{} · {}", session.kind, session.model))
                                        .text_sm()
                                        .flex_1()
                                        .number_of_lines(1),
                                )
                                .child(note(time::short(&session.started_at))),
                        )
                        .child(
                            text(first_line(session.error.as_deref().unwrap_or_default()).to_owned())
                                .text_xs()
                                .text_color(FAILED)
                                .number_of_lines(2),
                        )
                }))
                .into_element()
        })
    });
    card(
        "最近 LLM 异常",
        vec![
            source_status("议会会话", sources.council.sessions),
            source_status("守护会话", sources.guardians.sessions),
        ],
        body,
    )
}

// ---------------------------------------------------------------------------
// 程序健康

fn health_card(sources: Sources) -> View {
    card(
        "程序健康",
        Vec::new(),
        view()
            .gap_3()
            .child(program_health(sources.council))
            .child(program_health(sources.guardians))
            .into_element(),
    )
}

fn program_health(program: ProgramSources) -> View {
    let health = program.health;
    let reachable = move || health.with_error(|error| error.is_none()) && health.with(|l| l.is_some());
    view()
        .gap(px(2.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    view()
                        .size(px(8.0))
                        .rounded(px(4.0))
                        .bg(move || if reachable() { DONE } else if health.with_error(|e| e.is_some()) { FAILED } else { TODO }),
                )
                .child(
                    text(format!("{}程序", program.program.label()))
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .flex_1(),
                )
                .child(
                    text(move || {
                        if health.with_error(|e| e.is_some()) {
                            "不可达".to_owned()
                        } else {
                            health
                                .with(|l| l.map(|l| format!("版本 {}", l.data.version)))
                                .unwrap_or_default()
                        }
                    })
                    .text_sm(),
                ),
        )
        .child(note(move || {
            health
                .with(|l| {
                    l.map(|l| {
                        format!(
                            "{} · 启动于 {}",
                            l.data.component,
                            time::short(&l.data.started_at)
                        )
                    })
                })
                .unwrap_or_default()
        }))
        .child(source_status("健康检查", health))
}
