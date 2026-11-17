//! Checks the public behavior of the adjacent module.

use super::*;

#[test]
fn notification_ledger_rejects_repeated_events() {
    let mut ledger = NotificationLedger::default();
    let event = event(Kind::Threshold, "claude:threshold".into(), Facts::Credits);
    assert!(ledger.accept(&event));
    assert!(!ledger.accept(&event));
    ledger.clear();
    assert!(ledger.accept(&event));
}

#[test]
fn notification_ledger_trims_large_batches() {
    let mut ledger = NotificationLedger::default();
    for index in 0..513 {
        assert!(ledger.accept(&event(Kind::Threshold, index.to_string(), Facts::Credits)));
    }
    ledger.trim();
    assert!(ledger.accept(&event(Kind::Threshold, "0".into(), Facts::Credits)));
}

#[test]
fn notification_ledger_keeps_entries_at_capacity() {
    let mut ledger = NotificationLedger::default();
    for index in 0..512 {
        assert!(ledger.accept(&event(Kind::Threshold, index.to_string(), Facts::Credits)));
    }
    ledger.trim();
    assert!(!ledger.accept(&event(Kind::Threshold, "0".into(), Facts::Credits)));
}

#[test]
fn notification_route_is_consumed_once() {
    let mut routes = NotificationRoutes::default();
    routes.insert(7, "claude");
    assert_eq!(routes.take(7).as_deref(), Some("claude"));
    assert_eq!(routes.take(7), None);
}

#[test]
fn notification_routes_drop_old_entries_when_full() {
    let mut routes = NotificationRoutes::default();
    for id in 1..=512 {
        routes.insert(id, "claude");
    }
    routes.insert(513, "codex");
    assert_eq!(routes.take(1), None);
    assert_eq!(routes.take(512), None);
    assert_eq!(routes.take(513).as_deref(), Some("codex"));
}
use jiff::Timestamp;
use jiff::tz::Offset;
use quotaline_quota::CreditExpiry;

fn provider(used: &[f64], reset: i64, now: i64) -> Provider {
    let rows = used
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let mut row = Window::new(
                format!("row{index}"),
                format!("Window {index}"),
                *value,
                Some(
                    Timestamp::from_second(reset)
                        .unwrap()
                        .display_with_offset(Offset::UTC)
                        .to_string(),
                ),
                "session",
            );
            row.duration_seconds = Some(100_000);
            row
        })
        .collect();
    let mut provider = Provider::connected("codex", "Plus".into(), "fixture".into(), String::new(), rows);
    provider.updated = Some(now);
    provider
}

#[test]
fn auth_transition_exposes_no_display_hint() {
    let event = auth_transition("claude", false, true, false, &Settings::default()).unwrap();
    assert_eq!(event.kind, Kind::Authentication);
    assert_eq!(event.id, "claude:auth");
    assert_eq!(event.facts, Facts::Authentication { detail: None });
    assert!(auth_transition("claude", true, true, false, &Settings::default()).is_none());
}

#[test]
fn auth_transition_respects_each_suppression_condition() {
    let settings = Settings::default();
    for (enabled, sign_in, missing, previous_required, current_required) in [
        (false, true, false, false, true),
        (true, false, false, false, true),
        (true, true, true, false, true),
        (true, true, false, true, true),
        (true, true, false, false, false),
    ] {
        let changed = Settings {
            notifications: enabled,
            notify_sign_in: sign_in,
            ..settings.clone()
        };
        assert!(auth_transition("claude", previous_required, current_required, missing, &changed).is_none());
    }
}

#[test]
fn threshold_crossing_picks_highest_and_groups_windows() {
    let now = 1_780_000_000;
    let before = provider(&[74.0, 74.0], now + 50_000, now - 60);
    let after = provider(&[96.0, 82.0], now + 50_000, now);
    let settings = Settings {
        notify_pace: false,
        ..Settings::default()
    };
    let events = plan(Some(&before), &after, &settings, &[], now);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].facts,
        Facts::ThresholdGroup {
            crossings: vec![
                ThresholdCrossing {
                    label: "Window 0".into(),
                    used: 96.0
                },
                ThresholdCrossing {
                    label: "Window 1".into(),
                    used: 82.0
                },
            ],
            lowest: 75,
        }
    );
    let one = provider(&[96.0], now + 50_000, now);
    assert!(
        plan(
            Some(&provider(&[74.0], now + 50_000, now - 60)),
            &one,
            &settings,
            &[],
            now
        )[0]
        .id
        .contains(":90:")
    );
}

