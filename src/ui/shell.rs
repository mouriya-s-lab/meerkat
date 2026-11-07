//! The shell (#9 「骨架与导航」): in windows at least 900 pt wide a 224 pt
//! sidebar (Quorum, the ⌘K search, the pages in their groups with what
//! needs you there, the programs and the GitHub API at its foot) beside
//! the page; narrower windows keep the pages as tabs in a top bar. Each
//! page opens under a toolbar with its title and, once, how fresh the data
//! is. ⌘K opens the command bar, ⌘1… the pages in the sidebar's order, ⌘R
//! refreshes.

use std::rc::Rc;

use nsh_base::{button, theme};
use nsh_core::{element::Tokens, keymap::When, prelude::*};

use super::{
    command::{self, Palette},
    config::{self, ConfigState},
    icons::{Icon, icon},
    issues::{self, IssuesState, Settings},
    overview,
    parts::{Look, action, hint, icon_button, kbd, meter, nothing, small},
    runtime::{self, Rewarms},
    sessions::{self, OpenSession, SessionsState},
    status::{self, Role},
    text::Msg,
    tokens::{Type, Typeset, color, font, len, mark},
};
use crate::{
    attention::{self, Concern, Inputs, Need, ProgramView},
    config::{
        AppConfig, Attention, ColorToken::*, ConfigError, Digest, FontToken, LengthToken::*, LoadedFile,
    },
    dag::{self, Council, Graph},
    gh::{Credential, GitHub, IssueRef},
    pmconfig::ConfigRepoClient,
    programs::{BaseState, Program, ProgramClient, QueueState, SessionState},
    sources::{ProgramSources, Sources},
    time,
};

/// The width from which the sidebar shows.
const WIDE: f32 = 900.0;

fn wide() -> bool {
    window().width >= px(WIDE)
}

/// A page's list and its inspector (#9 「骨架与导航」).
#[derive(Clone)]
pub(super) struct Split {
    pub list: Rc<dyn Fn() -> AnyElement>,
    pub inspector: Rc<dyn Fn() -> AnyElement>,
    /// Whether the person has opened something to inspect.
    pub open: Rc<dyn Fn() -> bool>,
    /// Closes what is open.
    pub back: Rc<dyn Fn()>,
}

/// Where the sidebar shows, `beside` lays out the list with its inspector;
/// narrower windows show the list alone until something is open, then the
/// inspector full width under a 返回 bar.
pub(super) fn split(parts: Split, beside: impl Fn(&Split) -> AnyElement + 'static) -> View {
    let narrow = parts.clone();
    view().flex_1().min_h(Points::ZERO).child(when(wide, move || beside(&parts)).otherwise(move || {
        let open = narrow.open.clone();
        let shown = narrow.clone();
        let list = narrow.list.clone();
        when(move || open(), move || detail(&shown)).otherwise(move || list())
    }))
}

/// What is open, full width, under a bar that goes back to the list.
fn detail(parts: &Split) -> View {
    let back = parts.back.clone();
    view()
        .flex_1()
        .min_h(Points::ZERO)
        .child(
            view()
                .flex_row()
                .items_center()
                .h(len(Toolbar))
                .px(len(Space4))
                .border_b_1()
                .border_b_color(color(Border))
                .child(action(Msg::Back, Some(Icon::ArrowLeft), Look::Link, || false, move || back())),
        )
        .child(view().flex_1().min_h(Points::ZERO).child((parts.inspector)()))
}

/// The inspector beside a list: its width, and a rule on its left.
pub(super) fn side(inspector: AnyElement) -> View {
    view()
        .w(len(InspectorWidth) + len(Space8))
        .flex_shrink_0()
        .border_l_1()
        .border_l_color(color(Border))
        .child(inspector)
}

/// A page of the app, in the sidebar's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Overview,
    Issues,
    Sessions,
    Runtime,
    Config,
}

/// The sidebar's groups under 「概览」.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    Council,
    Run,
}

impl Page {
    pub const ALL: [Page; 5] = [Page::Overview, Page::Issues, Page::Sessions, Page::Runtime, Page::Config];

    pub fn title(self) -> Msg {
        match self {
            Page::Overview => Msg::PageOverview,
            Page::Issues => Msg::PageIssues,
            Page::Sessions => Msg::PageSessions,
            Page::Runtime => Msg::PageRuntime,
            Page::Config => Msg::PageConfig,
        }
    }

