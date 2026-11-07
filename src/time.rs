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

/// How long before `now` (Unix milliseconds) `at` was: `刚刚`, `12 秒前`,
/// `5 分钟前`, `3 小时前`, `2 天前`.
pub fn ago(at: f64, now: f64) -> String {
    let seconds = ((now - at) / 1000.0).max(0.0) as u64;
    match seconds {
        0..5 => "刚刚".to_owned(),
        5..60 => format!("{seconds} 秒前"),
        60..3600 => format!("{} 分钟前", seconds / 60),
        3600..86400 => format!("{} 小时前", seconds / 3600),
        _ => format!("{} 天前", seconds / 86400),
    }
}

/// A duration in milliseconds as the logs show it: `0.4s`, `8s`,
/// `4m 51s`, `1h 05m`.
pub fn span(ms: f64) -> String {
    let ms = ms.max(0.0);
    if ms < 1000.0 {
        return format!("{:.1}s", ms / 1000.0);
    }
    let seconds = (ms / 1000.0) as u64;
    match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60),
    }
}

/// `HH:MM:SS.mmm` of an RFC 3339 timestamp, or the text as it is.
pub fn precise(text: &str) -> String {
    match parse(text) {
        Some(at) => at
            .to_zoned(TimeZone::system())
            .strftime("%H:%M:%S%.3f")
            .to_string(),
        None => text.to_owned(),
    }
}