#[test]
fn threshold_notice_ignores_higher_uncrossed_values() {
    let now = 1_780_000_000;
    let before = provider(&[20.0], now + 50_000, now - 60);
    let after = provider(&[80.0], now + 50_000, now);
    let settings = Settings {
        notify_pace: false,
        ..Settings::default()
    };
    let events = plan(Some(&before), &after, &settings, &[], now);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].facts,
        Facts::Threshold {
            label: "Window 0".into(),
            used: 80.0,
            threshold: 75,
            reset_at: Some(now + 50_000),
            precision: quotaline_quota::ResetPrecision::Instant
        }
    );
}

#[test]
fn threshold_notice_ignores_values_already_crossed() {
    let now = 1_780_000_000;
    let before = provider(&[80.0], now + 50_000, now - 60);
    let after = provider(&[95.0], now + 50_000, now);
    let settings = Settings {
        notify_pace: false,
        ..Settings::default()
    };
    let events = plan(Some(&before), &after, &settings, &[], now);
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].facts,
        Facts::Threshold {
            label: "Window 0".into(),
            used: 95.0,
            threshold: 90,
            reset_at: Some(now + 50_000),
            precision: quotaline_quota::ResetPrecision::Instant,
        }
    );
}

#[test]
fn reset_requires_previous_usage_above_floor() {
    let now = 1_780_000_000;
    let settings = Settings {
        notify_pace: false,
        ..Settings::default()
    };
    assert_eq!(
        plan(
            Some(&provider(&[80.0], now + 10, now - 60)),
            &provider(&[2.0], now + 50_000, now),
            &settings,
            &[],
            now
        )
        .len(),
        1
    );
    assert!(
        plan(
            Some(&provider(&[20.0], now + 10, now - 60)),
            &provider(&[2.0], now + 50_000, now),
            &settings,
            &[],
            now
        )
        .is_empty()
    );
}

#[test]
fn reset_notice_respects_its_switch() {
    let now = 1_780_000_000;
    let settings = Settings {
        notify_resets: false,
        notify_pace: false,
        ..Settings::default()
    };
    assert!(
        plan(
            Some(&provider(&[80.0], now + 10, now - 60)),
            &provider(&[2.0], now + 50_000, now),
            &settings,
            &[],
            now
        )
        .is_empty()
    );
}

#[test]
fn empty_threshold_list_keeps_reset_alerts() {
    let now = 1_780_000_000;
    let settings = Settings {
        notify_thresholds: Vec::new(),
        notify_pace: false,
        ..Settings::default()
    };
    let events = plan(
        Some(&provider(&[80.0], now + 10, now - 60)),
        &provider(&[2.0], now + 50_000, now),
        &settings,
        &[],
        now,
    );
    assert_eq!(events.len(), 1);
    assert!(events[0].id.contains(":reset:"));
}

#[test]
fn pace_transitions_and_credit_events() {
    let now = 1_780_000_000;
    let mut before = provider(&[10.0], now + 50_000, now - 5000);
    let mut after = provider(&[70.0], now + 50_000, now);
    before.credit_balance = Some(2.0);
    after.credit_balance = Some(0.0);
    let events = plan(Some(&before), &after, &Settings::default(), &[], now);
    assert!(events.iter().any(|item| item.id.contains("pace-ahead")));
    assert!(events.iter().any(|item| item.id.contains("pace-runout")));
    assert!(events.iter().any(|item| item.id.ends_with(":credits")));
}

#[test]
fn pace_notice_does_not_repeat_while_still_ahead() {
    let now = 1_780_000_000;
    let settings = Settings {
        notify_thresholds: Vec::new(),
        ..Settings::default()
    };
    let before = provider(&[70.0], now + 50_000, now - 60);
    let after = provider(&[75.0], now + 50_000, now);
    assert!(plan(Some(&before), &after, &settings, &[], now).is_empty());
}

#[test]
fn pace_notice_waits_until_learning_period_ends() {
    let now = 1_780_000_000;
    let settings = Settings {
        notify_thresholds: Vec::new(),
        ..Settings::default()
    };
    let before = provider(&[10.0], now + 99_000, now - 60);
    let after = provider(&[20.0], now + 99_000, now);
    assert!(plan(Some(&before), &after, &settings, &[], now).is_empty());
}

#[test]
fn exhausted_limit_does_not_add_a_pace_notice() {
    let now = 1_780_000_000;
    let settings = Settings {
        notify_thresholds: Vec::new(),
        ..Settings::default()
    };
    let before = provider(&[99.0], now + 50_000, now - 60);
    let after = provider(&[100.0], now + 50_000, now);
    assert!(plan(Some(&before), &after, &settings, &[], now).is_empty());
}

