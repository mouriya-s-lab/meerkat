//! The overview (C4 `overviewView`), top to bottom: five KPIs, what needs
//! a person (failures first, each with its action), the queue's progress,
//! the programs, and the recent LLM errors. Each part shows its own
//! sources' trouble; a failing source leaves the others as they are.

use std::rc::Rc;

use nsh_core::prelude::*;

use super::{
    guardians::rewarm_button,
    parts::{
        Look, Span, body_row, card, card_head, chip, head_row, ink2, meta, nothing, press,
        source_trouble, th,
    },
    sessions::{self, OpenSession},
    shell::{Context, Nav, Page, critical, plain, strong},
    status::{self, Role, Status},
    tokens::{Type, Typeset, color, len},
};
use crate::{
    attention::{Concern, api_label, first_line},
    config::{ColorToken::*, LengthToken::*},
    gh::project::{Kind, ProjectItem, Status as ProjectStatus},
    programs::{BaseState, Program, QueueEntry, QueueState, Session},
    sources::{ProgramSources, Sources},
    time,
};

/// The overview page's content.
pub fn overview(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    view()
        .gap(len(Space5))
        .px(len(Space6))
        .pt(len(Space5))
        .pb(len(Space6))
        .child(kpis(sources, context.clone()))
        .child(queue_card(sources, context.clone(), nav))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap(len(Space5))
                .child(
                    progress_card(sources, nav)
                        .flex_grow(1.4)
                        .basis(Points::ZERO)
                        .min_w(len(DagNodeWidth)),
                )
                .child(
                    programs_card(sources, context.clone())
                        .flex_grow(1.0)
                        .basis(Points::ZERO)
                        .min_w(len(DagNodeWidth)),
                ),
        )
        .child(errors_card(sources, context, nav))
}

/// The page's sentence: how much needs a person and how much of it
/// failed, whether both programs answer, whether the API runs low.
pub fn summary(sources: Sources, context: &Context) -> Vec<Span> {
    let concerns = context.concerns.get();
    let failed = concerns.iter().filter(|c| c.failure()).count();
    let mut spans = if concerns.is_empty() {
        vec![plain("没有需要你处理的事。")]
    } else {
        vec![
            critical(format!("{} 件事", concerns.len())),
            plain("需要你处理，其中 "),
            strong(failed),
            plain(" 件失败。"),
        ]
    };
    let down: Vec<&str> = [sources.council, sources.guardians]
        .into_iter()
        .filter(|p| p.health.with_error(|e| e.is_some()))
        .map(|p| p.program.label())
        .collect();
    spans.push(match down.as_slice() {
        [] => plain("议会与守护都在线"),
        down => critical(format!("{}不可达", down.join("与"))),
    });
    let low = concerns
        .iter()
        .any(|c| matches!(c, Concern::RateLow { .. }));
    spans.push(plain(if low {
        "；GitHub API 余量偏低。"
    } else {
        "。"
    }));
    spans
}

// ---------------------------------------------------------------------------
// KPI

fn kpi(label: impl Into<SharedString>, value: AnyElement, foot: Vec<Span>) -> View {
    view()
        .flex_grow(1.0)
        .basis(Points::ZERO)
        .min_w(Points::ZERO)
        .gap(len(Space1))
        .px(len(Space4))
        .py(len(Space3))
        .child(meta(label.into()))
        .child(value)
        .child(
            super::parts::sentence(foot)
                .ty(Type::Meta)
                .number_of_lines(1),
        )
}

fn big(value: impl ToString, role: Option<Role>) -> Text {
    text(value.to_string())
        .ty(Type::Kpi)
        .font_weight(FontWeight::SEMIBOLD)
        .tabular_nums()
        .text_color(role.map_or(color(Foreground), Role::text))
}

fn count_of(n: usize, word: &str, role: Role) -> Span {
    if n == 0 {
        plain(format!("{n} {word}"))
    } else {
        match role {
            Role::Critical => critical(format!("{n} {word}")),
            _ => strong(format!("{n} {word}")),
        }
    }
}

