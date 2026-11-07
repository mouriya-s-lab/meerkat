//! 「会话」(C4 `sessionView`): the sessions of both programs side by side
//! with the one a person opens. The left column shows the sessions started
//! each hour (failures red) over the list, filtered by program, state and
//! kind, each row with its duration as a bar; the right column the open
//! session: its state, metadata, the error of a failed one, and its
//! decision log (C4 `logDigest`) as blocks, fetched when opened and parsed
//! off the engine thread where the platform has threads, or its record
//! line by line.

use std::{ops::Range, rc::Rc};

use nsh_base::button;
use nsh_core::prelude::*;
use serde_json::Value;

use super::{
    icons::{Icon, icon},
    parts::{
        Look, Span, card, chip, kv_chip, meta, nothing, press, segmented, source_trouble, tag,
    },
    shell::{Context, Nav, critical, plain, strong},
    status::{self, Glyph, Role, Status},
    tokens::{MARK, Type, Typeset, color, len},
};
use crate::{
    attention::first_line,
    config::{ColorToken::*, Digest, LengthToken::*},
    digest::{self, Body, DecisionLog, DigestItem, Entry, PmMeta, RetryStatus},
    programs::{Program, Session, SessionState},
    sources::Sources,
    time,
};

/// A session a page opened: which program has it, and its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSession {
    pub program: Program,
    pub id: String,
}

/// How long a session ran, or has run so far, in milliseconds.
fn duration_ms(session: &Session) -> Option<f64> {
    let start = time::parse_ms(&session.started_at)?;
    let end = match &session.ended_at {
        Some(ended) => time::parse_ms(ended)?,
        None => time::now_ms(),
    };
    Some((end - start).max(0.0))
}

/// A session's name: its kind, its label and its issue, each once (a label
/// such as `proxy owner/repo#45` already names both).
pub fn title(session: &Session) -> String {
    let mut title = String::new();
    if !session.label.starts_with(session.kind.as_str()) {
        title.push_str(&session.kind);
        title.push_str(" · ");
    }
    title.push_str(&session.label);
    if let Some(issue) = &session.reference {
        let issue = issue.to_string();
        if !session.label.contains(&issue) {
            title.push_str(" · ");
            title.push_str(&issue);
        }
    }
    title
}

/// How long a session ran, or has run so far.
pub fn duration(session: &Session) -> String {
    match duration_ms(session) {
        Some(ms) if session.ended_at.is_none() => format!("已运行 {}", time::span(ms)),
        Some(ms) => time::span(ms),
        None => "—".to_owned(),
    }
}

/// The width from which the list and the open session stand side by side.
const SIDE_BY_SIDE: f32 = 1000.0;

/// The list's filters (`None`: all).
#[derive(Clone, Copy)]
struct Filters {
    program: Signal<Option<Program>>,
    kind: Signal<Option<String>>,
    state: Signal<Option<SessionState>>,
}

