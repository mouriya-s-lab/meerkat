//! The shell: in windows at least 900 pt wide a sidebar (the pages with
//! their counts; the programs' and the GitHub API's state and the refresh
//! button at its foot) beside the page; narrower windows keep the pages as
//! tabs in a top bar. Every page opens with its title and a sentence that
//! sums it up. The pages: the overview, 「议题」, the guardians' bases and
//! the sessions.

use std::rc::Rc;

use nsh_base::{button, theme};
use nsh_core::{element::Tokens, prelude::*};

use super::{
    guardians::{self, Rewarms, guardians},
    icons::{Icon, icon},
    issues::{self, IssuesState, Settings, issues},
    overview::{self, overview},
    parts::{Look, Span, meta, nothing, press, refresh_button, sentence},
    sessions::{self, OpenSession, sessions},
    status::{self, Role},
    tokens::{Type, Typeset, color, len},
};
use crate::{
    attention::{self, Concern, Inputs, ProgramView},
    config::{AppConfig, Attention, ColorToken::*, ConfigError, Digest, LengthToken::*},
    gh::Credential,
    programs::{BaseState, Program, ProgramClient},
    sources::{ProgramSources, Source, Sources},
    time,
};

/// The width from which the sidebar shows.
const WIDE: f32 = 900.0;

fn wide() -> bool {
    window().width >= px(WIDE)
}

/// A page of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Issues,
    Guardians,
    Sessions,
}

impl Page {
    /// The pages, in the sidebar's order.
    const ALL: [Page; 4] = [
        Page::Overview,
        Page::Issues,
        Page::Guardians,
        Page::Sessions,
    ];

    fn label(self) -> &'static str {
        match self {
            Page::Overview => "概览",
            Page::Issues => "议题",
            Page::Guardians => "守护基座",
            Page::Sessions => "会话",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Overview => Icon::Overview,
            Page::Issues => Icon::Issues,
            Page::Guardians => Icon::Bases,
            Page::Sessions => Icon::Sessions,
        }
    }
}

/// Which page shows, and which session the sessions page has open (a
/// page may open one on another: a fork in the guardians page, a failed
/// session in the overview).
#[derive(Clone, Copy)]
pub struct Nav {
    pub page: Signal<Page>,
    pub session: Signal<Option<OpenSession>>,
}

impl Nav {
    /// Shows `session` on the sessions page.
    pub fn open_session(&self, session: OpenSession) {
        batch(|| {
            self.session.set(Some(session));
            self.page.set(Page::Sessions);
        });
    }
}

/// What the pages need besides their sources.
pub struct Context {
    pub council: ProgramClient,
    pub guardians: ProgramClient,
    pub attention: Attention,
    pub digest: Digest,
    pub refresh_seconds: u64,
    /// What needs a person, in the queue's order.
    pub concerns: Memo<Rc<Vec<Concern>>>,
    /// Each base's last rewarm, whichever page asked for it.
    pub rewarms: Rewarms,
    /// Now, to the second, for the "… ago" texts.
    pub now: Signal<f64>,
}

impl Context {
    pub fn program(&self, program: Program) -> &ProgramClient {
        match program {
            Program::Council => &self.council,
            Program::Guardians => &self.guardians,
        }
    }
}

/// The 「议题」 page's settings, and what the user set on it, which outlives
/// a visit to the page.
#[derive(Clone)]
struct Issues {
    settings: Rc<Settings>,
    state: IssuesState,
}

/// The app for `config`, once NSH draws with `tokens` (the theme's, set
/// by the root): until then nothing shows, as every page reads them.
pub fn app(config: AppConfig, tokens: Tokens) -> AnyElement {
    let ready = move || *nsh_core::element::tokens() == tokens;
    let config = Rc::new(config);
    view()
        .flex_1()
        .child(when(ready, move || shell(&config)))
        .into_element()
}

