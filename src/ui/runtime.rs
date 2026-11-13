//! 「运行状况」(C4 `baseView`, #9 「运行状况页」): the guardian bases
//! summed up and grouped by repository, a base's rewarm (never automatic,
//! behind a confirmation) on the hovered or selected row and in
//! the inspector with the sessions forked from it; the programs; the
//! GitHub API's limits and the sessions an LLM limited.

use std::{collections::HashMap, rc::Rc};

use nsh_base::{alert_dialog, button};
use nsh_core::prelude::*;

use super::{
    icons::Icon,
    overview::running_asks,
    parts::{
        Look, action, empty, error_text, group_header, head_row, hint, kv, meter,
        mono, nothing, small, source_bar, th,
    },
    sessions::{self, OpenSession, duration},
    shell::{Context, Nav, Split, side, split},
    status::{self, Role},
    text::Msg,
    tokens::{Type, Typeset, color, len},
};
use crate::{
    attention::first_line,
    config::{ColorToken::*, LengthToken::*},
    failure::Failure,
    gh::rate::{Api, Bucket},
    programs::{Base, BaseState, Program, QueueState, RewarmOutcome, Session},
    sources::{ProgramSources, Sources},
    time,
};

/// Where a rewarm the person confirmed is.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rewarm {
    /// The request is on its way.
    Sending,
    /// What the guardians answered, or why there was no answer.
    Answered(Result<RewarmOutcome, Failure>),
}

impl Rewarm {
    fn describe(&self) -> (Msg, Role) {
        match self {
            Rewarm::Sending => (Msg::RewarmSending, Role::Run),
            Rewarm::Answered(Result::Ok(RewarmOutcome::Started)) => (Msg::RewarmStarted, Role::Ok),
            Rewarm::Answered(Result::Ok(RewarmOutcome::Warming)) => (Msg::RewarmBusy, Role::Warn),
            Rewarm::Answered(Result::Ok(RewarmOutcome::Unknown)) => (Msg::RewarmUnknown, Role::Fail),
            Rewarm::Answered(Err(error)) => (Msg::RewarmFailed(error.clone()), Role::Fail),
        }
    }
}

/// Each base's last rewarm, kept by the shell so a rewarm asked for on the
/// overview shows here too, and runs on when the page changes.
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
/// rewarm is on its way.
pub fn rewarm_button(base: Base, sources: Sources, context: Rc<Context>) -> impl IntoElement {
    let rewarms = context.rewarms;
    let id = base.id.clone();
    let warming = base.state == BaseState::Warming;
    let busy = move || warming || rewarms.of(&id) == Some(Rewarm::Sending);
    let label = {
        let busy = busy.clone();
        move || SharedString::from(if busy() { Msg::Base(BaseState::Warming) } else { Msg::Rewarm })
    };
    action(label, Some(Icon::Refresh), Look::Plain, busy, move || {
        confirm_rewarm(&base, sources, context.clone())
    })
}

/// Asks the person to confirm, then sends the rewarm, keeps the answer
/// and loads the bases again.
fn confirm_rewarm(base: &Base, sources: Sources, context: Rc<Context>) {
    let answer = alert_dialog()
        .title(Msg::RewarmConfirm.to_string())
        .description(
            Msg::RewarmDetail {
                guardian: base.guardian.clone(),
                repo: base.repo.clone(),
                model: base.model.clone(),
            }
            .to_string(),
        )
        .cancel("cancel", Msg::Cancel.to_string())
        .destructive("rewarm", Msg::Rewarm.to_string())
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

/// `owner/name` without the owner.
pub fn short_repo(repo: &str) -> String {
    repo.rsplit('/').next().unwrap_or(repo).to_owned()
}

/// `provider/model` without the provider.
pub fn short_model(model: &str) -> String {
    model.rsplit('/').next().unwrap_or(model).to_owned()
}

/// A base by its repository and model, both short.
pub fn base_name(base: &Base) -> String {
    format!("{} · {}", short_repo(&base.repo), short_model(&base.model))
}

/// The guardians' sessions forked from `base` (whose `parent` is its
/// session) within the window, newest first.
fn forks_of<'a>(base: &Base, sessions: &'a [Session], since: f64) -> Vec<&'a Session> {
    let Some(session) = base.session.as_deref() else {
        return Vec::new();
    };
    let mut forks: Vec<&Session> = sessions
        .iter()
        .filter(|s| s.parent.as_deref() == Some(session))
        .filter(|s| time::parse_ms(&s.started_at).is_some_and(|at| at >= since))
        .collect();
    forks.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    forks
}