fn kpis(sources: Sources, context: Rc<Context>) -> View {
    let cells = dynamic(move || {
        let concerns = context.concerns.get();
        let queue = sources.queue.get().map(|l| l.data);
        let items = sources.project.get().map(|l| l.data);
        let escalations = sources.escalations.with(|l| l.map(|l| l.data.len()));
        let bases = sources.bases.get().map(|l| l.data);
        let errors = recent_errors(sources, &context);
        let context = context.clone();
        untracked(move || {
            let failed = concerns.iter().filter(|c| c.failure()).count();
            let unknown = || big("—", None).into_element();
            let attention = kpi(
                "需要处理",
                big(concerns.len(), (failed > 0).then_some(Role::Critical)).into_element(),
                vec![
                    count_of(failed, "失败", Role::Critical),
                    plain(" · "),
                    count_of(concerns.len() - failed, "待查看", Role::Warning),
                ],
            );
            let agenda = match &queue {
                Some(queue) => {
                    let n = |s: QueueState| queue.iter().filter(|e| e.state == s).count();
                    kpi(
                        "议题",
                        big(queue.len(), None).into_element(),
                        vec![
                            plain(format!(
                                "在审 {} · 可召集 {} · ",
                                n(QueueState::Deliberating),
                                n(QueueState::Ready)
                            )),
                            count_of(n(QueueState::Failed), "失败", Role::Critical),
                        ],
                    )
                }
                None => kpi("议题", unknown(), vec![plain("议会队列未读到")]),
            };
            let implementations = match &items {
                Some(items) => {
                    let all: Vec<&ProjectItem> = items
                        .iter()
                        .filter(|i| i.kind == Some(Kind::Implementation))
                        .collect();
                    let doing = all
                        .iter()
                        .filter(|i| i.status == Some(ProjectStatus::InProgress))
                        .count();
                    kpi(
                        "实现 issue",
                        big(all.len(), None).into_element(),
                        vec![plain(format!(
                            "进行中 {doing} · 询问待答 {}",
                            escalations.map_or("—".to_owned(), |n| n.to_string())
                        ))],
                    )
                }
                None => kpi("实现 issue", unknown(), vec![plain("Project 未读到")]),
            };
            let bases_kpi = match &bases {
                Some(bases) => {
                    let ready = bases.iter().filter(|b| b.state == BaseState::Ready).count();
                    let failed = bases
                        .iter()
                        .filter(|b| b.state == BaseState::Failed)
                        .count();
                    let stale = bases.iter().filter(|b| b.stale).count();
                    kpi(
                        "守护基座",
                        view()
                            .flex_row()
                            .items_baseline()
                            .gap(len(Space1))
                            .child(big(ready, None))
                            .child(ink2(format!("/ {} 就绪", bases.len()), Type::Heading))
                            .into_element(),
                        vec![
                            count_of(failed, "失败", Role::Critical),
                            plain(" · "),
                            count_of(stale, "过期", Role::Warning),
                        ],
                    )
                }
                None => kpi("守护基座", unknown(), vec![plain("基座未读到")]),
            };
            let latest = errors.first().map(|(_, s)| {
                format!(
                    "最近 {} · {}",
                    time::parse_ms(&s.started_at)
                        .map(time::clock)
                        .unwrap_or_default(),
                    first_line(s.error.as_deref().unwrap_or_default())
                )
            });
            let llm = kpi(
                format!(
                    "LLM 异常 · {} 小时",
                    context.attention.session_failure_hours
                ),
                big(errors.len(), None).into_element(),
                vec![plain(latest.unwrap_or_else(|| "没有".to_owned()))],
            );
            view()
                .flex_row()
                .flex_wrap()
                .children(
                    [attention, agenda, implementations, bases_kpi, llm]
                        .into_iter()
                        .enumerate()
                        .map(|(i, cell)| {
                            cell.when(i > 0, |c| c.border_l_1().border_l_color(color(Border)))
                        }),
                )
                .into_element()
        })
    });
    card().child(cells)
}

// ---------------------------------------------------------------------------
// 需要人处理

