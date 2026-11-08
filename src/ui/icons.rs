//! Meerkat's own 16 pt line icons (#9 「跨平台的画法」): Lucide's geometry
//! on a 16-unit grid, 1.5 pt strokes, drawn with a `Painter` as the status
//! glyphs are, in the color they sit in. They name pages, actions and
//! event kinds, never a state.

use nsh_core::{
    canvas::{LineCap, LineJoin},
    prelude::*,
};

use super::tokens::len;
use crate::config::LengthToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Overview,
    Graph,
    List,
    Sessions,
    Runtime,
    Config,
    Refresh,
    Search,
    Tool,
    File,
    Message,
    Retry,
    Alert,
    External,
    ChevronRight,
    ChevronDown,
    ArrowRight,
    ArrowLeft,
    Plus,
    Minus,
    Fit,
    /// An implementation, a worker.
    Code,
    /// The council's decision.
    Gavel,
    /// A wait.
    Clock,
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
            Icon::Graph => {
                "M5.3 4a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 1 1 3.6 0z\
                 M5.3 12a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 1 1 3.6 0z\
                 M14.3 8a1.8 1.8 0 1 1-3.6 0a1.8 1.8 0 1 1 3.6 0z\
                 M5.3 4.4l5.4 3M5.3 11.6l5.4-3"
            }
            Icon::List => "M5.5 4h8M5.5 8h8M5.5 12h8M2.5 4h.01M2.5 8h.01M2.5 12h.01",
            Icon::Sessions => "M2.5 3.5h11M2.5 8h11M2.5 12.5h7",
            Icon::Runtime => {
                "M13.5 4c0 1.1-2.5 2-5.5 2s-5.5-.9-5.5-2s2.5-2 5.5-2s5.5.9 5.5 2z\
                 M2.5 4v8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2V4M2.5 8c0 1.1 2.5 2 5.5 2s5.5-.9 5.5-2"
            }
            Icon::Config => {
                "M13.5 3.5h-4.5M6.5 3.5h-4M13.5 8h-6M5 8H2.5M13.5 12.5h-3M8 12.5H2.5\
                 M9 2v3M5 6.5v3M10.5 11v3"
            }
            Icon::Refresh => "M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3",
            Icon::Search => "M1.5 7a5.5 5.5 0 1 0 11 0a5.5 5.5 0 1 0-11 0zM14.5 14.5l-3.6-3.6",
            Icon::Tool => {
                "M10.5 2.5a3 3 0 0 0-3.4 4L2.5 11.1l2.4 2.4 4.6-4.6a3 3 0 0 0 4-3.4l-1.8 1.8-1.8-.4-.4-1.8z"
            }
            Icon::File => "M9.5 2H4.5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h7a1 1 0 0 0 1-1V5zM9.5 2v3h3",
            Icon::Message => "M2.5 3.5h11v7.5h-6.5l-3 2.5v-2.5h-1.5z",
            Icon::Retry => {
                "M2.5 8a5.5 5.5 0 0 1 9.4-3.9l1.6 1.4M13.5 2.5v3h-3\
                 M13.5 8a5.5 5.5 0 0 1-9.4 3.9l-1.6-1.4M2.5 13.5v-3h3"
            }
            Icon::Alert => "M8 2.5l6 10.5H2zM8 6.5v3M8 11.5h.01",
            Icon::External => {
                "M9.5 2.5h4v4M13.5 2.5l-6 6M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3"
            }
            Icon::ChevronRight => "M6 3.5l4.5 4.5L6 12.5",
            Icon::ChevronDown => "M3.5 6l4.5 4.5L12.5 6",
            Icon::ArrowRight => "M3 8h10M9 4l4 4-4 4",
            Icon::ArrowLeft => "M13 8H3M7 4L3 8l4 4",
            Icon::Plus => "M8 3v10M3 8h10",
            Icon::Minus => "M3 8h10",
            Icon::Fit => "M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10",
            Icon::Code => "M5.5 4.5L2 8l3.5 3.5M10.5 4.5L14 8l-3.5 3.5",
            Icon::Gavel => "M9 2.5l4.5 4.5M7.5 4l4.5 4.5M10.5 5.5L4 12M2.5 13.5h6",
            Icon::Clock => "M8 1.5a6.5 6.5 0 1 0 0 13a6.5 6.5 0 1 0 0-13zM8 4.5V8l2.5 1.5",
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
