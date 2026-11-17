//! Owns history navigation values and applies shared actions.

use jiff::civil::Date;

use super::{
    HistoryNavigation, HistoryRange, HistoryViewport, Zone, history_page, history_pause, history_reset,
    history_select_custom_end, history_select_custom_start, history_select_period, history_select_rollup,
    history_viewport,
};
use crate::WindowRollup;

#[derive(Clone, Debug, PartialEq)]
pub struct NavigationState {
    state: HistoryNavigation,
}

impl NavigationState {
    pub fn new(state: HistoryNavigation) -> Self {
        Self { state }
    }

    pub fn snapshot(&self) -> HistoryNavigation {
        self.state.clone()
    }

    pub fn range(&self) -> HistoryRange {
        self.state.range
    }

    pub fn daily_utc(&self) -> bool {
        self.state.daily_utc
    }

    pub fn set_daily_utc(&mut self, value: bool) {
        self.state.daily_utc = value;
    }

    pub fn set_mode(&mut self, range: HistoryRange, daily_utc: bool, time_zone: String, fallback_offset: i32) {
        self.state.range = range;
        self.state.daily_utc = daily_utc;
        self.state.time_zone = time_zone;
        self.state.fallback_offset = fallback_offset;
    }

    pub fn follow_now(&self) -> bool {
        self.state.follow_now
    }

    pub fn set_follow_now(&mut self, value: bool) {
        self.state.follow_now = value;
    }

    pub fn custom_start(&self) -> f64 {
        self.state.custom_start
    }

    pub fn set_custom_start(&mut self, value: f64) {
        self.state.custom_start = value;
    }

    pub fn custom_end(&self) -> f64 {
        self.state.custom_end
    }

    pub fn set_custom_end(&mut self, value: f64) {
        self.state.custom_end = value;
    }

    pub fn anchor_end(&self) -> Option<f64> {
        self.state.anchor_end
    }

    pub fn select_period(&mut self, range: Option<HistoryRange>, rollup: WindowRollup, now: f64) -> WindowRollup {
        let selected = history_select_period(&self.state, range, rollup, now);
        self.state = selected.navigation;
        selected.rollup
    }

    pub fn select_rollup(&mut self, rollup: WindowRollup) -> WindowRollup {
        let selected = history_select_rollup(&self.state, rollup);
        self.state = selected.navigation;
        selected.rollup
    }

    pub fn select_custom_start(&mut self, now: f64, requested: f64) {
        self.state = history_select_custom_start(&self.state, now, requested);
    }

    pub fn select_custom_end(&mut self, now: f64, requested: f64) {
        self.state = history_select_custom_end(&self.state, now, requested);
    }

    pub fn select_calendar_dates(&mut self, from: Date, to: Date) -> bool {
        let Some(next_day) = to.tomorrow().ok().filter(|_| from <= to) else {
            return false;
        };
        let zone = Zone::new(&self.state);
        self.state.range = HistoryRange::Custom;
        self.state.custom_start = zone.start_of_date(from);
        self.state.custom_end = zone.start_of_date(if self.state.daily_utc { to } else { next_day });
        self.state.follow_now = false;
        self.state.anchor_end = Some(self.state.custom_end);
        self.state.return_to_live_at = None;
        true
    }

    pub fn pause(&mut self, now: f64) {
        self.state = history_pause(&self.state, now);
    }

    pub fn reset(&mut self, now: f64) {
        self.state = history_reset(&self.state, now);
    }

    pub fn page(&mut self, now: f64, forward: bool) {
        self.state = history_page(&self.state, now, forward);
    }

    pub fn viewport(&self, now: f64) -> HistoryViewport {
        history_viewport(&self.state, now)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/navigation/state/tests.rs"]
mod tests;