/// A concern as a queue row shows it.
struct Row {
    status: Status,
    category: &'static str,
    title: String,
    reason: String,
    at: Option<f64>,
}

fn base_title(base: &crate::programs::Base) -> String {
    format!("守护 · {} · {} · {}", base.guardian, base.repo, base.model)
}

fn project_item<'a>(
    items: Option<&'a [ProjectItem]>,
    issue: &crate::gh::IssueRef,
) -> Option<&'a ProjectItem> {
    items?.iter().find(|item| item.issue == *issue)
}

fn row_of(concern: &Concern, items: Option<&[ProjectItem]>, context: &Context) -> Row {
    let at = concern.at_ms();
    match concern {
        Concern::BaseFailed(base) => Row {
            status: status::base(BaseState::Failed),
            category: "基座失败",
            title: base_title(base),
            reason: base
                .error
                .as_deref()
                .map(first_line)
                .unwrap_or("没有错误原文")
                .to_owned(),
            at,
        },
        Concern::BaseStale(base) => Row {
            status: status::stale(),
            category: "基座过期",
            title: base_title(base),
            reason: base.stale_reason.clone(),
            at,
        },
        Concern::SessionFailed { program, session } => Row {
            status: status::session(session.state),
            category: "会话失败",
            title: format!("{} · {}", program.label(), sessions::title(session)),
            reason: format!(
                "{}（{}）",
                session
                    .error
                    .as_deref()
                    .map(first_line)
                    .unwrap_or("没有错误原文"),
                session.model
            ),
            at,
        },
        Concern::AgendaFailed(entry) => {
            let item = project_item(items, &entry.reference);
            Row {
                status: status::queue(entry.state),
                category: "议题失败",
                title: format!(
                    "#{}{}{}",
                    entry.reference.number,
                    entry
                        .agenda
                        .as_deref()
                        .map(|a| format!(" {a}"))
                        .unwrap_or_default(),
                    item.map(|i| format!(" {}", i.title)).unwrap_or_default()
                ),
                reason: entry
                    .notes
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "议会队列判为失败".to_owned()),
                at,
            }
        }
        Concern::EscalationOpen(escalation) => Row {
            status: status::escalated(),
            category: "询问已提为议题",
            title: format!(
                "{} 的询问 {} → #{}",
                escalation.issue, escalation.ask, escalation.agenda.number
            ),
            reason: format!("尚无答复或取代；议会尚未定案 #{}", escalation.agenda.number),
            at,
        },
        Concern::Unreachable {
            program,
            error,
            last_success,
        } => Row {
            status: status::program(false),
            category: "程序不可达",
            title: format!(
                "{}程序 · {}",
                program.label(),
                context.program(*program).base.as_str()
            ),
            reason: format!(
                "{}（{}）",
                first_line(error),
                last_success
                    .map(|at| format!("上次成功 {}", time::clock(at)))
                    .unwrap_or_else(|| "从未成功".to_owned())
            ),
            at,
        },
        Concern::RateLow { api, bucket } => Row {
            status: status::rate_low(),
            category: "API 余量低",
            title: format!(
                "GitHub {} 剩 {} / {}",
                api_label(*api),
                bucket.remaining,
                bucket.limit
            ),
            reason: format!(
                "{} 重置；刷新间隔 {} 秒",
                time::clock(bucket.reset as f64 * 1000.0),
                context.refresh_seconds
            ),
            at,
        },
    }
}

