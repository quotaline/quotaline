//! Reads provider analytics into chart series.

use std::collections::BTreeMap;

use jiff::Timestamp;
use quotaline_analytics as analytics;
use quotaline_analytics::utc_day::{day, midnight, retained_utc_days};
use quotaline_catalog::IDS;
use quotaline_history::Store;

use super::{History, Point, Series};

pub fn history_analytics(
    store: &Store,
    metric: &str,
    start: i64,
    end: i64,
    providers: &BTreeMap<String, bool>,
) -> quotaline_history::Result<History> {
    let (start, end) = (start.min(end), start.max(end));
    let enabled = IDS
        .iter()
        .filter(|id| providers.get(**id) != Some(&false))
        .map(|id| (*id).to_string())
        .collect::<Vec<_>>();
    let mut series = BTreeMap::<String, Series>::new();
    for row in store.analytics_rows(metric, &enabled, &day(start as f64), &day(end as f64), true)? {
        let Some(at) = midnight(&row.day).map(|at| at as i64) else {
            continue;
        };
        if at < start || at >= end {
            continue;
        }
        let id = format!("{}:{}", row.provider, row.series);
        series
            .entry(id.clone())
            .or_insert_with(|| Series {
                id,
                provider: row.provider,
                label: row.series,
                points: Vec::new(),
            })
            .points
            .push(Point(at, row.value, None, false));
    }
    Ok(History {
        metric: metric.into(),
        start,
        end,
        series: series.into_values().collect(),
        canonical_total: None,
    }
    .prepared())
}

/// The `[start, end)` timestamps covering the last `days` UTC days through today.
pub(super) fn retained_range(now: Timestamp, days: u32) -> (i64, i64) {
    let (start, end) = retained_utc_days(now.as_second() as f64, days);
    let midnight = |day: &str| midnight(day).expect("generated day is valid") as i64;
    (midnight(&start), midnight(&end) + 86_400)
}

pub fn history_points(history: &History) -> Vec<analytics::Point> {
    history
        .series
        .iter()
        .flat_map(|series| {
            series.points.iter().map(|point| analytics::Point {
                day: day(point.0 as f64),
                provider: series.provider.clone(),
                metric: history.metric.clone(),
                series: series.label.clone(),
                value: point.1,
            })
        })
        .collect()
}
