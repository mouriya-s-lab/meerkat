//! The dashboard's own controls, styled once from the theme's tokens:
//! cards and their heads, buttons, segmented choices, check boxes, chips,
//! key–value chips, table heads and group rows, a source's trouble line,
//! summary sentences.

use nsh_base::button;
use nsh_core::prelude::*;

use super::{
    icons::{Icon, icon},
    status::Role,
    tokens::{MARK, Type, Typeset, color, len},
};
use crate::{
    config::{ColorToken::*, LengthToken::*},
    sources::Source,
    time,
};

/// A card: the theme's card surface, border and radius.
pub fn card() -> View {
    view()
        .rounded(len(Radius))
        .border_1()
        .border_color(color(Border))
        .bg(color(Card))
        .text_color(color(CardForeground))
        .overflow_hidden()
}

/// A card's head: its title, a note after it, and controls at the right.
pub fn card_head(
    title: impl Into<SharedString>,
    sub: impl IntoReactive<SharedString>,
    right: Vec<AnyElement>,
) -> View {
    view()
        .flex_row()
        .flex_wrap()
        .items_center()
        .gap(len(Space2))
        .px(len(Space4))
        .pt(len(Space3))
        .pb(len(Space2))
        .child(
            text(title.into())
                .ty(Type::Heading)
                .font_weight(FontWeight::SEMIBOLD),
        )
        .child(meta(sub).flex_shrink(1.0))
        .child(view().flex_1())
        .children(right)
}

/// Muted caption text.
pub fn meta(content: impl IntoReactive<SharedString>) -> Text {
    text(content)
        .ty(Type::Meta)
        .text_color(color(MutedForeground))
}

/// Secondary text in the second ink.
pub fn ink2(content: impl IntoReactive<SharedString>, ty: Type) -> Text {
    text(content).ty(ty).text_color(color(Ink2))
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Bordered, on the card.
    Plain,
    /// The main action: filled.
    Primary,
    /// No border, no fill.
    Ghost,
}

/// A button of `look`, small or not; `disabled` dims it and stops presses.
pub fn action(
    label: impl IntoReactive<SharedString>,
    look: Look,
    small: bool,
    disabled: impl Fn() -> bool + Clone + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let (fill, ink, edge) = match look {
        Look::Plain => (color(Card), color(Foreground), color(Input)),
        Look::Primary => (color(Primary), color(PrimaryForeground), color(Primary)),
        Look::Ghost => (
            color(Card).opacity(0.0),
            color(Ink2),
            color(Card).opacity(0.0),
        ),
    };
    let dim = disabled.clone();
    let label = label.into_reactive();
    button()
        .on_press(on_press)
        .disabled(disabled)
        .accessibility_label(label.clone().map(Some))
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(len(Space1))
        .px(len(if small { Space2 } else { Space3 }))
        .py(len(Space1))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(edge)
        .bg(fill)
        .opacity(move || if dim() { 0.5_f32 } else { 1.0 })
        .hover(|s| s.opacity(0.85))
        .active(|s| s.opacity(0.7))
        .focus(|s| s.outline(px(MARK)).outline_color(color(Ring)))
        .cursor_pointer()
        .child(
            text(label)
                .ty(if small { Type::Meta } else { Type::Dense })
                .font_weight(FontWeight::MEDIUM)
                .text_color(ink)
                .number_of_lines(1),
        )
}

/// [`action`] that is never disabled.
pub fn press(
    label: impl IntoReactive<SharedString>,
    look: Look,
    small: bool,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    action(label, look, small, || false, on_press)
}

/// The refresh button: an icon and its label.
pub fn refresh_button(
    label: impl IntoReactive<SharedString>,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let label = label.into_reactive();
    button()
        .on_press(on_press)
        .accessibility_label(label.clone().map(Some))
        .flex_row()
        .items_center()
        .justify_center()
        .gap(len(Space2))
        .py(len(Space1))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Input))
        .bg(color(Card))
        .hover(|s| s.bg(color(Accent)))
        .focus(|s| s.outline(px(MARK)).outline_color(color(Ring)))
        .cursor_pointer()
        .child(icon(Icon::Refresh, || color(Foreground)))
        .child(text(label).ty(Type::Dense).font_weight(FontWeight::MEDIUM))
}

/// One choice of [`segmented`]: its label, whether it is the current
/// one, and what pressing it does.
pub type Choice = (SharedString, Box<dyn Fn() -> bool>, Box<dyn Fn()>);

/// A row of choices in one frame (`.seg`): each a tab, marked while
/// `selected`.
pub fn segmented(choices: Vec<Choice>) -> View {
    let count = choices.len();
    view()
        .flex_row()
        .flex_shrink_0()
        .role(AccessibilityRole::Tablist)
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Input))
        .bg(color(Card))
        .overflow_hidden()
        .children(
            choices
                .into_iter()
                .enumerate()
                .map(|(i, (label, selected, on_press))| {
                    let selected: std::rc::Rc<dyn Fn() -> bool> = selected.into();
                    let (a, b, c) = (selected.clone(), selected.clone(), selected.clone());
                    button()
                        .on_press(on_press)
                        .accessibility_label(Some(label.clone()))
                        .role(Some(AccessibilityRole::Tab))
                        .selected(move || a())
                        .justify_center()
                        .px(len(Space3))
                        .py(len(Space1))
                        .when(i + 1 < count, |b| {
                            b.border_r_1().border_r_color(color(Input))
                        })
                        .bg(move || if b() { color(Accent) } else { color(Card) })
                        .hover(|s| s.bg(color(Accent)))
                        .cursor_pointer()
                        .child(
                            text(label)
                                .ty(Type::Dense)
                                .font_weight(if c() {
                                    FontWeight::SEMIBOLD
                                } else {
                                    FontWeight::NORMAL
                                })
                                .text_color(
                                    move || if c() { color(Foreground) } else { color(Ink2) },
                                ),
                        )
                }),
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
        .accessibility_label(label)
        .selected(checked)
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space2))
        .px(len(Space3))
        .py(len(Space1))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Input))
        .bg(color(Card))
        .hover(|s| s.bg(color(Accent)))
        .focus(|s| s.outline(px(MARK)).outline_color(color(Ring)))
        .cursor_pointer()
        .child(
            view()
                .size(len(Space3))
                .rounded(len(Space1))
                .border_1()
                .border_color(move || {
                    if checked() {
                        color(Primary)
                    } else {
                        color(BorderStrong)
                    }
                })
                .bg(move || {
                    if checked() {
                        color(Primary)
                    } else {
                        color(Card)
                    }
                }),
        )
        .child(ink2(label, Type::Dense))
}

