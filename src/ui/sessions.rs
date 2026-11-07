//! 「会话」(C4 `sessionView`, #9 「会话页」): the sessions of both
//! programs. A bar per hour of the window stays pinned over the table
//! (failures stacked in `fail`), the table scrolls under its sticky head,
//! filtered by state, program, kind and text; beside it, in a split the
//! person can drag, the open session: its facts, its error, and its
//! decision log (C4 `logDigest`) one event a line — automatic retries in a
//! row folded into one, the opening prompt folded to its line count — or
//! its record as it is (「JSON」). The record is fetched when the session
//! opens and parsed off the engine thread where the platform has threads.

use std::{ops::Range, rc::Rc};

use nsh_base::{button, h_resizable, resizable_panel};
use nsh_core::prelude::*;
use serde_json::Value;

use super::{
    icons::{Icon, icon},
    parts::{
        Choice, Look, action, clear, empty, error_text, head_row, kv, mono, nothing, row,
        search_field, segmented, small, source_bar, th, token,
    },
    runtime::short_model,
    shell::{Context, Nav, Split, split},
    status::{self, Role},
    text::Msg,
    tokens::{Type, Typeset, color, len},
};
use crate::{
    attention::first_line,
    config::{ColorToken::*, Digest, LengthToken::*},
    digest::{self, Body, DecisionLog, DigestItem, Entry},
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

/// How long a session ran, or has run so far.
pub fn duration(session: &Session) -> String {
    duration_ms(session).map(time::span).unwrap_or_default()
}

/// What a session was about, without its kind: its label with an issue
/// of the form `owner/repo#n` shortened to `#n`.
pub fn subject(session: &Session) -> String {
    let label = session
        .label
        .strip_prefix(session.kind.as_str())
        .map(str::trim_start)
        .unwrap_or(&session.label);
    match &session.reference {
        Some(issue) => label.replace(&issue.to_string(), &format!("#{}", issue.number)),
        None => label.to_owned(),
    }
}

/// A session's name: its kind's word and its subject.
pub fn title(session: &Session) -> String {
    format!("{} · {}", Msg::Kind(session.kind.clone()), subject(session))
}

/// Both programs' sessions, newest first (tracked).
pub fn all(sources: Sources) -> Vec<(Program, Session)> {
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
    all.sort_by(|a, b| b.1.started_at.cmp(&a.1.started_at));
    all
}

/// The sessions started in each hour of the last `hours`, oldest first,
/// as bars: failures stacked under the rest in `fail`, five ticks under.
pub fn histogram(sessions: &[(Program, Session)], hours: usize, height: Points) -> View {
    let now = time::now_ms();
    let starts = |failed: bool| {
        sessions
            .iter()
            .filter(move |(_, s)| !failed || s.state == SessionState::Failed)
            .filter_map(|(_, s)| time::parse_ms(&s.started_at))
            .collect::<Vec<_>>()
    };
    let all = time::hourly(starts(false), now, hours);
    let failed = time::hourly(starts(true), now, hours);
    let most = all.iter().copied().max().unwrap_or(0).max(1) as f32;
    let tick = |i: usize| {
        let label = if i + 1 == hours {
            Msg::Now.to_string()
        } else {
            time::hm(now - (hours - 1 - i) as f64 * 3_600_000.0)
        };
        small(label)
    };
    let ticks: Vec<usize> = (0..5).map(|k| k * (hours - 1) / 4).collect();
    view()
        .gap(len(Space1))
        .child(
            view()
                .flex_row()
                .items_end()
                .gap(len(Space1) / 2.0)
                .h(height)
                .children(all.into_iter().zip(failed).map(move |(n, f)| {
                    view()
                        .flex_1()
                        .h_full()
                        .justify_end()
                        .child(
                            view()
                                .h(relative((n - f) as f32 / most))
                                .min_h(len(Space1) / 2.0)
                                .bg(if n == 0 { color(Ink4).opacity(0.5) } else { color(BorderStrong) }),
                        )
                        .child(view().h(relative(f as f32 / most)).bg(Role::Fail.ink()))
                })),
        )
        .child(
            view()
                .flex_row()
                .justify_between()
                .children(ticks.into_iter().map(tick)),
        )
}

/// The page's filters and views, which outlive a visit to it.
#[derive(Clone, Copy)]
pub struct SessionsState {
    state: Signal<Option<SessionState>>,
    program: Signal<Option<Program>>,
    kind: Signal<Option<String>>,
    query: Signal<String>,
    /// The open session's record as it is, instead of its decision log.
    json: Signal<bool>,
    /// The decision log's errors, retries and continuations only.
    only_errors: Signal<bool>,
}

impl SessionsState {
    pub fn new() -> Self {
        Self {
            state: signal(None),
            program: signal(None),
            kind: signal(None),
            query: signal(String::new()),
            json: signal(false),
            only_errors: signal(false),
        }
    }

    /// Shows the failed sessions of `program` and `kind`.
    pub fn focus_failed(&self, program: Program, kind: String) {
        batch(|| {
            self.state.set(Some(SessionState::Failed));
            self.program.set(Some(program));
            self.kind.set(Some(kind));
            self.query.set(String::new());
        });
    }

    fn keeps(&self, program: Program, session: &Session) -> bool {
        let query = self.query.get().to_lowercase();
        self.state.get().is_none_or(|want| want == session.state)
            && self.program.get().is_none_or(|want| want == program)
            && self.kind.with(|k| k.as_ref().is_none_or(|want| *want == session.kind))
            && (query.is_empty()
                || [
                    session.id.as_str(),
                    session.label.as_str(),
                    session.model.as_str(),
                    session.error.as_deref().unwrap_or_default(),
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&query)))
    }
}

