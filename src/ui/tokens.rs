//! The design tokens of `dashboard/theme.json` as the UI reads them: the
//! app hands them to NSH once ([`of`]) and every color, text size, spacing,
//! radius and DAG length the pages draw with is read back here by its
//! token's name. The shell mounts only once NSH draws with them, so a
//! lookup always finds its token.

use nsh_core::{element::Tokens, prelude::*};

use crate::config::{ColorToken, FontToken, LengthToken, Theme};

/// NSH's design tokens of `theme`: each color with its light and dark
/// value, each length in points, each font as its CSS family list.
pub fn of(theme: &Theme) -> Tokens {
    let mut tokens = Tokens::default();
    for token in ColorToken::ALL {
        let color = theme.color(token);
        tokens.set_color(
            token.name(),
            AdaptiveColor::new(opaque(color.light.packed()), opaque(color.dark.packed())),
        );
    }
    for token in LengthToken::ALL {
        tokens.set_number(token.name(), theme.length(token));
    }
    for token in FontToken::ALL {
        tokens.set_font(token.name(), theme.font(token));
    }
    tokens
}

/// The opaque color `0xRRGGBB`.
fn opaque(hex: u32) -> Hsla {
    nsh_core::color::rgba((hex << 8) | 0xff).to_hsla()
}

const SET: &str = "the shell mounts once the theme's tokens are set";

/// The color of `token`.
pub fn color(token: ColorToken) -> AdaptiveColor {
    nsh_core::element::tokens().color(token.name()).expect(SET)
}

/// The length of `token`.
pub fn len(token: LengthToken) -> Points {
    px(nsh_core::element::tokens().number(token.name()).expect(SET))
}

/// The CSS family list of `token`.
pub fn font(token: FontToken) -> SharedString {
    let tokens = nsh_core::element::tokens();
    let family: &str = tokens.font(token.name()).expect(SET);
    SharedString::from(family.to_owned())
}

/// The width of a rule that marks (a selection's outline, the selected
/// row's edge, a highlighted DAG edge); ordinary borders are 1 pt.
pub const MARK: f32 = 2.0;

/// The opacity of what a selection puts in the background (DAG nodes
/// away from the selected one).
pub const DIM: f32 = 0.45;

/// The text roles of the issue's type scale: each a size token and the
/// line height the issue gives it (the theme names sizes only).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// A page's title: 20/28.
    Title,
    /// A card's title: 15/22.
    Heading,
    /// Body text: 14/22.
    Body,
    /// Tables and dense rows: 13/20.
    Dense,
    /// Captions, labels, chips: 12/18.
    Meta,
    /// A KPI's number: 26/32.
    Kpi,
}

impl Type {
    fn size(self) -> LengthToken {
        match self {
            Type::Title => LengthToken::TextTitle,
            Type::Heading => LengthToken::TextHeading,
            Type::Body => LengthToken::TextBody,
            Type::Dense => LengthToken::TextDense,
            Type::Meta => LengthToken::TextMeta,
            Type::Kpi => LengthToken::TextKpi,
        }
    }

    /// The line height, in points.
    pub fn line(self) -> f32 {
        match self {
            Type::Title => 28.0,
            Type::Heading | Type::Body => 22.0,
            Type::Dense => 20.0,
            Type::Meta => 18.0,
            Type::Kpi => 32.0,
        }
    }
}

/// Text set in the type scale.
pub trait Typeset: Styled + Sized {
    /// Size and line height of `role`.
    fn ty(self, role: Type) -> Self {
        self.text_size(len(role.size()))
            .line_height(px(role.line()))
    }

    /// The monospaced family (ids, times, models, error text), with
    /// digits of one width.
    fn mono(self) -> Self {
        self.font_family(font(FontToken::Mono)).tabular_nums()
    }
}

impl<S: Styled> Typeset for S {}
