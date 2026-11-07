//! The few line icons besides the status glyphs (the mock's `I`): the
//! pages in the sidebar, refresh, a tool call. Drawn from the
//! mock's SVG paths on a 16-unit grid, in the text color they sit in.

use nsh_core::{
    canvas::{LineCap, LineJoin},
    prelude::*,
};

use super::tokens::len;
use crate::config::LengthToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Overview,
    Issues,
    Bases,
    Sessions,
    Refresh,
    Tool,
}

impl Icon {
    /// SVG path data on the 16-unit grid, stroked.
    fn path(self) -> &'static str {
        match self {
            Icon::Overview => {
                "M3.2 2h2.6a1.2 1.2 0 0 1 1.2 1.2v2.6a1.2 1.2 0 0 1-1.2 1.2H3.2A1.2 1.2 0 0 1 2 5.8V3.2A1.2 1.2 0 0 1 3.2 2z\
                 M10.2 2h2.6a1.2 1.2 0 0 1 1.2 1.2v2.6a1.2 1.2 0 0 1-1.2 1.2h-2.6A1.2 1.2 0 0 1 9 5.8V3.2A1.2 1.2 0 0 1 10.2 2z\
                 M3.2 9h2.6a1.2 1.2 0 0 1 1.2 1.2v2.6a1.2 1.2 0 0 1-1.2 1.2H3.2A1.2 1.2 0 0 1 2 12.8v-2.6A1.2 1.2 0 0 1 3.2 9z\
                 M10.2 9h2.6a1.2 1.2 0 0 1 1.2 1.2v2.6a1.2 1.2 0 0 1-1.2 1.2h-2.6A1.2 1.2 0 0 1 9 12.8v-2.6A1.2 1.2 0 0 1 10.2 9z"
            }
            Icon::Issues => {
                "M5.3 4a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 1 1 3.6 0z\
                 M5.3 12a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 1 1 3.6 0z\
                 M14.3 8a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 1 1 3.6 0z\
                 M5.3 4.4l5.4 3M5.3 11.6l5.4-3"
            }
            Icon::Bases => {
                "M13.5 4c0 1.1-2.5 2-5.5 2s-5.5-.9-5.5-2s2.5-2 5.5-2s5.5.9 5.5 2z\
                 M2.5 4v8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2V4M2.5 8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2"
            }
            Icon::Sessions => "M2.5 3.5h11M2.5 8h11M2.5 12.5h7",
            Icon::Refresh => "M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3",
            Icon::Tool => {
                "M10.5 2.5a3 3 0 0 0-3.4 4L2.5 11.1l2.4 2.4 4.6-4.6a3 3 0 0 0 4-3.4l-1.8 1.8-1.8-.4-.4-1.8z"
            }
        }
    }
}

/// `icon` at the size of a `space-4`, stroked in `ink`.
pub fn icon(icon: Icon, ink: impl Fn() -> AdaptiveColor + 'static) -> CanvasView {
    let size = len(LengthToken::Space4);
    canvas(move || {
        let ink = ink();
        let k = size.0 / 16.0;
        Painter::new()
            .scale(k, k)
            .stroke_color(if dark() { ink.dark } else { ink.light })
            .line_width(1.5)
            .line_cap(LineCap::Round)
            .line_join(LineJoin::Round)
            .stroke_svg(icon.path())
    })
    .size(size)
    .flex_shrink_0()
}
