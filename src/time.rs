//! Times as Quorum shows them: in the system's time zone (jiff's;
//! in the browser it asks `Intl` through jiff's `js` feature, with the zone
//! database bundled).

use jiff::{Timestamp, tz::TimeZone};

/// Milliseconds since the Unix epoch, now (the engine's clock).
pub fn now_ms() -> f64 {
    nsh_core::time::unix_ms()
}

/// The instant `ms` milliseconds after the Unix epoch.
pub fn instant(ms: f64) -> Timestamp {
    Timestamp::from_millisecond(ms as i64).unwrap_or(Timestamp::UNIX_EPOCH)
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
        .map(|at| {
            at.to_zoned(TimeZone::system())
                .strftime("%H:%M:%S")
                .to_string()
        })
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

/// How long before `now` (Unix milliseconds) `at` was, in the largest
/// whole unit (the UI words it: 「刚刚」, 「12 秒前」, 「3 小时」).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ago {
    /// Under five seconds.
    Now,
    Seconds(u64),
    Minutes(u64),
    Hours(u64),
    Days(u64),
}

pub fn ago(at: f64, now: f64) -> Ago {
    let seconds = ((now - at) / 1000.0).max(0.0) as u64;
    match seconds {
        0..5 => Ago::Now,
        5..60 => Ago::Seconds(seconds),
        60..3600 => Ago::Minutes(seconds / 60),
        3600..86400 => Ago::Hours(seconds / 3600),
        _ => Ago::Days(seconds / 86400),
    }
}

/// `HH:MM` of a time in Unix milliseconds.
pub fn hm(ms: f64) -> String {
    Timestamp::from_millisecond(ms as i64)
        .map(|at| {
            at.to_zoned(TimeZone::system())
                .strftime("%H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// `MM-DD HH:MM:SS` of an RFC 3339 timestamp, or the text as it is.
pub fn seconds(text: &str) -> String {
    match parse(text) {
        Some(at) => at
            .to_zoned(TimeZone::system())
            .strftime("%m-%d %H:%M:%S")
            .to_string(),
        None => text.to_owned(),
    }
}

/// How many of `times` (Unix milliseconds) fall in each of the `hours`
/// hours up to `now`, oldest first.
pub fn hourly(times: impl IntoIterator<Item = f64>, now: f64, hours: usize) -> Vec<u32> {
    let mut counts = vec![0; hours];
    for at in times {
        let back = ((now - at) / 3_600_000.0).floor();
        if back >= 0.0 && (back as usize) < hours {
            counts[hours - 1 - back as usize] += 1;
        }
    }
    counts
}
/// A duration as the logs write it: `0.4s`, `8s`, `4m 51s`, `1h 05m`
/// (the UI words it, [`crate::Msg::Took`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Took {
    /// Under a second, in tenths.
    Tenths(u64),
    Seconds(u64),
    /// Minutes and the seconds past them.
    Minutes(u64, u64),
    /// Hours and the minutes past them.
    Hours(u64, u64),
}

/// A duration in milliseconds, in the largest units the logs use.
pub fn took(ms: f64) -> Took {
    let ms = ms.max(0.0);
    if ms < 1000.0 {
        return Took::Tenths((ms / 100.0).round() as u64);
    }
    let seconds = (ms / 1000.0) as u64;
    match seconds {
        0..60 => Took::Seconds(seconds),
        60..3600 => Took::Minutes(seconds / 60, seconds % 60),
        _ => Took::Hours(seconds / 3600, seconds % 3600 / 60),
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