/// Both programs' sessions, newest first.
fn all_sessions(sources: Sources) -> Vec<(Program, Session)> {
    let mut all: Vec<(Program, Session)> = [sources.council, sources.guardians]
        .into_iter()
        .flat_map(|p| {
            p.sessions
                .get()
                .map(|l| l.data)
                .unwrap_or_default()
                .into_iter()
                .map(move |s| (p.program, s))
        })
        .collect();
    all.sort_by(|a, b| {
        time::parse_ms(&b.1.started_at)
            .partial_cmp(&time::parse_ms(&a.1.started_at))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    all
}

/// The page's sentence: sessions started within the attention window,
/// running and failed among them.
pub fn summary(sources: Sources, context: &Context) -> Vec<Span> {
    let hours = context.attention.session_failure_hours.get();
    let since = time::now_ms() - f64::from(hours) * 3_600_000.0;
    let recent: Vec<Session> = all_sessions(sources)
        .into_iter()
        .map(|(_, s)| s)
        .filter(|s| time::parse_ms(&s.started_at).is_some_and(|at| at >= since))
        .collect();
    let running = recent
        .iter()
        .filter(|s| s.state == SessionState::Running)
        .count();
    let failed = recent
        .iter()
        .filter(|s| s.state == SessionState::Failed)
        .count();
    let mut spans = vec![
        plain(format!("最近 {hours} 小时 ")),
        strong(recent.len()),
        plain(format!(" 个会话：{running} 个运行中，")),
    ];
    spans.push(if failed > 0 {
        critical(format!("{failed} 个失败"))
    } else {
        plain("没有失败")
    });
    spans.push(plain("。"));
    spans
}

/// The sessions page's content.
pub fn sessions(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let filters = Filters {
        program: signal(None),
        kind: signal(None),
        state: signal(None),
    };
    let list = {
        let context = context.clone();
        move || list_column(sources, context.clone(), nav, filters)
    };
    let open = {
        let context = context.clone();
        move || open_column(sources, context.clone(), nav)
    };
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .gap(len(Space4))
        .px(len(Space6))
        .pt(len(Space5))
        .pb(len(Space6))
        .child(toolbar(sources, filters))
        .child(
            when(|| window().width >= px(SIDE_BY_SIDE), {
                let (list, open) = (list.clone(), open.clone());
                move || {
                    card()
                        .flex_1()
                        .flex_row()
                        .min_h(Points::ZERO)
                        .child(
                            view()
                                .w(px(len(DagNodeWidth).0 * 2.0))
                                .flex_shrink_0()
                                .border_r_1()
                                .border_r_color(color(Border))
                                .child(list()),
                        )
                        .child(view().flex_1().min_w(Points::ZERO).child(open()))
                }
            })
            .otherwise(move || {
                scroll().flex_1().child(
                    view()
                        .gap(len(Space4))
                        .child(card().child(list()))
                        .child(card().child(open())),
                )
            }),
        )
}

// ---------------------------------------------------------------------------
// 工具栏

fn toolbar(sources: Sources, filters: Filters) -> View {
    let choice = |label: &str, selected: Box<dyn Fn() -> bool>, press: Box<dyn Fn()>| {
        (SharedString::from(label.to_owned()), selected, press)
    };
    let programs = segmented(
        [
            (None, "全部"),
            (Some(Program::Council), "议会"),
            (Some(Program::Guardians), "守护"),
        ]
        .into_iter()
        .map(|(value, label)| {
            choice(
                label,
                Box::new(move || filters.program.get() == value),
                Box::new(move || filters.program.set(value)),
            )
        })
        .collect(),
    );
    let states = segmented(
        [
            (None, "全部状态"),
            (Some(SessionState::Running), "运行中"),
            (Some(SessionState::Done), "完成"),
            (Some(SessionState::Failed), "失败"),
        ]
        .into_iter()
        .map(|(value, label)| {
            choice(
                label,
                Box::new(move || filters.state.get() == value),
                Box::new(move || filters.state.set(value)),
            )
        })
        .collect(),
    );
    let kinds = dynamic(move || {
        let mut kinds: Vec<String> = all_sessions(sources)
            .into_iter()
            .map(|(_, s)| s.kind)
            .collect();
        kinds.sort();
        kinds.dedup();
        untracked(move || {
            segmented(
                std::iter::once(None)
                    .chain(kinds.into_iter().map(Some))
                    .map(|value| {
                        let label = value.clone().unwrap_or_else(|| "全部种类".to_owned());
                        let (a, b) = (value.clone(), value);
                        choice(
                            &label,
                            Box::new(move || filters.kind.with(|k| *k == a)),
                            Box::new(move || filters.kind.set(b.clone())),
                        )
                    })
                    .collect(),
            )
            .into_element()
        })
    });
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space2))
        .child(programs)
        .child(states)
        .child(kinds)
        .child(view().flex_1())
        .child(meta("每个程序最近 200 个 · 新的在前"))
}

// ---------------------------------------------------------------------------
// 左栏：柱条与列表

fn list_column(sources: Sources, context: Rc<Context>, nav: Nav, filters: Filters) -> View {
    let rows = dynamic(move || {
        let all = all_sessions(sources);
        let (program, kind, state) = (
            filters.program.get(),
            filters.kind.get(),
            filters.state.get(),
        );
        untracked(move || {
            let shown: Vec<_> = all
                .into_iter()
                .filter(|(p, s)| {
                    program.is_none_or(|want| want == *p)
                        && kind.as_ref().is_none_or(|want| *want == s.kind)
                        && state.is_none_or(|want| want == s.state)
                })
                .collect();
            if shown.is_empty() {
                return meta("没有符合筛选的会话").p(len(Space4)).into_element();
            }
            let longest = shown
                .iter()
                .filter_map(|(_, s)| duration_ms(s))
                .fold(0.0, f64::max);
            view()
                .children(
                    shown
                        .into_iter()
                        .map(|(program, session)| session_row(program, session, longest, nav)),
                )
                .into_element()
        })
    });
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(hours(sources, context))
        .child(source_trouble("议会会话", sources.council.sessions))
        .child(source_trouble("守护会话", sources.guardians.sessions))
        .child(scroll().flex_1().child(rows))
}