/// The page.
pub fn page(sources: Sources, context: Rc<Context>, nav: Nav) -> impl IntoElement {
    let state = context.sessions;
    let hours = context.hours();
    let since = move || time::now_ms() - f64::from(hours) * 3_600_000.0;
    let recent = computed(move || {
        let since = since();
        Rc::new(
            all(sources)
                .into_iter()
                .filter(|(_, s)| time::parse_ms(&s.started_at).is_some_and(|at| at >= since))
                .collect::<Vec<_>>(),
        )
    });
    let shown = computed(move || {
        Rc::new(
            recent
                .get()
                .iter()
                .filter(|(p, s)| state.keeps(*p, s))
                .cloned()
                .collect::<Vec<_>>(),
        )
    });
    let step = move |by: isize| {
        let rows = shown.get_untracked();
        if rows.is_empty() {
            return;
        }
        let at = nav.session.with_untracked(|open| {
            open.as_ref()
                .and_then(|o| rows.iter().position(|(p, s)| *p == o.program && s.id == o.id))
        });
        let next = match at {
            Some(at) => (at as isize + by).clamp(0, rows.len() as isize - 1) as usize,
            None => 0,
        };
        let (program, session) = &rows[next];
        nav.session.set(Some(OpenSession {
            program: *program,
            id: session.id.clone(),
        }));
    };
    on_key("j", move |_| step(1));
    on_key("k", move |_| step(-1));
    let list = {
        let context = context.clone();
        move || {
            view()
                .flex_1()
                .min_h(Points::ZERO)
                .child(filter_bar(recent, state))
                .child(
                    view()
                        .px(len(Space6))
                        .pt(len(Space3))
                        .pb(len(Space2))
                        .border_b_1()
                        .border_b_color(color(Border))
                        .child(dynamic(move || {
                            let shown = shown.get();
                            untracked(move || {
                                histogram(&shown, hours as usize, len(Space8) + len(Space3)).into_element()
                            })
                        })),
                )
                .child(source_bar(sources.council.sessions, 0))
                .child(source_bar(sources.guardians.sessions, 0))
                .child(table(shown, context.clone(), nav))
                .into_element()
        }
    };
    let parts = Split {
        list: Rc::new(list),
        inspector: Rc::new(move || inspector(sources, context.clone(), nav).into_element()),
        open: Rc::new(move || nav.session.get().is_some()),
        back: Rc::new(move || nav.session.set(None)),
    };
    split(parts, |parts| {
        h_resizable()
            .flex_1()
            .handle_class("w-px bg-border")
            .child(resizable_panel().child((parts.list)()))
            .child(
                resizable_panel()
                    .size(len(InspectorWidth) + len(Space8))
                    .size_range(len(InspectorWidth)..len(InspectorWidth) * 3.0)
                    .child((parts.inspector)()),
            )
            .into_element()
    })
}

