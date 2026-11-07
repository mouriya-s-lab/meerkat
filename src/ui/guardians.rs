//! 「守护基座」(C4 `baseView`): every base of the guardians, grouped by
//! guardian, with its state, a manual rewarm (REQ-26: never automatic)
//! behind a confirmation, and the sessions forked from the base a person
//! selects.

use std::{collections::HashMap, rc::Rc};

use nsh_base::{alert_dialog, button};
use nsh_core::prelude::*;

use super::{
    parts::{
        Look, Span, action, body_row, card, card_head, chip, group_row, head_row, meta, nothing,
        source_trouble, tag, th,
    },
    sessions::{self, OpenSession, duration},
    shell::{Context, Nav, critical, plain, strong},
    status::{self, Role},
    tokens::{Type, Typeset, color, len},
};
use crate::{
    attention::first_line,
    config::{ColorToken::*, LengthToken::*},
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
    fn describe(&self) -> (String, Role) {
        match self {
            Rewarm::Sending => ("正在请求重新预热…".to_owned(), Role::Active),
            Rewarm::Answered(Ok(RewarmOutcome::Started)) => {
                ("已开始重新预热".to_owned(), Role::Good)
            }
            Rewarm::Answered(Ok(RewarmOutcome::Warming)) => {
                ("该基座已在预热，请等它完成".to_owned(), Role::Warning)
            }
            Rewarm::Answered(Ok(RewarmOutcome::Unknown)) => {
                ("守护程序没有这个基座".to_owned(), Role::Critical)
            }
            Rewarm::Answered(Err(error)) => (format!("请求失败：{error}"), Role::Critical),
        }
    }
}

/// Each base's last rewarm, kept by the shell so a rewarm asked for on the
/// overview shows on this page too, and runs on when the page changes.
#[derive(Clone, Copy)]
pub struct Rewarms {
    last: Signal<HashMap<String, Rewarm>>,
    /// The shell's owner: rewarms run under it.
    owner: Stored<Owner>,
}

impl Rewarms {
    pub fn new() -> Self {
        Self {
            last: signal(HashMap::new()),
            owner: stored(Owner::current()),
        }
    }

    fn of(&self, base: &str) -> Option<Rewarm> {
        self.last.with(|r| r.get(base).cloned())
    }
}

/// The rewarm button of `base`: disabled (「预热中」) while it warms or a
/// rewarm is on its way; filled when the base failed or is stale.
pub fn rewarm_button(base: Base, sources: Sources, context: Rc<Context>) -> impl IntoElement {
    let rewarms = context.rewarms;
    let id = base.id.clone();
    let warming = base.state == BaseState::Warming;
    let busy = move || warming || rewarms.of(&id) == Some(Rewarm::Sending);
    let look = if base.state == BaseState::Failed || base.stale {
        Look::Primary
    } else {
        Look::Plain
    };
    let label = {
        let busy = busy.clone();
        move || SharedString::from(if busy() { "预热中" } else { "重新预热" })
    };
    action(label, look, true, busy, move || {
        confirm_rewarm(&base, sources, context.clone())
    })
}

/// Asks the person to confirm, then sends the rewarm, keeps the answer
/// and loads the bases again.
fn confirm_rewarm(base: &Base, sources: Sources, context: Rc<Context>) {
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
    let rewarms = context.rewarms;
    rewarms.owner.with(|owner| {
        owner.run(move || {
            spawn(async move {
                let confirmed = answer
                    .await
                    .is_ok_and(|r| r.action.as_deref() == Some("rewarm"));
                if !confirmed {
                    return;
                }
                rewarms
                    .last
                    .update(|r| _ = r.insert(id.clone(), Rewarm::Sending));
                let outcome = context.guardians.rewarm(&id).await;
                rewarms
                    .last
                    .update(|r| _ = r.insert(id, Rewarm::Answered(outcome)));
                sources.bases.refetch();
            })
        })
    });
}

/// The page's sentence: how many bases, ready (and stale), failed, warming.
pub fn summary(sources: Sources) -> Vec<Span> {
    let Some(bases) = sources.bases.get().map(|l| l.data) else {
        return vec![plain("正在读取守护程序的基座…")];
    };
    let count = |state: BaseState| bases.iter().filter(|b| b.state == state).count();
    let stale = bases.iter().filter(|b| b.stale).count();
    let failed = count(BaseState::Failed);
    let mut spans = vec![
        plain(format!("{} 个基座：", bases.len())),
        strong(count(BaseState::Ready)),
        plain(" 就绪"),
    ];
    if stale > 0 {
        spans.push(plain(format!("（其中 {stale} 个过期）")));
    }
    if failed > 0 {
        spans.extend([plain("、"), critical(format!("{failed} 失败"))]);
    }
    spans.push(plain(format!("、{} 预热中。", count(BaseState::Warming))));
    spans
}