/// The page's selection and the row under the pointer.
#[derive(Clone, Copy)]
struct State {
    selected: Signal<Option<String>>,
    hovered: Signal<Option<String>>,
}

/// The page.
pub fn page(sources: Sources, context: Rc<Context>, nav: Nav) -> impl IntoElement {
    let state = State {
        selected: signal(None),
        hovered: signal(None),
    };
    // The first base is selected until the person selects one.
    let selected = move || {
        state.selected.get().or_else(|| {
            sources
                .bases
                .with(|b| b.and_then(|b| b.data.first().map(|b| b.id.clone())))
        })
    };
    let list = {
        let context = context.clone();
        move || {
            scroll()
                .flex_1()
                .child(
                    view()
                        .pb(len(Space6))
                        .child(summary(sources, context.clone()))
                        .child(bases_table(sources, context.clone(), state, selected))
                        .child(programs(sources, context.clone()))
                        .child(limits(sources, context.clone())),
                )
                .into_element()
        }
    };
    let parts = Split {
        list: Rc::new(list),
        inspector: Rc::new(move || inspector(sources, context.clone(), nav, selected).into_element()),
        open: Rc::new(move || state.selected.get().is_some()),
        back: Rc::new(move || state.selected.set(None)),
    };
    split(parts, |parts| {
        view()
            .flex_1()
            .min_h(Points::ZERO)
            .flex_row()
            .child(view().flex_1().min_w(Points::ZERO).child((parts.list)()))
            .child(side((parts.inspector)()))
            .into_element()
    })
}

/// 就绪 a/n · 过期 k · 最旧预热 · 分叉会话 in the window.
fn summary(sources: Sources, context: Rc<Context>) -> View {
    let hours = context.hours();
    let now = context.now;
    view()
        .flex_row()
        .items_center()
        .gap(len(Space8))
        .px(len(Space6))
        .py(len(Space4))
        .border_b_1()
        .border_b_color(color(Border))
        .child(text(SharedString::from(Msg::Bases)).ty(Type::Heading).w(len(Space8) * 3.0))
        .child(dynamic(move || {
            let bases = sources.bases.get().map(|l| l.data);
            let sessions = sources.guardians.sessions.get().map(|l| l.data).unwrap_or_default();
            let now = now.get();
            untracked(move || {
                let Some(bases) = bases else {
                    return nothing();
                };
                let since = now - f64::from(hours) * 3_600_000.0;
                let ready = bases.iter().filter(|b| b.state == BaseState::Ready).count();
                let stale = bases.iter().filter(|b| b.stale).count();
                let oldest = bases
                    .iter()
                    .filter_map(|b| b.warmed_at.as_deref().and_then(time::parse_ms))
                    .fold(None, |o: Option<f64>, at| Some(o.map_or(at, |o| o.min(at))));
                let forks: usize = bases.iter().map(|b| forks_of(b, &sessions, since).len()).sum();
                let figure = |value: String, label: Msg, role: Option<Role>| {
                    view()
                        .child(text(value).ty(Type::Num).text_color(role.map_or(color(Foreground), Role::text)))
                        .child(small(SharedString::from(label)))
                };
                // Figures that do not fit the row go on to the next line.
                view()
                    .flex_row()
                    .flex_wrap()
                    .flex_1()
                    .min_w(Points::ZERO)
                    .gap(len(Space8))
                    .child(figure(Msg::Fraction(ready, bases.len()).to_string(), Msg::Base(BaseState::Ready), None))
                    .child(figure(stale.to_string(), Msg::Stale, (stale > 0).then_some(Role::Warn)))
                    .child(figure(
                        oldest.map(|at| Msg::Span(time::ago(at, now)).to_string()).unwrap_or_default(),
                        Msg::OldestWarm,
                        None,
                    ))
                    .child(figure(forks.to_string(), Msg::ForksInHours(hours), None))
                    .into_element()
            })
        }))
}

