//! 「守护」(C4 `baseView`): every base of the guardians with its state, a
//! manual rewarm (REQ-26: never automatic) behind a confirmation, and the
//! sessions forked from the base a person selects.

use std::{collections::HashMap, rc::Rc};

use nsh_base::{alert_dialog, button, theme};
use nsh_core::prelude::*;

use super::{
    overview::{Context, base_state},
    parts::{badge, card, note, source_status},
    sessions::{OpenSession, duration, session_state},
    shell::Nav,
    theme::{DONE, FAILED, IN_PROGRESS, TODO, WAITING},
};
use crate::{
    attention::first_line,
    programs::{Base, BaseState, Program, RewarmOutcome, Session},
    sources::Sources,
    time,
};

/// Where a rewarm the person confirmed is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rewarm {
    /// The request is on its way.
    Sending,
    /// What the guardians answered, or why there was no answer.
    Answered(Result<RewarmOutcome, String>),
}

impl Rewarm {
    fn describe(&self) -> (String, AdaptiveColor) {
        match self {
            Rewarm::Sending => ("正在请求重新预热…".to_owned(), IN_PROGRESS),
            Rewarm::Answered(Ok(RewarmOutcome::Started)) => {
                ("已开始重新预热".to_owned(), DONE)
            }
            Rewarm::Answered(Ok(RewarmOutcome::Warming)) => {
                ("正在预热：该基座已在预热，请等它完成".to_owned(), WAITING)
            }
            Rewarm::Answered(Ok(RewarmOutcome::Unknown)) => {
                ("基座不存在：守护程序没有这个基座".to_owned(), FAILED)
            }
            Rewarm::Answered(Err(error)) => (format!("请求失败：{error}"), FAILED),
        }
    }
}

/// The page's state: the selected base and each base's last rewarm.
#[derive(Clone, Copy)]
struct State {
    selected: Signal<Option<String>>,
    rewarms: Signal<HashMap<String, Rewarm>>,
    /// The page's owner: rewarms run under it, so a refresh that redraws
    /// the table does not end one in flight.
    owner: Stored<Owner>,
}

/// The guardians page's content.
pub fn guardians(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let state = State {
        selected: signal(None),
        rewarms: signal(HashMap::new()),
        owner: stored(Owner::current()),
    };
    view()
        .gap_4()
        .p_4()
        .child(bases_card(sources, context, state))
        .child(forks_card(sources, nav, state))
}

// ---------------------------------------------------------------------------
// 基座

fn bases_card(sources: Sources, context: Rc<Context>, state: State) -> View {
    let table = dynamic(move || {
        let Some(bases) = sources.bases.get().map(|l| l.data) else {
            return view().into_element();
        };
        let context = context.clone();
        untracked(move || {
            if bases.is_empty() {
                return note("守护程序没有基座").into_element();
            }
            view()
                .border_t_1()
                .border_t_color(move || theme().border)
                .children(
                    bases
                        .into_iter()
                        .map(|base| base_row(base, sources, context.clone(), state)),
                )
                .into_element()
        })
    });
    card(
        "守护基座",
        vec![
            source_status("基座", sources.bases),
            note("选中一行，下方列出从该基座分叉的会话；重新预热只在人确认后发出。").into_element(),
        ],
        table,
    )
}

/// A labelled value of a base.
fn field(label: &'static str, value: impl Into<SharedString>) -> View {
    view()
        .flex_row()
        .gap_1()
        .child(note(label))
        .child(text(value.into()).text_xs().selectable(true))
}

/// A base: guardian, repository, model, state, stale and why, when it was
/// warmed, its error; the rewarm on the right.
fn base_row(base: Base, sources: Sources, context: Rc<Context>, state: State) -> View {
    let id = base.id.clone();
    let selected = {
        let id = id.clone();
        move || state.selected.with(|s| s.as_deref() == Some(id.as_str()))
    };
    let (state_label, state_color) = base_state(base.state);
    let facts = view()
        .gap(px(3.0))
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(badge(state_label, state_color))
                .when(base.stale, |row| row.child(badge("过期", WAITING)))
                .child(
                    text(format!("{} · {}", base.guardian, base.repo))
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM),
                ),
        )
        .child(
            view()
                .flex_row()
                .flex_wrap()
                .gap_x(px(16.0))
                .child(field("模型", base.model.clone()))
                .child(field(
                    "预热于",
                    base.warmed_at
                        .as_deref()
                        .map(time::short)
                        .unwrap_or_else(|| "—".to_owned()),
                ))
                .child(field("过期", if base.stale { "是" } else { "否" })),
        )
        .when(base.stale, |facts| {
            facts.child(
                text(format!("过期原因：{}", base.stale_reason))
                    .text_xs()
                    .text_color(WAITING)
                    .selectable(true),
            )
        })
        .when_some(base.error.clone(), |facts, error| {
            facts.child(
                text(format!("错误：{}", first_line(&error)))
                    .text_xs()
                    .text_color(FAILED)
                    .number_of_lines(3)
                    .selectable(true),
            )
        });
    let select = button()
        .on_press({
            let id = id.clone();
            move || state.selected.set(Some(id.clone()))
        })
        .selected(selected.clone())
        .flex_1()
        .min_w(px(0.0))
        .px_2()
        .py_2()
        .cursor_pointer()
        .child(facts);
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .border_b_1()
        .border_b_color(move || theme().border)
        .bg({
            let selected = selected.clone();
            move || {
                if selected() {
                    theme().accent
                } else {
                    transparent_black().into()
                }
            }
        })
        .child(select)
        .child(
            view()
                .w(px(200.0))
                .flex_shrink_0()
                .px_2()
                .py_2()
                .child(rewarm_cell(base, sources, context, state)),
        )
}

