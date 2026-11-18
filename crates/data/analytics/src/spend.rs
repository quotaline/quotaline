//! Aggregates daily provider costs for both desktop frontends.

use jiff::civil::Date;
use jiff::tz::Offset;
use jiff::{Timestamp, ToSpan};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

pub const WINDOW_DAYS: i64 = 30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpendDays {
    pub start: String,
    pub yesterday: String,
    pub today: String,
}

pub fn spend_days(now: Timestamp) -> SpendDays {
    let today = Offset::UTC.to_datetime(now).date();
    SpendDays {
        start: today
            .checked_sub((WINDOW_DAYS - 1).days())
            .expect("valid UTC day")
            .to_string(),
        yesterday: today.yesterday().expect("valid UTC day").to_string(),
        today: today.to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpendRow {
    pub day: String,
    pub provider: String,
    pub model: String,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpendModel {
    pub provider: String,
    pub model: String,
    pub cost: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpendSummary {
    pub today: f64,
    pub yesterday: f64,
    pub last_window: f64,
    pub models: Vec<SpendModel>,
    pub providers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SpendReport {
    pub total: SpendSummary,
    pub by_provider: BTreeMap<String, SpendSummary>,
}

pub fn spend_summary(rows: &[SpendRow], now: Timestamp) -> SpendSummary {
    let days = spend_days(now);
    let mut totals = (0.0, 0.0, 0.0);
    let mut models = BTreeMap::<(String, String), f64>::new();
    let mut providers = BTreeSet::new();
    for row in rows {
        if Date::strptime("%Y-%m-%d", &row.day).is_err() {
            continue;
        }
        if row.day < days.start || row.day > days.today {
            continue;
        }
        if row.day == days.today {
            totals.0 += row.value;
        }
        if row.day == days.yesterday {
            totals.1 += row.value;
        }
        totals.2 += row.value;
        *models.entry((row.provider.clone(), row.model.clone())).or_default() += row.value;
        providers.insert(row.provider.clone());
    }
    let mut models = models
        .into_iter()
        .map(|((provider, model), cost)| SpendModel { provider, model, cost })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        right
            .cost
            .total_cmp(&left.cost)
            .then_with(|| (&left.provider, &left.model).cmp(&(&right.provider, &right.model)))
    });
    SpendSummary {
        today: totals.0,
        yesterday: totals.1,
        last_window: totals.2,
        models,
        providers: providers.into_iter().collect(),
    }
}

pub fn spend_report(rows: &[SpendRow], now: Timestamp) -> SpendReport {
    let total = spend_summary(rows, now);
    let by_provider = total
        .providers
        .iter()
        .map(|provider| {
            (
                provider.clone(),
                spend_summary(
                    &rows
                        .iter()
                        .filter(|row| &row.provider == provider)
                        .cloned()
                        .collect::<Vec<_>>(),
                    now,
                ),
            )
        })
        .collect();
    SpendReport { total, by_provider }
}

#[cfg(test)]
#[path = "../tests/unit/spend/tests.rs"]
mod tests;