    fn icon(self) -> Icon {
        match self {
            Page::Overview => Icon::Overview,
            Page::Issues => Icon::Graph,
            Page::Sessions => Icon::Sessions,
            Page::Runtime => Icon::Runtime,
            Page::Config => Icon::Config,
        }
    }

    fn group(self) -> Option<Group> {
        match self {
            Page::Overview => None,
            Page::Issues | Page::Sessions => Some(Group::Council),
            Page::Runtime | Page::Config => Some(Group::Run),
        }
    }
}

/// Which page shows, which session the sessions page has open, and whether
/// the command bar is open.
#[derive(Clone, Copy)]
pub struct Nav {
    pub page: Signal<Page>,
    pub session: Signal<Option<OpenSession>>,
    pub palette: Signal<bool>,
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
    /// 「需要你处理」's rows, in the consequence order.
    pub needs: Memo<Rc<Vec<Need>>>,
    /// Every agenda issue, unfiltered (the command bar searches it).
    pub all_issues: Memo<Option<Rc<Graph>>>,
    /// Each base's last rewarm, whichever page asked for it.
    pub rewarms: Rewarms,
    /// The 「议题」 page's settings and what the user set on it.
    pub issues_settings: Rc<Settings>,
    pub issues: IssuesState,
    pub sessions: SessionsState,
    /// The 「配置」 page's selection and unsaved text.
    pub config: ConfigState,
    /// The config repository's branch, its readers and saves.
    pub config_repo: Rc<ConfigRepoClient>,
    /// The config files this build loaded, with their blob shas.
    pub loaded: Rc<Vec<LoadedFile>>,
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

    /// The window `attention.sessionFailureHours` sets, in hours.
    pub fn hours(&self) -> u32 {
        self.attention.session_failure_hours.get()
    }

    /// Shows `issue` selected on the 「议题」 page.
    pub fn open_issue(&self, nav: Nav, issue: IssueRef) {
        batch(|| {
            self.issues.select(Some(issue));
            nav.page.set(Page::Issues);
        });
    }
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
    let credential = Credential::for_this_build();
    let api = config.endpoints.github_api.clone();
    let github = computed(move || {
        credential.with(|credential| {
            credential
                .clone()
                .map(|credential| Rc::new(GitHub::new(api.clone(), credential)))
        })
    });
    let config_repo = Rc::new(ConfigRepoClient::new(
        github,
        config.config_repo.clone(),
        config.endpoints.clone(),
    ));
    let sources = Sources::new(config, credential, config_repo.clone());
    set_interval(config.refresh, move || sources.refresh());
    let now = signal(time::now_ms());
    set_interval(std::time::Duration::from_secs(1), move || {
        now.set(time::now_ms())
    });
    let attention_settings = config.attention.clone();
    let needs = computed(move || {
        Rc::new(attention::needs(concerns_of(sources, &attention_settings)))
    });
    let settings = Rc::new(Settings {
        pm: config.repos.pm.repo.clone(),
        fields: config.fields.clone(),
    });
    let pm = settings.pm.clone();
    let all_issues = computed(move || graph_of(sources, &pm, false));
    let context = Rc::new(Context {
        council: ProgramClient::new(Program::Council, config.endpoints.council.clone()),
        guardians: ProgramClient::new(Program::Guardians, config.endpoints.guardians.clone()),
        attention: config.attention.clone(),
        digest: config.digest.clone(),
        needs,
        all_issues,
        rewarms: Rewarms::new(),
        issues: IssuesState::new(config.filters),
        issues_settings: settings,
        sessions: SessionsState::new(),
        config: ConfigState::new(),
        config_repo,
        loaded: Rc::new(config.loaded.clone()),
        now,
    });
    let nav = Nav {
        page: signal(Page::Overview),
        session: signal(None),
        palette: signal(false),
    };
    on_key("mod+k", move |_| nav.palette.update(|open| *open = !*open));
    on_key("mod+r", move |_| sources.refresh());
    for (i, page) in Page::ALL.into_iter().enumerate() {
        on_key(&format!("mod+{}", i + 1), move |_| {
            batch(|| {
                nav.palette.set(false);
                nav.page.set(page);
            })
        });
    }
    let palette = Palette::new();
    view()
        .flex_1()
        .bg(color(Background))
        .text_color(color(Foreground))
        .font_family(font(FontToken::Sans))
        .child(
            when(wide, {
                let context = context.clone();
                move || {
                    view()
                        .flex_1()
                        .flex_row()
                        .child(sidebar(sources, context.clone(), nav))
                        .child(main(sources, context.clone(), nav))
                }
            })
            .otherwise({
                let context = context.clone();
                move || {
                    view()
                        .flex_1()
                        .child(top_bar(sources, context.clone(), nav))
                        .child(main(sources, context.clone(), nav))
                }
            }),
        )
        .child(when(move || nav.palette.get(), move || {
            command::bar(sources, context.clone(), nav, palette)
        }))
}

