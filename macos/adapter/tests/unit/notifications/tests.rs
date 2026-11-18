//! Checks notification behavior through the macOS boundary.

use super::*;
use crate::provider_record::{ProviderRecord, ProviderWindowRecord, ReadingSource};
use crate::test_clock::Clock;

fn plan(request: NotificationRequestRecord) -> Vec<NotificationEventRecord> {
    let wording = NotificationWordingRecord {
        provider_name: "Claude".into(),
        login_hint: "Run claude auth login.".into(),
        fetched_at: request.now,
    };
    plan_notifications(request, wording, Arc::new(Clock))
}

fn words(event: &NotificationEventRecord) -> (&str, &str) {
    (&event.title, &event.body)
}

fn options() -> NotificationOptionsRecord {
    NotificationOptionsRecord {
        enabled: true,
        thresholds: vec![75, 90, 100],
        resets: true,
        sign_in: true,
        expiring_credits: true,
        pace: false,
        workdays: 7,
    }
}

#[test]
fn thresholds_are_sorted_deduplicated_and_bounded() {
    assert_eq!(notification_thresholds(vec![150, 90, 0, 50, 90, -1]), vec![50, 90]);
}

#[test]
fn threshold_update_uses_shared_policy() {
    assert_eq!(notification_set_threshold(vec![90, 75, 90], 50, true), vec![50, 75, 90]);
    assert_eq!(notification_set_threshold(vec![90, 75, 90], 90, false), vec![75]);
}

#[test]
fn bridge_exposes_shared_threshold_catalog() {
    assert_eq!(notification_threshold_choices(), vec![50, 75, 90, 100]);
    assert_eq!(notification_default_thresholds(), vec![75, 90, 100]);
}

fn window(id: &str, label: &str, used: f64, reset_at: f64, duration: u64, by_day: bool) -> ProviderWindowRecord {
    ProviderWindowRecord {
        id: id.into(),
        label: label.into(),
        used,
        active: true,
        reset: jiff::Timestamp::from_second(reset_at as i64)
            .ok()
            .map(|at| at.display_with_offset(jiff::tz::Offset::UTC).to_string()),
        reset_by_day: by_day,
        group: id.into(),
        scope: None,
        severity: None,
        detail: String::new(),
        duration_seconds: Some(duration),
    }
}

fn snapshot(used: f64, reset_at: f64, fetched_at: f64) -> ProviderReadingRecord {
    let mut provider = ProviderRecord::from(Provider::empty("claude", "connected"));
    provider.windows = vec![window("session", "Current session", used, reset_at, 18_000, false)];
    ProviderReadingRecord {
        provider,
        fetched_at,
        source: ReadingSource::Network,
    }
}

fn request(
    provider_id: &str,
    previous: Option<ProviderReadingRecord>,
    current: Option<ProviderReadingRecord>,
    now: f64,
) -> NotificationRequestRecord {
    NotificationRequestRecord {
        provider_id: provider_id.into(),
        previous,
        current,
        previous_sign_in_required: false,
        current_sign_in_required: false,
        credential_missing: false,
        options: options(),
        samples: Vec::new(),
        now,
    }
}

#[test]
fn summary_lists_the_selected_alerts() {
    assert_eq!(
        notification_summary(options()),
        "On · 75%, 90%, 100%, resets, sign-in, credit expiry"
    );
}

#[test]
fn threshold_words_the_crossing_and_keeps_window_identity() {
    let now = 1_780_000_000.0;
    let events = plan(request(
        "claude",
        Some(snapshot(70.0, now + 3600.0, now - 60.0)),
        Some(snapshot(92.0, now + 3600.0, now)),
        now,
    ));
    assert_eq!(events[0].kind, NotificationKindRecord::Threshold);
    assert_eq!(events[0].window_id.as_deref(), Some("session"));
    assert_eq!(
        words(&events[0]),
        (
            "Claude Current session at 92%",
            format!(
                "Crossed 90% of the current session limit. Resets time {}.",
                now as i64 + 3600
            )
            .as_str()
        )
    );
}