/// The state segments with their counts, the program and kind tokens, and
/// the text filter.
fn filter_bar(recent: Memo<Rc<Vec<(Program, Session)>>>, state: SessionsState) -> View {
    let count = move |want: Option<SessionState>| {
        move || Some(recent.get().iter().filter(|(_, s)| want.is_none_or(|w| w == s.state)).count())
    };
    let segment = |want: Option<SessionState>| {
        Choice::new(
            match want {
                None => Msg::All,
                Some(s) => Msg::Session(s),
            },
            move || state.state.get() == want,
            move || state.state.set(want),
        )
        .count(count(want))
    };
    let kinds = move || {
        let mut kinds: Vec<String> = recent.get().iter().map(|(_, s)| s.kind.clone()).collect();
        kinds.sort();
        kinds.dedup();
        kinds
    };
    view()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space2))
        .flex_wrap()
        .min_h(len(Toolbar))
        .py(len(Space2))
        .px(len(Space6))
        .border_b_1()
        .border_b_color(color(Border))
        .child(segmented(vec![
            segment(None),
            segment(Some(SessionState::Running)),
            segment(Some(SessionState::Failed)),
            segment(Some(SessionState::Done)),
        ]))
        .child(token(
            Msg::FilterProgram,
            move || {
                SharedString::from(match state.program.get() {
                    None => Msg::All,
                    Some(p) => Msg::Program(p),
                })
            },
            move || {
                state.program.update(|p| {
                    *p = match p {
                        None => Some(Program::Council),
                        Some(Program::Council) => Some(Program::Guardians),
                        Some(Program::Guardians) => None,
                    }
                })
            },
        ))
        .child(token(
            Msg::FilterKind,
            move || {
                SharedString::from(match state.kind.get() {
                    None => Msg::All,
                    Some(k) => Msg::Kind(k),
                })
            },
            move || {
                let kinds = kinds();
                state.kind.update(|k| {
                    let at = k.as_ref().and_then(|k| kinds.iter().position(|x| x == k));
                    *k = match at {
                        None => kinds.first().cloned(),
                        Some(at) => kinds.get(at + 1).cloned(),
                    }
                })
            },
        ))
        .child(view().flex_1())
        .child(search_field(state.query, Msg::SessionsFilterHint).w(len(InspectorWidth) * 0.75))
}

/// The columns' widths: glyph, start, (session), model, duration.
fn columns() -> [Points; 4] {
    [len(Space4), len(Space8) * 2.0, len(Space8) * 4.0, len(Space8) * 2.0]
}

fn table(shown: Memo<Rc<Vec<(Program, Session)>>>, context: Rc<Context>, nav: Nav) -> View {
    let [glyph, start, model, took] = columns();
    let _ = context;
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(
            scroll().flex_1().child(
                view()
                    .child(head_row(vec![
                        view().w(glyph).into_element(),
                        th(Msg::Started).w(start).into_element(),
                        th(Msg::SessionColumn).flex_1().into_element(),
                        th(Msg::Model).w(model).into_element(),
                        th(Msg::Duration).w(took).text_right().into_element(),
                    ]))
                    .child(dynamic(move || {
                        let shown = shown.get();
                        untracked(move || {
                            if shown.is_empty() {
                                return empty(Msg::NoSessions).into_element();
                            }
                            view()
                                .children(shown.iter().map(|(program, session)| {
                                    session_row(*program, session.clone(), nav)
                                }))
                                .into_element()
                        })
                    })),
            ),
        )
}