fn bases_table(
    sources: Sources,
    context: Rc<Context>,
    state: State,
    selected: impl Fn() -> Option<String> + Clone + 'static,
) -> View {
    let hours = context.hours();
    view()
        .child(source_bar(sources.bases, 4))
        .child(head_row(vec![
            view().w(len(Space4)).into_element(),
            th(Msg::Model).flex_1().min_w(Points::ZERO).into_element(),
            th(Msg::State).w(len(Space8) * 2.5).into_element(),
            th(Msg::WarmedAt).w(len(Space8) * 2.5).into_element(),
            th(Msg::ForksShort(hours)).w(len(Space8) * 2.0).into_element(),
            view().w(len(Space8) * 3.5).into_element(),
        ]))
        .child(dynamic(move || {
            let Some(bases) = sources.bases.get().map(|l| l.data) else {
                return nothing();
            };
            let sessions = sources.guardians.sessions.get().map(|l| l.data).unwrap_or_default();
            let context = context.clone();
            let selected = selected.clone();
            untracked(move || {
                if bases.is_empty() {
                    return empty(Msg::NoBases).into_element();
                }
                let since = time::now_ms() - f64::from(hours) * 3_600_000.0;
                let mut repos: Vec<(&str, &str)> =
                    bases.iter().map(|b| (b.repo.as_str(), b.guardian.as_str())).collect();
                repos.sort_unstable();
                repos.dedup();
                let mut children: Vec<AnyElement> = Vec::new();
                for (repo, guardian) in repos {
                    let group: Vec<&Base> =
                        bases.iter().filter(|b| b.repo == repo && b.guardian == guardian).collect();
                    children.push(
                        group_header(nothing(), repo.to_owned(), group.len())
                            .child(small(guardian.to_owned()))
                            .into_element(),
                    );
                    for base in group {
                        let forks = forks_of(base, &sessions, since).len();
                        children.push(
                            base_row(base.clone(), forks, sources, context.clone(), state, selected.clone())
                                .into_element(),
                        );
                    }
                }
                view().children(children).into_element()
            })
        }))
}

