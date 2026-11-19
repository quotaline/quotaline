//! Drives the shared refresh session the way a desktop's refresh loop does.

use super::*;
use crate::RefreshPolicy;
use std::collections::BTreeSet;

const NOW: f64 = 1_000_000.0;

fn schedule(id: &str) -> ProviderSchedule<'_> {
    ProviderSchedule {
        id,
        active: true,
        minimum: 60.0,
        active_interval: 60.0,
        requested: 300.0,
        supports_analytics: false,
    }
}

fn inputs<'a>(providers: &'a [ProviderSchedule<'a>], now: f64) -> ScheduleInputs<'a> {
    ScheduleInputs {
        providers,
        visible: false,
        throttled: false,
        analytics_interval: 900.0,
        maximum: 1800.0,
        idle_threshold: 3600.0,
        now,
    }
}

fn intent(reason: RefreshReason, providers: Option<&[&str]>) -> RefreshIntent {
    RefreshIntent {
        reason,
        usage: if reason == RefreshReason::Scheduled {
            RefreshPolicy::IfDue
        } else {
            RefreshPolicy::Force
        },
        analytics: RefreshPolicy::IfDue,
        providers: providers.map(|ids| ids.iter().map(|id| (*id).to_string()).collect()),
    }
}

fn cycle(session: &mut RefreshSession, reason: RefreshReason, providers: Option<&[&str]>) -> Cycle {
    session.request(intent(reason, providers));
    session.start().unwrap()
}

fn complete(session: &mut RefreshSession, cycle: &Cycle, provider: &str, outcome: FetchOutcome, now: f64) {
    session
        .complete(Completion {
            generation: cycle.generation,
            provider,
            outcome,
            retry_after: None,
            retry_interval: 300.0,
            included_analytics: false,
            now,
        })
        .unwrap();
}

fn fetched(plans: &Plans) -> Vec<&str> {
    plans.fetches.iter().map(|fetch| fetch.provider.as_str()).collect()
}

#[test]
fn requests_made_during_a_cycle_run_once_afterwards() {
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    session.request(intent(RefreshReason::UserInitiated, Some(&["claude"])));
    session.request(intent(RefreshReason::UserInitiated, Some(&["codex"])));
    assert!(session.has_pending());
    session.finish(&first);
    let next = session.start().unwrap();
    let providers = next.intent.providers.clone().unwrap();
    assert_eq!(providers.into_iter().collect::<Vec<_>>(), ["claude", "codex"]);
    session.finish(&next);
    assert!(session.start().is_none());
}

#[test]
fn a_running_cycle_answers_requests_it_covers() {
    let mut session = RefreshSession::new(NOW);
    let running = cycle(&mut session, RefreshReason::UserInitiated, None);
    complete(&mut session, &running, "claude", FetchOutcome::Success, NOW);
    session.request(intent(RefreshReason::UserInitiated, Some(&["claude"])));
    assert!(!session.has_pending());
}

#[test]
fn cancelling_drops_queued_requests_and_stale_results() {
    let mut session = RefreshSession::new(NOW);
    let running = cycle(&mut session, RefreshReason::Scheduled, None);
    session.request(intent(RefreshReason::UserInitiated, None));
    session.cancel();
    assert!(!session.has_pending());
    let stale = Completion {
        generation: running.generation,
        provider: "claude",
        outcome: FetchOutcome::Success,
        retry_after: None,
        retry_interval: 300.0,
        included_analytics: false,
        now: NOW,
    };
    assert_eq!(session.complete(stale), None);
    assert_eq!(session.times("claude"), ProviderTimes::default());
    session.finish(&running);
    assert!(cycle(&mut session, RefreshReason::Scheduled, None).generation > running.generation);
}

#[test]
fn scheduled_cycles_fetch_providers_that_are_due() {
    let providers = [schedule("claude"), schedule("codex")];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    assert_eq!(
        fetched(&session.plan(&first, &inputs(&providers, NOW))),
        ["claude", "codex"]
    );
    complete(&mut session, &first, "claude", FetchOutcome::Success, NOW);
    session.finish(&first);
    let second = cycle(&mut session, RefreshReason::Scheduled, None);
    let plans = session.plan(&second, &inputs(&providers, NOW + 60.0));
    assert_eq!(fetched(&plans), ["codex"]);
    assert_eq!(plans.skipped, [("claude".to_string(), RefreshSkip::NoWork)]);
}

#[test]
fn inactive_providers_refresh_only_when_asked_for_by_name() {
    let providers = [ProviderSchedule {
        active: false,
        ..schedule("claude")
    }];
    let mut session = RefreshSession::new(NOW);
    let scheduled = cycle(&mut session, RefreshReason::Scheduled, None);
    let plans = session.plan(&scheduled, &inputs(&providers, NOW));
    assert_eq!(plans.skipped, [("claude".to_string(), RefreshSkip::Inactive)]);
    session.finish(&scheduled);
    let asked = cycle(&mut session, RefreshReason::UserInitiated, Some(&["claude"]));
    assert_eq!(fetched(&session.plan(&asked, &inputs(&providers, NOW))), ["claude"]);
}

#[test]
fn targeted_requests_leave_other_providers_out() {
    let providers = [schedule("claude"), schedule("codex")];
    let mut session = RefreshSession::new(NOW);
    let asked = cycle(&mut session, RefreshReason::UserInitiated, Some(&["codex"]));
    let plans = session.plan(&asked, &inputs(&providers, NOW));
    assert_eq!(fetched(&plans), ["codex"]);
    assert!(plans.skipped.is_empty());
}

#[test]
fn rate_limits_hold_even_a_user_request_until_the_backoff_ends() {
    let providers = [schedule("claude")];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::RateLimited, NOW);
    session.finish(&first);
    let times = session.times("claude");
    assert!(times.rate_limited);
    assert_eq!(times.retry_not_before, Some(NOW + 300.0));
    let asked = cycle(&mut session, RefreshReason::UserInitiated, None);
    let plans = session.plan(&asked, &inputs(&providers, NOW + 10.0));
    assert_eq!(plans.skipped, [("claude".to_string(), RefreshSkip::RetryBackoff)]);
}