/// The sessions started in each hour of the attention window, oldest
/// first: how many, how many of them failed.
fn per_hour(sessions: &[(Program, Session)], hours: u32, now: f64) -> Vec<(usize, usize)> {
    const HOUR: f64 = 3_600_000.0;
    let end = (now / HOUR).floor() + 1.0;
    let start = end - f64::from(hours);
    let mut counts = vec![(0, 0); hours as usize];
    for (_, session) in sessions {
        let Some(at) = time::parse_ms(&session.started_at) else {
            continue;
        };
        let slot = (at / HOUR).floor() - start;
        if slot >= 0.0 && slot < f64::from(hours) {
            let count = &mut counts[slot as usize];
            count.0 += 1;
            count.1 += usize::from(session.state == SessionState::Failed);
        }
    }
    counts
}

fn hours(sources: Sources, context: Rc<Context>) -> View {
    let hours = context.attention.session_failure_hours.get();
    let body = dynamic(move || {
        let all = all_sessions(sources);
        let now = context.now.with_untracked(|n| *n);
        untracked(move || {
            let counts = per_hour(&all, hours, now);
            let most = counts.iter().map(|c| c.0).max().unwrap_or(0).max(1) as f32;
            let height = len(Space6).0 + len(Space2).0;
            let first_hour =
                (now / 3_600_000.0).floor() * 3_600_000.0 - f64::from(hours - 1) * 3_600_000.0;
            let tick = |i: u32| {
                let at = first_hour + f64::from(i) * 3_600_000.0;
                text(time::clock(at)[..3].to_owned() + "00")
                    .ty(Type::Meta)
                    .mono()
                    .text_color(color(MutedForeground))
            };
            let ticks = [0, hours / 4, hours / 2, hours * 3 / 4, hours - 1];
            view()
                .child(
                    view()
                        .flex_row()
                        .items_end()
                        .gap(px(len(Space1).0 / 2.0))
                        .h(px(height))
                        .px(len(Space4))
                        .children(counts.into_iter().map(|(all, failed)| {
                            let unit = height / most;
                            view()
                                .flex_1()
                                .justify_end()
                                .child(
                                    view()
                                        .h(px(unit * failed as f32))
                                        .bg(Role::Critical.ink())
                                        .rounded(px(len(Space1).0 / 2.0)),
                                )
                                .child(
                                    view()
                                        .h(px(unit * (all - failed) as f32))
                                        .bg(color(BorderStrong))
                                        .rounded(px(len(Space1).0 / 2.0)),
                                )
                        })),
                )
                .child(
                    view()
                        .flex_row()
                        .justify_between()
                        .px(len(Space4))
                        .pt(len(Space1))
                        .pb(len(Space3))
                        .children(ticks.map(tick)),
                )
                .into_element()
        })
    });
    view()
        .child(
            meta(format!("每小时开始的会话 · 最近 {hours} 小时 · 失败标红"))
                .px(len(Space4))
                .pt(len(Space4))
                .pb(len(Space2)),
        )
        .child(body)
}