fn session_row(program: Program, session: Session, nav: Nav) -> impl IntoElement {
    let [glyph, start, model, took] = columns();
    let open = OpenSession {
        program,
        id: session.id.clone(),
    };
    let current = {
        let open = open.clone();
        move || nav.session.with(|s| s.as_ref() == Some(&open))
    };
    let state = status::session(session.state);
    row(current, title(&session), move || nav.session.set(Some(open.clone())))
        .child(view().w(glyph).flex_shrink_0().child(status::glyph(state.glyph, state.role)))
        .child(mono(time::parse_ms(&session.started_at).map(time::clock).unwrap_or_default()).w(start).flex_shrink_0())
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .flex_1()
                .min_w(Points::ZERO)
                .overflow_hidden()
                .child(small(Msg::Kind(session.kind.clone()).to_string()).flex_shrink_0())
                .child(text(subject(&session)).ty(Type::Body).flex_shrink(1.0).number_of_lines(1))
                .children(session.error.as_deref().map(|error| {
                    error_text(first_line(error).to_owned())
                        .flex_1()
                        .min_w(Points::ZERO)
                        .number_of_lines(1)
                })),
        )
        .child(small(short_model(&session.model)).w(model).flex_shrink_0().number_of_lines(1))
        .child(mono(duration(&session)).w(took).flex_shrink_0().text_right())
}

// ---------------------------------------------------------------------------
// The open session

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
        let text = String::from_utf8(bytes)
            .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned());
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
        .map_err(|error| Msg::ParseStopped(error.to_string()).to_string())
}

/// Reduces `bytes` on the engine thread (WebAssembly has no other), after
/// the frame that says it is parsing.
#[cfg(target_arch = "wasm32")]
async fn reduce(session: OpenSession, bytes: Vec<u8>, rules: Digest) -> Result<Record, String> {
    next_frame().await;
    Result::Ok(Record::new(session, bytes, &rules))
}

fn inspector(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let state = context.sessions;
    let record = {
        let context = context.clone();
        resource(
            move || nav.session.get(),
            move |open: OpenSession, _| {
                let context = context.clone();
                async move {
                    let bytes = context.program(open.program).log(&open.id).await?;
                    reduce(open, bytes, context.digest.clone()).await.map(Rc::new)
                }
            },
        )
    };
    view().flex_1().min_h(Points::ZERO).child(dynamic(move || {
        let Some(open) = nav.session.get() else {
            return empty(Msg::SelectSession).into_element();
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
        let json = state.json.get();
        let only_errors = state.only_errors.get();
        let context = context.clone();
        untracked(move || {
            let count = current.as_ref().map(|r| (r.log.entries.len(), r.lines.len()));
            let content = match (current, error) {
                (_, Some(error)) if !loading => error_text(Msg::RecordFailed(error).to_string())
                    .selectable(true)
                    .into_element(),
                (Some(record), _) if json => raw_lines(record),
                (Some(record), _) => decision_log(&record, only_errors),
                (None, _) => small(SharedString::from(Msg::Parsing)).into_element(),
            };
            view()
                .flex_1()
                .min_h(Points::ZERO)
                .px(len(Space5))
                .py(len(Space4))
                .gap(len(Space4))
                .child(header(&open, session.as_ref(), &context, nav))
                .children(
                    session
                        .as_ref()
                        .filter(|s| s.state == SessionState::Failed)
                        .map(error_callout),
                )
                .child(log_head(state, count))
                .child(content)
                .into_element()
        })
    }))
}

fn header(open: &OpenSession, session: Option<&Session>, context: &Rc<Context>, nav: Nav) -> View {
    let Some(s) = session else {
        return view()
            .gap(len(Space2))
            .child(small(SharedString::from(Msg::Program(open.program))))
            .child(text(open.id.clone()).ty(Type::Title))
            .child(small(SharedString::from(Msg::NotListed)));
    };
    let state = status::session(s.state);
    let link = s.reference.clone().map(|issue| {
        let context = context.clone();
        let label = Msg::OpenAgenda(super::issues::agenda_of(&context, &issue));
        action(label, Some(Icon::ArrowRight), Look::Link, || false, move || {
            context.open_issue(nav, issue.clone())
        })
    });
    let at = |text: Option<&str>| text.map(time::seconds).unwrap_or_default();
    let value = |text: String| mono(text).text_color(color(Foreground)).number_of_lines(1).selectable(true);
    view()
        .gap(len(Space3))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .child(status::label(&state, Type::Body))
                .child(small(
                    Msg::ProgramKind(s.kind.clone(), open.program).to_string(),
                ))
                .child(view().flex_1())
                .children(link),
        )
        .child(text(title(s)).ty(Type::Title).number_of_lines(2))
        .child(
            view()
                .gap(len(Space1))
                .child(kv(Msg::Model, value(s.model.clone())))
                .child(kv(Msg::Duration, value(duration(s))))
                .child(kv(Msg::Started, value(at(Some(&s.started_at)))))
                .child(kv(Msg::Ended, value(at(s.ended_at.as_deref()))))
                .children(s.parent.clone().map(|parent| kv(Msg::ForkedFrom, value(parent))))
                .child(kv(Msg::SessionColumn, value(s.id.clone()))),
        )
}