/// A monospaced value in a thin frame (`.chip`): a model, an id, a number.
pub fn chip(value: impl Into<SharedString>) -> View {
    view()
        .flex_shrink_0()
        .px(len(Space1))
        .rounded(len(Space1))
        .border_1()
        .border_color(color(Border))
        .child(
            text(value.into())
                .ty(Type::Meta)
                .mono()
                .text_color(color(Ink2))
                .truncate(),
        )
}

/// A word on the secondary surface (`.tag`): a kind, a program.
pub fn tag(value: impl Into<SharedString>) -> View {
    view()
        .flex_shrink_0()
        .px(len(Space1))
        .rounded(len(Space1))
        .bg(color(Secondary))
        .child(
            text(value.into())
                .ty(Type::Meta)
                .text_color(color(Ink2))
                .number_of_lines(1),
        )
}

/// A key and its value side by side in one frame (`.kvchip`).
pub fn kv_chip(key: &'static str, value: impl Into<SharedString>) -> View {
    view()
        .flex_row()
        .flex_shrink_0()
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Border))
        .overflow_hidden()
        .child(
            view()
                .justify_center()
                .px(len(Space2))
                .bg(color(Muted))
                .border_r_1()
                .border_r_color(color(Border))
                .child(meta(key)),
        )
        .child(
            view().justify_center().px(len(Space2)).child(
                text(value.into())
                    .ty(Type::Meta)
                    .mono()
                    .font_weight(FontWeight::MEDIUM)
                    .selectable(true),
            ),
        )
}

/// A table's header cell text.
pub fn th(label: impl Into<SharedString>) -> Text {
    text(label.into())
        .ty(Type::Meta)
        .font_weight(FontWeight::MEDIUM)
        .text_color(color(MutedForeground))
        .number_of_lines(1)
}

/// A table's head row: `cells` at the head's height, a rule under it.
pub fn head_row(cells: Vec<AnyElement>) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .px(len(Space4))
        .py(len(Space2))
        .border_b_1()
        .border_b_color(color(Border))
        .children(cells)
}

/// A table's body row of `row-dense` height, ruled above.
pub fn body_row() -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .px(len(Space4))
        .min_h(len(RowDense))
        .border_t_1()
        .border_t_color(color(Border))
}

/// A group's label row in a table (`tr.group`).
pub fn group_row(label: impl IntoReactive<SharedString>) -> View {
    view()
        .px(len(Space4))
        .py(len(Space1))
        .bg(color(Muted))
        .border_t_1()
        .border_t_color(color(Border))
        .child(
            text(label)
                .ty(Type::Meta)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(Ink2)),
        )
}

/// What is wrong with `source`, if anything: its error with when it last
/// loaded, or that its first load is under way. Nothing while it is fine.
pub fn source_trouble<T: 'static>(label: &'static str, source: Source<T>) -> AnyElement {
    dynamic(move || {
        let at = source.with(|loaded| loaded.map(|loaded| loaded.at));
        let line = match (source.error(), at) {
            (Some(error), _) => Some((
                format!(
                    "{label}：{error}（{}）",
                    at.map(|at| format!("上次成功 {}", time::clock(at)))
                        .unwrap_or_else(|| "从未成功".to_owned())
                ),
                Role::Critical.text(),
            )),
            (None, None) if source.state() == ResourceState::Unresolved => {
                Some((format!("{label}：等待前置数据"), color(MutedForeground)))
            }
            (None, None) => Some((format!("{label}：加载中…"), color(MutedForeground))),
            (None, Some(_)) => None,
        };
        match line {
            Some((line, ink)) => text(line)
                .ty(Type::Meta)
                .text_color(ink)
                .selectable(true)
                .number_of_lines(2)
                .into_element(),
            None => nothing(),
        }
    })
    .into_element()
}

/// An element that takes no room, not even a gap of its parent's: what a
/// conditional part shows while it has nothing to show.
pub fn nothing() -> AnyElement {
    view().hidden().into_element()
}

/// Text with its numbers in bold and its failures in `critical-text`:
/// the summary sentences.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    Plain(String),
    Strong(String),
    Critical(String),
}

/// A sentence of `spans` in the second ink.
pub fn sentence(spans: Vec<Span>) -> Text {
    text("")
        .ty(Type::Body)
        .text_color(color(Ink2))
        .children(spans.into_iter().map(|span| {
            match span {
                Span::Plain(s) => text(s),
                Span::Strong(s) => text(s)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(Foreground)),
                Span::Critical(s) => text(s)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(Role::Critical.text()),
            }
        }))
}
