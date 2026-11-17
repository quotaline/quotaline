//! Hands the shared demo readings and analytics to macOS.

use jiff::Timestamp;

use crate::CodexAnalyticsPoint;
use crate::provider_record::{ProviderReadingRecord, ReadingSource};

#[derive(Debug, PartialEq, Eq, thiserror::Error, uniffi::Error)]
pub enum DemoError {
    #[error("Invalid demo time")]
    InvalidTime,
    #[error("Unknown demo provider")]
    UnknownProvider,
}

#[uniffi::export]
pub fn demo_reading(provider: String, now_seconds: f64, long_text: bool) -> Result<ProviderReadingRecord, DemoError> {
    let at = demo_time(now_seconds)?;
    let provider = quotaline_demo::demo_provider_at(&provider, at, long_text).ok_or(DemoError::UnknownProvider)?;
    Ok(ProviderReadingRecord {
        provider: provider.into(),
        fetched_at: now_seconds,
        source: ReadingSource::Network,
    })
}

#[uniffi::export]
pub fn demo_analytics_points(
    provider: String,
    now_seconds: f64,
    days: u32,
    long_text: bool,
) -> Result<Option<Vec<CodexAnalyticsPoint>>, DemoError> {
    Ok(
        quotaline_demo::demo_analytics_at(&provider, demo_time(now_seconds)?, days, long_text).map(|points| {
            points
                .into_iter()
                .map(|point| CodexAnalyticsPoint {
                    day: point.day,
                    metric: point.metric,
                    series: point.series,
                    value: point.value,
                })
                .collect()
        }),
    )
}

fn demo_time(now_seconds: f64) -> Result<Timestamp, DemoError> {
    Timestamp::from_millisecond((now_seconds * 1000.0) as i64)
        .ok()
        .filter(|_| now_seconds.is_finite())
        .ok_or(DemoError::InvalidTime)
}

#[cfg(test)]
#[path = "../tests/unit/demo/tests.rs"]
mod tests;
