//! Owns the refresh state both desktops share: queued requests, retry strikes, stagger, attempt times, and the
//! generation that marks results from a cancelled cycle as stale. Each desktop supplies the clock and runs fetches.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    FetchOutcome, OutcomeTransition, RefreshDeadlineInput, RefreshIntent, RefreshPlanInput, RefreshQueue,
    RefreshReason, RefreshSkip, RefreshStagger, RetryLedger, next_deadline, plan, polling_interval,
    scheduled_usage_interval_with_idle_threshold,
};

/// One provider's scheduling inputs, in seconds.
#[derive(Clone, Copy, Debug)]
pub struct ProviderSchedule<'a> {
    pub id: &'a str,
    pub active: bool,
    pub minimum: f64,
    pub active_interval: f64,
    pub requested: f64,
    pub supports_analytics: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct ScheduleInputs<'a> {
    pub providers: &'a [ProviderSchedule<'a>],
    pub visible: bool,
    pub throttled: bool,
    pub analytics_interval: f64,
    pub maximum: f64,
    pub idle_threshold: f64,
    pub now: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderTimes {
    pub last_attempt: Option<f64>,
    pub last_analytics_attempt: Option<f64>,
    pub retry_not_before: Option<f64>,
    pub rate_limited: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cycle {
    pub intent: RefreshIntent,
    pub generation: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedFetch {
    pub provider: String,
    pub include_analytics: bool,
    pub retry_interval: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plans {
    pub fetches: Vec<PlannedFetch>,
    pub skipped: Vec<(String, RefreshSkip)>,
}

#[derive(Clone, Copy, Debug)]
pub struct Completion<'a> {
    pub generation: u64,
    pub provider: &'a str,
    pub outcome: FetchOutcome,
    pub retry_after: Option<f64>,
    pub retry_interval: f64,
    pub included_analytics: bool,
    pub now: f64,
}

/// The attempt and backoff history another session needs to keep the same pace and retry limits.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Handover {
    times: BTreeMap<String, ProviderTimes>,
    retry: BTreeMap<String, (u32, u32)>,
}

pub struct RefreshSession {
    queue: RefreshQueue,
    retry: RetryLedger,
    stagger: RefreshStagger,
    times: BTreeMap<String, ProviderTimes>,
    last_visible: f64,
    generation: u64,
    minute: f64,
}

struct Intervals {
    usage: f64,
    analytics: f64,
    retry: f64,
}

impl RefreshSession {
    pub fn new(now: f64) -> Self {
        Self::with_minute(now, 60.0)
    }

    /// `minute` shortens backoff the way a fixture's accelerated schedule shortens intervals.
    pub fn with_minute(now: f64, minute: f64) -> Self {
        Self {
            queue: RefreshQueue::default(),
            retry: RetryLedger::default(),
            stagger: RefreshStagger::default(),
            times: BTreeMap::new(),
            last_visible: now,
            generation: 0,
            minute,
        }
    }

    /// Queues a request; one the running or queued cycle already covers adds nothing.
    pub fn request(&mut self, intent: RefreshIntent) {
        self.queue.enqueue(intent);
    }

    pub fn has_pending(&self) -> bool {
        self.queue.has_pending()
    }

    /// Takes the next coalesced request; the caller runs one cycle at a time.
    pub fn start(&mut self) -> Option<Cycle> {
        let intent = self.queue.take_pending()?;
        Some(Cycle {
            intent,
            generation: self.generation,
        })
    }

    pub fn finish(&mut self, cycle: &Cycle) {
        if cycle.generation == self.generation {
            self.queue.finish_active();
        }
    }

    /// Drops queued requests and makes every result still in flight stale.
    pub fn cancel(&mut self) {
        self.generation += 1;
        self.queue.clear();
    }

    /// Forgets every provider's attempts and backoff, as a settings reset does, and makes results in flight stale.
    pub fn reset(&mut self, now: f64) {
        self.cancel();
        self.retry = RetryLedger::default();
        self.stagger = RefreshStagger::default();
        self.times.clear();
        self.last_visible = now;
    }

    pub fn mark_visible(&mut self, now: f64) {
        self.last_visible = now;
    }

    pub fn times(&self, provider: &str) -> ProviderTimes {
        self.times.get(provider).copied().unwrap_or_default()
    }

    pub fn handover(&self) -> Handover {
        Handover {
            times: self.times.clone(),
            retry: self.retry.strikes(),
        }
    }

    /// Continues from another session's history, as when this one takes over polling from it.
    pub fn adopt(&mut self, handover: Handover) {
        self.times = handover.times;
        self.retry = RetryLedger::from_strikes(handover.retry);
    }

    /// Makes analytics due again, as clearing history requires.
    pub fn forget_analytics_attempts(&mut self) {
        for times in self.times.values_mut() {
            times.last_analytics_attempt = None;
        }
    }

    pub fn retain(&mut self, providers: &BTreeSet<String>) {
        self.retry.retain_providers(providers);
        self.times.retain(|provider, _| providers.contains(provider));
    }

    fn intervals(&self, schedule: &ProviderSchedule<'_>, inputs: &ScheduleInputs<'_>, scheduled: bool) -> Intervals {
        let retry = polling_interval(
            schedule.minimum,
            schedule.active_interval,
            schedule.requested,
            inputs.visible,
        );
        if !scheduled {
            return Intervals {
                usage: retry,
                analytics: inputs.analytics_interval,
                retry,
            };
        }
        let index = inputs
            .providers
            .iter()
            .position(|candidate| candidate.id == schedule.id)
            .unwrap_or_default();
        let count = inputs.providers.len();
        let staggered = |interval| {
            self.stagger
                .interval(schedule.id, interval, index, inputs.visible, count)
        };
        Intervals {
            usage: scheduled_usage_interval_with_idle_threshold(
                staggered(retry),
                inputs.throttled,
                inputs.visible,
                inputs.now - self.last_visible,
                inputs.idle_threshold,
                inputs.maximum,
            ),
            analytics: staggered(inputs.analytics_interval),
            retry,
        }
    }

    pub fn plan(&self, cycle: &Cycle, inputs: &ScheduleInputs<'_>) -> Plans {
        let intent = &cycle.intent;
        let mut plans = Plans::default();
        for schedule in inputs.providers {
            let targeted = intent
                .providers
                .as_ref()
                .is_some_and(|providers| providers.contains(schedule.id));
            if intent.providers.is_some() && !targeted {
                continue;
            }
            let times = self.times(schedule.id);
            let intervals = self.intervals(schedule, inputs, intent.reason == RefreshReason::Scheduled);
            let decision = plan(RefreshPlanInput {
                reason: intent.reason,
                usage: intent.usage,
                analytics: intent.analytics,
                active: schedule.active,
                targeted,
                rate_limited: times.rate_limited,
                retry_not_before: times.retry_not_before,
                last_attempt: times.last_attempt,
                last_analytics_attempt: times.last_analytics_attempt,
                usage_interval: intervals.usage,
                analytics_interval: intervals.analytics,
                supports_analytics: schedule.supports_analytics,
                now: inputs.now,
            });
            match decision {
                Ok(include_analytics) => plans.fetches.push(PlannedFetch {
                    provider: schedule.id.into(),
                    include_analytics,
                    retry_interval: intervals.retry,
                }),
                Err(skip) => plans.skipped.push((schedule.id.into(), skip)),
            }
        }
        plans
    }

    /// Records a fetch result; a result from a cancelled cycle changes nothing and returns `None`.
    pub fn complete(&mut self, completion: Completion<'_>) -> Option<OutcomeTransition> {
        if completion.generation != self.generation {
            return None;
        }
        let transition = self.retry.advance_with_minute(
            completion.provider,
            completion.outcome,
            completion.retry_after,
            completion.retry_interval,
            completion.now,
            self.minute,
        );
        let times = self.times.entry(completion.provider.into()).or_default();
        // A provider's second attempt is the one pushed out by its stagger phase.
        self.stagger
            .set_pending(completion.provider, times.last_attempt.is_none());
        times.last_attempt = Some(completion.now);
        if completion.included_analytics {
            times.last_analytics_attempt = Some(completion.now);
        }
        times.retry_not_before = transition.retry_not_before;
        times.rate_limited = completion.outcome == FetchOutcome::RateLimited;
        Some(transition)
    }

    pub fn next_deadline(&self, inputs: &ScheduleInputs<'_>) -> Option<f64> {
        inputs
            .providers
            .iter()
            .filter(|schedule| schedule.active)
            .map(|schedule| {
                let times = self.times(schedule.id);
                let intervals = self.intervals(schedule, inputs, true);
                next_deadline(RefreshDeadlineInput {
                    retry_not_before: times.retry_not_before,
                    last_attempt: times.last_attempt,
                    last_analytics_attempt: times.last_analytics_attempt,
                    usage_interval: intervals.usage,
                    analytics_interval: intervals.analytics,
                    supports_analytics: schedule.supports_analytics,
                    now: inputs.now,
                })
            })
            .min_by(f64::total_cmp)
    }
}

#[cfg(test)]
#[path = "../tests/unit/session/tests.rs"]
mod tests;
