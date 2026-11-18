use jiff::civil::Date;

use super::{HistoryRange, HistorySession, HistorySessionConfig, WindowRollup};

const NOW: f64 = 1_788_955_200.0;

fn date(value: &str) -> Date {
    value.parse().unwrap()
}

fn session(range: HistoryRange) -> HistorySession {
    HistorySession::new(HistorySessionConfig {
        range,
        rollup: WindowRollup::Minute,
        metric: "usage".into(),
        daily_utc: false,
        utc: false,
        local_time_zone: "America/Los_Angeles".into(),
        local_offset: -7 * 3_600,
        now: NOW,
    })
}

#[test]
fn live_period_and_paging_share_the_core_calendar() {
    let mut history = session(HistoryRange::Today);
    let live = history.view(NOW);
    assert_eq!(live.days, 1);
    assert!(live.follow_now);
    assert_eq!(live.end, NOW as i64);
    assert_eq!(live.metric, "usage");
    assert_eq!(live.rollup, "minute");
    assert!(!live.utc);
    assert_eq!(live.custom_from, None);
    assert_eq!(live.custom_to, None);

    history.select_period(Some(HistoryRange::Today), NOW);
    assert!(!history.view(NOW).follow_now);
    history.page(NOW, false);
    assert_eq!(history.view(NOW).end, live.start);
    history.page(NOW, true);
    assert!(history.view(NOW).follow_now);
    history.select_period(None, NOW);
    assert_eq!(history.view(NOW), live);
}

#[test]
fn rollup_and_period_selection_keep_supported_pairings() {
    let mut history = session(HistoryRange::Today);
    for (rollup, name) in [
        (WindowRollup::Minute, "minute"),
        (WindowRollup::Hour, "hour"),
        (WindowRollup::Day, "day"),
    ] {
        history.select_rollup(rollup);
        assert_eq!(history.view(NOW).rollup, name);
    }
    assert_eq!(history.range(), HistoryRange::Week);
    assert_eq!(history.rollup(), WindowRollup::Day);
    history.select_period(Some(HistoryRange::Today), NOW);
    assert_eq!(history.rollup(), WindowRollup::Hour);
    assert_eq!(history.range(), HistoryRange::Today);
}

#[test]
fn custom_dates_follow_local_or_utc_calendar_boundaries() {
    let mut history = session(HistoryRange::Week);
    assert!(!history.select_custom_dates(date("2026-09-08"), date("2026-09-07")));
    assert!(!history.select_custom_dates(Date::MAX, Date::MAX));
    assert!(history.select_custom_dates(date("2026-09-06"), date("2026-09-08")));
    let local = history.view(NOW);
    assert_eq!(local.days, 0);
    assert!(!local.follow_now);
    assert_eq!(local.custom_from.as_deref(), Some("2026-09-06"));
    assert_eq!(local.custom_to.as_deref(), Some("2026-09-08"));
    assert_eq!(local.start, 1_788_678_000);

    history.select_utc(true);
    let utc = history.view(NOW);
    assert!(history.utc());
    assert!(utc.utc);
    assert_eq!(utc.start, 1_788_652_800);
    history.select_metric("inputTokens".into(), true);
    let daily = history.view(NOW);
    assert_eq!(history.metric(), "inputTokens");
    assert_eq!(daily.end - daily.start, 3 * 86_400);
    history.select_metric("usage".into(), false);
    history.select_utc(false);
    assert_eq!(history.view(NOW).start, local.start);
}

#[test]
fn zone_updates_rebase_applied_dates_and_ignore_repeated_input() {
    let mut history = session(HistoryRange::Week);
    assert!(history.select_custom_dates(date("2026-09-06"), date("2026-09-07")));
    let local = history.view(NOW);
    history.update_zone("America/Los_Angeles".into(), -7 * 3_600);
    assert_eq!(history.view(NOW), local);
    history.update_zone("UTC".into(), 0);
    assert_ne!(history.view(NOW).start, local.start);
    history.select_utc(true);
    let utc = history.view(NOW);
    history.update_zone("America/Los_Angeles".into(), -7 * 3_600);
    assert_eq!(history.view(NOW), utc);
}

#[test]
fn offset_changes_rebase_dates_when_iana_zone_is_unavailable() {
    let mut history = session(HistoryRange::Week);
    assert!(history.select_custom_dates(date("2026-09-06"), date("2026-09-07")));
    history.update_zone("Unknown/Zone".into(), -7 * 3_600);
    let previous = history.view(NOW);
    history.update_zone("Unknown/Zone".into(), -6 * 3_600);
    assert_eq!(history.view(NOW).start - previous.start, -3_600);
}

#[test]
fn paging_retains_last_custom_dates_for_the_calendar_picker() {
    let mut history = session(HistoryRange::Week);
    assert!(history.select_custom_dates(date("2026-09-06"), date("2026-09-07")));
    history.page(NOW, false);
    let paged = history.view(NOW);
    history.select_utc(true);
    assert_eq!(history.view(NOW).start, paged.start);
    assert_eq!(history.view(NOW).custom_from.as_deref(), Some("2026-09-06"));
    history.select_period(None, NOW);
    assert!(history.view(NOW).follow_now);
    assert_eq!(history.view(NOW).custom_to.as_deref(), Some("2026-09-07"));
}

#[test]
fn custom_calendar_range_rejoins_live_after_paging_delay() {
    let mut history = session(HistoryRange::Week);
    assert!(history.select_custom_dates(date("2026-09-08"), date("2026-09-09")));
    history.page(NOW, false);
    assert!(!history.view(NOW).follow_now);
    history.page(NOW + 11.0, true);
    assert!(history.view(NOW + 11.0).follow_now);
}
