//! The dashboard's colors. Surfaces, text and borders are the NSH theme's
//! tokens (`nsh_base::theme()`); the status hues are the dashboard's own
//! roles, defined only here: Todo grey, In progress blue, Done green,
//! Superseded purple, failed red, stale or waiting amber, human issues
//! pink.

use nsh_core::{element::Rgba, prelude::*};

/// A color for each appearance.
const fn tone(light: Rgba, dark: Rgba) -> AdaptiveColor {
    AdaptiveColor {
        light: light.to_hsla(),
        dark: dark.to_hsla(),
    }
}

/// Not started; neutral counts.
pub const TODO: AdaptiveColor = tone(rgb(0x6b7280), rgb(0x9ca3af));
/// Under way.
pub const IN_PROGRESS: AdaptiveColor = tone(rgb(0x2563eb), rgb(0x60a5fa));
/// Finished well.
pub const DONE: AdaptiveColor = tone(rgb(0x16a34a), rgb(0x4ade80));
/// Replaced by something else.
pub const SUPERSEDED: AdaptiveColor = tone(rgb(0x7c3aed), rgb(0xa78bfa));
/// Failed or unreachable.
pub const FAILED: AdaptiveColor = tone(rgb(0xdc2626), rgb(0xf87171));
/// Stale, or waiting on something.
pub const WAITING: AdaptiveColor = tone(rgb(0xd97706), rgb(0xfbbf24));
/// A human issue.
pub const HUMAN: AdaptiveColor = tone(rgb(0xdb2777), rgb(0xf472b6));
