//! Tracks per-provider retry attempts across refreshes.

use std::collections::{BTreeMap, BTreeSet};

use super::{FetchOutcome, OutcomeTransition, OutcomeTransitionInput, transition::outcome_transition_with_minute};

#[derive(Default)]
pub struct RetryLedger(BTreeMap<String, (u32, u32)>);

impl RetryLedger {
    pub fn strikes(&self) -> BTreeMap<String, (u32, u32)> {
        self.0.clone()
    }

    pub fn from_strikes(strikes: BTreeMap<String, (u32, u32)>) -> Self {
        Self(strikes)
    }

    pub fn advance_with_minute(
        &mut self,
        provider: &str,
        outcome: FetchOutcome,
        retry_after: Option<f64>,
        retry_interval: f64,
        now: f64,
        minute_seconds: f64,
    ) -> OutcomeTransition {
        let (rate_limit_strikes, failure_strikes) = self.0.get(provider).copied().unwrap_or_default();
        let transition = outcome_transition_with_minute(
            OutcomeTransitionInput {
                outcome,
                rate_limit_strikes,
                failure_strikes,
                retry_after,
                retry_interval,
                now,
            },
            minute_seconds,
        );
        if transition.rate_limit_strikes == 0 && transition.failure_strikes == 0 {
            self.0.remove(provider);
        } else {
            self.0.insert(
                provider.into(),
                (transition.rate_limit_strikes, transition.failure_strikes),
            );
        }
        transition
    }

    pub fn retain_providers(&mut self, providers: &BTreeSet<String>) {
        self.0.retain(|provider, _| providers.contains(provider));
    }
}

#[cfg(test)]
#[path = "../tests/unit/retry/tests.rs"]
mod tests;