fn session_row(program: Program, session: Session, longest: f64, nav: Nav) -> impl IntoElement {
    let open = OpenSession {
        program,
        id: session.id.clone(),
    };
    let current = {
        let open = open.clone();
        move || nav.session.with(|s| s.as_ref() == Some(&open))
    };
    let state = status::session(session.state);
    let share = duration_ms(&session).map_or(0.0, |ms| (ms / longest.max(1.0)) as f32);
    let bar_max = len(RowAttention).0;
    let failed = session.state == SessionState::Failed;
    let (a, b) = (current.clone(), current.clone());
    button()
        .on_press(move || nav.session.set(Some(open.clone())))
        .accessibility_label(title(&session))
        .selected(current)
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .px(len(Space4))
        .py(len(Space2))
        .border_t_1()
        .border_t_color(color(Border))
        .border_l(px(MARK))
        .border_l_color(move || {
            if a() {
                color(Selection)
            } else {
                color(Card).opacity(0.0)
            }
        })
        .bg(move || {
            if b() {
                Role::Active.subtle()
            } else {
                color(Card)
            }
        })
        .hover(|s| s.bg(color(Muted)))
        .cursor_pointer()
        .child(
            status::glyph(state.glyph, state.role)
                .self_start()
                .mt(len(Space1)),
        )
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .gap(len(Space1))
                .child(
                    text(title(&session))
                        .ty(Type::Dense)
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate(),
                )
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .child(
                            text(state.word.clone())
                                .ty(Type::Meta)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(state.role.text()),
                        )
                        .child(tag(program.label()))
                        .child(chip(session.model.clone()))
                        .child(
                            text(
                                time::parse_ms(&session.started_at)
                                    .map(time::clock)
                                    .unwrap_or_default(),
                            )
                            .ty(Type::Meta)
                            .mono()
                            .text_color(color(MutedForeground)),
                        ),
                ),
        )
        .child(
            view()
                .items_end()
                .gap(len(Space1))
                .flex_shrink_0()
                .child(text(duration(&session)).ty(Type::Meta).mono())
                .child(
                    view()
                        .w(px((bar_max * share).max(len(Space1).0)))
                        .h(len(Space1))
                        .rounded(len(Space1))
                        .bg(if failed {
                            Role::Critical.ink()
                        } else {
                            Role::Active.ink()
                        })
                        .opacity(if failed { 1.0 } else { 0.6 }),
                ),
        )
}

// ---------------------------------------------------------------------------
// 右栏：打开的会话

/// A session's record, read and reduced.
struct Record {
    session: OpenSession,
    text: String,
    /// Each line's bytes in `text`.
    lines: Vec<Range<usize>>,
    log: DecisionLog,
}

impl Record {
    fn new(session: OpenSession, bytes: Vec<u8>, rules: &Digest) -> Self {
        let log = digest::parse(&bytes, rules);
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => String::from_utf8_lossy(error.as_bytes()).into_owned(),
        };
        let mut lines = Vec::new();
        let mut start = 0;
        for (at, _) in text.match_indices('\n') {
            lines.push(start..at);
            start = at + 1;
        }
        if start < text.len() {
            lines.push(start..text.len());
        }
        Self {
            session,
            text,
            lines,
            log,
        }
    }
}

/// Reduces `bytes` on the runtime's blocking threads.
#[cfg(not(target_arch = "wasm32"))]
async fn reduce(session: OpenSession, bytes: Vec<u8>, rules: Digest) -> Result<Record, String> {
    blocking(move || Record::new(session, bytes, &rules))
        .await
        .map_err(|error| format!("解析中断：{error}"))
}

/// Reduces `bytes` on the engine thread (WebAssembly has no other), after
/// the frame that says it is parsing.
#[cfg(target_arch = "wasm32")]
async fn reduce(session: OpenSession, bytes: Vec<u8>, rules: Digest) -> Result<Record, String> {
    next_frame().await;
    Ok(Record::new(session, bytes, &rules))
}

fn open_column(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let raw = signal(false);
    let record = {
        let context = context.clone();
        resource(
            move || nav.session.get(),
            move |open: OpenSession, _| {
                let context = context.clone();
                async move {
                    let bytes = context.program(open.program).log(&open.id).await?;
                    reduce(open, bytes, context.digest.clone())
                        .await
                        .map(Rc::new)
                }
            },
        )
    };
    let body = dynamic(move || {
        let Some(open) = nav.session.get() else {
            return meta("在左边的列表里选一个会话。").into_element();
        };
        // The record of the open session only: a previous one stays in the
        // resource while the next loads.
        let current = record.with(|r| r.filter(|r| r.session == open).cloned());
        let error = record.error();
        let loading = record.loading();
        let session = [sources.council.sessions, sources.guardians.sessions]
            .into_iter()
            .find_map(|source| {
                source.with(|l| l.and_then(|l| l.data.iter().find(|s| s.id == open.id).cloned()))
            });
        let showing_raw = raw.get();
        untracked(move || {
            let entries = current.as_ref().map(|r| r.log.entries.len());
            let content = match (current, error) {
                (_, Some(error)) if !loading => text(format!("取记录失败：{error}"))
                    .ty(Type::Dense)
                    .text_color(Role::Critical.text())
                    .selectable(true)
                    .into_element(),
                (Some(record), _) if showing_raw => raw_lines(record),
                (Some(record), _) => decision_log(&record),
                (None, _) => meta("取记录并解析中…").into_element(),
            };
            view()
                .flex_1()
                .min_h(Points::ZERO)
                .gap(len(Space3))
                .child(header(&open, session.as_ref()))
                .children(
                    session
                        .as_ref()
                        .filter(|s| s.state == SessionState::Failed)
                        .map(banner),
                )
                .child(tabs(raw, entries))
                .child(content)
                .into_element()
        })
    });
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .px(len(Space5))
        .py(len(Space4))
        .child(body)
}

