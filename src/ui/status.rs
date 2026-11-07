//! The status vocabulary (issue #9 「状态词表」): every state the pages
//! show is a role, a glyph and a word, always together, so no state rests
//! on color alone. The roles' colors are the theme's (`critical`,
//! `critical-text`, `critical-subtle`, …); the glyphs are drawn here with
//! a `Painter`, the same on macOS and the web.

use nsh_core::{
    canvas::{LineCap, LineJoin},
    prelude::*,
};

use super::tokens::{Type, Typeset, color, len};
use crate::{
    config::{ColorToken, Fields, LengthToken},
    dag::{Fallback, NodeState},
    gh::project::Status as ProjectStatus,
    programs::{BaseState, QueueState, SessionState},
};

/// What a state means for a person: the five roles of the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Critical,
    Warning,
    Good,
    Active,
    Neutral,
}

impl Role {
    /// The glyph's color.
    pub fn ink(self) -> AdaptiveColor {
        color(match self {
            Role::Critical => ColorToken::Critical,
            Role::Warning => ColorToken::Warning,
            Role::Good => ColorToken::Good,
            Role::Active => ColorToken::Active,
            Role::Neutral => ColorToken::Neutral,
        })
    }

    /// Its words' color.
    pub fn text(self) -> AdaptiveColor {
        color(match self {
            Role::Critical => ColorToken::CriticalText,
            Role::Warning => ColorToken::WarningText,
            Role::Good => ColorToken::GoodText,
            Role::Active => ColorToken::ActiveText,
            Role::Neutral => ColorToken::NeutralText,
        })
    }

    /// The tint behind it (pills, icon boxes, banners).
    pub fn subtle(self) -> AdaptiveColor {
        color(match self {
            Role::Critical => ColorToken::CriticalSubtle,
            Role::Warning => ColorToken::WarningSubtle,
            Role::Good => ColorToken::GoodSubtle,
            Role::Active => ColorToken::ActiveSubtle,
            Role::Neutral => ColorToken::NeutralSubtle,
        })
    }
}

/// The eight geometric glyphs (the mock's `G`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// A filled circle with a cross.
    Critical,
    /// A filled triangle with an exclamation mark.
    Warning,
    /// A filled circle with a check.
    Good,
    /// A ring, half filled.
    Active,
    /// A ring.
    Ready,
    /// A dashed ring.
    Blocked,
    /// A ring with two bars.
    Held,
    /// A ring struck through.
    Superseded,
}

/// A state as the pages show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub role: Role,
    pub glyph: Glyph,
    pub word: SharedString,
}

impl Status {
    fn of(role: Role, glyph: Glyph, word: impl Into<SharedString>) -> Self {
        Self {
            role,
            glyph,
            word: word.into(),
        }
    }

    /// `self` with `note` after its word (`已定案（议会不可达）`).
    fn noted(mut self, note: &str) -> Self {
        self.word = format!("{}（{note}）", self.word).into();
        self
    }
}

/// A verdict of the council's queue.
pub fn queue(state: QueueState) -> Status {
    let (role, glyph) = match state {
        QueueState::Failed => (Role::Critical, Glyph::Critical),
        QueueState::Interrupted => (Role::Warning, Glyph::Warning),
        QueueState::Deliberating => (Role::Active, Glyph::Active),
        QueueState::Ready => (Role::Neutral, Glyph::Ready),
        QueueState::Blocked => (Role::Neutral, Glyph::Blocked),
        QueueState::Held => (Role::Neutral, Glyph::Held),
        QueueState::Decided => (Role::Good, Glyph::Good),
        QueueState::Superseded => (Role::Neutral, Glyph::Superseded),
    };
    Status::of(role, glyph, state.label())
}

/// The queue's states in the legend's order.
pub const QUEUE_STATES: [QueueState; 8] = [
    QueueState::Decided,
    QueueState::Deliberating,
    QueueState::Ready,
    QueueState::Blocked,
    QueueState::Held,
    QueueState::Interrupted,
    QueueState::Failed,
    QueueState::Superseded,
];

