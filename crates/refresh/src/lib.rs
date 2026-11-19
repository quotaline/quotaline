//! Combines refresh intents and computes polling backoff.

use std::collections::BTreeSet;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

mod apply;
pub use apply::{Applied, ApplyInput, Readings, apply};
mod transition;
pub use transition::{FetchOutcome, OutcomeTransition, OutcomeTransitionInput};
mod snapshot;
pub use snapshot::{SnapshotTransition, SnapshotTransitionInput, snapshot_transition};
mod retry;
pub use retry::RetryLedger;
mod queue;
pub use queue::RefreshQueue;
mod panel_open;
pub use panel_open::{needs_credentials, should_check_credentials};
mod rediscovery;
pub use rediscovery::{RediscoveryPolicy, RediscoveryTrigger};
mod setup_observation;
pub use setup_observation::{RestoredAvailability, SetupObservation, SetupObservationInput, setup_observation};
mod stagger;
mod state;
pub use stagger::RefreshStagger;
pub use state::{ProviderStatus, SettleInput, keep_shown_when_disabled, settle};
mod session;
pub use session::{
    Completion, Cycle, Handover, PlannedFetch, Plans, ProviderSchedule, ProviderTimes, RefreshSession, ScheduleInputs,
};

pub const RATE_LIMIT_BACKOFF: f64 = 300.0;
pub const MINIMUM_BACKOFF: f64 = 60.0;
pub const MAXIMUM_BACKOFF: f64 = 1800.0;
pub const NETWORK_BACKOFF: f64 = 60.0;
pub const IDLE_THRESHOLD: f64 = 3600.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RefreshReason {
    Scheduled,
    PopoverOpened,
    UserInitiated,
    Export,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum RefreshPolicy {
    Skip,
    IfDue,
    Force,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshIntent {
    pub reason: RefreshReason,
    pub usage: RefreshPolicy,
    pub analytics: RefreshPolicy,
    pub providers: Option<BTreeSet<String>>,
}

impl RefreshIntent {
    pub fn merged(self, other: Self) -> Self {
        Self {
            reason: self.reason.max(other.reason),
            usage: self.usage.max(other.usage),
            analytics: self.analytics.max(other.analytics),
            providers: match (self.providers, other.providers) {
                (Some(mut left), Some(right)) => {
                    left.extend(right);
                    Some(left)
                }
                _ => None,
            },
        }
    }

    pub fn covers(&self, other: &Self) -> bool {
        self.usage >= other.usage
            && self.analytics >= other.analytics
            && match (&self.providers, &other.providers) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(left), Some(right)) => left.is_superset(right),
            }
    }
}

pub fn due(policy: RefreshPolicy, last_attempt: Option<f64>, interval: f64, now: f64) -> bool {
    match policy {
        RefreshPolicy::Skip => false,
        RefreshPolicy::Force => true,
        RefreshPolicy::IfDue => last_attempt.is_none_or(|attempt| now - attempt >= interval - 1.0),
    }
}

pub fn polling_interval(minimum: f64, active_interval: f64, requested: f64, active: bool) -> f64 {
    requested
        .min(if active { active_interval } else { f64::INFINITY })
        .max(minimum)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshSkip {
    Inactive,
    RetryBackoff,
    AnalyticsNotDue,
    NoWork,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefreshPlanInput {
    pub reason: RefreshReason,
    pub usage: RefreshPolicy,
    pub analytics: RefreshPolicy,
    pub active: bool,
    pub targeted: bool,
    pub rate_limited: bool,
    pub retry_not_before: Option<f64>,
    pub last_attempt: Option<f64>,
    pub last_analytics_attempt: Option<f64>,
    pub usage_interval: f64,
    pub analytics_interval: f64,
    pub supports_analytics: bool,
    pub now: f64,
}

pub fn plan(input: RefreshPlanInput) -> Result<bool, RefreshSkip> {
    if !input.active && !(input.reason == RefreshReason::UserInitiated && input.targeted) {
        return Err(RefreshSkip::Inactive);
    }
    if input.retry_not_before.is_some_and(|deadline| deadline > input.now)
        && (input.reason != RefreshReason::UserInitiated || input.rate_limited)
    {
        return Err(RefreshSkip::RetryBackoff);
    }
    let usage_due = input.usage != RefreshPolicy::Skip
        && (input.retry_not_before.is_some() || due(input.usage, input.last_attempt, input.usage_interval, input.now));
    let analytics_due = input.supports_analytics
        && due(
            input.analytics,
            input.last_analytics_attempt,
            input.analytics_interval,
            input.now,
        );
    if usage_due || analytics_due {
        Ok(analytics_due)
    } else if input.usage == RefreshPolicy::Skip && input.analytics != RefreshPolicy::Skip && input.supports_analytics {
        Err(RefreshSkip::AnalyticsNotDue)
    } else {
        Err(RefreshSkip::NoWork)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RefreshDeadlineInput {
    pub retry_not_before: Option<f64>,
    pub last_attempt: Option<f64>,
    pub last_analytics_attempt: Option<f64>,
    pub usage_interval: f64,
    pub analytics_interval: f64,
    pub supports_analytics: bool,
    pub now: f64,
}

pub fn next_deadline(input: RefreshDeadlineInput) -> f64 {
    if let Some(retry) = input.retry_not_before {
        return retry.max(input.now);
    }
    let usage = input.last_attempt.map_or(input.now, |last| last + input.usage_interval);
    if input.supports_analytics {
        usage.min(
            input
                .last_analytics_attempt
                .map_or(input.now, |last| last + input.analytics_interval),
        )
    } else {
        usage
    }
}

pub fn staggered_interval(
    interval: f64,
    provider_index: usize,
    popover_visible: bool,
    provider_count: usize,
    stagger_pending: bool,
) -> f64 {
    if popover_visible || provider_count <= 1 || !stagger_pending {
        interval
    } else {
        interval * (1.0 + provider_index as f64 * 0.02)
    }
}

pub fn scheduled_usage_interval_with_idle_threshold(
    interval: f64,
    throttles_scanning: bool,
    popover_visible: bool,
    seconds_since_popover: f64,
    idle_threshold: f64,
    maximum: f64,
) -> f64 {
    let throttled = if throttles_scanning { 2.0 } else { 1.0 };
    let idle = if !popover_visible && seconds_since_popover >= idle_threshold {
        3.0
    } else {
        1.0
    };
    (interval * throttled * idle).min(maximum)
}

pub fn rate_limit_backoff(strikes: u32, retry_after: Option<f64>, retry_interval: f64) -> f64 {
    rate_limit_backoff_with_minute(strikes, retry_after, retry_interval, 60.0)
}

pub fn rate_limit_backoff_with_minute(
    strikes: u32,
    retry_after: Option<f64>,
    retry_interval: f64,
    minute_seconds: f64,
) -> f64 {
    let scale = minute_seconds / MINIMUM_BACKOFF;
    let local = (retry_interval.max(RATE_LIMIT_BACKOFF * scale) * 2.0f64.powi(strikes.saturating_sub(1) as i32))
        .min(MAXIMUM_BACKOFF * scale);
    local.max(retry_after.unwrap_or(0.0))
}

pub fn failure_backoff_with_minute(strikes: u32, retry_interval: f64, minute_seconds: f64) -> f64 {
    let scale = minute_seconds / MINIMUM_BACKOFF;
    (retry_interval.max(NETWORK_BACKOFF * scale) * 2.0f64.powi(strikes.saturating_sub(1) as i32))
        .min(MAXIMUM_BACKOFF * scale)
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