/// The GitHub page of an issue.
fn issue_url(issue: &crate::gh::IssueRef) -> String {
    format!("https://github.com/{}/issues/{}", issue.repo, issue.number)
}

fn header(open: &OpenSession, session: Option<&Session>) -> View {
    let Some(s) = session else {
        return view()
            .gap(len(Space2))
            .child(tag(open.program.label()))
            .child(
                text(open.id.clone())
                    .ty(Type::Title)
                    .font_weight(FontWeight::SEMIBOLD),
            )
            .child(meta("不在最近的会话列表里"));
    };
    let mut chips = vec![
        kv_chip("模型", s.model.clone()),
        kv_chip("开始", time::precise(&s.started_at)),
        kv_chip(
            "结束",
            s.ended_at
                .as_deref()
                .map(time::precise)
                .unwrap_or_else(|| "—".to_owned()),
        ),
        kv_chip("时长", duration(s)),
    ];
    if let Some(parent) = &s.parent {
        chips.push(kv_chip("分叉自", parent.clone()));
    }
    chips.push(kv_chip("会话", s.id.clone()));
    view()
        .gap(len(Space3))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .child(status::pill(&status::session(s.state)))
                .child(tag(open.program.label()))
                .child(meta(format!("{} 会话", s.kind)))
                .child(view().flex_1())
                .children(s.reference.as_ref().map(|issue| {
                    let url = issue_url(issue);
                    press(
                        format!("打开议题 {issue} →"),
                        Look::Ghost,
                        true,
                        move || drop(open_url(url.clone())),
                    )
                })),
        )
        .child(
            text(title(s))
                .ty(Type::Title)
                .font_weight(FontWeight::SEMIBOLD)
                .number_of_lines(2),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap(len(Space2))
                .children(chips),
        )
}

/// A failed session's error, in full.
fn banner(session: &Session) -> View {
    let failed = status::session(SessionState::Failed);
    view()
        .flex_row()
        .items_start()
        .gap(len(Space3))
        .px(len(Space4))
        .py(len(Space3))
        .rounded(len(Radius))
        .bg(failed.role.subtle())
        .child(status::glyph(failed.glyph, failed.role).mt(len(Space1)))
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .gap(len(Space1))
                .child(
                    text("会话失败")
                        .ty(Type::Dense)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(failed.role.text()),
                )
                .child(
                    text(
                        session
                            .error
                            .clone()
                            .unwrap_or_else(|| "没有错误原文".to_owned()),
                    )
                    .ty(Type::Meta)
                    .mono()
                    .text_color(failed.role.text())
                    .number_of_lines(6)
                    .selectable(true),
                ),
        )
}

fn tabs(raw: Signal<bool>, entries: Option<usize>) -> View {
    let tab = move |label: String, want: bool| {
        let current = move || raw.get() == want;
        button()
            .on_press(move || raw.set(want))
            .accessibility_label(label.clone())
            .selected(current)
            .role(Some(AccessibilityRole::Tab))
            .px(len(Space3))
            .py(len(Space2))
            .border_b(px(MARK))
            .border_b_color(move || {
                if current() {
                    color(Foreground)
                } else {
                    color(Card).opacity(0.0)
                }
            })
            .cursor_pointer()
            .child(
                text(label)
                    .ty(Type::Dense)
                    .font_weight(if current() {
                        FontWeight::SEMIBOLD
                    } else {
                        FontWeight::NORMAL
                    })
                    .text_color(move || {
                        if current() {
                            color(Foreground)
                        } else {
                            color(Ink2)
                        }
                    }),
            )
    };
    view()
        .flex_row()
        .role(AccessibilityRole::Tablist)
        .border_b_1()
        .border_b_color(color(Border))
        .child(tab(
            entries.map_or("决策日志".to_owned(), |n| format!("决策日志 · {n}")),
            false,
        ))
        .child(tab("原始记录".to_owned(), true))
}

