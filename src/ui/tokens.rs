//! The design tokens of `quorum/theme.json` as the UI reads them: the
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

/// The width of a rule that marks (a selection's leading bar, a focus
/// ring, a highlighted DAG edge); ordinary borders are 1 pt.
pub fn mark() -> Points {
    len(LengthToken::Mark)
}

/// The opacity of what a selection puts in the background (DAG nodes
/// away from the selected one) and of a disabled control.
pub const DIM: f32 = 0.45;

/// The eight text roles of the type scale (#9 「字号与字重」): each a size
/// token, a line height and a weight. Body text uses 400 and 500 only;
/// 600 is for titles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    /// A page's title: 20/28, 600.
    Title,
    /// A panel's or a section's title: 13/20, 600.
    Heading,
    /// Body text and table cells: 13/20, 400.
    Body,
    /// Emphasis in body text: 13/20, 500.
    BodyStrong,
    /// Captions, meta lines, axis labels: 12/16, 400.
    Small,
    /// Labels, counts, chips, buttons: 12/16, 500.
    Label,
    /// A summary number: 22/28, 500.
    Num,
    /// Ids, times, models, error text, logs: 12/18, 400, monospaced.
    Mono,
}

impl Type {
    fn size(self) -> LengthToken {
        match self {
            Type::Title => LengthToken::TextTitle,
            Type::Heading => LengthToken::TextHeading,
            Type::Body | Type::BodyStrong => LengthToken::TextBody,
            Type::Small | Type::Label => LengthToken::TextSmall,
            Type::Num => LengthToken::TextNum,
            Type::Mono => LengthToken::TextMono,
        }
    }

    /// The line height, in points.
    pub fn line(self) -> f32 {
        match self {
            Type::Title | Type::Num => 28.0,
            Type::Heading | Type::Body | Type::BodyStrong => 20.0,
            Type::Small | Type::Label => 16.0,
            Type::Mono => 18.0,
        }
    }

    fn weight(self) -> FontWeight {
        match self {
            Type::Title | Type::Heading => FontWeight::SEMIBOLD,
            Type::BodyStrong | Type::Label | Type::Num => FontWeight::MEDIUM,
            Type::Body | Type::Small | Type::Mono => FontWeight::NORMAL,
        }
    }
}

/// Text set in the type scale.
pub trait Typeset: Styled + Sized {
    /// Size, line height and weight of `role`; numbers of one width, and
    /// the monospaced family for [`Type::Mono`].
    fn ty(self, role: Type) -> Self {
        let set = self
            .text_size(len(role.size()))
            .line_height(px(role.line()))
            .font_weight(role.weight())
            .tabular_nums();
        match role {
            Type::Mono => set.font_family(font(FontToken::Mono)),
            _ => set,
        }
    }
}

impl<S: Styled> Typeset for S {}