#[test]
fn authentication_works_without_a_current_snapshot() {
    let events = plan(NotificationRequestRecord {
        current_sign_in_required: true,
        ..request("claude", None, None, 1_780_000_000.0)
    });
    assert_eq!(events[0].kind, NotificationKindRecord::Authentication);
    assert_eq!(words(&events[0]), ("Claude sign-in needed", "Run claude auth login."));
}

#[test]
fn grouped_thresholds_cross_the_swift_boundary_with_each_window() {
    let now = 1_780_000_000.0;
    let mut previous = snapshot(74.0, now + 3600.0, now - 60.0);
    previous
        .provider
        .windows
        .push(window("weekly", "Weekly", 74.0, now + 3600.0, 604_800, true));
    let mut current = previous.clone();
    current.provider.windows[0].used = 92.0;
    current.provider.windows[1].used = 78.0;
    let events = plan(request("claude", Some(previous), Some(current), now));
    assert_eq!(
        words(&events[0]),
        ("Claude: 2 limits crossed 75%", "Current session at 92%, Weekly at 78%.")
    );
}

#[test]
fn credit_expiry_has_no_window() {
    let now = 1_780_000_000.0;
    let mut current = snapshot(0.0, now + 3600.0, now);
    current.provider.windows.clear();
    current
        .provider
        .reset_credit_expiries
        .push(quotaline_quota::CreditExpiry {
            id: "credit".into(),
            at: now as i64 + 3600,
        });
    let events = plan(request("codex", None, Some(current), now));
    assert_eq!(events[0].kind, NotificationKindRecord::ExpiringCredit);
    assert_eq!(events[0].window_id, None);
    assert_eq!(
        words(&events[0]),
        ("Claude reset credit expires in 1 h", "Use it before it lapses.")
    );
}

#[test]
fn reset_maps_day_precision_and_window_identity() {
    let now = 1_780_000_000.0;
    let mut previous = snapshot(90.0, now + 60.0, now - 60.0);
    previous.provider.windows[0].reset_by_day = true;
    let mut current = snapshot(5.0, now + 18_000.0, now);
    current.provider.windows[0].reset_by_day = true;
    let events = plan(request("claude", Some(previous), Some(current), now));
    assert_eq!(events[0].kind, NotificationKindRecord::Reset);
    assert_eq!(events[0].window_id.as_deref(), Some("session"));
    assert_eq!(
        words(&events[0]),
        ("Claude Current session reset", "Usage is back to 5%.")
    );
}

#[test]
fn credit_depletion_maps_the_credits_kind() {
    let now = 1_780_000_000.0;
    let mut previous = snapshot(0.0, now + 3600.0, now);
    previous.provider.windows.clear();
    previous.provider.credit_balance = Some(5.0);
    let mut current = previous.clone();
    current.provider.credit_balance = Some(0.0);
    let events = plan(request("codex", Some(previous), Some(current), now));
    assert_eq!(events[0].kind, NotificationKindRecord::Credits);
    assert_eq!(
        words(&events[0]),
        (
            "Claude credits depleted",
            "Usage credits ran out; plan limits now apply."
        )
    );
}

#[test]
fn pace_events_map_both_kinds_and_samples() {
    let now = 1_780_000_000.0;
    let mut pace_options = options();
    pace_options.pace = true;
    pace_options.thresholds.clear();
    let events = plan(NotificationRequestRecord {
        options: pace_options,
        samples: vec![
            NotificationSampleRecord {
                window_id: "session".into(),
                at: now - 600.0,
                used: 45.0,
            },
            NotificationSampleRecord {
                window_id: "session".into(),
                at: now,
                used: 70.0,
            },
        ],
        ..request(
            "claude",
            Some(snapshot(45.0, now + 9000.0, now - 600.0)),
            Some(snapshot(70.0, now + 9000.0, now)),
            now,
        )
    });
    assert_eq!(
        events.iter().map(|event| event.kind).collect::<Vec<_>>(),
        vec![NotificationKindRecord::WillRunOut, NotificationKindRecord::AheadOfPace]
    );
}
