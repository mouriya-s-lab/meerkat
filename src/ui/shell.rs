//! The shell: a top bar (last refresh, manual refresh, failing sources,
//! API remaining) over the page; in windows at least 900 pt wide a sidebar
//! of the pages beside it, else one column. This issue has one page, the
//! overview.

use std::rc::Rc;

use nsh_base::theme;
use nsh_core::prelude::*;

use super::{
    overview::{Context, overview},
    parts::{action, note},
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
    });
    view()
        .flex_1()
        .child(
            when(wide, {
                let context = context.clone();
                move || {
                    view()
                        .flex_1()
                        .flex_row()
                        .child(sidebar())
                        .child(page(sources, context.clone(), false))
                }
            })
            .otherwise(move || page(sources, context.clone(), true)),
        )
        .into_element()
}

/// The top bar and the overview under it.
fn page(sources: Sources, context: Rc<Context>, narrow: bool) -> View {
    view()
        .flex_1()
        .min_w(px(0.0))
        .child(top_bar(sources, narrow))
        .child(scroll().flex_1().child(overview(sources, context)))
}

fn sidebar() -> View {
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
        .child(
            view()
                .h(px(32.0))
                .px_2()
                .justify_center()
                .rounded(px(6.0))
                .bg(move || theme().sidebar_accent)
                .role(AccessibilityRole::Button)
                .accessibility_state(AccessibilityState {
                    selected: true,
                    ..AccessibilityState::default()
                })
                .child(
                    text("概览")
                        .text_sm()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(move || theme().sidebar_accent_foreground),
                ),
        )
}

fn top_bar(sources: Sources, narrow: bool) -> View {
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
        .child(
            text(if narrow { "PM 仪表盘 · 概览" } else { "概览" })
                .font_weight(FontWeight::SEMIBOLD)
                .number_of_lines(1)
                .flex_shrink_0(),
        )
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