/// The agenda issues, once the Project and the queue (or its failure) are
/// in; only human issues and their successors when `human_only`.
pub fn graph_of(sources: Sources, pm: &crate::config::RepoName, human_only: bool) -> Option<Rc<Graph>> {
    let unreachable = sources.queue.with_error(|e| e.is_some());
    sources.project.with(|project| {
        let items = &project?.data;
        sources.queue.with(|queue| {
            let council = match (unreachable, queue) {
                (true, _) => Council::Unreachable,
                (false, Some(queue)) => Council::Queue(&queue.data),
                // The queue's first answer is still on its way.
                (false, None) => return None,
            };
            Some(Rc::new(dag::build(items, council, pm, human_only)))
        })
    })
}

/// Every concern among the sources' latest values (tracked).
fn concerns_of(sources: Sources, settings: &Attention) -> Vec<Concern> {
    let data = |s: &crate::sources::Source<Vec<crate::programs::Session>>| s.get().map(|l| l.data);
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
            council: view(&sources.council, &council_error, council_at, &council_sessions),
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

/// The page under its toolbar.
fn main(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    view()
        .flex_1()
        .min_w(Points::ZERO)
        .min_h(Points::ZERO)
        .child(toolbar(sources, context.clone(), nav))
        .child(dynamic(move || {
            let context = context.clone();
            match nav.page.get() {
                Page::Overview => overview::page(sources, context, nav).into_element(),
                Page::Issues => issues::page(sources, context, nav).into_element(),
                Page::Sessions => sessions::page(sources, context, nav).into_element(),
                Page::Runtime => runtime::page(sources, context, nav).into_element(),
                Page::Config => config::page(sources, context, nav).into_element(),
            }
        }))
}

/// The page's title (and its window, for the sessions), and how fresh the
/// data is with the refresh button: the only place that says so.
fn toolbar(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let hours = context.hours();
    view()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space3))
        .h(len(Toolbar))
        .px(len(Space6))
        .border_b_1()
        .border_b_color(color(Border))
        .child(text(move || SharedString::from(nav.page.get().title())).ty(Type::Title))
        .child(dynamic(move || match nav.page.get() {
            Page::Sessions => small(SharedString::from(Msg::LastHours(hours))).into_element(),
            _ => nothing(),
        }))
        .child(view().flex_1())
        .child(small(move || {
            SharedString::from(Msg::Updated(time::ago(
                sources.last_refresh.get(),
                context.now.get(),
            )))
        }))
        .child(icon_button(Icon::Refresh, Msg::Refresh, move || sources.refresh()))
}

/// A page's count in the sidebar, and the role of its color: only what
/// needs you there (#9 「计数的含义」).
fn count(sources: Sources, context: &Context, page: Page) -> Option<(usize, Role)> {
    let plain = |n: usize| (n, Role::Neutral);
    match page {
        Page::Overview => {
            let needs = context.needs.get();
            let role = if needs.iter().any(Need::failure) { Role::Fail } else { Role::Neutral };
            (!needs.is_empty()).then(|| (needs.len(), role))
        }
        Page::Issues => sources.queue.with(|q| {
            q.map(|q| {
                plain(
                    q.data
                        .iter()
                        .filter(|e| !matches!(e.state, QueueState::Decided | QueueState::Superseded))
                        .count(),
                )
            })
        }),
        Page::Sessions => {
            let running = [sources.council.sessions, sources.guardians.sessions]
                .into_iter()
                .map(|s| {
                    s.with(|l| {
                        l.map_or(0, |l| {
                            l.data
                                .iter()
                                .filter(|s| s.state == SessionState::Running)
                                .count()
                        })
                    })
                })
                .sum();
            Some(plain(running))
        }
        Page::Runtime => sources.bases.with(|b| {
            let b = b?;
            let failed = b.data.iter().any(|b| b.state == BaseState::Failed);
            let n = b
                .data
                .iter()
                .filter(|b| b.state == BaseState::Failed || b.stale)
                .count();
            (n > 0).then_some((n, if failed { Role::Fail } else { Role::Neutral }))
        }),
        // Files a program runs an older version of than the branch has.
        Page::Config => {
            let n = config::older_files(sources, &context.loaded).len();
            (n > 0).then_some((n, Role::Warn))
        }
    }
}

