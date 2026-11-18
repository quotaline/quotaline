//! Prepares GNOME history series for Shell charts.

use std::collections::{BTreeMap, BTreeSet};

use std::sync::Arc;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use quotaline_analytics as analytics;
use quotaline_analytics::{WindowChartRequest, WindowRollup};
use quotaline_history::Store;
use quotaline_history::chart::HistoryChartSession;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
/// A chart point: when, the value, the window's reset, and whether it only carries a reading to the period edge.
pub struct Point(pub i64, pub f64, pub Option<f64>, pub bool);
#[derive(Serialize)]
pub struct Series {
    pub id: String,
    pub provider: String,
    pub label: String,
    pub points: Vec<Point>,
}
#[derive(Serialize)]
pub struct History {
    pub metric: String,
    pub start: i64,
    pub end: i64,
    pub series: Vec<Series>,
    #[serde(rename = "canonicalTotal", skip_serializing_if = "Option::is_none")]
    pub canonical_total: Option<f64>,
}

impl History {
    pub fn retain_selected(&mut self, selected: Option<&BTreeSet<String>>) {
        if let Some(selected) = selected {
            self.series.retain(|series| selected.contains(&series.id));
        }
    }

    pub fn prepared(mut self) -> Self {
        self.series.retain(|item| !item.points.is_empty());
        if !matches!(self.metric.as_str(), "turns" | "threads" | "credits") {
            return self;
        }
        let readings = self
            .series
            .iter()
            .flat_map(|series| {
                series.points.iter().map(|point| analytics::HistoryReading {
                    provider: series.provider.clone(),
                    bucket: point.0.to_string(),
                    series: series.label.clone(),
                    value: point.1,
                })
            })
            .collect::<Vec<_>>();
        let projection = analytics::history_projection(&readings);
        self.canonical_total = Some(projection.canonical_total);
        for series in &mut self.series {
            if series.label == "total" {
                series.points.retain(|point| {
                    !projection
                        .detailed_buckets
                        .contains(&(series.provider.clone(), point.0.to_string()))
                });
            }
        }
        self.series.retain(|item| !item.points.is_empty());
        self
    }
}

#[cfg(test)]
#[path = "../../tests/unit/history/chart/tests.rs"]
mod tests;

// jiff's system zone honours TZ before /etc/localtime, so the bucket boundaries must name the same zone.
fn local_zone() -> String {
    std::env::var("TZ")
        .ok()
        .map(|zone| zone.trim_start_matches(':').to_owned())
        .filter(|zone| !zone.is_empty())
        .or_else(|| iana_time_zone::get_timezone().ok())
        .unwrap_or_default()
}

pub fn history(
    store: Arc<Store>,
    start: i64,
    end: i64,
    providers: &BTreeMap<String, bool>,
    rollup: WindowRollup,
    utc: bool,
) -> quotaline_history::Result<History> {
    let (start, end) = (start.min(end), start.max(end));
    let labels = store
        .summaries()?
        .into_iter()
        .filter(|summary| {
            summary
                .key
                .split_once(':')
                .is_some_and(|(provider, _)| providers.get(provider) != Some(&false))
        })
        .map(|summary| (summary.key, summary.label))
        .collect::<BTreeMap<_, _>>();
    let keys = labels.keys().cloned().collect::<Vec<_>>();
    // The same projection macOS draws: reset drops, clipping to the period, change points, and the line to now.
    let request = WindowChartRequest {
        visible: keys.clone(),
        keys,
        start: start as f64,
        end: end as f64,
        now: Timestamp::now().as_millisecond() as f64 / 1000.0,
        rollup,
        stacked: false,
        time_zone: if utc { "UTC".to_string() } else { local_zone() },
        fallback_offset: if utc {
            0
        } else {
            TimeZone::system().to_offset(Timestamp::now()).seconds()
        },
        includes_end: true,
    };
    let chart = HistoryChartSession::new(store, 0).window(request)?;
    Ok(History {
        metric: "usage".into(),
        start,
        end,
        series: chart
            .series
            .into_iter()
            .map(|line| Series {
                provider: line.key.split_once(':').expect("chart keys name a provider").0.into(),
                label: labels[&line.key].clone(),
                points: line
                    .points
                    .into_iter()
                    .map(|point| Point(point.at as i64, point.value, point.reset_at, point.carried))
                    .collect(),
                id: line.key,
            })
            .collect(),
        canonical_total: None,
    }
    .prepared())
}
