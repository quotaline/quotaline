//! Converts the ISO timestamps used by provider credentials and history, and words times for display.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

mod text;
pub use text::{ClockFormat, compact_countdown, countdown, relative_age, remaining, reset_clock};

/// Whether a reset time is exact or only known to the day.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ResetPrecision {
    #[default]
    Instant,
    Day,
}

pub fn parse_iso_epoch(value: &str) -> Option<f64> {
    value
        .parse::<Timestamp>()
        .ok()
        .map(|value| value.as_millisecond() as f64 / 1000.0)
}

pub fn format_iso_epoch(epoch: f64) -> Option<String> {
    if !epoch.is_finite() {
        return None;
    }
    Timestamp::from_millisecond((epoch * 1000.0).round() as i64)
        .ok()
        .map(|value| value.strftime("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
}

#[cfg(test)]
#[path = "../tests/unit/time/tests.rs"]
mod tests;