/// What a person does about `concern`, as a button.
fn concern_action(
    concern: &Concern,
    items: Option<&[ProjectItem]>,
    sources: Sources,
    context: Rc<Context>,
    nav: Nav,
) -> AnyElement {
    let open = |label: String, url: Option<String>| match url {
        Some(url) => press(label, Look::Plain, true, move || {
            drop(open_url(url.clone()))
        })
        .into_element(),
        None => nothing(),
    };
    match concern {
        Concern::BaseFailed(base) | Concern::BaseStale(base) => {
            rewarm_button(base.clone(), sources, context).into_element()
        }
        Concern::SessionFailed { program, session } => {
            let session = OpenSession {
                program: *program,
                id: session.id.clone(),
            };
            press("查看会话", Look::Plain, true, move || {
                nav.open_session(session.clone())
            })
            .into_element()
        }
        Concern::AgendaFailed(entry) => open(
            "打开议题".to_owned(),
            project_item(items, &entry.reference).map(|i| i.url.clone()),
        ),
        Concern::EscalationOpen(escalation) => open(
            format!("打开 #{}", escalation.agenda.number),
            Some(
                project_item(items, &escalation.agenda)
                    .map(|i| i.url.clone())
                    .unwrap_or_else(|| escalation.url.clone()),
            ),
        ),
        Concern::Unreachable { .. } | Concern::RateLow { .. } => nothing(),
    }
}

fn queue_card(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let rows = dynamic(move || {
        let concerns = context.concerns.get();
        let items = sources.project.get().map(|l| l.data);
        let now = context.now.get();
        let context = context.clone();
        untracked(move || {
            if concerns.is_empty() {
                return view()
                    .px(len(Space4))
                    .pb(len(Space4))
                    .child(status::label(
                        &Status {
                            role: Role::Good,
                            glyph: status::Glyph::Good,
                            word: "没有需要处理的事".into(),
                        },
                        Type::Body,
                    ))
                    .into_element();
            }
            view()
                .children(concerns.iter().map(|concern| {
                    let row = row_of(concern, items.as_deref(), &context);
                    let action =
                        concern_action(concern, items.as_deref(), sources, context.clone(), nav);
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space3))
                        .px(len(Space4))
                        .py(len(Space2))
                        .min_h(len(RowAttention))
                        .border_t_1()
                        .border_t_color(color(Border))
                        .hover(|s| s.bg(color(Muted)))
                        .child(status::icon_box(&row.status))
                        .child(
                            view()
                                .flex_1()
                                .min_w(Points::ZERO)
                                .gap(Points::ZERO)
                                .child(
                                    view()
                                        .flex_row()
                                        .items_center()
                                        .gap(len(Space2))
                                        .child(
                                            text(row.title)
                                                .ty(Type::Body)
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .truncate()
                                                .flex_shrink(1.0),
                                        )
                                        .child(status::tag(row.category, row.status.role)),
                                )
                                .child(ink2(row.reason, Type::Dense).truncate().selectable(true)),
                        )
                        .child(meta(
                            row.at.map(|at| time::ago(at, now)).unwrap_or_default(),
                        ))
                        .child(action)
                        .into_element()
                }))
                .into_element()
        })
    });
    card()
        .child(card_head(
            "需要人处理",
            "先失败、后待查看；同级按时间从新到旧",
            Vec::new(),
        ))
        .child(rows)
}

// ---------------------------------------------------------------------------
// 议题进度

fn progress_card(sources: Sources, nav: Nav) -> View {
    let body = dynamic(move || {
        let Some(queue) = sources.queue.get().map(|l| l.data) else {
            return nothing();
        };
        untracked(move || progress(&queue))
    });
    let sub = move || {
        let text = sources
            .queue
            .with(|q| q.map(|q| format!("议会队列的判定 · 共 {}", q.data.len())))
            .unwrap_or_else(|| "议会队列的判定".to_owned());
        SharedString::from(text)
    };
    card()
        .child(card_head(
            "议题进度",
            sub,
            vec![
                press("打开议题图 →", Look::Ghost, true, move || {
                    nav.page.set(Page::Issues)
                })
                .into_element(),
            ],
        ))
        .child(source_trouble("议会队列", sources.queue))
        .child(body)
}

