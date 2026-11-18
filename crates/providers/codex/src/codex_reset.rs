//! Parses Codex reset-credit offers and balances.

use jiff::Timestamp;
use quotaline_quota::Window;
use std::collections::{BTreeMap, BTreeSet};

mod cache;
pub use cache::{RESET_CREDIT_FAILURE_TTL, RESET_CREDIT_SUCCESS_TTL, ResetCreditCache, ResetCreditCacheRead};

pub trait ResetWindow: Clone {
    fn id(&self) -> &str;
    fn used_percent(&self) -> f64;
    fn reset_at(&self) -> Option<f64>;
    fn confirming(&self) -> Self;
}

pub struct Guard<T: ResetWindow> {
    accepted: BTreeMap<String, T>,
    pending: BTreeSet<String>,
}

impl<T: ResetWindow> Default for Guard<T> {
    fn default() -> Self {
        Self {
            accepted: BTreeMap::new(),
            pending: BTreeSet::new(),
        }
    }
}

impl<T: ResetWindow> Guard<T> {
    pub fn clear(&mut self) {
        self.accepted.clear();
        self.pending.clear();
    }

    pub fn confirm(&mut self, windows: Vec<T>, now: f64) -> Vec<T> {
        let confirmed: Vec<_> = windows
            .into_iter()
            .map(|window| {
                let was_pending = self.pending.remove(window.id());
                let Some(previous) = self.accepted.get(window.id()) else {
                    return window;
                };
                // Codex can briefly report zero before its known reset; require a second fetch.
                if !was_pending
                    && window.used_percent() < 1.0
                    && previous.used_percent() >= 1.0
                    && previous.reset_at().is_some_and(|reset| reset > now)
                {
                    self.pending.insert(window.id().to_owned());
                    previous.confirming()
                } else {
                    window
                }
            })
            .collect();
        self.accepted = confirmed
            .iter()
            .cloned()
            .map(|window| (window.id().to_owned(), window))
            .collect();
        confirmed
    }
}

impl ResetWindow for Window {
    fn id(&self) -> &str {
        &self.id
    }

    fn used_percent(&self) -> f64 {
        self.used
    }

    fn reset_at(&self) -> Option<f64> {
        self.reset
            .as_deref()
            .and_then(|text| text.parse::<Timestamp>().ok())
            .map(|date| date.as_millisecond() as f64 / 1000.0)
    }

    fn confirming(&self) -> Self {
        let mut window = self.clone();
        window.detail = "Confirming reset".into();
        window
    }
}

#[cfg(test)]
#[path = "../tests/unit/codex_reset/tests.rs"]
mod tests;