// ---------------------------------------------------------------------------
// 决策日志

fn decision_log(record: &Record) -> AnyElement {
    let log = &record.log;
    let mut notes = Vec::new();
    if let Some(cut) = log.truncated {
        notes.push(
            text(format!(
                "记录共 {} 字节，超过 digest.maxBytes：只解析了末尾 {} 字节，之前的步骤未显示。",
                cut.total,
                cut.total - cut.skipped
            ))
            .ty(Type::Meta)
            .text_color(Role::Warning.text())
            .into_element(),
        );
    }
    if log.unreadable > 0 {
        notes.push(meta(format!("{} 行不是完整的 JSON，已跳过。", log.unreadable)).into_element());
    }
    let count =
        |want: fn(&DigestItem) -> bool| log.entries.iter().filter(|e| want(&e.item)).count();
    notes.push(
        meta(format!(
            "LLM 异常 {} · 自动重试 {} · 继续提示 {}",
            count(|i| matches!(i, DigestItem::LlmError(_))),
            count(|i| matches!(i, DigestItem::Retry { .. })),
            count(|i| matches!(i, DigestItem::Continuation(_)))
        ))
        .into_element(),
    );
    scroll()
        .flex_1()
        .child(
            view()
                .gap(len(Space2))
                .pb(len(Space4))
                .children(notes)
                .children(log.entries.iter().map(entry)),
        )
        .into_element()
}

fn clock_of(at: &str) -> String {
    time::parse_ms(at).map(time::clock).unwrap_or_default()
}

/// A message block (`.msg`): a head with its label, a note and the time,
/// and its body under it.
fn message(label: &str, note: String, at: &str, accent: Option<Role>, body: AnyElement) -> View {
    let (edge, head) = match accent {
        Some(role) => (role.ink(), role.subtle()),
        None => (color(Border), color(Muted)),
    };
    view()
        .rounded(len(Radius))
        .border_1()
        .border_color(edge)
        .overflow_hidden()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .px(len(Space3))
                .py(len(Space1))
                .bg(head)
                .border_b_1()
                .border_b_color(color(Border))
                .child(
                    text(label.to_owned())
                        .ty(Type::Meta)
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(accent.map_or(color(Foreground), Role::text)),
                )
                .child(meta(note).flex_1().truncate())
                .child(
                    text(clock_of(at))
                        .ty(Type::Meta)
                        .mono()
                        .text_color(color(Ink2)),
                ),
        )
        .child(view().px(len(Space3)).py(len(Space2)).child(body))
}

/// A message's content: prose as body text, JSON as a tree.
fn body(content: &Body) -> AnyElement {
    match content {
        Body::Text(text_) => text(text_.clone())
            .ty(Type::Dense)
            .text_color(color(Ink2))
            .selectable(true)
            .into_element(),
        Body::Json(value) => json_tree(None, value, 0).into_element(),
    }
}

/// How many items an object or array holds.
fn items(value: &Value) -> Option<usize> {
    match value {
        Value::Object(fields) => Some(fields.len()),
        Value::Array(values) => Some(values.len()),
        _ => None,
    }
}