/// The app's sources start now and refresh every `config.refresh`.
fn shell(config: &AppConfig) -> View {
    let sources = Sources::new(config, Credential::for_this_build());
    set_interval(config.refresh, move || sources.refresh());
    let now = signal(time::now_ms());
    set_interval(std::time::Duration::from_secs(1), move || {
        now.set(time::now_ms())
    });
    let attention_settings = config.attention.clone();
    let concerns = computed(move || {
        Rc::new(attention::ordered(concerns_of(
            sources,
            &attention_settings,
        )))
    });
    let context = Rc::new(Context {
        council: ProgramClient::new(Program::Council, config.endpoints.council.clone()),
        guardians: ProgramClient::new(Program::Guardians, config.endpoints.guardians.clone()),
        attention: config.attention.clone(),
        digest: config.digest.clone(),
        refresh_seconds: config.refresh.as_secs(),
        concerns,
        rewarms: Rewarms::new(),
        now,
    });
    let settings = Settings {
        list: config.list.clone(),
        pm: config.repos.pm.repo.clone(),
        fields: config.fields.clone(),
    };
    let issues = Issues {
        state: IssuesState::new(&settings, config.filters),
        settings: Rc::new(settings),
    };
    let nav = Nav {
        page: signal(Page::Overview),
        session: signal(None),
    };
    view()
        .flex_1()
        .bg(color(Background))
        .text_color(color(Foreground))
        .font_family(super::tokens::font(crate::config::FontToken::Sans))
        .child(
            when(wide, {
                let context = context.clone();
                let issues = issues.clone();
                move || {
                    view()
                        .flex_1()
                        .flex_row()
                        .child(sidebar(sources, context.clone(), nav))
                        .child(page(sources, context.clone(), issues.clone(), nav))
                }
            })
            .otherwise(move || {
                view()
                    .flex_1()
                    .child(top_bar(sources, context.clone(), nav))
                    .child(page(sources, context.clone(), issues.clone(), nav))
            }),
        )
}

fn concerns_of(sources: Sources, settings: &Attention) -> Vec<Concern> {
    let data = |s: &Source<Vec<crate::programs::Session>>| s.get().map(|l| l.data);
    let bases = sources.bases.get().map(|l| l.data);
    let queue = sources.queue.get().map(|l| l.data);
    let escalations = sources.escalations.get().map(|l| l.data);
    let rate = sources.rate.get().map(|l| l.data);
    let council_sessions = data(&sources.council.sessions);
    let guardians_sessions = data(&sources.guardians.sessions);
    let council_error = sources.council.health.error();
    let guardians_error = sources.guardians.health.error();
    let council_at = sources.council.health.with(|l| l.map(|l| l.at));
    let guardians_at = sources.guardians.health.with(|l| l.map(|l| l.at));
    fn view<'a>(
        program: &ProgramSources,
        error: &'a Option<String>,
        at: Option<f64>,
        sessions: &'a Option<Vec<crate::programs::Session>>,
    ) -> ProgramView<'a> {
        ProgramView {
            program: program.program,
            unreachable: error.as_deref().map(|error| (error, at)),
            sessions: sessions.as_deref(),
        }
    }
    attention::concerns(
        Inputs {
            bases: bases.as_deref(),
            queue: queue.as_deref(),
            escalations: escalations.as_deref(),
            rate: rate.as_ref(),
            council: view(
                &sources.council,
                &council_error,
                council_at,
                &council_sessions,
            ),
            guardians: view(
                &sources.guardians,
                &guardians_error,
                guardians_at,
                &guardians_sessions,
            ),
            now_ms: time::now_ms(),
        },
        settings,
    )
}