#[test]
fn failures_back_off_scheduled_refreshes_but_not_user_requests() {
    let providers = [schedule("claude")];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::Failed, NOW);
    session.finish(&first);
    let scheduled = cycle(&mut session, RefreshReason::Scheduled, None);
    assert_eq!(
        session.plan(&scheduled, &inputs(&providers, NOW + 10.0)).skipped,
        [("claude".to_string(), RefreshSkip::RetryBackoff)]
    );
    session.finish(&scheduled);
    let asked = cycle(&mut session, RefreshReason::UserInitiated, None);
    assert_eq!(
        fetched(&session.plan(&asked, &inputs(&providers, NOW + 10.0))),
        ["claude"]
    );
}

#[test]
fn accelerated_minutes_shorten_backoff() {
    let mut session = RefreshSession::with_minute(NOW, 1.0);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::RateLimited, NOW);
    assert_eq!(session.times("claude").retry_not_before, Some(NOW + 30.0));
    complete(&mut session, &first, "codex", FetchOutcome::NetworkUnavailable, NOW);
    assert_eq!(session.times("codex").retry_not_before, Some(NOW + 30.0));
}

#[test]
fn the_second_attempt_is_staggered_by_provider_position() {
    let providers = [schedule("claude"), schedule("codex")];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::Success, NOW);
    complete(&mut session, &first, "codex", FetchOutcome::Success, NOW);
    session.finish(&first);
    let second = cycle(&mut session, RefreshReason::Scheduled, None);
    assert_eq!(
        fetched(&session.plan(&second, &inputs(&providers, NOW + 303.0))),
        ["claude"]
    );
    assert_eq!(session.next_deadline(&inputs(&providers[1..], NOW)), Some(NOW + 300.0));
    complete(&mut session, &second, "codex", FetchOutcome::Success, NOW + 306.0);
    session.finish(&second);
    let third = cycle(&mut session, RefreshReason::Scheduled, None);
    let plans = session.plan(&third, &inputs(&providers, NOW + 606.0));
    assert_eq!(fetched(&plans), ["claude", "codex"]);
}

#[test]
fn an_unattended_menu_slows_scheduled_refreshes_until_it_opens() {
    let providers = [schedule("claude")];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::Success, NOW + 3600.0);
    assert_eq!(
        session.next_deadline(&inputs(&providers, NOW + 3600.0)),
        Some(NOW + 3600.0 + 900.0)
    );
    session.mark_visible(NOW + 3600.0);
    assert_eq!(
        session.next_deadline(&inputs(&providers, NOW + 3600.0)),
        Some(NOW + 3900.0)
    );
}

#[test]
fn open_menus_poll_at_the_provider_minimum() {
    let providers = [schedule("claude")];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::Success, NOW);
    let visible = ScheduleInputs {
        visible: true,
        ..inputs(&providers, NOW)
    };
    assert_eq!(session.next_deadline(&visible), Some(NOW + 60.0));
}