/// The page's selection: the base whose forks show.
#[derive(Clone, Copy)]
struct State {
    selected: Signal<Option<String>>,
}

/// The guardians page's content.
pub fn guardians(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let state = State {
        selected: signal(None),
    };
    view()
        .gap(len(Space5))
        .px(len(Space6))
        .pt(len(Space5))
        .pb(len(Space6))
        .child(bases_card(sources, context, state))
        .child(meta(
            "基座只在首次启动时预热，之后只在你按「重新预热」时重来；过期表示仓库在预热之后有了新提交。选中一行，下方列出从该基座分叉的会话。",
        ))
        .child(forks_card(sources, nav, state))
}

// ---------------------------------------------------------------------------
// 基座

/// The columns' shares of the row's width: state, repository, model,
/// warmed at, reason, rewarm.
const SHARES: [f32; 6] = [2.0, 3.0, 3.5, 1.6, 4.0, 1.4];

fn cell(share: usize) -> View {
    view()
        .flex_grow(SHARES[share])
        .basis(Points::ZERO)
        .min_w(Points::ZERO)
}

fn bases_card(sources: Sources, context: Rc<Context>, state: State) -> View {
    let rows = dynamic(move || {
        let Some(bases) = sources.bases.get().map(|l| l.data) else {
            return nothing();
        };
        let context = context.clone();
        untracked(move || {
            if bases.is_empty() {
                return meta("守护程序没有基座").p(len(Space4)).into_element();
            }
            let mut guardians: Vec<&str> = bases.iter().map(|b| b.guardian.as_str()).collect();
            guardians.sort_unstable();
            guardians.dedup();
            view()
                .children(guardians.into_iter().flat_map(|guardian| {
                    let group: Vec<Base> = bases
                        .iter()
                        .filter(|b| b.guardian == guardian)
                        .cloned()
                        .collect();
                    std::iter::once(
                        group_row(format!("守护 · {guardian} · {}", group.len())).into_element(),
                    )
                    .chain(
                        group.into_iter().map(|base| {
                            base_row(base, sources, context.clone(), state).into_element()
                        }),
                    )
                    .collect::<Vec<_>>()
                }))
                .into_element()
        })
    });
    card()
        .child(source_trouble("基座", sources.bases))
        .child(head_row(
            ["状态", "仓库", "模型", "预热于", "原因", ""]
                .into_iter()
                .enumerate()
                .map(|(i, label)| cell(i).child(th(label)).into_element())
                .collect(),
        ))
        .child(rows)
}

/// A base: state (and stale), repository, model, when it was warmed, why
/// it needs a look, the rewarm and its last answer.
fn base_row(base: Base, sources: Sources, context: Rc<Context>, state: State) -> impl IntoElement {
    let id = base.id.clone();
    let selected = {
        let id = id.clone();
        move || state.selected.with(|s| s.as_deref() == Some(id.as_str()))
    };
    let reason: AnyElement = match (&base.error, base.state, base.stale) {
        (Some(error), _, _) => text(first_line(error).to_owned())
            .ty(Type::Meta)
            .mono()
            .text_color(Role::Critical.text())
            .truncate()
            .selectable(true)
            .into_element(),
        (None, _, true) => text(base.stale_reason.clone())
            .ty(Type::Dense)
            .text_color(Role::Warning.text())
            .truncate()
            .into_element(),
        (None, BaseState::Warming, false) => meta("预热中").into_element(),
        (None, _, false) => meta("—").into_element(),
    };
    let rewarms = context.rewarms;
    let answer = {
        let id = id.clone();
        dynamic(move || match rewarms.of(&id) {
            Some(rewarm) => {
                let (message, role) = rewarm.describe();
                text(message)
                    .ty(Type::Meta)
                    .text_color(role.text())
                    .truncate()
                    .selectable(true)
                    .into_element()
            }
            None => nothing(),
        })
    };
    let select = {
        let id = id.clone();
        move || state.selected.set(Some(id.clone()))
    };
    let stale = base.stale;
    let base_state = base.state;
    body_row()
        .bg({
            let selected = selected.clone();
            move || {
                if selected() {
                    Role::Active.subtle()
                } else {
                    color(Card)
                }
            }
        })
        .child(
            button()
                .on_press(select)
                .accessibility_label(format!("{} {}", base.repo, base.model))
                .selected(selected)
                .flex_row()
                .items_center()
                .gap(len(Space3))
                .flex_grow(SHARES[..5].iter().sum::<f32>())
                .basis(Points::ZERO)
                .min_w(Points::ZERO)
                .self_stretch()
                .cursor_pointer()
                .child(
                    cell(0)
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .child(status::label(&status::base(base_state), Type::Dense))
                        .when(stale, |c| {
                            c.child(status::label(&status::stale(), Type::Dense))
                        }),
                )
                .child(cell(1).child(text(base.repo.clone()).ty(Type::Dense).mono().truncate()))
                .child(
                    cell(2).flex_row().child(
                        chip(base.model.clone())
                            .flex_shrink(1.0)
                            .min_w(Points::ZERO),
                    ),
                )
                .child(
                    cell(3).child(
                        text(
                            base.warmed_at
                                .as_deref()
                                .map(time::short)
                                .unwrap_or_else(|| "—".to_owned()),
                        )
                        .ty(Type::Meta)
                        .mono()
                        .text_color(color(MutedForeground)),
                    ),
                )
                .child(cell(4).child(reason).child(answer)),
        )
        .child(
            cell(5)
                .flex_row()
                .justify_end()
                .child(rewarm_button(base, sources, context)),
        )
}