/// The page's title and summary over its content.
fn page(sources: Sources, context: Rc<Context>, issues_page: Issues, nav: Nav) -> View {
    let header = {
        let context = context.clone();
        let issues_page = issues_page.clone();
        dynamic(move || {
            let current = nav.page.get();
            let spans = match current {
                Page::Overview => overview::summary(sources, &context),
                Page::Issues => issues::summary(sources, issues_page.state),
                Page::Guardians => guardians::summary(sources),
                Page::Sessions => sessions::summary(sources, &context),
            };
            untracked(move || {
                view()
                    .gap(len(Space1))
                    .flex_1()
                    .min_w(Points::ZERO)
                    .child(
                        text(current.label())
                            .ty(Type::Title)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(sentence(spans))
                    .into_element()
            })
        })
    };
    let refreshed = {
        let context = context.clone();
        meta(move || {
            format!(
                "{}刷新",
                time::ago(sources.last_refresh.get(), context.now.get())
            )
        })
    };
    view()
        .flex_1()
        .min_w(Points::ZERO)
        .min_h(Points::ZERO)
        .child(
            view()
                .flex_row()
                .items_start()
                .gap(len(Space4))
                .px(len(Space6))
                .pt(len(Space5))
                .child(header)
                .child(refreshed),
        )
        .child(dynamic(move || {
            let context = context.clone();
            match nav.page.get() {
                Page::Overview => scroll()
                    .flex_1()
                    .child(overview(sources, context, nav))
                    .into_element(),
                Page::Issues => {
                    issues(sources, issues_page.settings.clone(), issues_page.state).into_element()
                }
                Page::Guardians => scroll()
                    .flex_1()
                    .child(guardians(sources, context, nav))
                    .into_element(),
                Page::Sessions => sessions(sources, context, nav).into_element(),
            }
        }))
}

/// A page's count in the sidebar, red when something there failed.
fn count(sources: Sources, context: &Context, page: Page) -> Option<(String, bool)> {
    match page {
        Page::Overview => {
            let concerns = context.concerns.get();
            (!concerns.is_empty()).then(|| {
                (
                    concerns.len().to_string(),
                    concerns.iter().any(Concern::failure),
                )
            })
        }
        Page::Issues => sources
            .queue
            .with(|q| q.map(|q| (q.data.len().to_string(), false))),
        Page::Guardians => sources.bases.with(|b| {
            b.map(|b| {
                let ready = b
                    .data
                    .iter()
                    .filter(|b| b.state == BaseState::Ready)
                    .count();
                let failed = b.data.iter().any(|b| b.state == BaseState::Failed);
                (format!("{ready}/{}", b.data.len()), failed)
            })
        }),
        Page::Sessions => None,
    }
}

fn sidebar(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    view()
        .w(len(SidebarWidth))
        .flex_shrink_0()
        .px(len(Space3))
        .py(len(Space4))
        .bg(color(Card))
        .border_r_1()
        .border_r_color(color(Border))
        .child(
            view()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .px(len(Space2))
                .pb(len(Space4))
                .child(
                    view()
                        .size(len(Space5))
                        .items_center()
                        .justify_center()
                        .rounded(len(RadiusControl))
                        .bg(color(Foreground))
                        .child(
                            text("Q")
                                .ty(Type::Dense)
                                .font_weight(FontWeight::BOLD)
                                .text_color(color(Card)),
                        ),
                )
                .child(
                    text("Quorum")
                        .ty(Type::Heading)
                        .font_weight(FontWeight::SEMIBOLD),
                ),
        )
        .children(Page::ALL.map(|page| {
            let context = context.clone();
            let current = move || nav.page.get() == page;
            button()
                .on_press(move || nav.page.set(page))
                .accessibility_label(page.label())
                .selected(current)
                .role(Some(AccessibilityRole::Tab))
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .h(len(Space6))
                .px(len(Space2))
                .rounded(len(RadiusControl))
                .bg(move || {
                    if current() {
                        color(Accent)
                    } else {
                        color(Card)
                    }
                })
                .hover(|s| s.bg(color(Accent)))
                .cursor_pointer()
                .child(icon(page.icon(), move || {
                    if current() {
                        color(Foreground)
                    } else {
                        color(Ink2)
                    }
                }))
                .child(
                    text(page.label())
                        .ty(Type::Body)
                        .flex_1()
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
                .child(dynamic(move || match count(sources, &context, page) {
                    Some((n, alert)) => text(n)
                        .ty(Type::Meta)
                        .tabular_nums()
                        .font_weight(if alert {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(if alert {
                            Role::Critical.text()
                        } else {
                            color(MutedForeground)
                        })
                        .into_element(),
                    None => nothing(),
                }))
        }))
        .child(view().flex_1())
        .child(health_footer(sources, context))
}

/// The programs' health, the GitHub API's allowance and the refresh
/// button: the sidebar's foot.
fn health_footer(sources: Sources, context: Rc<Context>) -> View {
    view()
        .gap(len(Space2))
        .pt(len(Space3))
        .px(len(Space2))
        .border_t_1()
        .border_t_color(color(Border))
        .child(program_line(sources.council, context.clone()))
        .child(program_line(sources.guardians, context.clone()))
        .child(rate_line(sources, context.clone()))
        .child(refresh_button(
            format!("刷新 · 每 {} 秒", context.refresh_seconds),
            move || sources.refresh(),
        ))
}

fn foot_row(label: &'static str) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .child(text(label).ty(Type::Meta).text_color(color(Ink2)))
}

fn program_line(program: ProgramSources, context: Rc<Context>) -> View {
    let health = program.health;
    foot_row(program.program.label()).child(dynamic(move || {
        let failed = health.with_error(|e| e.is_some());
        let at = health.with(|l| l.map(|l| l.at));
        let now = context.now.get();
        untracked(move || {
            let state = match (failed, at) {
                (false, None) => return meta("检查中…").into_element(),
                (failed, _) => status::program(!failed),
            };
            view()
                .flex_1()
                .flex_row()
                .items_center()
                .child(status::label(&state, Type::Meta))
                .child(view().flex_1())
                .child(meta(
                    at.map(|at| time::ago(at, now))
                        .unwrap_or_else(|| "从未成功".to_owned()),
                ))
                .into_element()
        })
    }))
}

fn rate_line(sources: Sources, context: Rc<Context>) -> View {
    foot_row("GitHub API").child(dynamic(move || {
        let threshold = context.attention.rate_limit_percent.get();
        let lowest = sources.rate.with(|rate| {
            rate.and_then(|loaded| {
                loaded
                    .data
                    .buckets()
                    .into_iter()
                    .min_by_key(|(_, b)| b.remaining * 1000 / b.limit.max(1))
                    .map(|(_, b)| (b.below(threshold), format!("{}/{}", b.remaining, b.limit)))
            })
        });
        untracked(move || match lowest {
            Some((low, left)) => view()
                .flex_1()
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .when(low, |row| {
                    row.child(status::label(&status::rate_low(), Type::Meta))
                })
                .when(!low, |row| row.child(meta("余量")))
                .child(view().flex_1())
                .child(meta(left).tabular_nums())
                .into_element(),
            None => meta("未知").into_element(),
        })
    }))
}

/// The pages as tabs, the API's allowance and the refresh button: the top
/// of windows without the sidebar.
fn top_bar(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space2))
        .px(len(Space4))
        .py(len(Space2))
        .bg(color(Card))
        .border_b_1()
        .border_b_color(color(Border))
        .child(
            text("Quorum")
                .ty(Type::Heading)
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(
            view()
                .flex_row()
                .role(AccessibilityRole::Tablist)
                .gap(len(Space1))
                .children(Page::ALL.map(|page| {
                    let current = move || nav.page.get() == page;
                    button()
                        .on_press(move || nav.page.set(page))
                        .accessibility_label(page.label())
                        .selected(current)
                        .role(Some(AccessibilityRole::Tab))
                        .px(len(Space2))
                        .py(len(Space1))
                        .rounded(len(RadiusControl))
                        .bg(move || {
                            if current() {
                                color(Accent)
                            } else {
                                color(Card)
                            }
                        })
                        .cursor_pointer()
                        .child(
                            text(page.label())
                                .ty(Type::Dense)
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(move || {
                                    if current() {
                                        color(Foreground)
                                    } else {
                                        color(Ink2)
                                    }
                                }),
                        )
                })),
        )
        .child(view().flex_1())
        .child(program_line(sources.council, context.clone()).min_w(len(SidebarWidth)))
        .child(press(
            format!("刷新 · 每 {} 秒", context.refresh_seconds),
            Look::Plain,
            true,
            move || sources.refresh(),
        ))
}

/// The config could not be read: which file, which key and why. Drawn
/// before the theme is known, with NSH's own theme.
pub fn config_error(error: &ConfigError) -> AnyElement {
    log::error!("config: {error}");
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p_6()
        .child(
            view()
                .max_w(px(WIDE * 0.7))
                .gap_3()
                .p_6()
                .rounded(move || theme().radius)
                .border_1()
                .border_color(move || theme().destructive)
                .bg(move || theme().card)
                .child(
                    text("配置错误")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().destructive),
                )
                .child(text(error.to_string()).text_sm().selectable(true))
                .child(
                    text(
                        "PM_CONFIG_DIR 应指向 PM 配置目录（含 dashboard/dashboard.yaml、dashboard/theme.json、shared/repos.yaml、shared/project.yaml）；修正后重新启动。",
                    )
                    .text_xs()
                    .text_color(move || theme().muted_foreground),
                ),
        )
        .into_element()
}

/// Spans of a summary sentence.
pub(super) fn plain(text: impl Into<String>) -> Span {
    Span::Plain(text.into())
}

pub(super) fn strong(text: impl ToString) -> Span {
    Span::Strong(text.to_string())
}

pub(super) fn critical(text: impl Into<String>) -> Span {
    Span::Critical(text.into())
}