/// A sidebar count's color: its role's words, grey when it has none.
fn count_color(role: Role) -> AdaptiveColor {
    match role {
        Role::Neutral => color(Ink3),
        role => role.text(),
    }
}

fn sidebar(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    let group_label = |group: Group| {
        small(SharedString::from(match group {
            Group::Council => Msg::GroupCouncil,
            Group::Run => Msg::GroupRun,
        }))
        .px(len(Space2))
        .pt(len(Space4))
        .pb(len(Space1))
    };
    let mut items: Vec<AnyElement> = Vec::new();
    let mut last = None;
    for page in Page::ALL {
        if page.group() != last {
            if let Some(group) = page.group() {
                items.push(group_label(group).into_element());
            }
            last = page.group();
        }
        items.push(nav_item(sources, context.clone(), nav, page).into_element());
    }
    view()
        .w(len(SidebarWidth))
        .flex_shrink_0()
        .px(len(Space3))
        .pt(len(Space3))
        .pb(len(Space3))
        .gap(len(Space1) / 2.0)
        .bg(color(Sidebar))
        .border_r_1()
        .border_r_color(color(Border))
        .child(brand())
        .child(search_button(nav).mt(len(Space2)).mb(len(Space3)))
        .children(items)
        .child(view().flex_1())
        .child(footer(sources, context))
}

/// The app's icon and name: once.
fn brand() -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Control))
        .px(len(Space2))
        .child(
            view()
                .size(len(Space5))
                .items_center()
                .justify_center()
                .rounded(len(RadiusControl))
                .bg(color(Foreground))
                .child(text("Q").ty(Type::Label).text_color(color(Background))),
        )
        .child(text("Quorum").ty(Type::Heading))
}

/// The search field that opens the command bar.
fn search_button(nav: Nav) -> nsh_base::ButtonElement {
    button()
        .on_press(move || nav.palette.set(true))
        .accessibility_label(SharedString::from(Msg::SearchHint))
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Control))
        .px(len(Space2))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Border))
        .bg(color(Background))
        .hover(|s| s.border_color(color(BorderStrong)))
        .cursor_pointer()
        .child(icon(Icon::Search, || color(Ink3)))
        .child(small(SharedString::from(Msg::SearchHint)).flex_1().number_of_lines(1))
        .child(kbd(Msg::KeyPalette))
}

fn nav_item(sources: Sources, context: Rc<Context>, nav: Nav, page: Page) -> impl IntoElement {
    let current = move || nav.page.get() == page;
    button()
        .on_press(move || nav.page.set(page))
        .accessibility_label(SharedString::from(page.title()))
        .selected(current)
        .role(Some(AccessibilityRole::Tab))
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Row) - len(Space1))
        .pl(len(Space2) - mark())
        .pr(len(Space2))
        .rounded(len(RadiusControl))
        .border_l(mark())
        .border_l_color(move || if current() { color(Selection) } else { super::parts::clear() })
        .bg(move || if current() { color(NavSelected) } else { super::parts::clear() })
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
        .child(icon(page.icon(), move || {
            if current() { color(Foreground) } else { color(Ink2) }
        }))
        .child(
            text(SharedString::from(page.title()))
                .ty(Type::Body)
                .flex_1()
                .text_color(move || if current() { color(Foreground) } else { color(Ink2) }),
        )
        .child(dynamic(move || match count(sources, &context, page) {
            Some((n, role)) => text(n.to_string())
                .ty(Type::Label)
                .text_color(count_color(role))
                .into_element(),
            None => nothing(),
        }))
}

/// The programs' state and versions, the GitHub API's allowance and when
/// it resets: the sidebar's foot.
fn footer(sources: Sources, context: Rc<Context>) -> View {
    view()
        .gap(len(Space2))
        .pt(len(Space3))
        .px(len(Space2))
        .border_t_1()
        .border_t_color(color(Border))
        .child(program_line(sources.council))
        .child(program_line(sources.guardians))
        .child(rate_block(sources, context))
}

