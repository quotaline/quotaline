use super::{
    HistoryNavigation, HistoryRange, HistoryViewport, history_page, history_pause, history_query_rollup, history_reset,
    history_select_custom_end, history_select_custom_start, history_select_period, history_select_rollup,
    history_viewport,
};
use crate::WindowRollup;

fn at(value: &str) -> f64 {
    value.parse::<jiff::Timestamp>().unwrap().as_second() as f64
}

fn state(range: HistoryRange) -> HistoryNavigation {
    HistoryNavigation {
        range,
        follow_now: true,
        anchor_end: None,
        custom_start: at("2026-08-17T12:00:00Z"),
        custom_end: at("2026-08-19T12:00:00Z"),
        return_to_live_at: None,
        daily_utc: false,
        time_zone: "UTC".into(),
        fallback_offset: 0,
    }
}

#[test]
fn period_selection_owns_range_and_rollup_rules() {
    let now = at("2026-08-20T12:00:00Z");
    for (range, rollup, expected_rollup) in [
        (HistoryRange::Today, WindowRollup::Day, WindowRollup::Hour),
        (HistoryRange::Week, WindowRollup::Day, WindowRollup::Day),
        (HistoryRange::Today, WindowRollup::Minute, WindowRollup::Minute),
    ] {
        let selected = history_select_period(&state(HistoryRange::Week), Some(range), rollup, now);
        assert_eq!(selected.navigation.range, range);
        assert_eq!(selected.navigation.anchor_end, Some(now));
        assert!(!selected.navigation.follow_now);
        assert_eq!(selected.rollup, expected_rollup);
    }
    let selected = history_select_period(
        &state(HistoryRange::Custom),
        Some(HistoryRange::Custom),
        WindowRollup::Hour,
        now,
    );
    assert_eq!(selected.navigation.custom_end, now);
    assert_eq!(selected.navigation.custom_start, at("2026-08-17T12:00:00Z"));
}

#[test]
fn now_selection_preserves_custom_duration_and_minimum() {
    let now = at("2026-08-20T12:00:00Z");
    let selected = history_select_period(&state(HistoryRange::Custom), None, WindowRollup::Hour, now);
    assert_eq!(selected.navigation.custom_start, now - 2.0 * 86_400.0);
    assert_eq!(selected.navigation.custom_end, now);
    assert!(selected.navigation.follow_now);
    assert_eq!(selected.navigation.anchor_end, None);
    let mut short = state(HistoryRange::Custom);
    short.custom_start = short.custom_end;
    assert_eq!(
        history_select_period(&short, None, WindowRollup::Hour, now)
            .navigation
            .custom_start,
        now - 60.0
    );
    let fixed = history_select_period(&state(HistoryRange::Week), None, WindowRollup::Hour, now);
    assert_eq!(fixed.navigation.custom_start, at("2026-08-17T12:00:00Z"));
}

#[test]
fn rollup_selection_moves_daily_today_to_week() {
    for (range, rollup, expected) in [
        (HistoryRange::Today, WindowRollup::Day, HistoryRange::Week),
        (HistoryRange::Today, WindowRollup::Hour, HistoryRange::Today),
        (HistoryRange::Week, WindowRollup::Day, HistoryRange::Week),
    ] {
        let selected = history_select_rollup(&state(range), rollup);
        assert_eq!(selected.navigation.range, expected);
        assert_eq!(selected.rollup, rollup);
    }
}

#[test]
fn custom_start_and_end_selection_keep_a_minute_between_bounds() {
    let now = at("2026-08-20T12:00:00Z");
    let start = history_select_custom_start(&state(HistoryRange::Week), now, now + 1_000.0);
    assert_eq!(start.range, HistoryRange::Custom);
    assert_eq!(start.custom_start, now - 60.0);
    assert_eq!(start.custom_end, now);
    assert_eq!(start.anchor_end, Some(now));
    assert!(!start.follow_now);
    let earlier = history_select_custom_start(&state(HistoryRange::Week), now, now - 3_600.0);
    assert_eq!(earlier.custom_start, now - 3_600.0);

    let end = history_select_custom_end(&state(HistoryRange::Week), now, now - 8.0 * 86_400.0);
    assert_eq!(end.range, HistoryRange::Custom);
    assert_eq!(end.custom_end, end.custom_start + 60.0);
    assert_eq!(end.anchor_end, Some(end.custom_end));
    assert!(!end.follow_now);
    let later = history_select_custom_end(&state(HistoryRange::Week), now, now - 3_600.0);
    assert_eq!(later.custom_end, now - 3_600.0);
}