/// A base: state, model with its stale reason or error, the states'
/// words, when it was warmed, its forks, and its rewarm while hovered or
/// selected.
fn base_row(
    base: Base,
    forks: usize,
    sources: Sources,
    context: Rc<Context>,
    state: State,
    selected: impl Fn() -> Option<String> + Clone + 'static,
) -> impl IntoElement {
    let id = base.id.clone();
    let is_selected = {
        let id = id.clone();
        move || selected().as_deref() == Some(id.as_str())
    };
    let shows_action = {
        let (id, is_selected) = (id.clone(), is_selected.clone());
        move || is_selected() || state.hovered.with(|h| h.as_deref() == Some(id.as_str()))
    };
    let glyph = if base.stale { status::stale() } else { status::base(base.state) };
    let words = if base.stale {
        Msg::StateAndStale(base.state).to_string()
    } else {
        Msg::Base(base.state).to_string()
    };
    let warmed = base.warmed_at.clone();
    let now = context.now;
    let rewarms = context.rewarms;
    let answer = {
        let id = id.clone();
        dynamic(move || match rewarms.of(&id) {
            Some(rewarm) => {
                let (message, role) = rewarm.describe();
                small(SharedString::from(message)).text_color(role.text()).into_element()
            }
            None => nothing(),
        })
    };
    let under = match (&base.error, base.stale) {
        (Some(error), _) => Some(error_text(first_line(error).to_owned()).number_of_lines(2)),
        (None, true) => Some(mono(base.stale_reason.clone()).text_color(Role::Warn.text()).number_of_lines(2)),
        (None, false) => None,
    };
    let (enter, leave) = (id.clone(), id.clone());
    let select = id.clone();
    let action_base = base.clone();
    let action_context = context.clone();
    let absolute = base.warmed_at.as_deref().map(time::seconds).unwrap_or_default();
    let row = super::parts::row(is_selected, format!("{} {}", base.repo, base.model), move || {
        state.selected.set(Some(select.clone()))
    })
    .child(status::glyph(glyph.glyph, glyph.role))
    .child(
        view()
            .flex_1()
            .min_w(Points::ZERO)
            .py(len(Space1))
            .child(mono(short_model(&base.model)).text_color(color(Foreground)).number_of_lines(1))
            .children(under)
            .child(answer),
    )
    .child(text(words).ty(Type::Body).w(len(Space8) * 2.5))
    .child(
        view().w(len(Space8) * 2.5).child(
            small(move || {
                warmed
                    .as_deref()
                    .and_then(time::parse_ms)
                    .map(|at| SharedString::from(Msg::Ago(time::ago(at, now.get()))))
                    .unwrap_or_default()
            })
            .text_color(color(Foreground))
            .tooltip(hint(absolute)),
        ),
    )
    .child(text(forks.to_string()).ty(Type::Body).w(len(Space8) * 2.0))
    .child(
        view()
            .w(len(Space8) * 3.5)
            .flex_row()
            .justify_end()
            .child(dynamic(move || {
                if shows_action() {
                    rewarm_button(action_base.clone(), sources, action_context.clone()).into_element()
                } else {
                    nothing()
                }
            })),
    );
    view()
        .on_mouse_enter(move |_: &MouseEvent| state.hovered.set(Some(enter.clone())))
        .on_mouse_leave(move |_: &MouseEvent| {
            state.hovered.update(|h| {
                if h.as_deref() == Some(leave.as_str()) {
                    *h = None
                }
            })
        })
        .child(row)
}

/// The selected base: when it was warmed, whether it is stale and why,
/// its ids, the rewarm, and the sessions forked from it.
fn inspector(
    sources: Sources,
    context: Rc<Context>,
    nav: Nav,
    selected: impl Fn() -> Option<String> + 'static,
) -> View {
    let hours = context.hours();
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(scroll().flex_1().child(dynamic(move || {
            let base = selected().and_then(|id| {
                sources.bases.with(|b| b.and_then(|b| b.data.iter().find(|x| x.id == id).cloned()))
            });
            let sessions = sources.guardians.sessions.get().map(|l| l.data).unwrap_or_default();
            let context = context.clone();
            untracked(move || {
                let Some(base) = base else {
                    return empty(Msg::SelectBase).into_element();
                };
                let since = time::now_ms() - f64::from(hours) * 3_600_000.0;
                let forks: Vec<Session> = forks_of(&base, &sessions, since).into_iter().cloned().collect();
                let failed = forks.iter().filter(|s| s.state == crate::programs::SessionState::Failed).count();
                let glyph = if base.stale { status::stale() } else { status::base(base.state) };
                let now = context.now;
                let warmed = base.warmed_at.clone();
                view()
                    .px(len(Space5))
                    .py(len(Space4))
                    .gap(len(Space3))
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(len(Space2))
                            .child(status::label(&glyph, Type::Body))
                            .child(small(base.guardian.clone())),
                    )
                    .child(text(base_name(&base)).ty(Type::Title).number_of_lines(2))
                    .child(
                        view()
                            .gap(len(Space1))
                            .child(kv(
                                Msg::WarmedAt,
                                small(move || {
                                    SharedString::from(Msg::AtAndAgo(
                                        warmed.as_deref().map(time::seconds).unwrap_or_default(),
                                        warmed.as_deref().and_then(time::parse_ms).map(|at| time::ago(at, now.get())),
                                    ))
                                })
                                .text_color(color(Foreground)),
                            ))
                            .child(kv(
                                Msg::Stale,
                                text(if base.stale {
                                    base.stale_reason.clone()
                                } else {
                                    Msg::No.to_string()
                                })
                                .ty(if base.stale { Type::Mono } else { Type::Body })
                                .text_color(if base.stale { Role::Warn.text() } else { color(Foreground) }),
                            ))
                            .children(base.error.as_ref().map(|error| {
                                kv(Msg::Error, error_text(error.clone()).selectable(true))
                            }))
                            .child(kv(Msg::BaseId, mono(base.id.clone()).selectable(true)))
                            .child(kv(
                                Msg::SessionId,
                                mono(base.session.clone().unwrap_or_default()).selectable(true),
                            )),
                    )
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(len(Space3))
                            .child(rewarm_button(base.clone(), sources, context.clone()))
                            .child(small(SharedString::from(Msg::RewarmNote)).flex_1()),
                    )
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .pt(len(Space3))
                            .border_t_1()
                            .border_t_color(color(Border))
                            .child(small(SharedString::from(Msg::ForksInHours(hours))).flex_1())
                            .child(small(Msg::ForksNote { forks: forks.len(), failed }.to_string())),
                    )
                    .children(forks.iter().take(8).map(|fork| fork_row(fork.clone(), nav)))
                    .children((forks.len() > 8).then(|| {
                        let n = forks.len();
                        action(Msg::AllForks(n), Some(Icon::ArrowRight), Look::Link, || false, move || {
                            nav.page.set(super::shell::Page::Sessions)
                        })
                    }))
                    .into_element()
            })
        })))
}

