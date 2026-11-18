//! Reads review history and projects shared daily counts.

use std::collections::BTreeMap;

use jiff::Timestamp;
use quotaline_analytics as analytics;
use quotaline_history::Store;

use super::analytics::retained_range;
use super::{History, history_analytics, history_points};

pub fn review_counts(
    store: &Store,
    providers: &BTreeMap<String, bool>,
    retention_days: u32,
    now: Timestamp,
) -> quotaline_history::Result<Option<analytics::ReviewCounts>> {
    let (start, end) = retained_range(now, retention_days);
    let history = history_analytics(store, "codeReviews", start, end, providers)?;
    Ok(counts_from_history(&history, now))
}

fn counts_from_history(history: &History, now: Timestamp) -> Option<analytics::ReviewCounts> {
    let rows = history_points(history)
        .into_iter()
        .filter(|point| point.provider == "codex")
        .map(|point| analytics::ReviewRow {
            day: point.day,
            value: point.value,
        })
        .collect::<Vec<_>>();
    analytics::review_counts(&rows, now)
}

#[cfg(test)]
#[path = "../../tests/unit/history/reviews/tests.rs"]
mod tests;