/// The rewarm button and what the last rewarm of this base got.
fn rewarm_cell(base: Base, sources: Sources, context: Rc<Context>, state: State) -> View {
    let id = base.id.clone();
    let rewarm = {
        let id = id.clone();
        move || state.rewarms.with(|r| r.get(&id).cloned())
    };
    let disabled = {
        let rewarm = rewarm.clone();
        let warming = base.state == BaseState::Warming;
        move || warming || rewarm() == Some(Rewarm::Sending)
    };
    let press = move || confirm_rewarm(&base, sources, context.clone(), state);
    view()
        .gap_1()
        .child(
            button()
                .on_press(press)
                .disabled(disabled.clone())
                .h(px(26.0))
                .px_3()
                .self_start()
                .items_center()
                .justify_center()
                .rounded(move || theme().radius)
                .border_1()
                .border_color(move || theme().border)
                .bg({
                    let disabled = disabled.clone();
                    move || {
                        if disabled() {
                            theme().muted
                        } else {
                            theme().background
                        }
                    }
                })
                .hover(|s| s.bg(move || theme().accent))
                .cursor_pointer()
                .child(text("重新预热").text_sm().text_color({
                    let disabled = disabled.clone();
                    move || {
                        if disabled() {
                            theme().muted_foreground
                        } else {
                            theme().foreground
                        }
                    }
                })),
        )
        .child(dynamic(move || match rewarm() {
            Some(rewarm) => {
                let (message, color) = rewarm.describe();
                text(message)
                    .text_xs()
                    .text_color(color)
                    .number_of_lines(3)
                    .selectable(true)
                    .into_element()
            }
            None => view().into_element(),
        }))
}

/// Asks the person to confirm, then sends the rewarm, shows the answer
/// and loads the bases again.
fn confirm_rewarm(base: &Base, sources: Sources, context: Rc<Context>, state: State) {
    let answer = alert_dialog()
        .title("重新预热基座？")
        .description(format!(
            "守护者：{}\n仓库：{}\n模型：{}\n\n会清空该基座的缓存前缀，后续分叉首轮要重新读入。",
            base.guardian, base.repo, base.model
        ))
        .cancel("cancel", "取消")
        .destructive("rewarm", "重新预热")
        .show();
    let id = base.id.clone();
    state.owner.with(|owner| {
        owner.run(move || {
            spawn(async move {
                let confirmed = answer
                    .await
                    .is_ok_and(|r| r.action.as_deref() == Some("rewarm"));
                if !confirmed {
                    return;
                }
                state
                    .rewarms
                    .update(|r| _ = r.insert(id.clone(), Rewarm::Sending));
                let outcome = context.guardians.rewarm(&id).await;
                state
                    .rewarms
                    .update(|r| _ = r.insert(id, Rewarm::Answered(outcome)));
                sources.bases.refetch();
            })
        })
    });
}

// ---------------------------------------------------------------------------
// 分叉会话

fn forks_card(sources: Sources, nav: Nav, state: State) -> View {
    let body = dynamic(move || {
        let Some(id) = state.selected.get() else {
            return note("在上表选中一个基座。").into_element();
        };
        let base = sources
            .bases
            .with(|l| l.and_then(|l| l.data.iter().find(|b| b.id == id).cloned()));
        let sessions = sources.guardians.sessions.get().map(|l| l.data);
        untracked(move || forks(base, sessions, nav))
    });
    card(
        "分叉会话",
        vec![source_status("守护会话", sources.guardians.sessions)],
        body,
    )
}

fn forks(base: Option<Base>, sessions: Option<Vec<Session>>, nav: Nav) -> AnyElement {
    let Some(base) = base else {
        return note("选中的基座已不在基座表里。").into_element();
    };
    let title = note(format!("{} · {} · {}", base.guardian, base.repo, base.model));
    let Some(session) = base.session.clone() else {
        return view()
            .gap_1()
            .child(title)
            .child(note("该基座还没有会话，没有分叉。"))
            .into_element();
    };
    let Some(sessions) = sessions else {
        return title.into_element();
    };
    let forks: Vec<Session> = sessions
        .into_iter()
        .filter(|s| s.parent.as_deref() == Some(session.as_str()))
        .collect();
    view()
        .gap_1()
        .child(title)
        .child(note(format!("基座会话 {session} · 分叉 {} 个", forks.len())))
        .children(forks.into_iter().map(|fork| fork_row(fork, nav)))
        .into_element()
}

fn fork_row(fork: Session, nav: Nav) -> impl IntoElement {
    let (state_label, state_color) = session_state(fork.state);
    let open = OpenSession {
        program: Program::Guardians,
        id: fork.id.clone(),
    };
    button()
        .on_press(move || nav.open_session(open.clone()))
        .gap(px(2.0))
        .px_2()
        .py_1()
        .rounded(px(6.0))
        .hover(|s| s.bg(move || theme().accent))
        .cursor_pointer()
        .child(
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(badge(fork.kind.clone(), TODO))
                .child(badge(state_label, state_color))
                .child(
                    text(format!(
                        "{}{}",
                        fork.label,
                        fork.reference
                            .as_ref()
                            .map(|r| format!(" · {r}"))
                            .unwrap_or_default()
                    ))
                    .text_sm()
                    .flex_1()
                    .number_of_lines(1),
                )
                .child(note(format!(
                    "{} · {}",
                    time::short(&fork.started_at),
                    duration(&fork)
                ))),
        )
        .children(fork.error.as_deref().map(|error| {
            text(first_line(error).to_owned())
                .text_xs()
                .text_color(FAILED)
                .number_of_lines(2)
        }))
}