#[test]
fn pause_and_reset_own_navigation_state() {
    let now = at("2026-08-20T12:00:00Z");
    let paused = history_pause(&state(HistoryRange::Week), now);
    assert_eq!(paused.anchor_end, Some(now));
    assert!(!paused.follow_now);
    let reset = history_reset(&paused, now);
    assert!(reset.follow_now);
    assert_eq!(reset.anchor_end, None);
    assert_eq!(reset.custom_start, now - 86_400.0);
    assert_eq!(reset.custom_end, now);
}

#[test]
fn fixed_ranges_keep_calendar_days_and_today_starts_at_midnight() {
    let now = at("2026-08-20T12:00:00Z");
    for (case, range, start) in [
        ("today", HistoryRange::Today, at("2026-08-20T00:00:00Z")),
        ("week", HistoryRange::Week, at("2026-08-13T12:00:00Z")),
        ("month", HistoryRange::Month, at("2026-07-21T12:00:00Z")),
        ("two months", HistoryRange::TwoMonths, at("2026-06-21T12:00:00Z")),
    ] {
        assert_eq!(
            history_viewport(&state(range), now),
            HistoryViewport { start, end: now },
            "{case}"
        );
    }
}

#[test]
fn pinned_today_keeps_its_hour() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Today);
    input.follow_now = false;
    input.anchor_end = Some(now - 86_400.0);
    assert_eq!(
        history_viewport(&input, now),
        HistoryViewport {
            start: at("2026-08-18T12:00:00Z"),
            end: at("2026-08-19T12:00:00Z")
        }
    );
}

#[test]
fn pinned_range_without_an_anchor_uses_now_and_clamps_future_anchor() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Week);
    input.follow_now = false;
    for (case, anchor) in [("missing", None), ("future", Some(now + 86_400.0))] {
        input.anchor_end = anchor;
        assert_eq!(history_viewport(&input, now).end, now, "{case}");
    }
}

#[test]
fn custom_viewport_clamps_crossed_endpoints() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Custom);
    input.custom_start = now + 100.0;
    assert_eq!(
        history_viewport(&input, now),
        HistoryViewport {
            start: now - 60.0,
            end: now
        }
    );
    input.follow_now = false;
    input.custom_end = now - 3_600.0;
    assert_eq!(history_viewport(&input, now).end, now - 3_600.0);
}

#[test]
fn custom_daily_viewport_uses_utc_days() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Custom);
    input.custom_start = now + 100.0;
    input.follow_now = false;
    input.custom_end = now - 3_600.0;
    input.daily_utc = true;
    for follow in [false, true] {
        input.follow_now = follow;
        assert_eq!(
            history_viewport(&input, now),
            HistoryViewport {
                start: at("2026-08-20T00:00:00Z"),
                end: at("2026-08-20T00:00:00Z")
            },
            "follow={follow}"
        );
    }
}

#[test]
fn daily_fixed_range_has_inclusive_start_and_pages_without_overlap() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Week);
    input.daily_utc = true;
    let current = history_viewport(&input, now);
    assert_eq!(current.start, at("2026-08-14T00:00:00Z"));
    let previous = history_page(&input, now, false);
    assert!(!previous.follow_now);
    assert_eq!(history_viewport(&previous, now).end, at("2026-08-13T00:00:00Z"));
    let current_again = history_page(&previous, now, true);
    assert!(current_again.follow_now);
    assert_eq!(history_viewport(&current_again, now).start, current.start);
}

#[test]
fn custom_daily_page_preserves_whole_day_count() {
    let now = at("2026-08-25T12:00:00Z");
    let mut input = state(HistoryRange::Custom);
    input.follow_now = false;
    input.daily_utc = true;
    let previous = history_page(&input, now, false);
    assert_eq!(previous.custom_start, at("2026-08-14T00:00:00Z"));
    assert_eq!(previous.custom_end, at("2026-08-16T00:00:00Z"));
    assert!(!previous.follow_now);
    let current = history_page(&previous, now, true);
    assert_eq!(current.custom_start, at("2026-08-17T00:00:00Z"));
    assert_eq!(current.custom_end, at("2026-08-19T00:00:00Z"));
    let following = history_page(&current, now, true);
    assert_eq!(following.custom_end, at("2026-08-22T00:00:00Z"));
}

#[test]
fn custom_hour_page_clamps_to_now_and_retains_duration() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Custom);
    input.follow_now = false;
    let forward = history_page(&input, now, true);
    assert!(forward.follow_now);
    assert_eq!(forward.custom_end, now);
    assert_eq!(forward.custom_start, now - 2.0 * 86_400.0);
    let backward = history_page(&forward, now, false);
    assert!(!backward.follow_now);
    assert_eq!(backward.custom_end, now - 2.0 * 86_400.0);
}

