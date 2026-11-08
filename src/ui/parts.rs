//! Meerkat's own controls, styled once from the theme's tokens: panels and
//! their heads, buttons, segmented choices, filter tokens, a search field,
//! table heads, selectable rows and group headers, inline error and wait
//! bars, skeleton rows, key–value lines, meters and the summary sentence.

use std::rc::Rc;

use nsh_base::{button, input};
use nsh_core::prelude::*;

use super::{
    icons::{Icon, icon},
    status::Role,
    text::Msg,
    tokens::{DIM, Type, Typeset, color, len, mark},
};
use crate::{
    config::{ColorToken::*, LengthToken::*},
    failure::Failure,
    gh::Credential,
    sources::Source,
    time,
};

/// Nothing: a color no surface shows (an unselected row's leading bar).
pub fn clear() -> AdaptiveColor {
    color(Background).opacity(0.0)
}

/// A panel: the card surface, its border and radius.
pub fn panel() -> View {
    view()
        .rounded(len(RadiusPanel))
        .border_1()
        .border_color(color(Border))
        .bg(color(Card))
        .text_color(color(CardForeground))
        .overflow_hidden()
}

/// A panel's head: its title, a note after it, and controls at the right,
/// ruled under.
pub fn panel_head(
    title: impl Into<SharedString>,
    note: impl IntoReactive<SharedString>,
    right: Vec<AnyElement>,
) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(RowGroup) - len(Space3))
        .px(len(Space4))
        .border_b_1()
        .border_b_color(color(Border))
        .child(text(title.into()).ty(Type::Heading).number_of_lines(1))
        .child(small(note).flex_shrink(1.0).number_of_lines(1))
        .child(view().flex_1())
        .children(right)
}

/// A section's head on a page: its title and a note, a link at the right.
pub fn section_head(
    title: impl Into<SharedString>,
    note: impl IntoReactive<SharedString>,
    right: Vec<AnyElement>,
) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(Control))
        .child(text(title.into()).ty(Type::Heading))
        .child(small(note))
        .child(view().flex_1())
        .children(right)
}

/// Captions and meta lines: small, in the third ink.
pub fn small(content: impl IntoReactive<SharedString>) -> Text {
    text(content).ty(Type::Small).text_color(color(Ink3))
}

/// Monospaced text in the second ink: ids, models, times.
pub fn mono(content: impl IntoReactive<SharedString>) -> Text {
    text(content).ty(Type::Mono).text_color(color(Ink2))
}

/// An error's text: monospaced, in `fail-text`.
pub fn error_text(content: impl IntoReactive<SharedString>) -> Text {
    text(content).ty(Type::Mono).text_color(Role::Fail.text())
}

/// A tooltip's body: `content` in mono on the foreground ink (an
/// absolute time, a full version).
pub fn hint(content: impl Into<SharedString>) -> AnyElement {
    view()
        .px(len(Space2))
        .py(len(Space1))
        .rounded(len(RadiusControl))
        .bg(color(Foreground))
        .child(text(content.into()).ty(Type::Mono).text_color(color(Background)))
        .into_element()
}

/// How a button looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// Bordered.
    Plain,
    /// The page's main action: filled.
    Primary,
    /// Text in the primary color, no frame.
    Link,
}