fn progress(queue: &[QueueEntry]) -> AnyElement {
    let count = |state: QueueState| queue.iter().filter(|e| e.state == state).count();
    let fill = |state: QueueState| match state {
        QueueState::Superseded => color(BorderStrong),
        state => status::queue(state).role.ink(),
    };
    let bar = view()
        .flex_row()
        .gap(px(len(Space1).0 / 2.0))
        .h(len(Space2))
        .mx(len(Space4))
        .mt(len(Space1))
        .children(
            status::QUEUE_STATES
                .into_iter()
                .filter(|s| count(*s) > 0)
                .map(|state| {
                    view()
                        .flex_grow(count(state) as f32)
                        .basis(Points::ZERO)
                        .rounded(len(Space1))
                        .bg(fill(state))
                }),
        );
    let legend = view()
        .gap(len(Space2))
        .px(len(Space4))
        .pt(len(Space3))
        .pb(len(Space4))
        .children(status::QUEUE_STATES.chunks(4).map(|states| {
            view()
                .flex_row()
                .gap(len(Space4))
                .children(states.iter().map(|&state| {
                    view()
                        .flex_row()
                        .items_center()
                        .flex_grow(1.0)
                        .basis(Points::ZERO)
                        .min_w(Points::ZERO)
                        .child(status::label(&status::queue(state), Type::Dense))
                        .child(view().flex_1())
                        .child(
                            text(count(state).to_string())
                                .ty(Type::Dense)
                                .font_weight(FontWeight::SEMIBOLD)
                                .tabular_nums(),
                        )
                }))
        }));
    view().child(bar).child(legend).into_element()
}

// ---------------------------------------------------------------------------
// 程序

fn programs_card(sources: Sources, context: Rc<Context>) -> View {
    card()
        .child(card_head(
            "程序",
            format!("健康端点 · 每 {} 秒", context.refresh_seconds),
            Vec::new(),
        ))
        .child(
            view()
                .px(len(Space4))
                .pb(len(Space4))
                .gap(len(Space2))
                .child(program(sources.council, sources, context.clone()))
                .child(view().border_t_1().border_t_color(color(Border)))
                .child(program(sources.guardians, sources, context)),
        )
}

/// `host:port` of a base URL: where the program answers.
fn port(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split('/').next().unwrap_or(rest);
    match host.rsplit_once(':') {
        Some((_, port)) => format!(":{port}"),
        None => host.to_owned(),
    }
}

fn program(program: ProgramSources, sources: Sources, context: Rc<Context>) -> View {
    let health = program.health;
    view().child(dynamic(move || {
        let failed = health.with_error(|e| e.is_some());
        let loaded = health.get().map(|l| l.data);
        let error = health.error();
        let detail = match program.program {
            Program::Council => sources
                .queue
                .with(|q| q.map(|q| format!("队列 {} 项", q.data.len()))),
            Program::Guardians => sources.bases.with(|b| {
                b.map(|b| {
                    let ready = b
                        .data
                        .iter()
                        .filter(|b| b.state == BaseState::Ready)
                        .count();
                    format!("基座 {ready}/{} 就绪", b.data.len())
                })
            }),
        };
        let now = context.now.get();
        let address = port(context.program(program.program).base.as_str());
        untracked(move || {
            let state = status::program(!failed && loaded.is_some());
            let second = match (&error, &loaded) {
                (Some(error), _) => first_line(error).to_owned(),
                (None, Some(health)) => [
                    Some(format!("启动于 {}", time::short(&health.started_at))),
                    time::parse_ms(&health.started_at)
                        .map(|at| format!("运行 {}", time::ago(at, now).trim_end_matches('前'))),
                    detail,
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · "),
                (None, None) => "检查中…".to_owned(),
            };
            view()
                .gap(Points::ZERO)
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space3))
                        .child(
                            text(program.program.label())
                                .ty(Type::Body)
                                .font_weight(FontWeight::SEMIBOLD),
                        )
                        .child(status::label(&state, Type::Dense))
                        .child(view().flex_1())
                        .child(
                            text(
                                loaded
                                    .as_ref()
                                    .map(|h| h.version.clone())
                                    .unwrap_or_default(),
                            )
                            .ty(Type::Meta)
                            .mono()
                            .text_color(color(Ink2))
                            .truncate()
                            .flex_shrink(1.0),
                        ),
                )
                .child(
                    view()
                        .flex_row()
                        .gap(len(Space3))
                        .child(
                            text(second)
                                .ty(Type::Meta)
                                .text_color(if error.is_some() {
                                    Role::Critical.text()
                                } else {
                                    color(Ink2)
                                })
                                .flex_1()
                                .number_of_lines(2),
                        )
                        .child(text(address).ty(Type::Meta).mono().text_color(color(Ink2))),
                )
                .into_element()
        })
    }))
}

