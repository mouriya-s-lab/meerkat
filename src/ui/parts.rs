//! The dashboard's own controls, styled once: cards, badges, count rows,
//! a source's status line, the refresh button and link rows.

use nsh_base::{button, link, theme};
use nsh_core::prelude::*;

use super::theme::FAILED;
use crate::{sources::Source, time};

/// A titled card: `status` lines under the title, then `body`.
pub fn card(title: &'static str, status: Vec<AnyElement>, body: AnyElement) -> View {
    view()
        .gap_2()
        .p_4()
        .rounded(move || theme().radius)
        .border_1()
        .border_color(move || theme().border)
        .bg(move || theme().card)
        .text_color(move || theme().card_foreground)
        .child(
            text(title)
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD),
        )
        .children(status)
        .child(body)
}

/// A small label tinted with `color`.
pub fn badge(label: impl IntoReactive<SharedString>, color: AdaptiveColor) -> View {
    view()
        .flex_shrink_0()
        .px_2()
        .py(px(1.0))
        .rounded(px(4.0))
        .bg(color.opacity(0.14))
        .child(
            text(label)
                .text_xs()
                .font_weight(FontWeight::MEDIUM)
                .text_color(color)
                .number_of_lines(1),
        )
}

/// A dot of `color`, a label and a count.
pub fn count_row(color: AdaptiveColor, label: impl Into<SharedString>, count: usize) -> View {
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .child(view().size(px(8.0)).rounded(px(4.0)).bg(color))
        .child(text(label.into()).text_sm().flex_1())
        .child(
            text(count.to_string())
                .text_sm()
                .tabular_nums()
                .font_weight(FontWeight::SEMIBOLD),
        )
}

/// Muted secondary text.
pub fn note(content: impl IntoReactive<SharedString>) -> Text {
    text(content)
        .text_xs()
        .text_color(move || theme().muted_foreground)
}

/// Where `source` is: loading, its error with when it last loaded, or
/// when its data was loaded.
pub fn source_status<T: 'static>(label: &'static str, source: Source<T>) -> AnyElement {
    let failed = move || source.with_error(|error| error.is_some());
    text(move || {
        let at = source.with(|loaded| loaded.map(|loaded| loaded.at));
        let last = || match at {
            Some(at) => format!("上次成功 {}", time::clock(at)),
            None => "从未成功".to_owned(),
        };
        match (source.error(), source.state()) {
            (Some(error), _) => format!("{label}：错误 {error}（{}）", last()),
            (None, ResourceState::Unresolved) => format!("{label}：等待前置数据"),
            (None, _) if at.is_none() => format!("{label}：加载中…"),
            (None, state) => format!(
                "{label}：{}更新于 {}",
                if state.loading() { "刷新中 · " } else { "" },
                last().trim_start_matches("上次成功 ")
            ),
        }
    })
    .text_xs()
    .selectable(true)
    .text_color(move || {
        if failed() {
            FAILED
        } else {
            theme().muted_foreground
        }
    })
    .into_element()
}

/// The app's button.
pub fn action(label: &'static str, on_press: impl Fn() + 'static) -> impl IntoElement {
    button()
        .on_press(on_press)
        .h(px(28.0))
        .px_3()
        .items_center()
        .justify_center()
        .rounded(move || theme().radius)
        .bg(move || theme().primary)
        .active(|s| s.opacity(0.8))
        .hover(|s| s.opacity(0.9))
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .cursor_pointer()
        .child(
            text(label)
                .text_sm()
                .text_color(move || theme().primary_foreground),
        )
}

/// A check box with its label, as a button screen readers hear as
/// selected while `checked`; a press asks for the other value.
pub fn toggle(
    label: &'static str,
    checked: impl Fn() -> bool + Copy + 'static,
    on_change: impl Fn(bool) + 'static,
) -> impl IntoElement {
    button()
        .on_press(move || on_change(!checked()))
        .selected(checked)
        .flex_row()
        .items_center()
        .gap_2()
        .h(px(28.0))
        .px_1()
        .rounded(px(4.0))
        .hover(|s| s.bg(move || theme().accent))
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .cursor_pointer()
        .child(
            view()
                .size(px(14.0))
                .rounded(px(3.0))
                .border_1()
                .border_color(move || {
                    if checked() {
                        theme().primary
                    } else {
                        theme().border
                    }
                })
                .bg(move || {
                    if checked() {
                        theme().primary
                    } else {
                        theme().background
                    }
                })
                .items_center()
                .justify_center()
                .child(
                    text(move || if checked() { "✓" } else { "" })
                        .text_xs()
                        .text_color(move || theme().primary_foreground),
                ),
        )
        .child(text(label).text_sm())
}

/// One choice of a row of choices (a tab, a page): highlighted while
/// `selected`.
pub fn segment(
    label: &'static str,
    selected: impl Fn() -> bool + Copy + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    button()
        .on_press(on_press)
        .role(Some(AccessibilityRole::Tab))
        .selected(selected)
        .h(px(28.0))
        .px_3()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .bg(move || {
            if selected() {
                theme().secondary
            } else {
                theme().background.opacity(0.0)
            }
        })
        .hover(|s| s.bg(move || theme().accent))
        .focus(|s| s.outline(px(2.0)).outline_color(move || theme().ring))
        .cursor_pointer()
        .child(
            text(label)
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(move || {
                    if selected() {
                        theme().secondary_foreground
                    } else {
                        theme().muted_foreground
                    }
                }),
        )
}

/// `content`, opening `href` when pressed; as it is with no `href`.
pub fn maybe_link(href: Option<String>, content: View) -> AnyElement {
    match href {
        Some(href) => link()
            .href(SharedString::from(href))
            .rounded(px(4.0))
            .hover(|s| s.bg(move || theme().accent))
            .cursor_pointer()
            .child(content)
            .into_element(),
        None => content.into_element(),
    }
}