fn fork_row(fork: Session, nav: Nav) -> impl IntoElement {
    let open = OpenSession {
        program: Program::Guardians,
        id: fork.id.clone(),
    };
    let state = status::session(fork.state);
    button()
        .on_press(move || nav.open_session(open.clone()))
        .accessibility_label(sessions::title(&fork))
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(RowLog))
        .rounded(len(RadiusControl))
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(status::glyph(state.glyph, state.role))
        .child(mono(time::parse_ms(&fork.started_at).map(time::clock).unwrap_or_default()))
        .child(
            text(sessions::subject(&fork))
                .ty(Type::Body)
                .flex_shrink_0()
                .number_of_lines(1),
        )
        .children(fork.error.as_deref().map(|error| {
            error_text(first_line(error).to_owned()).flex_1().min_w(Points::ZERO).number_of_lines(1)
        }))
        .child(view().flex_1())
        .child(small(duration(&fork)))
}

fn section(title: Msg) -> View {
    view()
        .px(len(Space6))
        .pt(len(Space6))
        .pb(len(Space2))
        .child(text(SharedString::from(title)).ty(Type::Heading))
}

/// Each program: state, its full version and how long it runs, then its
/// work and when it was last checked under them.
fn programs(sources: Sources, context: Rc<Context>) -> View {
    let line = move |program: ProgramSources| {
        let now = context.now;
        let version = dynamic(move || {
            let full = program.health.with(|l| l.map(|l| l.data.version.clone())).unwrap_or_default();
            mono(full.clone()).truncate().tooltip(hint(full)).into_element()
        });
        view()
            .flex_row()
            .items_center()
            .gap(len(Space3))
            .min_h(len(Row) + len(Space1))
            .py(len(Space2))
            .border_b_1()
            .border_b_color(color(Border))
            .child(dynamic(move || {
                let state = status::program(!program.health.with_error(|e| e.is_some()));
                status::label(&state, Type::Body).w(len(Space8) * 2.5).into_element()
            }))
            .child(text(SharedString::from(Msg::Program(program.program))).ty(Type::BodyStrong).w(len(Space8) * 2.0))
            .child(
                view()
                    .flex_1()
                    .min_w(Points::ZERO)
                    .gap(len(Space1))
                    .child(
                        view()
                            .flex_row()
                            .items_center()
                            .gap(len(Space3))
                            .child(view().flex_1().min_w(Points::ZERO).child(version))
                            .child(small(move || {
                                program
                                    .health
                                    .with(|l| l.and_then(|l| time::parse_ms(&l.data.started_at)))
                                    .map(|at| SharedString::from(Msg::Running(time::ago(at, now.get()))))
                                    .unwrap_or_default()
                            }).text_color(color(Foreground)).flex_shrink_0()),
                    )
                    .child(small(move || {
                        let checked = program.health.with(|l| l.map(|l| l.at));
                        let work = match program.program {
                            Program::Council => sources.queue.with(|q| {
                                q.map(|q| Msg::QueueWork {
                                    open: q.data.iter().filter(|e| !e.state.settled()).count(),
                                    deliberating: q.data.iter().filter(|e| e.state == QueueState::Deliberating).count(),
                                })
                            }),
                            Program::Guardians => Some(Msg::AsksRunning(running_asks(program))),
                        };
                        SharedString::from(Msg::WorkAndChecked(
                            work.map(Box::new),
                            checked.map(|at| time::ago(at, now.get())),
                        ))
                    }).truncate()),
            )
    };
    view()
        .child(section(Msg::Programs))
        .child(
            view()
                .px(len(Space6))
                .child(source_bar(sources.council.health, 0))
                .child(source_bar(sources.guardians.health, 0))
                .child(line(sources.council))
                .child(line(sources.guardians)),
        )
}