// ---------------------------------------------------------------------------
// 最近 LLM 异常

/// How many errored sessions the table lists.
const ERRORS_SHOWN: usize = 8;

/// The sessions with an error that started within the attention window,
/// newest first.
fn recent_errors(sources: Sources, context: &Context) -> Vec<(Program, Session)> {
    let window = f64::from(context.attention.session_failure_hours.get()) * 3_600_000.0;
    let now = time::now_ms();
    let mut errored: Vec<(Program, Session)> = [sources.council, sources.guardians]
        .into_iter()
        .flat_map(|p| {
            p.sessions
                .get()
                .map(|l| l.data)
                .unwrap_or_default()
                .into_iter()
                .filter(|s| {
                    s.error.is_some()
                        && time::parse_ms(&s.started_at).is_some_and(|at| now - at <= window)
                })
                .map(move |s| (p.program, s))
        })
        .collect();
    errored.sort_by(|a, b| {
        time::parse_ms(&b.1.started_at)
            .partial_cmp(&time::parse_ms(&a.1.started_at))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    errored
}

/// The table's columns' shares: time, program, session, model, error, link.
const SHARES: [f32; 6] = [1.2, 0.8, 3.0, 2.2, 4.5, 0.8];

fn cell(share: usize) -> View {
    view()
        .flex_grow(SHARES[share])
        .basis(Points::ZERO)
        .min_w(Points::ZERO)
}

fn errors_card(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let sub = format!(
        "{} 小时 · 最新在前",
        context.attention.session_failure_hours
    );
    let rows = dynamic(move || {
        let errors = recent_errors(sources, &context);
        untracked(move || {
            if errors.is_empty() {
                return meta("没有带异常的会话")
                    .px(len(Space4))
                    .pb(len(Space4))
                    .into_element();
            }
            view()
                .child(head_row(
                    ["时间", "程序", "会话", "模型", "错误", ""]
                        .into_iter()
                        .enumerate()
                        .map(|(i, l)| cell(i).child(th(l)).into_element())
                        .collect(),
                ))
                .children(
                    errors
                        .into_iter()
                        .take(ERRORS_SHOWN)
                        .map(|(program, session)| {
                            let open = OpenSession {
                                program,
                                id: session.id.clone(),
                            };
                            body_row()
                                .child(
                                    cell(0).child(
                                        text(
                                            time::parse_ms(&session.started_at)
                                                .map(time::clock)
                                                .unwrap_or_default(),
                                        )
                                        .ty(Type::Meta)
                                        .mono(),
                                    ),
                                )
                                .child(cell(1).child(text(program.label()).ty(Type::Dense)))
                                .child(cell(2).child(
                                    text(sessions::title(&session)).ty(Type::Dense).truncate(),
                                ))
                                .child(cell(3).flex_row().child(chip(session.model.clone())))
                                .child(
                                    cell(4).child(
                                        text(
                                            first_line(
                                                session.error.as_deref().unwrap_or_default(),
                                            )
                                            .to_owned(),
                                        )
                                        .ty(Type::Meta)
                                        .mono()
                                        .text_color(Role::Critical.text())
                                        .truncate()
                                        .selectable(true),
                                    ),
                                )
                                .child(cell(5).flex_row().justify_end().child(press(
                                    "会话 →",
                                    Look::Ghost,
                                    true,
                                    move || nav.open_session(open.clone()),
                                )))
                        }),
                )
                .into_element()
        })
    });
    card()
        .child(card_head(
            "最近 LLM 异常",
            sub,
            vec![
                press("全部会话 →", Look::Ghost, true, move || {
                    nav.page.set(Page::Sessions)
                })
                .into_element(),
            ],
        ))
        .child(source_trouble("议会会话", sources.council.sessions))
        .child(source_trouble("守护会话", sources.guardians.sessions))
        .child(rows)
}
