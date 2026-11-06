//! Times as the dashboard shows them: in the system's time zone (jiff's;
//! in the browser it asks `Intl` through jiff's `js` feature, with the zone
//! database bundled).

use jiff::{Timestamp, tz::TimeZone};

/// Milliseconds since the Unix epoch, now (the engine's clock).
pub fn now_ms() -> f64 {
    nsh_core::time::unix_ms()
}

/// An RFC 3339 timestamp (`2026-11-06T22:05:21.335Z`).
pub fn parse(text: &str) -> Option<Timestamp> {
    text.parse().ok()
}

/// Milliseconds since the epoch of an RFC 3339 timestamp.
pub fn parse_ms(text: &str) -> Option<f64> {
    parse(text).map(|at| at.as_millisecond() as f64)
}

/// `HH:MM:SS` of a time in Unix milliseconds.
pub fn clock(ms: f64) -> String {
    Timestamp::from_millisecond(ms as i64)
        .map(|at| at.to_zoned(TimeZone::system()).strftime("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// `MM-DD HH:MM` of an RFC 3339 timestamp, or the text as it is.
pub fn short(text: &str) -> String {
    match parse(text) {
        Some(at) => at
            .to_zoned(TimeZone::system())
            .strftime("%m-%d %H:%M")
            .to_string(),
        None => text.to_owned(),
    }
}