#[test]
fn missing_credit_balance_does_not_report_depletion() {
    let now = 1_780_000_000;
    let mut before = provider(&[], now, now - 60);
    before.credit_balance = Some(2.0);
    let after = provider(&[], now, now);
    assert!(plan(Some(&before), &after, &Settings::default(), &[], now).is_empty());
}

#[test]
fn reported_credit_flag_reports_depletion_without_a_balance() {
    let now = 1_780_000_000;
    let mut before = provider(&[], now, now - 60);
    let mut after = provider(&[], now, now);
    for (provider, has_credits) in [(&mut before, true), (&mut after, false)] {
        provider.usage_credits = Some(quotaline_quota::UsageCredits {
            unlimited: false,
            has_credits: Some(has_credits),
            overage_limit_reached: false,
            local_messages: None,
            cloud_messages: None,
        });
    }
    let events = plan(Some(&before), &after, &Settings::default(), &[], now);
    assert_eq!(events.len(), 1);
    assert!(events[0].id.ends_with(":credits"));
}

#[test]
fn recent_samples_trigger_runout_only_for_the_matching_window() {
    let now = 1_780_000_000;
    let before = provider(&[0.0], now + 50_000, now - 5000);
    let after = provider(&[20.0], now + 50_000, now);
    assert!(plan(Some(&before), &after, &Settings::default(), &[], now).is_empty());
    let samples = [
        ("other".into(), now - 1800, 0.0),
        ("row0".into(), now - 1800, 10.0),
        ("row0".into(), now, 20.0),
    ];
    let events = plan(Some(&before), &after, &Settings::default(), &samples, now);
    assert_eq!(events.len(), 1);
    assert!(events[0].id.contains("pace-runout"));
}

#[test]
fn credit_expiry_and_authentication_are_transition_based() {
    let now = 1_780_000_000;
    let mut before = provider(&[], now, now - 90_000);
    let mut after = provider(&[], now, now);
    let credit = CreditExpiry {
        id: "a".into(),
        at: now + 3600,
    };
    after.reset_credit_expiries.push(credit.clone());
    assert_eq!(plan(Some(&before), &after, &Settings::default(), &[], now).len(), 1);
    before.reset_credit_expiries.push(credit);
    before.updated = Some(now - 10);
    assert!(plan(Some(&before), &after, &Settings::default(), &[], now).is_empty());
    after.status = "sign-in".into();
    after.notice = "Log in".into();
    assert!(auth_problem(Some(&before), &after, &Settings::default(), false).is_some());
    assert!(auth_problem(Some(&before), &after, &Settings::default(), true).is_none());
}

#[test]
fn credit_expiry_warns_when_an_existing_credit_enters_the_window() {
    let now = 1_780_000_000;
    let credit = CreditExpiry {
        id: "credit".into(),
        at: now + 3600,
    };
    let mut before = provider(&[], now, now - 90_000);
    before.reset_credit_expiries.push(credit.clone());
    let mut after = provider(&[], now, now);
    after.reset_credit_expiries.push(credit);
    let events = plan(Some(&before), &after, &Settings::default(), &[], now);
    assert_eq!(events.len(), 1);
    assert!(events[0].id.contains(":reset-credit:"));
}

#[test]
fn unrelated_previous_credit_does_not_suppress_expiry_notice() {
    let now = 1_780_000_000;
    let mut before = provider(&[], now, now - 60);
    before.reset_credit_expiries.push(CreditExpiry {
        id: "other".into(),
        at: now + 3600,
    });
    let mut after = provider(&[], now, now);
    after.reset_credit_expiries.push(CreditExpiry {
        id: "current".into(),
        at: now + 3600,
    });
    let events = plan(Some(&before), &after, &Settings::default(), &[], now);
    assert_eq!(events.len(), 1);
    assert!(events[0].id.contains(":reset-credit:current:"));
}

#[test]
fn unknown_previous_refresh_does_not_suppress_credit_expiry() {
    let now = 1_780_000_000;
    let credit = CreditExpiry {
        id: "credit".into(),
        at: now + 3600,
    };
    let mut before = provider(&[], now, now - 60);
    before.updated = None;
    before.reset_credit_expiries.push(credit.clone());
    let mut after = provider(&[], now, now);
    after.reset_credit_expiries.push(credit);
    let events = plan(Some(&before), &after, &Settings::default(), &[], now);
    assert_eq!(events.len(), 1);
    assert!(events[0].id.contains(":reset-credit:"));
}

#[path = "tests/transitions.rs"]
mod transitions;