/// `value` under `key`: a container as a row that opens and closes (open
/// for the first two levels) with its item count, a scalar as one line.
fn json_tree(key: Option<String>, value: &Value, depth: usize) -> View {
    let name = key.map(|k| {
        text(format!("{k}:"))
            .ty(Type::Meta)
            .mono()
            .text_color(color(MutedForeground))
    });
    let Some(count) = items(value) else {
        let shown = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let ink = match value {
            Value::String(_) => color(Ink2),
            _ => Role::Active.text(),
        };
        return view()
            .flex_row()
            .items_start()
            .gap(len(Space1))
            .children(name)
            .child(
                text(shown)
                    .ty(Type::Meta)
                    .mono()
                    .text_color(ink)
                    .flex_1()
                    .selectable(true),
            );
    };
    let open = signal(depth < 2);
    let (brackets, unit) = match value {
        Value::Array(_) => ("[…]", "项"),
        _ => ("{…}", "项"),
    };
    let children: Vec<(Option<String>, Value)> = match value {
        Value::Object(fields) => fields
            .iter()
            .map(|(k, v)| (Some(k.clone()), v.clone()))
            .collect(),
        Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(i, v)| (Some(i.to_string()), v.clone()))
            .collect(),
        _ => Vec::new(),
    };
    view()
        .gap(len(Space1))
        .child(
            button()
                .on_press(move || open.update(|o| *o = !*o))
                .selected(move || open.get())
                .flex_row()
                .items_center()
                .gap(len(Space1))
                .cursor_pointer()
                .child(
                    text(move || if open.get() { "▾" } else { "▸" })
                        .ty(Type::Meta)
                        .text_color(color(MutedForeground)),
                )
                .children(name)
                .child(
                    text(brackets)
                        .ty(Type::Meta)
                        .mono()
                        .text_color(color(MutedForeground)),
                )
                .child(tag(format!("{count} {unit}"))),
        )
        .child(dynamic(move || {
            if !open.get() {
                return nothing();
            }
            let children = children.clone();
            untracked(move || {
                view()
                    .gap(len(Space1))
                    .pl(len(Space4))
                    .border_l_1()
                    .border_l_color(color(Border))
                    .ml(len(Space1))
                    .children(
                        children
                            .iter()
                            .map(|(k, v)| json_tree(k.clone(), v, depth + 1)),
                    )
                    .into_element()
            })
        }))
}

/// A tool call's arguments in a line: a string as it is, else each field's
/// value, shortened.
fn summary_of(args: &Value) -> String {
    let part = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => format!("[{} 项]", a.len()),
        Value::Object(o) => format!("{{{} 项}}", o.len()),
        other => other.to_string(),
    };
    match args {
        Value::Object(fields) => fields.values().map(part).collect::<Vec<_>>().join(" · "),
        other => part(other),
    }
}

/// A one-line block (`.tool`): a lead, a name, its detail and a time at
/// the right; `body` opens under it on a press.
fn line_block(
    lead: AnyElement,
    name: String,
    detail: String,
    right: String,
    role: Option<Role>,
    body: Rc<dyn Fn() -> AnyElement>,
) -> View {
    let open = signal(false);
    view()
        .rounded(len(Radius))
        .border_1()
        .border_color(role.map_or(color(Border), Role::ink))
        .bg(role.map_or(color(Card), Role::subtle))
        .child(
            button()
                .on_press(move || open.update(|o| *o = !*o))
                .selected(move || open.get())
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .px(len(Space3))
                .py(len(Space2))
                .cursor_pointer()
                .child(lead)
                .child(
                    text(name)
                        .ty(Type::Dense)
                        .text_color(role.map_or(color(Foreground), Role::text))
                        .flex_shrink_0(),
                )
                .child(
                    text(detail)
                        .ty(Type::Meta)
                        .mono()
                        .text_color(role.map_or(color(MutedForeground), Role::text))
                        .truncate()
                        .flex_1(),
                )
                .child(
                    text(right)
                        .ty(Type::Meta)
                        .mono()
                        .text_color(color(MutedForeground)),
                ),
        )
        .child(dynamic(move || {
            if open.get() {
                untracked(|| body())
            } else {
                nothing()
            }
        }))
}