/// GitHub's REST and GraphQL allowances, and the sessions an LLM limited
/// (an error with `429`) in the window.
fn limits(sources: Sources, context: Rc<Context>) -> View {
    let hours = context.hours();
    let bucket_cell = |api: Api, bucket: Option<Bucket>, threshold: u8| {
        let Some(bucket) = bucket else {
            return view().flex_1().child(small(SharedString::from(Msg::Unknown)));
        };
        let low = bucket.below(threshold);
        view()
            .flex_1()
            .gap(len(Space2))
            .child(
                view()
                    .flex_row()
                    .gap(len(Space2))
                    .child(text(SharedString::from(Msg::GitHubBucket(api))).ty(Type::BodyStrong).flex_1())
                    .child(small(Msg::Allowance(bucket.remaining, bucket.limit).to_string()).text_color(color(Foreground))),
            )
            .child(meter(
                bucket.remaining as f32 / bucket.limit.max(1) as f32,
                if low { Role::Warn.ink() } else { Role::Ok.ink() },
            ))
            .child(small(Msg::Resets(time::hm(bucket.reset as f64 * 1000.0)).to_string()))
    };
    let threshold = context.attention.rate_limit_percent.get();
    view()
        .child(section(Msg::Limits))
        .child(
            view()
                .px(len(Space6))
                .child(source_bar(sources.rate, 0))
                .child(dynamic(move || {
                    let rate = sources.rate.get().map(|l| l.data);
                    let since = time::now_ms() - f64::from(hours) * 3_600_000.0;
                    let limited = sessions::all(sources)
                        .into_iter()
                        .filter(|(_, s)| time::parse_ms(&s.started_at).is_some_and(|at| at >= since))
                        .filter(|(_, s)| s.error.as_deref().is_some_and(|e| e.contains("429")))
                        .count();
                    untracked(move || {
                        view()
                            .flex_row()
                            .gap(len(Space6))
                            .pt(len(Space2))
                            .child(bucket_cell(Api::Rest, rate.map(|r| r.rest), threshold))
                            .child(bucket_cell(Api::GraphQl, rate.map(|r| r.graphql), threshold))
                            .child(
                                view()
                                    .flex_1()
                                    .gap(len(Space2))
                                    .child(
                                        view()
                                            .flex_row()
                                            .gap(len(Space2))
                                            .child(text(SharedString::from(Msg::Llm429)).ty(Type::BodyStrong).flex_1())
                                            .child(small(Msg::LimitedSessions { hours, sessions: limited }.to_string()).text_color(color(Foreground))),
                                    )
                                    .child(meter(0.0, color(Ink3)))
                                    .child(small(SharedString::from(Msg::LimitedNote))),
                            )
                            .into_element()
                    })
                })),
        )
}
