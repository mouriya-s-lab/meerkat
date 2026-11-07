//! 「会话」(C4 `sessionView`): the sessions of both programs in one list,
//! filtered by program, kind and state; the session a person opens shows
//! its decision log (C4 `logDigest`), fetched when opened and parsed off
//! the engine thread where the platform has threads, or its record line by
//! line.

use std::{ops::Range, rc::Rc};

use nsh_base::{button, theme};
use nsh_core::prelude::*;

use super::{
    overview::Context,
    parts::{badge, card, note, source_status},
    shell::Nav,
    theme::{DONE, FAILED, IN_PROGRESS, TODO, WAITING},
};
use crate::{
    attention::first_line,
    config::Digest,
    digest::{self, DecisionLog, DigestItem, Entry, PmMeta, RetryStatus},
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

/// A state's label and color.
pub fn session_state(state: SessionState) -> (&'static str, AdaptiveColor) {
    match state {
        SessionState::Running => ("运行中", IN_PROGRESS),
        SessionState::Done => ("完成", DONE),
        SessionState::Failed => ("失败", FAILED),
    }
}

/// How long a session ran, or has run so far.
pub fn duration(session: &Session) -> String {
    let start = time::parse_ms(&session.started_at);
    let end = match &session.ended_at {
        Some(ended) => time::parse_ms(ended),
        None => Some(time::now_ms()),
    };
    let (Some(start), Some(end)) = (start, end) else {
        return "—".to_owned();
    };
    let seconds = ((end - start) / 1000.0).max(0.0) as u64;
    let text = match seconds {
        0..60 => format!("{seconds} 秒"),
        60..3600 => format!("{} 分 {} 秒", seconds / 60, seconds % 60),
        _ => format!("{} 小时 {} 分", seconds / 3600, seconds % 3600 / 60),
    };
    if session.ended_at.is_none() {
        format!("已运行 {text}")
    } else {
        text
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

/// The sessions page's content.
pub fn sessions(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let filters = Filters {
        program: signal(None),
        kind: signal(None),
        state: signal(None),
    };
    let list = move || list_card(sources, nav, filters);
    let open = {
        let context = context.clone();
        move || open_card(sources, context.clone(), nav)
    };
    view().flex_1().min_h(px(0.0)).child(
        when(
            || window().width >= px(SIDE_BY_SIDE),
            {
                let open = open.clone();
                move || {
                    view()
                        .flex_1()
                        .flex_row()
                        .min_h(px(0.0))
                        .gap_4()
                        .p_4()
                        .child(scroll().w(px(380.0)).flex_shrink_0().child(list()))
                        .child(view().flex_1().min_w(px(0.0)).child(open()))
                }
            },
        )
        .otherwise(move || {
            scroll()
                .flex_1()
                .child(view().gap_4().p_4().child(list()).child(open()))
        }),
    )
}

// ---------------------------------------------------------------------------
// 会话列表

fn list_card(sources: Sources, nav: Nav, filters: Filters) -> View {
    let rows = dynamic(move || {
        let council = sources.council.sessions.get().map(|l| l.data);
        let guardians = sources.guardians.sessions.get().map(|l| l.data);
        let (program, kind, state) = (
            filters.program.get(),
            filters.kind.get(),
            filters.state.get(),
        );
        untracked(move || {
            let mut all: Vec<(Program, Session)> = [
                (Program::Council, council),
                (Program::Guardians, guardians),
            ]
            .into_iter()
            .flat_map(|(program, sessions)| {
                sessions
                    .unwrap_or_default()
                    .into_iter()
                    .map(move |session| (program, session))
            })
            .collect();
            all.sort_by(|a, b| {
                time::parse_ms(&b.1.started_at)
                    .partial_cmp(&time::parse_ms(&a.1.started_at))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let mut kinds: Vec<String> = all.iter().map(|(_, s)| s.kind.clone()).collect();
            kinds.sort();
            kinds.dedup();
            let shown: Vec<_> = all
                .into_iter()
                .filter(|(p, s)| {
                    program.is_none_or(|want| want == *p)
                        && kind.as_ref().is_none_or(|want| *want == s.kind)
                        && state.is_none_or(|want| want == s.state)
                })
                .collect();
            view()
                .gap_2()
                .child(filter_bar(filters, kinds))
                .child(note(format!("显示 {} 个会话", shown.len())))
                .children(
                    shown
                        .into_iter()
                        .map(|(program, session)| session_row(program, session, nav)),
                )
                .into_element()
        })
    });
    card(
        "会话",
        vec![
            source_status("议会会话", sources.council.sessions),
            source_status("守护会话", sources.guardians.sessions),
        ],
        rows,
    )
}

fn filter_bar(filters: Filters, kinds: Vec<String>) -> View {
    let row = |label: &'static str, chips: Vec<AnyElement>| {
        view()
            .flex_row()
            .flex_wrap()
            .items_center()
            .gap_1()
            .child(note(label).w(px(36.0)))
            .children(chips)
    };
    view()
        .gap_1()
        .child(row(
            "来源",
            [
                (None, "全部"),
                (Some(Program::Council), Program::Council.label()),
                (Some(Program::Guardians), Program::Guardians.label()),
            ]
            .into_iter()
            .map(|(value, label)| chip(label, filters.program, value))
            .collect(),
        ))
        .child(row(
            "种类",
            std::iter::once((None, "全部".to_owned()))
                .chain(kinds.into_iter().map(|k| (Some(k.clone()), k)))
                .map(|(value, label)| chip(label, filters.kind, value))
                .collect(),
        ))
        .child(row(
            "状态",
            [
                (None, "全部"),
                (Some(SessionState::Running), "运行中"),
                (Some(SessionState::Done), "完成"),
                (Some(SessionState::Failed), "失败"),
            ]
            .into_iter()
            .map(|(value, label)| chip(label, filters.state, value))
            .collect(),
        ))
}

/// A filter choice: sets `filter` to `value`, marked while it is.
fn chip<T: Clone + PartialEq + 'static>(
    label: impl Into<SharedString>,
    filter: Signal<Option<T>>,
    value: Option<T>,
) -> AnyElement {
    let current = {
        let value = value.clone();
        move || filter.with(|f| *f == value)
    };
    button()
        .on_press(move || filter.set(value.clone()))
        .selected(current.clone())
        .h(px(24.0))
        .px_2()
        .justify_center()
        .rounded(px(12.0))
        .border_1()
        .border_color(move || theme().border)
        .bg({
            let current = current.clone();
            move || {
                if current() {
                    theme().primary
                } else {
                    theme().background
                }
            }
        })
        .cursor_pointer()
        .child(text(label.into()).text_xs().text_color(move || {
            if current() {
                theme().primary_foreground
            } else {
                theme().foreground
            }
        }))
        .into_element()
}

fn session_row(program: Program, session: Session, nav: Nav) -> impl IntoElement {
    let open = OpenSession {
        program,
        id: session.id.clone(),
    };
    let current = {
        let open = open.clone();
        move || nav.session.with(|s| s.as_ref() == Some(&open))
    };
    let (state_label, state_color) = session_state(session.state);
    button()
        .on_press(move || nav.session.set(Some(open.clone())))
        .selected(current.clone())
        .gap(px(2.0))
        .px_2()
        .py_2()
        .rounded(px(6.0))
        .border_1()
        .border_color(move || theme().border)
        .bg(move || {
            if current() {
                theme().accent
            } else {
                theme().card
            }
        })
        .hover(|s| s.bg(move || theme().accent))
        .cursor_pointer()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(badge(program.label(), TODO))
                .child(badge(session.kind.clone(), TODO))
                .child(badge(state_label, state_color))
                .child(
                    text(session.label.clone())
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .flex_1()
                        .number_of_lines(1),
                ),
        )
        .child(note(format!(
            "{} · 开始 {} · {}{}",
            session.model,
            time::short(&session.started_at),
            duration(&session),
            session
                .reference
                .as_ref()
                .map(|r| format!(" · {r}"))
                .unwrap_or_default()
        )))
        .children(session.error.as_deref().map(|error| {
            text(first_line(error).to_owned())
                .text_xs()
                .text_color(FAILED)
                .number_of_lines(2)
        }))
}

// ---------------------------------------------------------------------------
// 打开的会话

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

fn open_card(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let raw = signal(false);
    let record = {
        let context = context.clone();
        resource(
            move || nav.session.get(),
            move |open: OpenSession, _| {
                let context = context.clone();
                async move {
                    let client = match open.program {
                        Program::Council => &context.council,
                        Program::Guardians => &context.guardians,
                    };
                    let bytes = client.log(&open.id).await?;
                    reduce(open, bytes, context.digest.clone()).await.map(Rc::new)
                }
            },
        )
    };
    let body = dynamic(move || {
        let Some(open) = nav.session.get() else {
            return note("在会话列表里选一个会话。").into_element();
        };
        // The record of the open session only: a previous one stays in the
        // resource while the next loads.
        let current = record.with(|r| r.filter(|r| r.session == open).cloned());
        let error = record.error();
        let loading = record.loading();
        let session = [
            sources.council.sessions,
            sources.guardians.sessions,
        ]
        .into_iter()
        .find_map(|source| {
            source.with(|l| {
                l.and_then(|l| l.data.iter().find(|s| s.id == open.id).cloned())
            })
        });
        let showing_raw = raw.get();
        untracked(move || {
            let header = header(&open, session.as_ref(), raw);
            let content = match (current, error) {
                (_, Some(error)) if !loading => text(format!("取记录失败：{error}"))
                    .text_sm()
                    .text_color(FAILED)
                    .selectable(true)
                    .into_element(),
                (Some(record), _) if showing_raw => raw_lines(record),
                (Some(record), _) => decision_log(&record),
                (None, _) => note("取记录并解析中…").into_element(),
            };
            view()
                .flex_1()
                .min_h(px(0.0))
                .gap_3()
                .child(header)
                .child(content)
                .into_element()
        })
    });
    view()
        .flex_1()
        .min_h(px(0.0))
        .p_4()
        .gap_2()
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .child(body)
}

fn header(open: &OpenSession, session: Option<&Session>, raw: Signal<bool>) -> View {
    let mode = |label: &'static str, want: bool| {
        let current = move || raw.get() == want;
        button()
            .on_press(move || raw.set(want))
            .selected(current)
            .h(px(26.0))
            .px_3()
            .justify_center()
            .rounded(px(6.0))
            .bg(move || {
                if current() {
                    theme().primary
                } else {
                    theme().muted
                }
            })
            .cursor_pointer()
            .child(text(label).text_sm().text_color(move || {
                if current() {
                    theme().primary_foreground
                } else {
                    theme().foreground
                }
            }))
    };
    let title = match session {
        Some(s) => format!("{} · {} · {}", open.program.label(), s.kind, s.label),
        None => format!("{} · {}", open.program.label(), open.id),
    };
    view()
        .gap_1()
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(
                    text(title)
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex_1()
                        .number_of_lines(1),
                )
                .child(mode("决策日志", false))
                .child(mode("原始记录", true)),
        )
        .child(
            note(match session {
                Some(s) => format!(
                    "{} · {} · 开始 {} · {}{}",
                    s.id,
                    s.model,
                    time::short(&s.started_at),
                    duration(s),
                    s.parent
                        .as_ref()
                        .map(|p| format!(" · 分叉自 {p}"))
                        .unwrap_or_default()
                ),
                None => format!("{} 不在最近的会话列表里", open.id),
            })
            .selectable(true),
        )
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
            .text_xs()
            .text_color(WAITING)
            .into_element(),
        );
    }
    if log.unreadable > 0 {
        notes.push(note(format!("{} 行不是完整的 JSON，已跳过。", log.unreadable)).into_element());
    }
    let errors = log
        .entries
        .iter()
        .filter(|e| matches!(e.item, DigestItem::LlmError(_)))
        .count();
    let retries = log
        .entries
        .iter()
        .filter(|e| matches!(e.item, DigestItem::Retry { .. }))
        .count();
    let continuations = log
        .entries
        .iter()
        .filter(|e| matches!(e.item, DigestItem::Continuation(_)))
        .count();
    notes.push(
        note(format!(
            "{} 步 · LLM 异常 {errors} · 自动重试 {retries} · 继续提示 {continuations}",
            log.entries.len()
        ))
        .into_element(),
    );
    scroll()
        .flex_1()
        .child(
            view()
                .gap_2()
                .children(notes)
                .children(log.entries.iter().map(entry)),
        )
        .into_element()
}