#[test]
fn custom_forward_page_rejoins_live_after_clock_advances() {
    let now = at("2026-08-20T12:00:00Z");
    let previous = history_page(&state(HistoryRange::Custom), now, false);
    assert_eq!(previous.return_to_live_at, Some(now));
    let resumed = history_page(&previous, now + 11.0, true);
    assert!(resumed.follow_now);
    assert_eq!(resumed.custom_end, now + 11.0);
    assert_eq!(resumed.return_to_live_at, None);
}

#[test]
fn paging_a_past_custom_range_does_not_rejoin_live() {
    let now = at("2026-08-20T12:00:00Z");
    let mut selected = state(HistoryRange::Custom);
    selected.follow_now = false;
    let previous = history_page(&selected, now, false);
    assert_eq!(previous.return_to_live_at, None);
    let returned = history_page(&previous, now + 11.0, true);
    assert!(!returned.follow_now);
    assert_eq!(returned.custom_end, selected.custom_end);
}

#[test]
fn custom_hour_page_uses_a_minute_for_zero_length_selection() {
    let now = at("2026-08-20T12:00:00Z");
    let mut input = state(HistoryRange::Custom);
    input.follow_now = false;
    input.custom_start = input.custom_end;
    let previous = history_page(&input, now, false);
    assert_eq!(previous.custom_end - previous.custom_start, 60.0);
}

#[test]
fn today_pages_first_to_midnight_then_to_yesterday() {
    let now = at("2026-08-20T12:00:00Z");
    let input = state(HistoryRange::Today);
    let midnight = history_page(&input, now, false);
    assert_eq!(midnight.anchor_end, Some(at("2026-08-20T00:00:00Z")));
    let yesterday = history_page(&midnight, now, false);
    assert_eq!(yesterday.anchor_end, Some(at("2026-08-19T00:00:00Z")));
    let today = history_page(&yesterday, now, true);
    assert_eq!(today.anchor_end, Some(at("2026-08-20T00:00:00Z")));
}

#[test]
fn weekly_pages_back_and_forward_to_now() {
    let now = at("2026-08-20T12:00:00Z");
    let input = state(HistoryRange::Week);
    let previous = history_page(&input, now, false);
    assert_eq!(previous.anchor_end, Some(now - 7.0 * 86_400.0));
    assert!(history_page(&previous, now, true).follow_now);
}

#[test]
fn fixed_period_paging_rejoins_live_after_clock_advances() {
    let now = at("2026-08-20T12:00:00Z");
    for (case, selected) in [
        ("live", state(HistoryRange::Week)),
        (
            "selected",
            history_select_period(
                &state(HistoryRange::Week),
                Some(HistoryRange::Week),
                WindowRollup::Hour,
                now,
            )
            .navigation,
        ),
    ] {
        let previous = history_page(&selected, now, false);
        assert_eq!(previous.return_to_live_at, Some(now), "{case}");
        let resumed = history_page(&previous, now + 11.0, true);
        assert!(resumed.follow_now, "{case}");
        assert_eq!(history_viewport(&resumed, now + 11.0).end, now + 11.0, "{case}");
    }
}

#[test]
fn named_zone_preserves_local_hour_across_dst() {
    let mut input = state(HistoryRange::Week);
    input.time_zone = "America/Los_Angeles".into();
    let now = at("2026-03-10T12:00:00Z");
    assert_eq!(history_viewport(&input, now).start, at("2026-03-03T13:00:00Z"));
}

#[test]
fn unknown_zone_uses_fixed_offset() {
    let mut input = state(HistoryRange::Week);
    input.time_zone = "unknown".into();
    input.fallback_offset = -7 * 3_600;
    let now = at("2026-03-10T12:00:00Z");
    assert_eq!(history_viewport(&input, now).start, now - 7.0 * 86_400.0);
}

#[test]
fn nonexistent_local_hour_advances_to_the_next_valid_hour() {
    let mut input = state(HistoryRange::Week);
    input.time_zone = "America/Los_Angeles".into();
    let now = at("2026-03-15T09:30:00Z");
    assert_eq!(history_viewport(&input, now).start, at("2026-03-08T10:30:00Z"));
}

#[test]
fn rollup_respects_selected_and_sample_cadences_and_point_budget() {
    for (case, start, end, selected, expected) in [
        ("sample interval", 0.0, 0.0, 60.0, 300.0),
        ("selected cadence", 0.0, 3_600.0, 3_600.0, 3_600.0),
        ("point budget", 0.0, 60.0 * 86_400.0, 60.0, 6_600.0),
        ("reversed range", 100.0, 0.0, 60.0, 300.0),
    ] {
        assert_eq!(
            history_query_rollup(start, end, selected, 300.0, 800),
            expected,
            "{case}"
        );
    }
}
