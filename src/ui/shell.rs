//! The shell: a top bar (the page, last refresh, manual refresh, failing
//! sources, API remaining) over the page; in windows at least 900 pt wide
//! a sidebar of the pages beside it, else one column with the pages in the
//! top bar. The pages: the overview, 「议题」, the guardians' bases and the
//! sessions.

use std::rc::Rc;

use nsh_base::{button, theme};
use nsh_core::prelude::*;

use super::{
    guardians::guardians,
    issues::{IssuesState, Settings, issues},
    overview::{Context, overview},
    parts::{action, note},
    sessions::{OpenSession, sessions},
    theme::FAILED,
};
use crate::{
    attention::api_label,
    config::{AppConfig, ConfigError},
    gh::Credential,
    programs::{Program, ProgramClient},
    sources::Sources,
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
    const ALL: [Page; 4] = [Page::Overview, Page::Issues, Page::Guardians, Page::Sessions];

    fn label(self) -> &'static str {
        match self {
            Page::Overview => "概览",
            Page::Issues => "议题",
            Page::Guardians => "守护",
            Page::Sessions => "会话",
        }
    }
}

/// Which page shows, and which session the sessions page has open (a
/// page may open one on another: a fork in the guardians page).
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

/// The 「议题」 page's settings, and what the user set on it, which outlives
/// a visit to the page.
#[derive(Clone)]
struct Issues {
    settings: Rc<Settings>,
    state: IssuesState,
}

/// The app for `config`: its sources start now and refresh every
/// `config.refresh`.
pub fn shell(config: AppConfig) -> AnyElement {
    let sources = Sources::new(&config, Credential::for_this_build());
    set_interval(config.refresh, move || sources.refresh());
    let context = Rc::new(Context {
        council: ProgramClient::new(Program::Council, config.endpoints.council.clone()),
        guardians: ProgramClient::new(Program::Guardians, config.endpoints.guardians.clone()),
        attention: config.attention.clone(),
        fields: config.fields.clone(),
        digest: config.digest.clone(),
    });
    let settings = Settings {
        dag: config.dag.clone(),
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
        .child(
            when(wide, {
                let context = context.clone();
                let issues = issues.clone();
                move || {
                    view()
                        .flex_1()
                        .flex_row()
                        .child(sidebar(nav))
                        .child(page(sources, context.clone(), issues.clone(), nav, false))
                }
            })
            .otherwise(move || page(sources, context.clone(), issues.clone(), nav, true)),
        )
        .into_element()
}

/// The top bar and the current page under it.
fn page(sources: Sources, context: Rc<Context>, issues_page: Issues, nav: Nav, narrow: bool) -> View {
    view()
        .flex_1()
        .min_w(px(0.0))
        .child(top_bar(sources, nav, narrow))
        .child(dynamic(move || {
            let context = context.clone();
            match nav.page.get() {
                Page::Overview => scroll()
                    .flex_1()
                    .child(overview(sources, context))
                    .into_element(),
                Page::Issues => issues(
                    sources,
                    issues_page.settings.clone(),
                    issues_page.state,
                    wide,
                )
                .into_element(),
                Page::Guardians => scroll()
                    .flex_1()
                    .child(guardians(sources, context, nav))
                    .into_element(),
                Page::Sessions => sessions(sources, context, nav).into_element(),
            }
        }))
}

fn sidebar(nav: Nav) -> View {
    view()
        .w(px(200.0))
        .gap_1()
        .px_3()
        .py_4()
        .bg(move || theme().sidebar)
        .border_r_1()
        .border_r_color(move || theme().sidebar_border)
        .child(
            text("PM 仪表盘")
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(move || theme().sidebar_foreground)
                .px_2()
                .pb_3(),
        )
        .children(Page::ALL.map(|page| {
            let current = move || nav.page.get() == page;
            button()
                .on_press(move || nav.page.set(page))
                .selected(current)
                .h(px(32.0))
                .px_2()
                .justify_center()
                .rounded(px(6.0))
                .bg(move || {
                    if current() {
                        theme().sidebar_accent
                    } else {
                        transparent_black().into()
                    }
                })
                .hover(|s| s.bg(move || theme().sidebar_accent))
                .cursor_pointer()
                .child(
                    text(page.label())
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || {
                            if current() {
                                theme().sidebar_accent_foreground
                            } else {
                                theme().sidebar_foreground
                            }
                        }),
                )
        }))
}

/// The pages as a row, for windows without the sidebar.
fn page_tabs(nav: Nav) -> View {
    view().flex_row().gap_1().children(Page::ALL.map(|page| {
        let current = move || nav.page.get() == page;
        button()
            .on_press(move || nav.page.set(page))
            .selected(current)
            .h(px(26.0))
            .px_2()
            .justify_center()
            .rounded(px(6.0))
            .bg(move || {
                if current() {
                    theme().accent
                } else {
                    transparent_black().into()
                }
            })
            .cursor_pointer()
            .child(
                text(page.label())
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(move || theme().foreground),
            )
    }))
}

fn top_bar(sources: Sources, nav: Nav, narrow: bool) -> View {
    let failing = move || sources.failing();
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap_x(px(16.0))
        .gap_y(px(4.0))
        .px_4()
        .py_2()
        .border_b_1()
        .border_b_color(move || theme().border)
        .child(if narrow {
            view()
                .flex_row()
                .items_center()
                .gap_2()
                .child(text("PM 仪表盘").font_weight(FontWeight::SEMIBOLD))
                .child(page_tabs(nav))
                .into_element()
        } else {
            text(move || nav.page.get().label())
                .font_weight(FontWeight::SEMIBOLD)
                .number_of_lines(1)
                .flex_shrink_0()
                .into_element()
        })
        .child(view().flex_1())
        .child(note(move || {
            format!("上次刷新 {}", time::clock(sources.last_refresh.get()))
        }))
        .child(
            text(move || format!("失败来源 {}/{}", failing(), Sources::COUNT))
                .text_xs()
                .text_color(move || {
                    if failing() > 0 {
                        FAILED
                    } else {
                        theme().muted_foreground
                    }
                }),
        )
        .child(note(move || {
            sources
                .rate
                .with(|rate| {
                    rate.map(|loaded| {
                        loaded
                            .data
                            .buckets()
                            .map(|(api, bucket)| {
                                format!("{} {}/{}", api_label(api), bucket.remaining, bucket.limit)
                            })
                            .join(" · ")
                    })
                })
                .map(|left| format!("API 余量 {left}"))
                .unwrap_or_else(|| "API 余量 未知".to_owned())
        }))
        .child(action("刷新", move || sources.refresh()))
}

/// The config could not be read: which file, which key and why.
pub fn config_error(error: &ConfigError) -> AnyElement {
    log::error!("config: {error}");
    view()
        .flex_1()
        .items_center()
        .justify_center()
        .p_6()
        .child(
            view()
                .max_w(px(640.0))
                .gap_3()
                .p_6()
                .rounded(move || theme().radius)
                .border_1()
                .border_color(FAILED)
                .bg(move || theme().card)
                .child(
                    text("配置错误")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(FAILED),
                )
                .child(text(error.to_string()).text_sm().selectable(true))
                .child(note(
                    "PM_CONFIG_DIR 应指向 PM 配置目录（含 dashboard/dashboard.yaml、shared/repos.yaml、shared/project.yaml）；修正后重新启动。",
                )),
        )
        .into_element()
}
