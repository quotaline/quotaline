//! Maps history actions and the saved history preferences to navigation state.

use super::{HistoryRange, HistorySession, HistorySessionConfig, WindowRollup};
use jiff::civil::Date;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HistoryAction {
    Load,
    Now,
    Period { days: u32 },
    Rollup { rollup: String },
    Metric { metric: String },
    Utc { utc: bool },
    Custom { from: String, to: String },
    Page { forward: bool },
}

/// The history choices a desktop persists between launches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedHistory {
    pub days: u32,
    pub rollup: String,
    pub metric: String,
    pub utc: bool,
}

pub fn from_saved(saved: &SavedHistory, time_zone: String, offset: i32, now: f64) -> HistorySession {
    HistorySession::new(HistorySessionConfig {
        range: range(saved.days).unwrap_or(HistoryRange::Today),
        rollup: rollup(&saved.rollup).unwrap_or(WindowRollup::Minute),
        metric: saved.metric.clone(),
        daily_utc: saved.metric != "usage",
        utc: saved.utc,
        local_time_zone: time_zone,
        local_offset: offset,
        now,
    })
}

impl HistoryAction {
    pub fn apply(self, session: &mut HistorySession, now: f64) -> Result<(), &'static str> {
        match self {
            Self::Load => {}
            Self::Now => session.select_period(None, now),
            Self::Period { days } => session.select_period(Some(range(days).ok_or("Unknown history period")?), now),
            Self::Rollup { rollup: name } => {
                session.select_rollup(rollup(&name).ok_or("Unknown history rollup")?);
            }
            Self::Metric { metric } => {
                if metric != "usage" && !crate::METRICS.iter().any(|(id, _, _, _)| *id == metric) {
                    return Err("Unknown metric");
                }
                let daily = metric != "usage";
                session.select_metric(metric, daily);
            }
            Self::Utc { utc } => session.select_utc(utc),
            Self::Custom { from, to } => {
                let first = parse_date(&from)?;
                let last = parse_date(&to)?;
                if !session.select_custom_dates(first, last) {
                    return Err("Choose a valid date range");
                }
            }
            Self::Page { forward } => session.page(now, forward),
        }
        Ok(())
    }
}

pub fn sync_saved(session: &HistorySession, saved: &mut SavedHistory) -> bool {
    let mut changed = false;
    if session.range() != HistoryRange::Custom {
        let days = session.range().days() as u32;
        if saved.days != days {
            saved.days = days;
            changed = true;
        }
    }
    if saved.metric != session.metric() {
        saved.metric = session.metric().into();
        changed = true;
    }
    let rollup = match session.rollup() {
        WindowRollup::Minute => "minute",
        WindowRollup::Hour => "hour",
        WindowRollup::Day => "day",
    };
    if saved.rollup != rollup {
        saved.rollup = rollup.into();
        changed = true;
    }
    if saved.utc != session.utc() {
        saved.utc = session.utc();
        changed = true;
    }
    changed
}

fn range(days: u32) -> Option<HistoryRange> {
    match days {
        1 => Some(HistoryRange::Today),
        7 => Some(HistoryRange::Week),
        30 => Some(HistoryRange::Month),
        60 => Some(HistoryRange::TwoMonths),
        _ => None,
    }
}

fn rollup(name: &str) -> Option<WindowRollup> {
    match name {
        "minute" => Some(WindowRollup::Minute),
        "hour" => Some(WindowRollup::Hour),
        "day" => Some(WindowRollup::Day),
        _ => None,
    }
}

fn parse_date(value: &str) -> Result<Date, &'static str> {
    if value.len() != 10 || value.as_bytes()[4] != b'-' || value.as_bytes()[7] != b'-' {
        return Err("Enter dates as YYYY-MM-DD");
    }
    Date::strptime("%Y-%m-%d", value).map_err(|_| "Choose a valid date range")
}

#[cfg(test)]
#[path = "../../tests/unit/navigation/settings/tests.rs"]
mod tests;
