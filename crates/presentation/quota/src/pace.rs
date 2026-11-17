//! Estimates quota pace and projected exhaustion.

use crate::Window;
use jiff::Timestamp;
use jiff::civil::Time;
use jiff::tz::TimeZone;
use serde::{Deserialize, Serialize};

pub const SAMPLE_HORIZON_SECONDS: i64 = 3600;
pub const MIN_WORKDAYS: u8 = 4;
pub const MAX_WORKDAYS: u8 = 7;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PaceStatus {
    Unknown,
    OnTrack,
    Ahead,
    Behind,
    Exhausted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaceEstimate {
    pub status: PaceStatus,
    pub expected_percent: Option<f64>,
    pub ratio: Option<f64>,
    pub projected_exhaustion: Option<i64>,
}

impl PaceEstimate {
    pub fn estimate(window: &Window, samples: &[(i64, f64)], workdays: u8, now: i64) -> Self {
        Self::estimate_in_timezone(window, samples, workdays, now, &TimeZone::system())
    }

    pub fn estimate_in_timezone(
        window: &Window,
        samples: &[(i64, f64)],
        workdays: u8,
        now: i64,
        timezone: &TimeZone,
    ) -> Self {
        if window.used >= 100.0 {
            return Self::new(PaceStatus::Exhausted, None, None, Some(now));
        }
        let reset = window
            .reset
            .as_deref()
            .and_then(|value| value.parse::<Timestamp>().ok())
            .map(Timestamp::as_second);
        let Some((reset, duration)) = reset
            .zip(window.duration_seconds)
            .filter(|(reset, duration)| *reset > now && *duration > 0 && *duration <= 366 * 86_400)
        else {
            return Self::new(PaceStatus::Unknown, None, None, None);
        };
        let start = reset - duration as i64;
        let linear = ((now - start) as f64 / duration as f64).clamp(0.0, 1.0) * 100.0;
        let expected = if workdays < MAX_WORKDAYS && duration > 86_400 {
            let total = workday_seconds(start, reset, workdays, timezone);
            let elapsed = workday_seconds(start, now.min(reset), workdays, timezone);
            total
                .filter(|value| *value > 0.0)
                .zip(elapsed)
                .map(|(total, elapsed)| (elapsed / total).clamp(0.0, 1.0) * 100.0)
                .unwrap_or(linear)
        } else {
            linear
        };
        let recent = samples
            .iter()
            .copied()
            .filter(|(at, _)| *at >= now - SAMPLE_HORIZON_SECONDS && *at <= now);
        let first = recent.clone().min_by_key(|item| item.0);
        let last = recent.max_by_key(|item| item.0);
        let rate = match first.zip(last) {
            Some(((first_at, first_used), (last_at, last_used))) if last_at > first_at && last_used > first_used => {
                (last_used - first_used) / (last_at - first_at) as f64
            }
            _ if window.used > 0.0 && now > start => window.used / (now - start) as f64,
            _ => 0.0,
        };
        let projected = if rate > 0.0 {
            let at = now.saturating_add(((100.0 - window.used) / rate) as i64);
            (at < reset).then_some(at)
        } else {
            None
        };
        if expected < 5.0 {
            return Self::new(PaceStatus::Unknown, Some(expected), None, projected);
        }
        let ratio = window.used / expected;
        let status = if ratio > 1.25 {
            PaceStatus::Ahead
        } else if ratio < 0.75 {
            PaceStatus::Behind
        } else {
            PaceStatus::OnTrack
        };
        Self::new(status, Some(expected), Some(ratio), projected)
    }

    fn new(
        status: PaceStatus,
        expected_percent: Option<f64>,
        ratio: Option<f64>,
        projected_exhaustion: Option<i64>,
    ) -> Self {
        Self {
            status,
            expected_percent,
            ratio,
            projected_exhaustion,
        }
    }
}

fn workday_seconds(start: i64, end: i64, workdays: u8, timezone: &TimeZone) -> Option<f64> {
    let mut cursor = start;
    let mut seconds = 0.0;
    while cursor < end {
        let local = timezone.to_datetime(Timestamp::from_second(cursor).ok()?);
        let midnight = local.date().tomorrow().ok()?.to_datetime(Time::midnight());
        let next = timezone.to_timestamp(midnight).ok()?.as_second();
        let segment = next.min(end) - cursor;
        if local.weekday().to_monday_zero_offset() < workdays as i8 {
            seconds += segment as f64;
        }
        cursor += segment;
    }
    Some(seconds)
}

#[cfg(test)]
#[path = "../tests/unit/pace/tests.rs"]
mod tests;
