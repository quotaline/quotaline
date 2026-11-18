//! Owns history period and calendar selection across UI events.

use jiff::civil::Date;

use super::{HistoryNavigation, HistoryRange, NavigationState};
use crate::WindowRollup;

pub struct HistorySessionConfig {
    pub range: HistoryRange,
    pub rollup: WindowRollup,
    pub metric: String,
    pub daily_utc: bool,
    pub utc: bool,
    pub local_time_zone: String,
    pub local_offset: i32,
    pub now: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistorySession {
    navigation: NavigationState,
    rollup: WindowRollup,
    metric: String,
    utc: bool,
    local_time_zone: String,
    local_offset: i32,
    last_custom_dates: Option<(Date, Date)>,
    applied_custom_dates: Option<(Date, Date)>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct HistorySessionView {
    pub days: i64,
    pub follow_now: bool,
    pub start: i64,
    pub end: i64,
    pub metric: String,
    pub rollup: &'static str,
    pub utc: bool,
    pub custom_from: Option<String>,
    pub custom_to: Option<String>,
}

impl HistorySession {
    pub fn new(config: HistorySessionConfig) -> Self {
        let mut session = Self {
            navigation: NavigationState::new(HistoryNavigation {
                range: config.range,
                follow_now: true,
                anchor_end: None,
                custom_start: config.now - 86_400.0,
                custom_end: config.now,
                return_to_live_at: None,
                daily_utc: config.daily_utc,
                time_zone: String::new(),
                fallback_offset: 0,
            }),
            rollup: config.rollup,
            metric: config.metric,
            utc: config.utc,
            local_time_zone: config.local_time_zone,
            local_offset: config.local_offset,
            last_custom_dates: None,
            applied_custom_dates: None,
        };
        session.update_mode();
        session
    }

    pub fn range(&self) -> HistoryRange {
        self.navigation.range()
    }

    pub fn rollup(&self) -> WindowRollup {
        self.rollup
    }

    pub fn metric(&self) -> &str {
        &self.metric
    }

    pub fn utc(&self) -> bool {
        self.utc
    }

    pub fn update_zone(&mut self, time_zone: String, fallback_offset: i32) {
        if self.local_time_zone == time_zone && self.local_offset == fallback_offset {
            return;
        }
        self.local_time_zone = time_zone;
        self.local_offset = fallback_offset;
        self.update_mode();
    }

    pub fn select_period(&mut self, range: Option<HistoryRange>, now: f64) {
        self.rollup = self.navigation.select_period(range, self.rollup, now);
        self.applied_custom_dates = None;
    }

    pub fn select_rollup(&mut self, rollup: WindowRollup) {
        self.rollup = self.navigation.select_rollup(rollup);
    }

    pub fn select_metric(&mut self, metric: String, daily_utc: bool) {
        self.metric = metric;
        self.navigation.set_daily_utc(daily_utc);
        self.update_mode();
    }

    pub fn select_utc(&mut self, utc: bool) {
        self.utc = utc;
        self.update_mode();
    }

    pub fn select_custom_dates(&mut self, from: Date, to: Date) -> bool {
        if !self.navigation.select_calendar_dates(from, to) {
            return false;
        }
        self.last_custom_dates = Some((from, to));
        self.applied_custom_dates = Some((from, to));
        true
    }

    pub fn page(&mut self, now: f64, forward: bool) {
        self.navigation.page(now, forward);
        self.applied_custom_dates = None;
    }

    pub fn view(&self, now: f64) -> HistorySessionView {
        let viewport = self.navigation.viewport(now);
        HistorySessionView {
            days: self.navigation.range().days(),
            follow_now: self.navigation.follow_now(),
            start: viewport.start as i64,
            end: viewport.end as i64 + if self.navigation.daily_utc() { 86_400 } else { 0 },
            metric: self.metric.clone(),
            rollup: match self.rollup {
                WindowRollup::Minute => "minute",
                WindowRollup::Hour => "hour",
                WindowRollup::Day => "day",
            },
            utc: self.utc,
            custom_from: self.last_custom_dates.map(|(from, _)| from.to_string()),
            custom_to: self.last_custom_dates.map(|(_, to)| to.to_string()),
        }
    }

    fn update_mode(&mut self) {
        let use_utc = self.navigation.daily_utc() || self.utc;
        let time_zone = if use_utc {
            "UTC".into()
        } else {
            self.local_time_zone.clone()
        };
        self.navigation.set_mode(
            self.navigation.range(),
            self.navigation.daily_utc(),
            time_zone,
            if use_utc { 0 } else { self.local_offset },
        );
        if let Some((from, to)) = self.applied_custom_dates {
            self.navigation.select_calendar_dates(from, to);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/navigation/session/tests.rs"]
mod tests;