/// A failed session's error: its first line, then the rest as a note.
fn error_callout(session: &Session) -> View {
    let error = session.error.clone().unwrap_or_default();
    let line = first_line(&error).to_owned();
    let rest = error
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && *l != line)
        .collect::<Vec<_>>()
        .join(" · ");
    view()
        .flex_row()
        .items_start()
        .gap(len(Space2))
        .px(len(Space4))
        .py(len(Space3))
        .rounded(len(RadiusControl))
        .bg(Role::Fail.subtle())
        .child(icon(Icon::Alert, || Role::Fail.text()).mt(len(Space1) / 2.0))
        .child(
            view()
                .flex_1()
                .min_w(Points::ZERO)
                .gap(len(Space1))
                .child(
                    text(line)
                        .ty(Type::Mono)
                        .text_color(Role::Fail.text())
                        .selectable(true),
                )
                .child(
                    small(if rest.is_empty() { Msg::SessionEnded.to_string() } else { rest })
                        .text_color(Role::Fail.text())
                        .number_of_lines(3)
                        .selectable(true),
                ),
        )
}

fn log_head(state: SessionsState, count: Option<(usize, usize)>) -> View {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space2))
        .pb(len(Space2))
        .border_b_1()
        .border_b_color(color(Border))
        .child(text(SharedString::from(Msg::DecisionLog)).ty(Type::Heading))
        .children(count.map(|(entries, lines)| {
            small(Msg::LogCounts { entries, lines }.to_string())
        }))
        .child(view().flex_1())
        .child(
            button()
                .on_press(move || state.only_errors.update(|o| *o = !*o))
                .accessibility_label(SharedString::from(Msg::OnlyErrors))
                .selected(move || state.only_errors.get())
                .h(len(Control))
                .px(len(Space3))
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(move || {
                    if state.only_errors.get() { color(Selection) } else { color(Input) }
                })
                .bg(move || if state.only_errors.get() { color(Selected) } else { clear() })
                .cursor_pointer()
                .child(text(SharedString::from(Msg::OnlyErrors)).ty(Type::Label)),
        )
        .child(segmented(vec![
            Choice::new(Msg::Compact, move || !state.json.get(), move || state.json.set(false)),
            Choice::new(Msg::Json, move || state.json.get(), move || state.json.set(true)),
        ]))
}

// ---------------------------------------------------------------------------
// The decision log

/// A line of the log: an entry, or automatic retries in a row with the
/// same error folded into one.
enum Line<'a> {
    One(&'a Entry),
    Retries {
        line: String,
        entries: Vec<&'a Entry>,
    },
}

