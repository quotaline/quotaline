//! Exposes shared pace estimates to the macOS UI.

use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use quotaline_quota::Window;
use quotaline_quota::pace::PaceEstimate;
use quotaline_quota::pace::PaceStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct PaceConfigurationRecord {
    pub sample_horizon_seconds: i64,
    pub min_workdays: u8,
    pub max_workdays: u8,
}

#[uniffi::export]
pub fn pace_configuration() -> PaceConfigurationRecord {
    PaceConfigurationRecord {
        sample_horizon_seconds: quotaline_quota::pace::SAMPLE_HORIZON_SECONDS,
        min_workdays: quotaline_quota::pace::MIN_WORKDAYS,
        max_workdays: quotaline_quota::pace::MAX_WORKDAYS,
    }
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PaceWindowRecord {
    pub used: f64,
    pub reset_at: Option<f64>,
    pub duration: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PaceSampleRecord {
    pub at: f64,
    pub used: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum PaceStatusRecord {
    Unknown,
    OnTrack,
    Ahead,
    Behind,
    Exhausted,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct PaceEstimateRecord {
    pub status: PaceStatusRecord,
    pub expected_percent: Option<f64>,
    pub ratio: Option<f64>,
    pub projected_exhaustion: Option<f64>,
}

#[uniffi::export]
pub fn pace_estimate(
    window: PaceWindowRecord,
    samples: Vec<PaceSampleRecord>,
    workdays: u8,
    now: f64,
    timezone_name: String,
    timezone_offset_seconds: i32,
) -> PaceEstimateRecord {
    let reset = window
        .reset_at
        .and_then(|seconds| Timestamp::from_second(seconds as i64).ok())
        .map(|date| date.display_with_offset(Offset::UTC).to_string());
    let mut core_window = Window::new("pace", "Pace", window.used, reset, "weekly");
    core_window.duration_seconds = window.duration.map(|seconds| seconds as u64);
    let samples = samples
        .into_iter()
        .map(|sample| (sample.at as i64, sample.used))
        .collect::<Vec<_>>();
    let timezone = TimeZone::get(&timezone_name)
        .unwrap_or_else(|_| TimeZone::fixed(Offset::from_seconds(timezone_offset_seconds).unwrap_or(Offset::UTC)));
    let estimate = PaceEstimate::estimate_in_timezone(&core_window, &samples, workdays, now as i64, &timezone);
    PaceEstimateRecord {
        status: match estimate.status {
            PaceStatus::Unknown => PaceStatusRecord::Unknown,
            PaceStatus::OnTrack => PaceStatusRecord::OnTrack,
            PaceStatus::Ahead => PaceStatusRecord::Ahead,
            PaceStatus::Behind => PaceStatusRecord::Behind,
            PaceStatus::Exhausted => PaceStatusRecord::Exhausted,
        },
        expected_percent: estimate.expected_percent,
        ratio: estimate.ratio,
        projected_exhaustion: estimate.projected_exhaustion.map(|at| at as f64),
    }
}

#[cfg(test)]
#[path = "../tests/unit/pace/tests.rs"]
mod tests;