/// A button of `look` with an optional leading icon; `disabled` dims it
/// and stops presses.
pub fn action(
    label: impl IntoReactive<SharedString>,
    lead: Option<Icon>,
    look: Look,
    disabled: impl Fn() -> bool + Clone + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let (fill, ink, edge) = match look {
        Look::Plain => (color(Background), color(Foreground), color(Input)),
        Look::Primary => (color(Primary), color(PrimaryForeground), color(Primary)),
        Look::Link => (clear(), color(Primary), clear()),
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
        .h(len(Control))
        .px(len(if look == Look::Link { Space1 } else { Space3 }))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(edge)
        .bg(fill)
        .opacity(move || if dim() { DIM } else { 1.0 })
        .hover(move |s| match look {
            Look::Plain => s.bg(color(Accent)),
            Look::Primary | Look::Link => s.opacity(0.85),
        })
        .focus(|s| s.outline(mark()).outline_color(color(Ring)).outline_offset(mark()))
        .cursor_pointer()
        .children(lead.map(|lead| icon(lead, move || ink)))
        .child(text(label).ty(Type::Label).text_color(ink).number_of_lines(1))
}

/// [`action`] that is never disabled and has no icon.
pub fn press(
    label: impl IntoReactive<SharedString>,
    look: Look,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    action(label, None, look, || false, on_press)
}

/// A square button holding an icon only, `label` for screen readers.
pub fn icon_button(icon_: Icon, label: Msg, on_press: impl Fn() + 'static) -> impl IntoElement {
    button()
        .on_press(on_press)
        .accessibility_label(SharedString::from(label))
        .size(len(Control))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(len(RadiusControl))
        .hover(|s| s.bg(color(Accent)))
        .focus(|s| s.outline(mark()).outline_color(color(Ring)).outline_offset(mark()))
        .cursor_pointer()
        .child(icon(icon_, || color(Ink2)))
}

/// One choice of [`segmented`]: its label, a count after it, whether it is
/// the current one, and what pressing it does.
pub struct Choice {
    pub label: SharedString,
    pub lead: Option<Icon>,
    pub count: Option<Box<dyn Fn() -> Option<usize>>>,
    pub selected: Rc<dyn Fn() -> bool>,
    pub on_press: Box<dyn Fn()>,
}

impl Choice {
    pub fn new(
        label: impl Into<SharedString>,
        selected: impl Fn() -> bool + 'static,
        on_press: impl Fn() + 'static,
    ) -> Self {
        Self {
            label: label.into(),
            lead: None,
            count: None,
            selected: Rc::new(selected),
            on_press: Box::new(on_press),
        }
    }

    pub fn lead(mut self, icon: Icon) -> Self {
        self.lead = Some(icon);
        self
    }

    pub fn count(mut self, count: impl Fn() -> Option<usize> + 'static) -> Self {
        self.count = Some(Box::new(count));
        self
    }
}

/// Choices side by side on a sunken track; the current one raised.
pub fn segmented(choices: Vec<Choice>) -> View {
    view()
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space1) / 2.0)
        .p(len(Space1) / 2.0)
        .h(len(Control) + len(Space1))
        .role(AccessibilityRole::Tablist)
        .rounded(len(RadiusControl))
        .bg(color(Secondary))
        .children(choices.into_iter().map(|choice| {
            let Choice {
                label,
                lead,
                count,
                selected,
                on_press,
            } = choice;
            let (a, b, c, d) = (selected.clone(), selected.clone(), selected.clone(), selected);
            button()
                .on_press(on_press)
                .accessibility_label(Some(label.clone()))
                .role(Some(AccessibilityRole::Tab))
                .selected(move || a())
                .flex_row()
                .items_center()
                .gap(len(Space2))
                .h(len(Control) - len(Space1))
                .px(len(Space3))
                .rounded(len(RadiusControl) - len(Space1) / 2.0)
                .border_1()
                .border_color(move || if b() { color(Border) } else { clear() })
                .bg(move || if c() { color(Background) } else { clear() })
                .cursor_pointer()
                .children(lead.map(|lead| {
                    let d = d.clone();
                    icon(lead, move || if d() { color(Foreground) } else { color(Ink2) })
                }))
                .child(text(label).ty(Type::Label).text_color({
                    let d = d.clone();
                    move || if d() { color(Foreground) } else { color(Ink2) }
                }))
                .children(count.map(|count| {
                    text(move || count().map(|n| n.to_string()).unwrap_or_default())
                        .ty(Type::Small)
                        .text_color(color(Ink3))
                }))
        }))
}

/// A filter token: its key and current value in a pill; a press steps the
/// value.
pub fn token(
    key: Msg,
    value: impl Fn() -> SharedString + 'static,
    on_press: impl Fn() + 'static,
) -> impl IntoElement {
    let key = SharedString::from(key);
    button()
        .on_press(on_press)
        .accessibility_label(key.clone())
        .flex_row()
        .flex_shrink_0()
        .items_center()
        .gap(len(Space1))
        .h(len(Control))
        .px(len(Space3))
        .rounded_full()
        .border_1()
        .border_color(color(Input))
        .bg(color(Background))
        .hover(|s| s.bg(color(Accent)))
        .focus(|s| s.outline(mark()).outline_color(color(Ring)).outline_offset(mark()))
        .cursor_pointer()
        .child(text(Msg::TokenKey(key.to_string()).to_string()).ty(Type::Small).text_color(color(Ink2)))
        .child(text(value).ty(Type::Label))
}

/// A text field with a search icon; what is typed goes into `value`.
pub fn search_field(value: Signal<String>, placeholder: Msg) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Control))
        .px(len(Space2))
        .rounded(len(RadiusControl))
        .border_1()
        .border_color(color(Input))
        .bg(color(Background))
        .child(icon(Icon::Search, || color(Ink3)))
        .child(
            input()
                .flex_1()
                .min_w(Points::ZERO)
                .value(move || value.get())
                .on_change(move |text: &str| value.set(text.to_owned()))
                .placeholder(placeholder.to_string())
                .placeholder_color(move || pick(color(Ink3)))
                .text_size(len(TextBody))
                .text_color(color(Foreground)),
        )
}