/// A Project `Status` shown in place of a verdict: neutral, the glyph of
/// the state it amounts to, its option's name and why.
pub fn project(status: &Option<ProjectStatus>, why: Fallback, fields: &Fields) -> Status {
    let options = &fields.status.options;
    let (glyph, word) = match status {
        Some(ProjectStatus::Todo) => (Glyph::Ready, options.todo.to_string()),
        Some(ProjectStatus::InProgress) => (Glyph::Active, options.in_progress.to_string()),
        Some(ProjectStatus::Done) => (Glyph::Good, options.done.to_string()),
        Some(ProjectStatus::Superseded) => (Glyph::Superseded, options.superseded.to_string()),
        Some(ProjectStatus::Other(name)) => (Glyph::Ready, name.clone()),
        None => (Glyph::Ready, format!("{} 未设置", fields.status.name)),
    };
    Status::of(Role::Neutral, glyph, word).noted(match why {
        Fallback::Unreachable => "议会不可达",
        Fallback::Unlisted => "议会队列未列出",
    })
}

/// A DAG node's state: its verdict, or its Project `Status`.
pub fn node(state: &NodeState, fields: &Fields) -> Status {
    match state {
        NodeState::Queue(state) => queue(*state),
        NodeState::Project { status, why } => project(status, *why, fields),
    }
}

/// A session's state.
pub fn session(state: SessionState) -> Status {
    match state {
        SessionState::Running => Status::of(Role::Active, Glyph::Active, "运行中"),
        SessionState::Done => Status::of(Role::Good, Glyph::Good, "完成"),
        SessionState::Failed => Status::of(Role::Critical, Glyph::Critical, "失败"),
    }
}

/// A base's state.
pub fn base(state: BaseState) -> Status {
    match state {
        BaseState::Warming => Status::of(Role::Active, Glyph::Active, "预热中"),
        BaseState::Ready => Status::of(Role::Good, Glyph::Good, "就绪"),
        BaseState::Failed => Status::of(Role::Critical, Glyph::Critical, "失败"),
    }
}

/// A base whose repository moved on since it was warmed.
pub fn stale() -> Status {
    Status::of(Role::Warning, Glyph::Warning, "过期")
}

/// A program by its health check.
pub fn program(reachable: bool) -> Status {
    if reachable {
        Status::of(Role::Good, Glyph::Good, "在线")
    } else {
        Status::of(Role::Critical, Glyph::Critical, "不可达")
    }
}

/// An implementation's ask that became an agenda issue no answer settled
/// yet: waiting, as the mock draws it.
pub fn escalated() -> Status {
    Status::of(Role::Warning, Glyph::Held, "询问已提为议题")
}

/// A GitHub API allowance below the `attention` threshold.
pub fn rate_low() -> Status {
    Status::of(Role::Warning, Glyph::Warning, "余量低")
}

// ---------------------------------------------------------------------------
// Drawing

/// A glyph's size in points (the issue's 14 pt); its geometry is laid out
/// on a 14-unit grid, as the mock's SVGs are.
pub const GLYPH: f32 = 14.0;

/// `glyph` in `role`'s color, its cut-outs in the card's color.
pub fn glyph(glyph: Glyph, role: Role) -> CanvasView {
    let (ink, cut) = (role.ink(), color(ColorToken::Card));
    canvas(move || {
        let pick = |c: AdaptiveColor| if dark() { c.dark } else { c.light };
        paint(glyph, pick(ink), pick(cut))
    })
    .size(px(GLYPH))
    .flex_shrink_0()
}

/// A unit of the glyph grid.
fn u(v: f32) -> Points {
    px(v * GLYPH / 14.0)
}