#[test]
fn analytics_join_a_fetch_only_when_due() {
    let providers = [ProviderSchedule {
        supports_analytics: true,
        ..schedule("codex")
    }];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    assert!(session.plan(&first, &inputs(&providers, NOW)).fetches[0].include_analytics);
    session
        .complete(Completion {
            generation: first.generation,
            provider: "codex",
            outcome: FetchOutcome::Success,
            retry_after: None,
            retry_interval: 300.0,
            included_analytics: true,
            now: NOW,
        })
        .unwrap();
    session.finish(&first);
    assert_eq!(session.times("codex").last_analytics_attempt, Some(NOW));
    let asked = cycle(&mut session, RefreshReason::UserInitiated, None);
    assert!(!session.plan(&asked, &inputs(&providers, NOW + 60.0)).fetches[0].include_analytics);
    assert_eq!(session.next_deadline(&inputs(&providers, NOW)), Some(NOW + 300.0));
}

#[test]
fn forgetting_analytics_attempts_makes_analytics_due_again() {
    let providers = [ProviderSchedule {
        supports_analytics: true,
        ..schedule("codex")
    }];
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    session
        .complete(Completion {
            generation: first.generation,
            provider: "codex",
            outcome: FetchOutcome::Success,
            retry_after: None,
            retry_interval: 300.0,
            included_analytics: true,
            now: NOW,
        })
        .unwrap();
    session.finish(&first);
    session.forget_analytics_attempts();
    let asked = cycle(&mut session, RefreshReason::UserInitiated, None);
    assert!(session.plan(&asked, &inputs(&providers, NOW + 60.0)).fetches[0].include_analytics);
}

#[test]
fn deadlines_follow_backoff_and_ignore_inactive_providers() {
    let inactive = [ProviderSchedule {
        active: false,
        ..schedule("claude")
    }];
    let mut session = RefreshSession::new(NOW);
    assert_eq!(session.next_deadline(&inputs(&inactive, NOW)), None);
    let providers = [schedule("claude")];
    assert_eq!(session.next_deadline(&inputs(&providers, NOW)), Some(NOW));
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::RateLimited, NOW);
    assert_eq!(
        session.next_deadline(&inputs(&providers, NOW + 10.0)),
        Some(NOW + 300.0)
    );
}

#[test]
fn removed_providers_forget_their_attempts() {
    let mut session = RefreshSession::new(NOW);
    let first = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &first, "claude", FetchOutcome::Failed, NOW);
    complete(&mut session, &first, "codex", FetchOutcome::Failed, NOW);
    session.retain(&BTreeSet::from(["codex".to_string()]));
    assert_eq!(session.times("claude"), ProviderTimes::default());
    assert!(session.times("codex").retry_not_before.is_some());
}

#[test]
fn reset_forgets_attempts_and_drops_results_in_flight() {
    let providers = [schedule("claude")];
    let mut session = RefreshSession::new(NOW);
    let running = cycle(&mut session, RefreshReason::Scheduled, None);
    complete(&mut session, &running, "claude", FetchOutcome::RateLimited, NOW);
    session.reset(NOW + 10.0);
    assert_eq!(session.times("claude"), ProviderTimes::default());
    let late = Completion {
        generation: running.generation,
        provider: "claude",
        outcome: FetchOutcome::Success,
        retry_after: None,
        retry_interval: 300.0,
        included_analytics: false,
        now: NOW + 20.0,
    };
    assert_eq!(session.complete(late), None);
    let next = cycle(&mut session, RefreshReason::Scheduled, None);
    assert_eq!(
        fetched(&session.plan(&next, &inputs(&providers, NOW + 20.0))),
        ["claude"]
    );
}

#[test]
fn a_session_that_takes_over_keeps_the_backoff_and_its_strikes() {
    let providers = [schedule("claude")];
    let mut previous = RefreshSession::new(NOW);
    let first = cycle(&mut previous, RefreshReason::Scheduled, None);
    complete(&mut previous, &first, "claude", FetchOutcome::RateLimited, NOW);
    previous.finish(&first);
    let handover: Handover = serde_json::from_value(serde_json::to_value(previous.handover()).unwrap()).unwrap();

    let mut next = RefreshSession::new(NOW);
    next.adopt(handover);
    assert_eq!(next.times("claude"), previous.times("claude"));
    let asked = cycle(&mut next, RefreshReason::UserInitiated, None);
    assert_eq!(
        next.plan(&asked, &inputs(&providers, NOW + 10.0)).skipped,
        [("claude".to_string(), RefreshSkip::RetryBackoff)]
    );
    next.finish(&asked);
    let again = cycle(&mut next, RefreshReason::Scheduled, None);
    complete(&mut next, &again, "claude", FetchOutcome::RateLimited, NOW + 400.0);
    let mut fresh = RefreshSession::new(NOW);
    let only = cycle(&mut fresh, RefreshReason::Scheduled, None);
    complete(&mut fresh, &only, "claude", FetchOutcome::RateLimited, NOW + 400.0);
    assert!(
        next.times("claude").retry_not_before > fresh.times("claude").retry_not_before,
        "the second strike did not lengthen the backoff"
    );
}