// ---------------------------------------------------------------------------
// 分叉会话

fn forks_card(sources: Sources, nav: Nav, state: State) -> View {
    let body = dynamic(move || {
        let Some(id) = state.selected.get() else {
            return meta("在上表选中一个基座。")
                .px(len(Space4))
                .pb(len(Space4))
                .into_element();
        };
        let base = sources
            .bases
            .with(|l| l.and_then(|l| l.data.iter().find(|b| b.id == id).cloned()));
        let sessions = sources.guardians.sessions.get().map(|l| l.data);
        untracked(move || forks(base, sessions, nav))
    });
    card()
        .child(card_head(
            "分叉会话",
            "从选中基座的会话分叉出的守护会话",
            Vec::new(),
        ))
        .child(source_trouble("守护会话", sources.guardians.sessions))
        .child(body)
}

fn forks(base: Option<Base>, sessions: Option<Vec<Session>>, nav: Nav) -> AnyElement {
    let note = |s: String| meta(s).px(len(Space4)).pb(len(Space3)).into_element();
    let Some(base) = base else {
        return note("选中的基座已不在基座表里。".to_owned());
    };
    let Some(session) = base.session.clone() else {
        return note(format!(
            "{} · {}：该基座还没有会话，没有分叉。",
            base.guardian, base.repo
        ));
    };
    let Some(sessions) = sessions else {
        return nothing();
    };
    let forks: Vec<Session> = sessions
        .into_iter()
        .filter(|s| s.parent.as_deref() == Some(session.as_str()))
        .collect();
    view()
        .child(note(format!(
            "{} · {} · {} · 基座会话 {session} · 分叉 {} 个",
            base.guardian,
            base.repo,
            base.model,
            forks.len()
        )))
        .children(forks.into_iter().map(|fork| fork_row(fork, nav)))
        .into_element()
}

fn fork_row(fork: Session, nav: Nav) -> impl IntoElement {
    let open = OpenSession {
        program: Program::Guardians,
        id: fork.id.clone(),
    };
    button()
        .on_press(move || nav.open_session(open.clone()))
        .accessibility_label(sessions::title(&fork))
        .hover(|s| s.bg(color(Muted)))
        .cursor_pointer()
        .child(
            body_row()
                .child(status::label(&status::session(fork.state), Type::Dense))
                .child(tag(fork.kind.clone()))
                .child(
                    text(format!(
                        "{}{}",
                        fork.label,
                        fork.reference
                            .as_ref()
                            .map(|r| format!(" · {r}"))
                            .unwrap_or_default()
                    ))
                    .ty(Type::Dense)
                    .font_weight(FontWeight::MEDIUM)
                    .flex_1()
                    .truncate(),
                )
                .children(fork.error.as_deref().map(|error| {
                    text(first_line(error).to_owned())
                        .ty(Type::Meta)
                        .mono()
                        .text_color(Role::Critical.text())
                        .truncate()
                        .flex_1()
                }))
                .child(
                    text(format!(
                        "{} · {}",
                        time::short(&fork.started_at),
                        duration(&fork)
                    ))
                    .ty(Type::Meta)
                    .mono()
                    .text_color(color(MutedForeground)),
                ),
        )
}
