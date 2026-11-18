//! Exposes shared review counts to Swift.

use jiff::Timestamp;
use quotaline_analytics as analytics;
use quotaline_analytics::ReviewRow;

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ReviewRowRecord {
    pub day: String,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ReviewCountsRecord {
    pub today: i64,
    pub week: i64,
}

#[uniffi::export]
pub fn review_counts(rows: Vec<ReviewRowRecord>, now: f64) -> Option<ReviewCountsRecord> {
    let now = Timestamp::from_second(now as i64).ok()?;
    analytics::review_counts(
        &rows
            .into_iter()
            .map(|row| ReviewRow {
                day: row.day,
                value: row.value,
            })
            .collect::<Vec<_>>(),
        now,
    )
    .map(|counts| ReviewCountsRecord {
        today: counts.today,
        week: counts.week,
    })
}

#[cfg(test)]
#[path = "../tests/unit/reviews/tests.rs"]
mod tests;