/// The log's lines: consecutive retries with the same first line folded.
fn lines(entries: &[Entry]) -> Vec<Line<'_>> {
    let mut lines: Vec<Line<'_>> = Vec::new();
    for entry in entries {
        if let DigestItem::Retry { error, .. } = &entry.item {
            let first = first_line(error);
            if let Some(Line::Retries { line, entries }) = lines.last_mut()
                && line == first
            {
                entries.push(entry);
                continue;
            }
            lines.push(Line::Retries {
                line: first.to_owned(),
                entries: vec![entry],
            });
        } else {
            lines.push(Line::One(entry));
        }
    }
    lines
}

/// Whether `line` is an error, a retry or a continuation.
fn abnormal(line: &Line<'_>) -> bool {
    match line {
        Line::Retries { .. } => true,
        Line::One(entry) => match &entry.item {
            DigestItem::LlmError(_) | DigestItem::Continuation(_) => true,
            DigestItem::ToolCall { result, .. } => result.as_ref().is_some_and(|r| r.is_error),
            _ => false,
        },
    }
}

fn decision_log(record: &Record, only_errors: bool) -> AnyElement {
    let log = &record.log;
    let mut notes: Vec<AnyElement> = Vec::new();
    if let Some(cut) = log.truncated {
        notes.push(
            small(Msg::Truncated { total: cut.total, kept: cut.total - cut.skipped }.to_string())
                .text_color(Role::Warn.text())
                .into_element(),
        );
    }
    if log.unreadable > 0 {
        notes.push(small(Msg::Unreadable(log.unreadable).to_string()).into_element());
    }
    let shown: Vec<AnyElement> = lines(&log.entries)
        .into_iter()
        .filter(|line| !only_errors || abnormal(line))
        .map(log_line)
        .collect();
    scroll()
        .flex_1()
        .child(
            view()
                .pb(len(Space4))
                .children(notes)
                .children(if shown.is_empty() {
                    vec![empty(Msg::NothingAbnormal).into_element()]
                } else {
                    shown
                }),
        )
        .into_element()
}

fn clock_of(at: &str) -> String {
    time::parse_ms(at).map(time::clock).unwrap_or_default()
}

/// A body's text in one line.
fn one_line(content: &Body) -> String {
    match content {
        Body::Text(text) => first_line(text).to_owned(),
        Body::Json(value) => compact(value),
    }
}

