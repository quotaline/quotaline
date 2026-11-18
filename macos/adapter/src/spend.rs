//! Exposes shared spend aggregation to Swift.

use jiff::Timestamp;
use quotaline_analytics as analytics;

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct SpendDaysRecord {
    pub start: String,
    pub yesterday: String,
    pub today: String,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpendRowRecord {
    pub day: String,
    pub provider: String,
    pub model: String,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpendModelRecord {
    pub provider: String,
    pub model: String,
    pub cost: f64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpendSummaryRecord {
    pub today: f64,
    pub yesterday: f64,
    pub last_window: f64,
    pub models: Vec<SpendModelRecord>,
    pub providers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct SpendReportRecord {
    pub total: SpendSummaryRecord,
    pub by_provider: std::collections::HashMap<String, SpendSummaryRecord>,
}

fn date(now: f64) -> Timestamp {
    Timestamp::from_second(now as i64).expect("supported timestamp")
}

impl From<analytics::SpendSummary> for SpendSummaryRecord {
    fn from(value: analytics::SpendSummary) -> Self {
        Self {
            today: value.today,
            yesterday: value.yesterday,
            last_window: value.last_window,
            models: value
                .models
                .into_iter()
                .map(|model| SpendModelRecord {
                    provider: model.provider,
                    model: model.model,
                    cost: model.cost,
                })
                .collect(),
            providers: value.providers,
        }
    }
}

fn core_rows(rows: Vec<SpendRowRecord>) -> Vec<analytics::SpendRow> {
    rows.into_iter()
        .map(|row| analytics::SpendRow {
            day: row.day,
            provider: row.provider,
            model: row.model,
            value: row.value,
        })
        .collect()
}

#[uniffi::export]
pub fn spend_days(now: f64) -> SpendDaysRecord {
    let days = analytics::spend_days(date(now));
    SpendDaysRecord {
        start: days.start,
        yesterday: days.yesterday,
        today: days.today,
    }
}

#[uniffi::export]
pub fn spend_summary(rows: Vec<SpendRowRecord>, now: f64) -> SpendSummaryRecord {
    analytics::spend_summary(&core_rows(rows), date(now)).into()
}

#[uniffi::export]
pub fn spend_report(rows: Vec<SpendRowRecord>, now: f64) -> SpendReportRecord {
    let report = analytics::spend_report(&core_rows(rows), date(now));
    SpendReportRecord {
        total: report.total.into(),
        by_provider: report
            .by_provider
            .into_iter()
            .map(|(id, summary)| (id, summary.into()))
            .collect(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/spend/tests.rs"]
mod tests;