fn meta_line(meta: &PmMeta) -> String {
    [
        Some(meta.kind.clone()),
        meta.seat.clone(),
        meta.reference.clone(),
        meta.base.clone(),
        meta.model.clone(),
        meta.round.map(|r| format!("第 {r} 轮")),
        meta.version.map(|v| format!("版本 {v}")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

fn entry(entry: &Entry) -> AnyElement {
    let at = entry.at.as_str();
    match &entry.item {
        DigestItem::Opening(content) => {
            message("开场消息", note_of(content), at, None, body(content))
        }
        DigestItem::ProgramPrompt(content) => {
            message("程序提示", note_of(content), at, None, body(content))
        }
        DigestItem::Continuation(content) => message(
            "继续提示",
            "session-stop-continuation".to_owned(),
            at,
            Some(Role::Warning),
            body(content),
        ),
        DigestItem::AssistantText(text_) => message(
            "回答",
            String::new(),
            at,
            None,
            body(&Body::Text(text_.clone())),
        ),
        DigestItem::ToolCall { name, args, result } => {
            let took = result
                .as_ref()
                .and_then(|r| Some(time::parse_ms(&r.at)? - time::parse_ms(at)?))
                .map(time::span)
                .unwrap_or_else(|| "没有结果".to_owned());
            let failed = result.as_ref().is_some_and(|r| r.is_error);
            let (args_, result_) = (args.clone(), result.clone());
            let detail = move || {
                view()
                    .gap(len(Space2))
                    .px(len(Space3))
                    .pb(len(Space3))
                    .child(meta("参数"))
                    .child(json_tree(None, &args_, 0))
                    .child(meta(match &result_ {
                        Some(r) if r.is_error => "结果（工具报错）",
                        Some(_) => "结果",
                        None => "没有结果",
                    }))
                    .children(result_.as_ref().map(|r| body(&r.body)))
                    .into_element()
            };
            line_block(
                icon(Icon::Tool, move || {
                    if failed {
                        Role::Critical.text()
                    } else {
                        color(Ink2)
                    }
                })
                .into_element(),
                name.clone(),
                summary_of(args),
                took,
                failed.then_some(Role::Critical),
                Rc::new(detail),
            )
        }
        DigestItem::Folded { name, count } => view()
            .px(len(Space3))
            .child(meta(format!("{name} × {count}（已折叠）"))),
        DigestItem::LlmError(error) => line_block(
            status::glyph(Glyph::Critical, Role::Critical).into_element(),
            "LLM 调用".to_owned(),
            format!("{} · 未重试", first_line(error)),
            clock_of(at),
            Some(Role::Critical),
            {
                let error = error.clone();
                Rc::new(move || {
                    text(error.clone())
                        .ty(Type::Meta)
                        .mono()
                        .text_color(Role::Critical.text())
                        .selectable(true)
                        .px(len(Space3))
                        .pb(len(Space3))
                        .into_element()
                })
            },
        ),
        DigestItem::Retry {
            attempt,
            status,
            error,
        } => {
            let retry = Status {
                role: Role::Warning,
                glyph: Glyph::Warning,
                word: "自动重试".into(),
            };
            line_block(
                status::glyph(retry.glyph, retry.role).into_element(),
                format!("自动重试 · 第 {attempt} 次"),
                format!(
                    "{} · {}",
                    first_line(error),
                    match status {
                        RetryStatus::Recovered => "之后恢复",
                        RetryStatus::Superseded => "之后再次重试",
                    }
                ),
                clock_of(at),
                Some(retry.role),
                {
                    let error = error.clone();
                    Rc::new(move || {
                        text(error.clone())
                            .ty(Type::Meta)
                            .mono()
                            .selectable(true)
                            .px(len(Space3))
                            .pb(len(Space3))
                            .into_element()
                    })
                },
            )
        }
        DigestItem::Meta(meta_) => view()
            .px(len(Space3))
            .child(meta(format!("pm-meta · {}", meta_line(meta_))).selectable(true)),
    }
    .into_element()
}

/// A message's head note: how much a JSON body holds.
fn note_of(content: &Body) -> String {
    match content {
        Body::Text(_) => String::new(),
        Body::Json(value) => match items(value) {
            Some(n) => format!("JSON · {n} 项"),
            None => "JSON".to_owned(),
        },
    }
}

// ---------------------------------------------------------------------------
// 原始记录

fn raw_lines(record: Rc<Record>) -> AnyElement {
    let count = record.lines.len();
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .gap(len(Space1))
        .child(meta(format!(
            "{} 行 · {} 字节，与 /v1/sessions/{{id}}/log 相同",
            count,
            record.text.len()
        )))
        .child(
            virtual_list(count, move |at| {
                let record = record.clone();
                let line = move || {
                    let range = record.lines[at.index()].clone();
                    record.text[range].to_owned()
                };
                view()
                    .flex_row()
                    .gap(len(Space2))
                    .py(len(Space1))
                    .border_b_1()
                    .border_b_color(color(Border))
                    .child(
                        text(move || (at.index() + 1).to_string())
                            .ty(Type::Meta)
                            .mono()
                            .w(len(Space6))
                            .text_color(color(MutedForeground)),
                    )
                    .child(
                        text(line)
                            .ty(Type::Meta)
                            .mono()
                            .number_of_lines(4)
                            .flex_1()
                            .selectable(true),
                    )
            })
            .estimated_item_size(px(len(RowAttention).0))
            .flex_1(),
        )
        .into_element()
}
