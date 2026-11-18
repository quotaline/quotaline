//! Converts between Unix timestamps and the `YYYY-MM-DD` UTC days that key daily analytics.

use jiff::civil::{Date, Time};
use jiff::tz::Offset;
use jiff::{Timestamp, ToSpan};

fn date(at: f64) -> Date {
    let timestamp = Timestamp::from_second(at.floor() as i64).unwrap_or(Timestamp::UNIX_EPOCH);
    Offset::UTC.to_datetime(timestamp).date()
}

pub fn day(at: f64) -> String {
    date(at).to_string()
}

pub fn midnight(day: &str) -> Option<f64> {
    let date = Date::strptime("%Y-%m-%d", day).ok()?;
    Offset::UTC
        .to_timestamp(date.to_datetime(Time::midnight()))
        .ok()
        .map(|timestamp| timestamp.as_second() as f64)
}

pub fn retained_utc_days(now: f64, retention_days: u32) -> (String, String) {
    let end = date(now);
    let start = end - i64::from(retention_days.saturating_sub(1)).days();
    (start.to_string(), end.to_string())
}

#[cfg(test)]
#[path = "../tests/unit/utc_day/tests.rs"]
mod tests;