/// A step of the log: a label, its time, and what it holds.
fn step(label: String, at: &str, accent: Option<AdaptiveColor>, body: Vec<AnyElement>) -> View {
    let block = view()
        .gap_1()
        .px_3()
        .py_2()
        .rounded(px(6.0))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(
                    text(label)
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || accent.unwrap_or(theme().muted_foreground))
                        .flex_1(),
                )
                .child(note(time::parse_ms(at).map(time::clock).unwrap_or_default())),
        )
        .children(body);
    match accent {
        Some(color) => block
            .bg(color.opacity(0.10))
            .border_l(px(3.0))
            .border_l_color(color),
        None => block.bg(move || theme().muted.opacity(0.5)),
    }
}

fn body_text(text_: &str) -> AnyElement {
    text(text_.to_owned())
        .text_sm()
        .selectable(true)
        .into_element()
}

fn mono(text_: &str) -> AnyElement {
    view()
        .px_2()
        .py_1()
        .rounded(px(4.0))
        .bg(move || theme().background)
        .child(
            text(text_.to_owned())
                .text_xs()
                .font_family("SF Mono, Menlo, monospace")
                .selectable(true),
        )
        .into_element()
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
        DigestItem::Opening(text_) => step("开场消息".to_owned(), at, None, vec![body_text(text_)]),
        DigestItem::ProgramPrompt(text_) => step(
            "程序的继续提示".to_owned(),
            at,
            Some(WAITING),
            vec![body_text(text_)],
        ),
        DigestItem::AssistantText(text_) => {
            step("回答".to_owned(), at, None, vec![body_text(text_)])
        }
        DigestItem::ToolCall { name, args, result } => {
            let mut body = vec![note("参数").into_element(), mono(args)];
            match result {
                Some(output) => {
                    body.push(
                        text(if output.is_error { "结果（工具报错）" } else { "结果" })
                            .text_xs()
                            .text_color({
                                let is_error = output.is_error;
                                move || {
                                    if is_error {
                                        FAILED
                                    } else {
                                        theme().muted_foreground
                                    }
                                }
                            })
                            .into_element(),
                    );
                    body.push(mono(&output.text));
                }
                None => body.push(note("没有结果").into_element()),
            }
            step(format!("工具 {name}"), at, Some(IN_PROGRESS), body)
        }
        DigestItem::Folded { name, count } => view()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .child(note(format!("{name} × {count}（已折叠）"))),
        DigestItem::LlmError(error) => step(
            "LLM 异常".to_owned(),
            at,
            Some(FAILED),
            vec![body_text(error)],
        ),
        DigestItem::Retry {
            attempt,
            status,
            error,
        } => step(
            format!(
                "自动重试 · 第 {attempt} 次{}",
                match status {
                    RetryStatus::Recovered => "，之后恢复",
                    RetryStatus::Superseded => "，之后再次重试",
                }
            ),
            at,
            Some(WAITING),
            vec![body_text(error)],
        ),
        DigestItem::Continuation(text_) => step(
            "继续提示（session-stop-continuation）".to_owned(),
            at,
            Some(WAITING),
            vec![body_text(text_)],
        ),
        DigestItem::Meta(meta) => view()
            .px_3()
            .child(note(format!("pm-meta · {}", meta_line(meta))).selectable(true)),
    }
    .into_element()
}

// ---------------------------------------------------------------------------
// 原始记录

fn raw_lines(record: Rc<Record>) -> AnyElement {
    let count = record.lines.len();
    view()
        .flex_1()
        .min_h(px(0.0))
        .gap_1()
        .child(note(format!(
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
                    .gap_2()
                    .py(px(2.0))
                    .border_b_1()
                    .border_b_color(move || theme().border)
                    .child(
                        text(move || (at.index() + 1).to_string())
                            .text_xs()
                            .tabular_nums()
                            .w(px(44.0))
                            .text_color(move || theme().muted_foreground),
                    )
                    .child(
                        text(line)
                            .text_xs()
                            .font_family("SF Mono, Menlo, monospace")
                            .number_of_lines(4)
                            .flex_1()
                            .selectable(true),
                    )
            })
            .estimated_item_size(px(40.0))
            .flex_1(),
        )
        .into_element()
}
