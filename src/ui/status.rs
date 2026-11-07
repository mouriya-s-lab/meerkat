//! The status vocabulary (#9 「状态词表」): every state the pages show is
//! a role, a glyph and a word, always together, so no state rests on color
//! alone. The roles' colors are the theme's (`fail`, `fail-text`,
//! `fail-subtle`, …); the glyphs are drawn here with a `Painter`, the same
//! on macOS and the web. Red is for failures a person still has to act on;
//! what is settled is the quiet `done` grey.

use nsh_core::{
    canvas::{LineCap, LineJoin},
    prelude::*,
};

use super::{
    text::Msg,
    tokens::{Type, Typeset, color, len},
};
use crate::{
    config::{ColorToken, Fields, LengthToken},
    dag::{Fallback, NodeState},
    gh::project::Status as ProjectStatus,
    programs::{BaseState, QueueState, SessionState},
};

/// What a state means for a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Failed, and a person has to act.
    Fail,
    /// Needs a look: interrupted, stale, low.
    Warn,
    /// Working as it should.
    Ok,
    /// Under way.
    Run,
    /// Waits for something else.
    Wait,
    /// Settled: decided, superseded.
    Done,
    /// Waits for a person's decision.
    Human,
    /// Nothing to say: ready, a Project `Status`.
    Neutral,
}

impl Role {
    /// The glyph's color.
    pub fn ink(self) -> AdaptiveColor {
        color(match self {
            Role::Fail => ColorToken::Fail,
            Role::Warn => ColorToken::Warn,
            Role::Ok => ColorToken::Ok,
            Role::Run => ColorToken::Run,
            Role::Wait => ColorToken::Wait,
            Role::Done => ColorToken::Done,
            Role::Human => ColorToken::Human,
            Role::Neutral => ColorToken::Ink3,
        })
    }

    /// Its words' color.
    pub fn text(self) -> AdaptiveColor {
        color(match self {
            Role::Fail => ColorToken::FailText,
            Role::Warn => ColorToken::WarnText,
            Role::Ok => ColorToken::OkText,
            Role::Run => ColorToken::RunText,
            Role::Wait => ColorToken::WaitText,
            Role::Done | Role::Neutral => ColorToken::Ink3,
            Role::Human => ColorToken::HumanText,
        })
    }

    /// The tint behind it (an error bar, a count badge).
    pub fn subtle(self) -> AdaptiveColor {
        color(match self {
            Role::Fail => ColorToken::FailSubtle,
            Role::Warn => ColorToken::WarnSubtle,
            Role::Ok => ColorToken::OkSubtle,
            Role::Run => ColorToken::RunSubtle,
            Role::Wait | Role::Done | Role::Neutral => ColorToken::WaitSubtle,
            Role::Human => ColorToken::HumanSubtle,
        })
    }
}

/// The eight geometric glyphs; each alone tells the queue's eight states
/// apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// A filled circle with a cross.
    Cross,
    /// A filled triangle with an exclamation mark.
    Triangle,
    /// A filled circle with a check.
    Check,
    /// A ring, half filled.
    Half,
    /// A ring.
    Ring,
    /// A dashed ring.
    Dashed,
    /// A ring with two bars.
    Bars,
    /// A ring struck through.
    Struck,
}

/// A state as the pages show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub role: Role,
    pub glyph: Glyph,
    pub word: SharedString,
}

impl Status {
    fn of(role: Role, glyph: Glyph, word: Msg) -> Self {
        Self {
            role,
            glyph,
            word: word.into(),
        }
    }
}

/// A verdict of the council's queue.
pub fn queue(state: QueueState) -> Status {
    let (role, glyph) = match state {
        QueueState::Failed => (Role::Fail, Glyph::Cross),
        QueueState::Interrupted => (Role::Warn, Glyph::Triangle),
        QueueState::Deliberating => (Role::Run, Glyph::Half),
        QueueState::Ready => (Role::Neutral, Glyph::Ring),
        QueueState::Blocked => (Role::Wait, Glyph::Dashed),
        QueueState::Held => (Role::Human, Glyph::Bars),
        QueueState::Decided => (Role::Done, Glyph::Check),
        QueueState::Superseded => (Role::Done, Glyph::Struck),
    };
    Status::of(role, glyph, Msg::Queue(state))
}