/// A JSON value inline, as written.
fn compact(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// How many lines a body has.
fn line_count(content: &Body) -> usize {
    match content {
        Body::Text(text) => text.lines().count(),
        Body::Json(value) => serde_json::to_string_pretty(value).map_or(1, |t| t.lines().count()),
    }
}

/// A body in full: prose as text, JSON as a tree.
fn body(content: &Body) -> AnyElement {
    match content {
        Body::Text(text_) => text(text_.clone())
            .ty(Type::Body)
            .text_color(color(Ink2))
            .selectable(true)
            .into_element(),
        Body::Json(value) => json_tree(None, value, 0).into_element(),
    }
}

/// One log line: time, icon, what, its detail, a note at the right; a
/// press opens `more` under it.
#[allow(clippy::too_many_arguments)]
fn log_row(
    at: String,
    lead: Icon,
    what: String,
    detail: String,
    right: String,
    role: Option<Role>,
    mono_what: bool,
    more: Option<Rc<dyn Fn() -> AnyElement>>,
) -> AnyElement {
    let open = signal(false);
    let ink = role.map_or(color(Foreground), Role::text);
    let has_more = more.is_some();
    view()
        .border_b_1()
        .border_b_color(color(Border))
        .bg(role.map_or(clear(), Role::subtle))
        .child(
            button()
                .on_press(move || {
                    if has_more {
                        open.update(|o| *o = !*o)
                    }
                })
                .selected(move || open.get())
                .flex_row()
                .items_center()
                .gap(len(Space3))
                .h(len(RowLog))
                .px(len(Space2))
                .when(has_more, |b| b.cursor_pointer().hover(|s| s.bg(color(Accent))))
                .child(mono(at).text_color(color(Ink3)).w(len(Space8) * 2.0))
                .child(icon(lead, move || role.map_or(color(Ink3), Role::text)))
                .child(
                    text(what)
                        .ty(if mono_what { Type::Mono } else { Type::BodyStrong })
                        .text_color(ink)
                        .flex_shrink_0()
                        .number_of_lines(1),
                )
                .child(
                    text(detail)
                        .ty(Type::Mono)
                        .text_color(role.map_or(color(Ink2), Role::text))
                        .flex_1()
                        .min_w(Points::ZERO)
                        .number_of_lines(1),
                )
                .child(small(right)),
        )
        .child(dynamic(move || match (&more, open.get()) {
            (Some(more), true) => untracked(|| {
                view()
                    .px(len(Space8) + len(Space4))
                    .pb(len(Space3))
                    .child(more())
                    .into_element()
            }),
            _ => nothing(),
        }))
        .into_element()
}

fn log_line(line: Line<'_>) -> AnyElement {
    let entry = match line {
        Line::Retries { line, entries } => {
            let first = entries[0];
            let last = entries[entries.len() - 1];
            let errors: Vec<String> = entries
                .iter()
                .filter_map(|e| match &e.item {
                    DigestItem::Retry { attempt, error, .. } => {
                        Some(Msg::RetryAttempt(*attempt, first_line(error).to_owned()).to_string())
                    }
                    _ => None,
                })
                .collect();
            return log_row(
                clock_of(&first.at),
                Icon::Retry,
                Msg::Retries(entries.len()).to_string(),
                line,
                format!("{} – {}", clock_of(&first.at), clock_of(&last.at)),
                Some(Role::Warn),
                false,
                Some(Rc::new(move || {
                    view()
                        .children(errors.iter().map(|e| small(e.clone()).into_element()))
                        .into_element()
                })),
            );
        }
        Line::One(entry) => entry,
    };
    let at = clock_of(&entry.at);
    let content = |c: &Body| {
        let c = c.clone();
        Some(Rc::new(move || body(&c)) as Rc<dyn Fn() -> AnyElement>)
    };
    match &entry.item {
        DigestItem::Opening(c) => log_row(
            at,
            Icon::Message,
            Msg::Opening.to_string(),
            one_line(c),
            Msg::Lines(line_count(c)).to_string(),
            None,
            false,
            content(c),
        ),
        DigestItem::ProgramPrompt(c) => log_row(
            at,
            Icon::Message,
            Msg::ProgramPrompt.to_string(),
            one_line(c),
            Msg::Lines(line_count(c)).to_string(),
            None,
            false,
            content(c),
        ),
        DigestItem::Continuation(c) => log_row(
            at,
            Icon::Retry,
            Msg::Continuation.to_string(),
            one_line(c),
            String::new(),
            Some(Role::Warn),
            false,
            content(c),
        ),
        DigestItem::AssistantText(t) => {
            let c = Body::Text(t.clone());
            log_row(at, Icon::Message, Msg::Assistant.to_string(), one_line(&c), String::new(), None, false, content(&c))
        }
        DigestItem::ToolCall { name, args, result } => {
            let took = result
                .as_ref()
                .and_then(|r| Some(time::parse_ms(&r.at)? - time::parse_ms(&entry.at)?))
                .map(time::span)
                .unwrap_or_else(|| Msg::NoResult.to_string());
            let failed = result.as_ref().is_some_and(|r| r.is_error);
            let (args_, result_) = (args.clone(), result.clone());
            log_row(
                at,
                Icon::Tool,
                name.clone(),
                compact(args),
                took,
                failed.then_some(Role::Fail),
                true,
                Some(Rc::new(move || {
                    view()
                        .gap(len(Space2))
                        .child(small(SharedString::from(Msg::Arguments)))
                        .child(json_tree(None, &args_, 0))
                        .children(result_.as_ref().map(|r| {
                            small(SharedString::from(if r.is_error { Msg::ToolFailed } else { Msg::Result }))
                        }))
                        .children(result_.as_ref().map(|r| body(&r.body)))
                        .into_element()
                })),
            )
        }
        DigestItem::Folded { name, count } => log_row(
            at,
            Icon::File,
            Msg::Folded(name.clone(), *count).to_string(),
            String::new(),
            String::new(),
            None,
            false,
            None,
        ),
        DigestItem::LlmError(error) => {
            let e = error.clone();
            log_row(
                at,
                Icon::Alert,
                first_line(error).to_owned(),
                Msg::NotRetried.to_string(),
                String::new(),
                Some(Role::Fail),
                true,
                Some(Rc::new(move || error_text(e.clone()).selectable(true).into_element())),
            )
        }
        DigestItem::Retry { attempt, error, .. } => log_row(
            at,
            Icon::Retry,
            Msg::RetryAttempt(*attempt, String::new()).to_string(),
            first_line(error).to_owned(),
            String::new(),
            Some(Role::Warn),
            false,
            None,
        ),
        DigestItem::Meta(meta) => log_row(
            at,
            Icon::Sessions,
            "pm-meta".to_owned(),
            [
                Some(meta.kind.clone()),
                meta.seat.clone(),
                meta.reference.clone(),
                meta.base.clone(),
                meta.model.clone(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
            String::new(),
            None,
            false,
            None,
        ),
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
    let name = key.map(|k| mono(format!("{k}:")).text_color(color(Ink3)));
    let Some(count) = items(value) else {
        let shown = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let ink = match value {
            Value::String(_) => color(Ink2),
            _ => Role::Run.text(),
        };
        return view()
            .flex_row()
            .items_start()
            .gap(len(Space1))
            .children(name)
            .child(mono(shown).text_color(ink).flex_1().selectable(true));
    };
    let open = signal(depth < 2);
    let brackets = match value {
        Value::Array(_) => "[…]",
        _ => "{…}",
    };
    let children: Vec<(Option<String>, Value)> = match value {
        Value::Object(fields) => fields.iter().map(|(k, v)| (Some(k.clone()), v.clone())).collect(),
        Value::Array(values) => values
            .iter()
            .enumerate()
            .map(|(i, v)| (Some(i.to_string()), v.clone()))
            .collect(),
        _ => Vec::new(),
    };
    view()
        .gap(len(Space1) / 2.0)
        .child(
            button()
                .on_press(move || open.update(|o| *o = !*o))
                .selected(move || open.get())
                .flex_row()
                .items_center()
                .gap(len(Space1))
                .cursor_pointer()
                .child(dynamic(move || {
                    icon(if open.get() { Icon::ChevronDown } else { Icon::ChevronRight }, || color(Ink3))
                        .into_element()
                }))
                .children(name)
                .child(mono(brackets).text_color(color(Ink3)))
                .child(small(Msg::Items(count).to_string())),
        )
        .child(dynamic(move || {
            if !open.get() {
                return nothing();
            }
            let children = children.clone();
            untracked(move || {
                view()
                    .gap(len(Space1) / 2.0)
                    .pl(len(Space4))
                    .ml(len(Space1))
                    .border_l_1()
                    .border_l_color(color(Border))
                    .children(children.iter().map(|(k, v)| json_tree(k.clone(), v, depth + 1)))
                    .into_element()
            })
        }))
}

/// The record line by line, as `/v1/sessions/{id}/log` returns it.
fn raw_lines(record: Rc<Record>) -> AnyElement {
    let count = record.lines.len();
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
            .child(mono(move || (at.index() + 1).to_string()).text_color(color(Ink3)).w(len(Space6)))
            .child(mono(line).number_of_lines(4).flex_1().selectable(true))
    })
    .estimated_item_size(len(Row))
    .flex_1()
    .into_element()
}