fn paint(glyph: Glyph, ink: Hsla, cut: Hsla) -> Painter {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let ring = |p: Painter| {
        p.stroke_color(ink)
            .line_width(u(1.6))
            .begin_path()
            .arc(u(7.0), u(7.0), u(5.3), 0.0, TAU)
            .stroke()
    };
    let disc = |p: Painter| {
        p.fill_color(ink)
            .begin_path()
            .arc(u(7.0), u(7.0), u(6.0), 0.0, TAU)
            .fill()
            .stroke_color(cut)
            .line_width(u(1.6))
            .line_cap(LineCap::Round)
            .line_join(LineJoin::Round)
    };
    let p = Painter::new();
    match glyph {
        Glyph::Critical => disc(p)
            .begin_path()
            .move_to(u(4.6), u(4.6))
            .line_to(u(9.4), u(9.4))
            .move_to(u(9.4), u(4.6))
            .line_to(u(4.6), u(9.4))
            .stroke(),
        Glyph::Good => disc(p)
            .begin_path()
            .move_to(u(4.3), u(7.2))
            .line_to(u(6.2), u(9.1))
            .line_to(u(9.8), u(5.3))
            .stroke(),
        Glyph::Warning => p
            .fill_color(ink)
            .stroke_color(ink)
            .line_width(u(1.0))
            .line_join(LineJoin::Round)
            .begin_path()
            .move_to(u(7.0), u(1.3))
            .line_to(u(13.0), u(12.2))
            .line_to(u(1.0), u(12.2))
            .close_path()
            .fill()
            .stroke()
            .stroke_color(cut)
            .line_width(u(1.6))
            .line_cap(LineCap::Round)
            .begin_path()
            .move_to(u(7.0), u(5.0))
            .line_to(u(7.0), u(8.2))
            .stroke()
            .fill_color(cut)
            .begin_path()
            .arc(u(7.0), u(10.1), u(0.9), 0.0, TAU)
            .fill(),
        Glyph::Active => ring(p)
            .fill_color(ink)
            .begin_path()
            .arc(u(7.0), u(7.0), u(5.3), -FRAC_PI_2, FRAC_PI_2)
            .close_path()
            .fill(),
        Glyph::Ready => ring(p),
        Glyph::Blocked => ring(p.line_dash(&[u(2.2), u(2.0)])),
        Glyph::Held => ring(p)
            .line_cap(LineCap::Round)
            .begin_path()
            .move_to(u(5.6), u(4.8))
            .line_to(u(5.6), u(9.2))
            .move_to(u(8.4), u(4.8))
            .line_to(u(8.4), u(9.2))
            .stroke(),
        Glyph::Superseded => ring(p)
            .begin_path()
            .move_to(u(3.4), u(10.6))
            .line_to(u(10.6), u(3.4))
            .stroke(),
    }
}

/// Glyph and word in the role's text color (`.st` of the mock), at the
/// size of `ty`.
pub fn label(status: &Status, ty: Type) -> View {
    let role = status.role;
    view()
        .flex_row()
        .items_center()
        .gap(len(LengthToken::Space1))
        .flex_shrink_0()
        .child(glyph(status.glyph, role))
        .child(
            text(status.word.clone())
                .ty(ty)
                .font_weight(FontWeight::MEDIUM)
                .text_color(role.text())
                .number_of_lines(1),
        )
}

/// Glyph and word on the role's tint, rounded (`.pill`).
pub fn pill(status: &Status) -> View {
    label(status, Type::Meta)
        .px(len(LengthToken::Space2))
        .py(Points::ZERO)
        .rounded_full()
        .bg(status.role.subtle())
}

/// A word on the role's tint without a glyph: a category next to a
/// glyph that already shows the state (the attention queue's).
pub fn tag(word: impl Into<SharedString>, role: Role) -> View {
    view()
        .flex_shrink_0()
        .px(len(LengthToken::Space2))
        .rounded_full()
        .bg(role.subtle())
        .child(
            text(word.into())
                .ty(Type::Meta)
                .font_weight(FontWeight::MEDIUM)
                .text_color(role.text())
                .number_of_lines(1),
        )
}

/// The glyph in a square of the role's tint (`.ico`): a queue row's lead.
pub fn icon_box(status: &Status) -> View {
    view()
        .size(len(LengthToken::Space6))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(len(LengthToken::Radius))
        .bg(status.role.subtle())
        .child(glyph(status.glyph, status.role))
}
