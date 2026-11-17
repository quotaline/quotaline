//! Normalizes daily provider analytics into chart metrics.

use jiff::Timestamp;
use jiff::civil::Date;
use quotaline_text::json::number;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

mod history;
mod merge;
pub use merge::{AnalyticsSnapshot, CoverageScope, merge_analytics};
mod navigation;
mod scope;
mod selection;
mod visibility;
pub use history::{
    CHANGE_THRESHOLD, HistoryProjection, HistoryReading, SAMPLE_INTERVAL, UsageSample, history_projection,
    should_record_usage,
};
pub use navigation::{
    HISTORY_MAX_QUERY_POINTS, HistoryAction, HistoryNavigation, HistoryRange, HistorySelection, HistorySession,
    HistorySessionConfig, HistorySessionView, HistoryViewport, NavigationState, SavedHistory, history_page,
    history_pause, history_query_rollup, history_reset, history_select_custom_end, history_select_custom_start,
    history_select_period, history_select_rollup, history_session_from_saved, history_viewport, sync_saved_history,
};
pub use scope::{ScopeKey, scoped_indices};
pub use selection::step_reading;
pub use visibility::{HistoryVisibility, VisibilityAction};
mod chart;
pub use chart::{downsample, insert_reset_zeros};
mod claude;
pub use claude::{DayTotal, ModelTotal, ProjectTotal, claude_points};
mod metadata;
pub use metadata::{MetricDefinition, metric_definitions};
mod series;
pub use series::{AnalyticsChart, AnalyticsChartPoint, AnalyticsChartSeries, analytics_chart};
mod daily;
pub use daily::{DailyBucket, daily_buckets};
mod window;
pub use window::{
    WindowChart, WindowChartPoint, WindowChartRequest, WindowChartSeries, WindowResetEvent, WindowRollup, WindowSample,
    bucket_key, offset_at, window_change_points, window_chart, window_clip, window_extend_to, window_nearest,
    window_stack,
};
mod budget;
pub use budget::point_limits;
mod codex_batch;
pub use codex_batch::{CodexBatchPlan, CodexBatchResult, EndpointResponse, codex_batch};
mod spend;
pub use spend::{SpendDays, SpendModel, SpendReport, SpendRow, SpendSummary, spend_days, spend_report, spend_summary};
mod reviews;
pub use reviews::{ReviewCounts, ReviewRow, review_counts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum Endpoint {
    TokenUsage,
    WorkspaceCounts,
    Skills,
    Plugins,
    CodeReview,
}

impl Endpoint {
    pub const ALL: [Self; 5] = [
        Self::TokenUsage,
        Self::WorkspaceCounts,
        Self::Skills,
        Self::Plugins,
        Self::CodeReview,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::TokenUsage => "tokenUsage",
            Self::WorkspaceCounts => "workspaceCounts",
            Self::Skills => "skills",
            Self::Plugins => "plugins",
            Self::CodeReview => "codeReview",
        }
    }

    pub fn path(self) -> &'static str {
        match self {
            Self::TokenUsage => "wham/usage/daily-token-usage-breakdown",
            Self::WorkspaceCounts => "wham/analytics/daily-workspace-usage-counts",
            Self::Skills => "wham/analytics/daily-skill-usage-metrics",
            Self::Plugins => "wham/analytics/daily-plugin-usage-metrics",
            Self::CodeReview => "wham/analytics/daily-code-review-metrics",
        }
    }

    pub fn metrics(self) -> &'static [&'static str] {
        match self {
            Self::TokenUsage => &["surfaceUsagePercent", "modelCredits"],
            Self::WorkspaceCounts => &[
                "inputTokens",
                "cachedInputTokens",
                "outputTokens",
                "turns",
                "threads",
                "credits",
            ],
            Self::Skills => &["skillInvocations"],
            Self::Plugins => &["pluginInvocations"],
            Self::CodeReview => &["codeReviews"],
        }
    }

    pub fn query(self, start: &str, end: &str) -> Vec<(&'static str, String)> {
        let mut query = vec![
            ("start_date", start.into()),
            ("end_date", end.into()),
            ("group_by", "day".into()),
        ];
        if self != Self::TokenUsage {
            query.push(("workspace_user", "true".into()));
        }
        match self {
            Self::Skills => query.push(("top_skill_limit", "20".into())),
            Self::Plugins => query.push(("top_plugin_limit", "20".into())),
            _ => {}
        }
        query
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Point {
    pub day: String,
    pub provider: String,
    pub metric: String,
    pub series: String,
    pub value: f64,
}

pub struct Batch {
    pub endpoint: Endpoint,
    pub account_fingerprint: String,
    pub start: String,
    pub end: String,
    pub points: Vec<Point>,
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "StoredCreditEvent"))]
pub struct CreditEvent {
    pub id: String,
    pub timestamp: f64,
    pub service: String,
    pub credits_used: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreditEvents {
    pub events: Vec<CreditEvent>,
    pub missing_amount_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodexPayloadError {
    InvalidJson,
    MissingData,
}

fn codex_document(payload: &[u8]) -> Result<Value, CodexPayloadError> {
    let document: Value = serde_json::from_slice(payload).map_err(|_| CodexPayloadError::InvalidJson)?;
    codex_data(&document)?;
    Ok(document)
}

pub fn codex_endpoint_batch(
    endpoint: Endpoint,
    document: &Value,
    account_fingerprint: &str,
    start: &str,
    end: &str,
) -> Result<Batch, CodexPayloadError> {
    let rows = codex_data(document)?;
    Ok(Batch {
        endpoint,
        account_fingerprint: account_fingerprint.into(),
        start: start.into(),
        end: end.into(),
        points: codex_points(endpoint, rows)
            .into_iter()
            .filter(|point| point.day.as_str() >= start && point.day.as_str() <= end)
            .collect(),
    })
}

fn codex_data(document: &Value) -> Result<&[Value], CodexPayloadError> {
    document["data"]
        .as_array()
        .map(Vec::as_slice)
        .ok_or(CodexPayloadError::MissingData)
}

pub const METRICS: [(&str, &str, &str, &str); 19] = [
    (
        "surfaceUsagePercent",
        "Usage by surface",
        "%",
        "Codex · by surface · daily UTC",
    ),
    (
        "modelCredits",
        "Credits by model",
        "credits",
        "Codex · by model · daily UTC",
    ),
    ("turns", "Turns", "count", "Codex · by model and surface · daily UTC"),
    (
        "threads",
        "Threads",
        "count",
        "Codex · by model and surface · daily UTC",
    ),
    (
        "credits",
        "Credits",
        "credits",
        "Codex · by model and surface · daily UTC",
    ),
    (
        "creditBalance",
        "Credit balance",
        "credits",
        "Codex · last observed balance · daily UTC",
    ),
    (
        "inputTokens",
        "Input tokens",
        "tokens",
        "Claude + Codex · Claude by model, Codex total · daily UTC",
    ),
    (
        "cachedInputTokens",
        "Cached input tokens",
        "tokens",
        "Claude + Codex · Claude by model, Codex total · daily UTC",
    ),
    (
        "outputTokens",
        "Output tokens",
        "tokens",
        "Claude + Codex · Claude by model, Codex total · daily UTC",
    ),
    (
        "skillInvocations",
        "Skills used",
        "count",
        "Codex · by skill · daily UTC",
    ),
    (
        "pluginInvocations",
        "Plugin calls",
        "count",
        "Codex · by plugin · daily UTC",
    ),
    (
        "codeReviews",
        "Code reviews",
        "count",
        "Codex · by review type · daily UTC",
    ),
    (
        "cacheWriteTokens",
        "Cache write tokens",
        "tokens",
        "Claude · by model · daily UTC",
    ),
    ("costUSD", "API-equivalent cost", "USD", "Claude · by model · daily UTC"),
    ("messages", "Messages", "count", "Claude · one series · daily UTC"),
    ("sessions", "Sessions", "count", "Claude · one series · daily UTC"),
    ("toolCalls", "Tool calls", "count", "Claude · one series · daily UTC"),
    (
        "projectCost",
        "Cost by project",
        "USD",
        "Claude · by project · daily UTC",
    ),
    (
        "projectMessages",
        "Messages by project",
        "count",
        "Claude · by project · daily UTC",
    ),
];

pub const SHARED_METRICS: [&str; 3] = ["inputTokens", "cachedInputTokens", "outputTokens"];

pub const METRIC_GROUPS: [(&str, &[&str]); 5] = [
    ("Windows", &["usage"]),
    ("Claude and Codex", &SHARED_METRICS),
    (
        "Claude",
        &["cacheWriteTokens", "costUSD", "messages", "sessions", "toolCalls"],
    ),
    (
        "Codex",
        &[
            "surfaceUsagePercent",
            "modelCredits",
            "turns",
            "threads",
            "credits",
            "creditBalance",
            "skillInvocations",
            "pluginInvocations",
            "codeReviews",
        ],
    ),
    ("Projects", &["projectCost", "projectMessages"]),
];

fn codex_points(endpoint: Endpoint, rows: &[Value]) -> Vec<Point> {
    let mut points = Vec::new();
    for row in rows {
        let Some(day) = row["date"]
            .as_str()
            .filter(|day| Date::strptime("%Y-%m-%d", day).is_ok())
        else {
            continue;
        };
        match endpoint {
            Endpoint::TokenUsage => {
                if let Some(surfaces) = row["product_surface_usage_values"].as_object() {
                    for (series, value) in surfaces {
                        push(&mut points, day, "surfaceUsagePercent", series, value);
                    }
                }
                for model in row["models"].as_array().into_iter().flatten() {
                    if let Some(series) = model["model"].as_str() {
                        push(&mut points, day, "modelCredits", series, &model["credits"]);
                    }
                }
            }
            Endpoint::WorkspaceCounts => {
                for (field, metric) in [
                    ("uncached_text_input_tokens", "inputTokens"),
                    ("cached_text_input_tokens", "cachedInputTokens"),
                    ("text_output_tokens", "outputTokens"),
                    ("turns", "turns"),
                    ("threads", "threads"),
                    ("credits", "credits"),
                ] {
                    push(&mut points, day, metric, "total", &row["totals"][field]);
                }
                for (collection, prefix, key) in [("models", "model", "model"), ("clients", "surface", "client_id")] {
                    for entry in row[collection].as_array().into_iter().flatten() {
                        let Some(name) = entry[key].as_str() else {
                            continue;
                        };
                        let series = format!("{prefix}:{name}");
                        for (field, metric) in [("turns", "turns"), ("threads", "threads"), ("credits", "credits")] {
                            push(&mut points, day, metric, &series, &entry[field]);
                        }
                    }
                }
            }
            Endpoint::Skills => {
                for skill in row["skill_usage_overviews"].as_array().into_iter().flatten() {
                    if let Some(name) = skill["display_name"].as_str().or_else(|| skill["skill_name"].as_str()) {
                        push(&mut points, day, "skillInvocations", name, &skill["invocation_counts"]);
                    }
                }
            }
            Endpoint::Plugins => {
                let mut totals = BTreeMap::<String, f64>::new();
                for plugin in row["plugin_usage_overviews"].as_array().into_iter().flatten() {
                    if let Some((name, count)) = plugin["plugin_name"]
                        .as_str()
                        .or_else(|| plugin["display_name"].as_str())
                        .zip(number(&plugin["invocation_counts"]))
                    {
                        *totals.entry(name.into()).or_default() += count;
                    }
                }
                for (name, count) in totals {
                    points.push(Point {
                        day: day.into(),
                        provider: "codex".into(),
                        metric: "pluginInvocations".into(),
                        series: name,
                        value: count,
                    });
                }
            }
            Endpoint::CodeReview => {
                for (key, value) in row.as_object().unwrap() {
                    if key != "date" {
                        push(&mut points, day, "codeReviews", key, value);
                    }
                }
            }
        }
    }
    points
}

pub fn codex_credit_events(payload: &Value) -> CreditEvents {
    let mut events = Vec::new();
    let mut missing_amount_count = 0;
    for (index, row) in payload["data"].as_array().into_iter().flatten().enumerate() {
        let Some(date_text) = ["date", "created_at", "timestamp"]
            .iter()
            .find_map(|key| row[key].as_str())
        else {
            continue;
        };
        let Some(timestamp) = date_text
            .parse::<Timestamp>()
            .ok()
            .map(|date| date.as_millisecond() as f64 / 1000.0)
            .or_else(|| utc_day::midnight(date_text))
        else {
            continue;
        };
        let Some(credits_used) = ["credit_amount", "credits_used", "credits", "amount"]
            .iter()
            .find_map(|key| number(&row[key]))
        else {
            missing_amount_count += 1;
            continue;
        };
        events.push(CreditEvent {
            id: row["usage_id"]
                .as_str()
                .or_else(|| row["id"].as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{date_text}-{index}")),
            timestamp,
            service: ["product_surface", "service", "product"]
                .iter()
                .find_map(|key| row[key].as_str())
                .unwrap_or("Codex")
                .into(),
            credits_used,
        });
    }
    CreditEvents {
        events,
        missing_amount_count,
    }
}

fn push(points: &mut Vec<Point>, day: &str, metric: &str, series: &str, value: &Value) {
    if let Some(value) = number(value) {
        points.push(Point {
            day: day.into(),
            provider: "codex".into(),
            metric: metric.into(),
            series: series.into(),
            value,
        });
    }
}

pub mod project;
pub mod utc_day;
pub mod watermark;

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