/// `c` in the current appearance.
pub fn pick(c: AdaptiveColor) -> Hsla {
    if dark() { c.dark } else { c.light }
}

/// A key's name in a frame: `⌘K`, `esc`.
pub fn kbd(key: impl Into<SharedString>) -> View {
    view()
        .flex_shrink_0()
        .px(len(Space1))
        .rounded(len(RadiusChip))
        .border_1()
        .border_color(color(Border))
        .bg(color(Background))
        .child(text(key.into()).ty(Type::Small).text_color(color(Ink3)))
}

/// A word on a role's tint: the 「人类」 tag.
pub fn tag(word: impl Into<SharedString>, role: Role) -> View {
    view()
        .flex_shrink_0()
        .px(len(Space1))
        .rounded(len(RadiusChip))
        .bg(role.subtle())
        .child(text(word.into()).ty(Type::Label).text_color(role.text()).number_of_lines(1))
}

/// A count on a role's tint (a group's size beside its title).
pub fn badge(count: usize, role: Role) -> View {
    view()
        .flex_shrink_0()
        .px(len(Space1))
        .min_w(len(Space4))
        .items_center()
        .rounded_full()
        .bg(role.subtle())
        .child(text(count.to_string()).ty(Type::Label).text_color(role.text()))
}

/// A table's header cell text.
pub fn th(label: impl Into<SharedString>) -> Text {
    text(label.into())
        .ty(Type::Label)
        .text_color(color(Ink3))
        .number_of_lines(1)
}

/// A table's head: `cells` on the sunken surface, ruled under; it sticks to
/// the top of the scroll view around it.
pub fn head_row(cells: Vec<AnyElement>) -> View {
    view()
        .sticky()
        .top(Points::ZERO)
        .z_index(1)
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .h(len(Row))
        .px(len(Space4))
        .bg(color(Background))
        .border_b_1()
        .border_b_color(color(Border))
        .children(cells)
}

/// A selectable row of `row` height: hover tints it, the selection gives
/// it the `selected` surface and a leading `selection` bar.
pub fn row(
    selected: impl Fn() -> bool + Clone + 'static,
    label: impl Into<SharedString>,
    on_press: impl Fn() + 'static,
) -> nsh_base::ButtonElement {
    let (a, b, c) = (selected.clone(), selected.clone(), selected);
    button()
        .on_press(on_press)
        .accessibility_label(label.into())
        .selected(a)
        .flex_row()
        .items_center()
        .gap(len(Space3))
        .min_h(len(Row))
        .pr(len(Space4))
        .pl(len(Space4) - mark())
        .border_l(mark())
        .border_l_color(move || if b() { color(Selection) } else { clear() })
        .border_b_1()
        .border_b_color(color(Border))
        .bg(move || if c() { color(Selected) } else { clear() })
        .hover(|s| s.bg(color(Accent)))
        .cursor_pointer()
}

/// A group's header in a table: its glyph, title and count on the sunken
/// surface; a press opens or closes it when it can.
pub fn group_header(lead: AnyElement, title: impl Into<SharedString>, count: usize) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .h(len(Row))
        .px(len(Space4))
        .bg(color(Sunken))
        .border_b_1()
        .border_b_color(color(Border))
        .child(lead)
        .child(text(title.into()).ty(Type::Label))
        .child(small(count.to_string()))
}

/// An inline error bar over a panel whose source failed: the glyph, why,
/// when it last loaded, and 「重试」.
pub fn error_bar(error: Failure, last: Option<f64>, retry: impl Fn() + 'static) -> View {
    let error = Msg::Failure(error).to_string();
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(Row))
        .px(len(Space4))
        .bg(Role::Fail.subtle())
        .border_b_1()
        .border_b_color(color(Border))
        .child(super::status::glyph(super::status::Glyph::Cross, Role::Fail))
        .child(
            text(crate::attention::first_line(&error).to_owned())
                .ty(Type::Body)
                .text_color(Role::Fail.text())
                .flex_1()
                .min_w(Points::ZERO)
                .number_of_lines(1)
                .selectable(true),
        )
        .child(small(Msg::LastLoaded(last.map(time::clock)).to_string()))
        .child(press(Msg::Retry, Look::Plain, retry))
}

