//! The dashboard's screens, its design tokens, status vocabulary and its
//! own component library.

mod guardians;
mod icons;
mod issues;
mod overview;
mod parts;
mod sessions;
mod shell;
mod status;
mod tokens;

pub use shell::{app, config_error};
pub use tokens::of as tokens;