/// A Project `Status` shown in place of a verdict: neutral, the glyph of
/// the state it amounts to, its option's name and why.
pub fn project(status: &Option<ProjectStatus>, why: Fallback, fields: &Fields) -> Status {
    let options = &fields.status.options;
    let (glyph, word) = match status {
        Some(ProjectStatus::Todo) => (Glyph::Ring, options.todo.to_string()),
        Some(ProjectStatus::InProgress) => (Glyph::Half, options.in_progress.to_string()),
        Some(ProjectStatus::Done) => (Glyph::Check, options.done.to_string()),
        Some(ProjectStatus::Superseded) => (Glyph::Struck, options.superseded.to_string()),
        Some(ProjectStatus::Other(name)) => (Glyph::Ring, name.clone()),
        None => (
            Glyph::Ring,
            Msg::StatusUnset(fields.status.name.to_string()).to_string(),
        ),
    };
    Status::of(Role::Neutral, glyph, Msg::Fallback { word, why })
}

/// A node's state: its verdict, or its Project `Status`.
pub fn node(state: &NodeState, fields: &Fields) -> Status {
    match state {
        NodeState::Queue(state) => queue(*state),
        NodeState::Project { status, why } => project(status, *why, fields),
    }
}

/// A session's state.
pub fn session(state: SessionState) -> Status {
    let (role, glyph) = match state {
        SessionState::Running => (Role::Run, Glyph::Half),
        SessionState::Done => (Role::Ok, Glyph::Check),
        SessionState::Failed => (Role::Fail, Glyph::Cross),
    };
    Status::of(role, glyph, Msg::Session(state))
}

/// A base's state.
pub fn base(state: BaseState) -> Status {
    let (role, glyph) = match state {
        BaseState::Warming => (Role::Run, Glyph::Half),
        BaseState::Ready => (Role::Ok, Glyph::Check),
        BaseState::Failed => (Role::Fail, Glyph::Cross),
    };
    Status::of(role, glyph, Msg::Base(state))
}

/// A base whose repository moved on since it was warmed.
pub fn stale() -> Status {
    Status::of(Role::Warn, Glyph::Triangle, Msg::Stale)
}

/// A program by its health check.
pub fn program(reachable: bool) -> Status {
    if reachable {
        Status::of(Role::Ok, Glyph::Check, Msg::Online)
    } else {
        Status::of(Role::Fail, Glyph::Cross, Msg::Unreachable)
    }
}

/// An implementation's ask that became an agenda issue no answer settled
/// yet: a person decides.
pub fn escalated() -> Status {
    Status::of(Role::Human, Glyph::Bars, Msg::Escalated)
}

/// A GitHub API allowance below the `attention` threshold.
pub fn rate_low() -> Status {
    Status::of(Role::Warn, Glyph::Triangle, Msg::RateLow)
}

// ---------------------------------------------------------------------------
// Drawing

/// A glyph's size in points (#9's 14 pt); its geometry is laid out on a
/// 14-unit grid.
pub const GLYPH: f32 = 14.0;

/// `glyph` in `role`'s color, its cut-outs in the page's color.
pub fn glyph(glyph: Glyph, role: Role) -> CanvasView {
    let (ink, cut) = (role.ink(), color(ColorToken::Background));
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
        Glyph::Cross => disc(p)
            .begin_path()
            .move_to(u(4.6), u(4.6))
            .line_to(u(9.4), u(9.4))
            .move_to(u(9.4), u(4.6))
            .line_to(u(4.6), u(9.4))
            .stroke(),
        Glyph::Check => disc(p)
            .begin_path()
            .move_to(u(4.3), u(7.2))
            .line_to(u(6.2), u(9.1))
            .line_to(u(9.8), u(5.3))
            .stroke(),
        Glyph::Triangle => p
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
        Glyph::Half => ring(p)
            .fill_color(ink)
            .begin_path()
            .arc(u(7.0), u(7.0), u(5.3), -FRAC_PI_2, FRAC_PI_2)
            .close_path()
            .fill(),
        Glyph::Ring => ring(p),
        Glyph::Dashed => ring(p.line_dash(&[u(2.2), u(2.0)])),
        Glyph::Bars => ring(p)
            .line_cap(LineCap::Round)
            .begin_path()
            .move_to(u(5.6), u(4.8))
            .line_to(u(5.6), u(9.2))
            .move_to(u(8.4), u(4.8))
            .line_to(u(8.4), u(9.2))
            .stroke(),
        Glyph::Struck => ring(p)
            .begin_path()
            .move_to(u(3.4), u(10.6))
            .line_to(u(10.6), u(3.4))
            .stroke(),
    }
}

/// Glyph and word in the role's text color, at the size of `ty`.
pub fn label(status: &Status, ty: Type) -> View {
    view()
        .flex_row()
        .items_center()
        .gap(len(LengthToken::Space2))
        .flex_shrink_0()
        .child(glyph(status.glyph, status.role))
        .child(
            text(status.word.clone())
                .ty(ty)
                .text_color(status.role.text())
                .number_of_lines(1),
        )
}