/// What `source` lacks: an error bar while its last load failed, skeleton
/// rows while its first load is on its way; nothing while it is fine.
pub fn source_bar<T: 'static>(source: Source<T>, rows: usize) -> AnyElement {
    dynamic(move || {
        let at = source.with(|loaded| loaded.map(|loaded| loaded.at));
        match (source.error(), at) {
            (Some(error), at) => error_bar(error, at, move || source.refetch()).into_element(),
            (None, None) => skeleton(rows).into_element(),
            (None, Some(_)) => nothing(),
        }
    })
    .into_element()
}

/// What a source read from GitHub lacks: while the credential is fetched,
/// a bar saying so (the source has not started); then as [`source_bar`].
pub fn github_bar<T: 'static>(
    credential: Signal<Option<Credential>>,
    source: Source<T>,
    rows: usize,
) -> AnyElement {
    dynamic(move || {
        if credential.with(Option::is_none) {
            wait_bar(Msg::FetchingCredential).into_element()
        } else {
            source_bar(source, rows)
        }
    })
    .into_element()
}

/// An inline bar over a panel whose source waits for something else: the
/// glyph and what it waits for.
fn wait_bar(waits_for: Msg) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(Space2))
        .min_h(len(Row))
        .px(len(Space4))
        .bg(Role::Wait.subtle())
        .border_b_1()
        .border_b_color(color(Border))
        .child(super::status::glyph(super::status::Glyph::Dashed, Role::Wait))
        .child(
            text(SharedString::from(waits_for))
                .ty(Type::Body)
                .text_color(Role::Wait.text())
                .flex_1()
                .min_w(Points::ZERO)
                .number_of_lines(1),
        )
}

/// `rows` placeholder rows at the final row height, while data loads.
pub fn skeleton(rows: usize) -> View {
    view().children((0..rows).map(|i| {
        view()
            .h(len(Row))
            .px(len(Space4))
            .justify_center()
            .border_b_1()
            .border_b_color(color(Border))
            .child(
                view()
                    .h(len(Space2))
                    .w(relative(if i % 2 == 0 { 0.6 } else { 0.4 }))
                    .rounded(len(RadiusChip))
                    .bg(color(Secondary)),
            )
    }))
}

/// An empty panel's one grey line.
pub fn empty(message: Msg) -> View {
    view()
        .px(len(Space4))
        .py(len(Space3))
        .child(small(SharedString::from(message)))
}

/// An inspector's key–value line: the key in a fixed column.
pub fn kv(key: Msg, value: impl IntoElement) -> View {
    view()
        .flex_row()
        .items_start()
        .gap(len(Space3))
        .min_h(len(Row) - len(Space2))
        .child(
            small(SharedString::from(key))
                .w(len(Space8) + len(Space6))
                .flex_shrink_0(),
        )
        .child(view().flex_1().min_w(Points::ZERO).child(value))
}

/// A thin bar `share` (0–1) full of `ink`.
pub fn meter(share: f32, ink: AdaptiveColor) -> View {
    view()
        .h(len(Space1))
        .w_full()
        .rounded_full()
        .bg(color(Secondary))
        .overflow_hidden()
        .child(
            view()
                .h_full()
                .w(relative(share.clamp(0.0, 1.0)))
                .rounded_full()
                .bg(ink),
        )
}

/// A panel's seats: one block per seat, voted in `ink-2`, pending empty.
pub fn votes_bar(voted: usize, total: usize) -> View {
    view()
        .flex_row()
        .flex_shrink_0()
        .gap(len(Space1) / 2.0)
        .children((0..total).map(|i| {
            view()
                .w(len(Space3))
                .h(len(Space1) + len(Space1) / 2.0)
                .rounded(len(RadiusChip) / 2.0)
                .bg(if i < voted { color(Ink3) } else { color(Secondary) })
        }))
}

/// An element that takes no room, not even a gap of its parent's: what a
/// conditional part shows while it has nothing to show.
pub fn nothing() -> AnyElement {
    view().hidden().into_element()
}

/// Text with its numbers in `foreground` and its failures in `fail-text`:
/// the status sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Span {
    Plain(String),
    Strong(String),
    Fail(String),
}

/// A sentence of `spans` in the second ink.
pub fn sentence(spans: Vec<Span>) -> Text {
    text("")
        .ty(Type::Body)
        .text_color(color(Ink2))
        .children(spans.into_iter().map(|span| match span {
            Span::Plain(s) => text(s),
            Span::Strong(s) => text(s).ty(Type::BodyStrong).text_color(color(Foreground)),
            Span::Fail(s) => text(s).ty(Type::BodyStrong).text_color(Role::Fail.text()),
        }))
}