/// A program in the sidebar's foot: its state and name, always shown, and
/// its short version on one line, cut when it does not fit; the full
/// version is its tooltip.
fn program_line(program: ProgramSources) -> View {
    let health = program.health;
    let name = program.program;
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .child(dynamic(move || {
            let failed = health.with_error(|e| e.is_some());
            let loaded = health.with(|l| l.is_some());
            match (failed, loaded) {
                (false, false) => small(SharedString::from(Msg::Checking)).into_element(),
                _ => {
                    let state = status::program(!failed);
                    status::glyph(state.glyph, state.role).into_element()
                }
            }
        }))
        .child(text(SharedString::from(Msg::Program(name))).ty(Type::Small).flex_shrink_0())
        .child(view().flex_1().min_w(Points::ZERO).items_end().child(dynamic(move || {
            match health.with(|l| l.map(|l| (l.data.short_version().to_owned(), l.data.version.clone()))) {
                Some((short, full)) => small(short).truncate().tooltip(hint(full)).into_element(),
                None => nothing(),
            }
        })))
}

fn rate_block(sources: Sources, context: Rc<Context>) -> View {
    view().gap(len(Space1)).child(dynamic(move || {
        let threshold = context.attention.rate_limit_percent.get();
        let lowest = sources.rate.with(|rate| {
            rate.and_then(|loaded| {
                loaded
                    .data
                    .buckets()
                    .into_iter()
                    .min_by_key(|(_, b)| b.remaining * 1000 / b.limit.max(1))
                    .map(|(_, b)| b)
            })
        });
        untracked(move || {
            let Some(bucket) = lowest else {
                return view()
                    .flex_row()
                    .child(text("GitHub API").ty(Type::Small).flex_1())
                    .child(small(SharedString::from(Msg::Unknown)))
                    .into_element();
            };
            let low = bucket.below(threshold);
            let share = bucket.remaining as f32 / bucket.limit.max(1) as f32;
            view()
                .gap(len(Space1))
                .child(
                    view()
                        .flex_row()
                        .items_center()
                        .gap(len(Space2))
                        .child(text("GitHub API").ty(Type::Small).flex_1())
                        .children(low.then(|| status::glyph(status::Glyph::Triangle, Role::Warn)))
                        .child(small(Msg::Allowance(bucket.remaining, bucket.limit).to_string())),
                )
                .child(meter(share, if low { Role::Warn.ink() } else { Role::Ok.ink() }))
                .child(small(
                    Msg::Resets(time::hm(bucket.reset as f64 * 1000.0)).to_string(),
                ))
                .into_element()
        })
    }))
}

/// The pages as tabs and the freshness: the top of windows without the
/// sidebar.
fn top_bar(sources: Sources, context: Rc<Context>, nav: Nav) -> View {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space2))
        .px(len(Space4))
        .py(len(Space2))
        .bg(color(Sidebar))
        .border_b_1()
        .border_b_color(color(Border))
        .child(text("Quorum").ty(Type::Heading))
        .child(
            view()
                .flex_row()
                .role(AccessibilityRole::Tablist)
                .gap(len(Space1))
                .children(Page::ALL.map(|page| {
                    let context = context.clone();
                    let current = move || nav.page.get() == page;
                    button()
                        .on_press(move || nav.page.set(page))
                        .accessibility_label(SharedString::from(page.title()))
                        .selected(current)
                        .role(Some(AccessibilityRole::Tab))
                        .flex_row()
                        .items_center()
                        .gap(len(Space1))
                        .h(len(Control))
                        .px(len(Space2))
                        .rounded(len(RadiusControl))
                        .bg(move || if current() { color(NavSelected) } else { super::parts::clear() })
                        .cursor_pointer()
                        .child(
                            text(SharedString::from(page.title()))
                                .ty(Type::Label)
                                .text_color(move || if current() { color(Foreground) } else { color(Ink2) }),
                        )
                        .child(dynamic(move || match count(sources, &context, page) {
                            Some((n, role)) => text(n.to_string())
                                .ty(Type::Small)
                                .text_color(count_color(role))
                                .into_element(),
                            None => nothing(),
                        }))
                })),
        )
        .child(view().flex_1())
        .child(icon_button(Icon::Search, Msg::SearchHint, move || nav.palette.set(true)))
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
                    text(SharedString::from(Msg::ConfigError))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(move || theme().destructive),
                )
                .child(text(error.to_string()).text_sm().selectable(true))
                .child(
                    text(SharedString::from(Msg::ConfigHint))
                        .text_xs()
                        .text_color(move || theme().muted_foreground),
                ),
        )
        .into_element()
}

pub(super) fn keys_when_typing() -> KeyOptions {
    KeyOptions {
        when: When::Always,
        repeat: true,
    }
}
