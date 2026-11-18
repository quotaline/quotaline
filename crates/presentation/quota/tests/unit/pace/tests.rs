//! Checks the public behavior of the adjacent module.

use super::*;
use jiff::civil::date;
use jiff::tz::{Offset, TimeZone};

fn window(used: f64, now: i64, elapsed: i64, duration: u64) -> Window {
    let reset = Timestamp::from_second(now + duration as i64 - elapsed)
        .unwrap()
        .display_with_offset(Offset::UTC)
        .to_string();
    let mut window = Window::new("weekly", "Weekly", used, Some(reset), "weekly");
    window.duration_seconds = Some(duration);
    window
}

fn local(year: i16, month: i8, day: i8, hour: i8) -> i64 {
    let local = date(year, month, day).at(hour, 0, 0, 0);
    TimeZone::system().to_timestamp(local).unwrap().as_second()
}

#[test]
fn estimates_statuses_and_learning_phase() {
    let now = 1_780_000_000;
    let duration = 100_000;
    assert_eq!(
        PaceEstimate::estimate(&window(10.0, now, 1_000, duration), &[], 7, now).status,
        PaceStatus::Unknown
    );
    assert_eq!(
        PaceEstimate::estimate(&window(70.0, now, 50_000, duration), &[], 7, now).status,
        PaceStatus::Ahead
    );
    assert_eq!(
        PaceEstimate::estimate(&window(50.0, now, 50_000, duration), &[], 7, now).status,
        PaceStatus::OnTrack
    );
    assert_eq!(
        PaceEstimate::estimate(&window(20.0, now, 50_000, duration), &[], 7, now).status,
        PaceStatus::Behind
    );
    assert_eq!(
        PaceEstimate::estimate(&window(100.0, now, 50_000, duration), &[], 7, now).status,
        PaceStatus::Exhausted
    );
}

#[test]
fn recent_slope_projects_exhaustion_before_reset() {
    let now = 1_780_000_000;
    let pace = PaceEstimate::estimate(
        &window(50.0, now, 50_000, 100_000),
        &[(now - 1800, 25.0), (now, 50.0)],
        7,
        now,
    );
    assert_eq!(pace.projected_exhaustion, Some(now + 3600));
    let slow = PaceEstimate::estimate(
        &window(20.0, now, 50_000, 100_000),
        &[(now - 1800, 19.0), (now, 20.0)],
        7,
        now,
    );
    assert_eq!(slow.projected_exhaustion, None);
}

#[test]
fn six_workdays_count_saturday_without_sunday() {
    let start = local(2026, 10, 9, 0);
    let now = local(2026, 10, 10, 12);
    let reset = local(2026, 10, 12, 0);
    let candidate = window(75.0, now, now - start, (reset - start) as u64);
    assert_eq!(
        PaceEstimate::estimate(&candidate, &[], 5, now).expected_percent,
        Some(100.0)
    );
    assert_eq!(
        PaceEstimate::estimate(&candidate, &[], 6, now).expected_percent,
        Some(75.0)
    );
}

#[test]
fn explicit_timezone_controls_workday_expectation() {
    let utc = TimeZone::fixed(Offset::UTC);
    let start = utc.to_timestamp(date(2026, 8, 29).into()).unwrap().as_second();
    let now = utc.to_timestamp(date(2026, 8, 31).at(9, 0, 0, 0)).unwrap().as_second();
    let estimate = PaceEstimate::estimate_in_timezone(&window(30.0, now, now - start, 7 * 86_400), &[], 5, now, &utc);
    assert!((estimate.expected_percent.unwrap() - 7.5).abs() < 0.0001);
    assert_eq!(estimate.status, PaceStatus::Ahead);
}

#[test]
fn unavailable_or_expired_window_has_unknown_pace() {
    let now = 1_780_000_000;
    let mut missing = Window::new("missing", "Missing", 20.0, None, "weekly");
    missing.duration_seconds = Some(100_000);
    let mut invalid = Window::new("invalid", "Invalid", 20.0, Some("invalid".into()), "weekly");
    invalid.duration_seconds = Some(100_000);
    let mut cases = vec![missing, invalid, window(20.0, now, 100_000, 100_000)];
    cases.push(window(20.0, now, 1_000, 100_000));
    cases.last_mut().unwrap().duration_seconds = Some(0);
    cases.push(window(20.0, now, 1_000, 100_000));
    cases.last_mut().unwrap().duration_seconds = Some(367 * 86_400);
    for candidate in cases {
        assert_eq!(
            PaceEstimate::estimate(&candidate, &[], 7, now).status,
            PaceStatus::Unknown
        );
    }
}

#[test]
fn unchanged_samples_do_not_project_exhaustion() {
    let now = 1_780_000_000;
    let pace = PaceEstimate::estimate(
        &window(0.0, now, 50_000, 100_000),
        &[(now - 1800, 0.0), (now, 0.0)],
        7,
        now,
    );
    assert_eq!(pace.projected_exhaustion, None);
}

#[test]
fn stale_samples_use_lifetime_rate() {
    let now = 1_780_000_000;
    let pace = PaceEstimate::estimate(
        &window(20.0, now, 50_000, 100_000),
        &[(now - 7200, 10.0), (now - 5400, 15.0)],
        7,
        now,
    );
    assert_eq!(pace.projected_exhaustion, None);
    assert_eq!(pace.status, PaceStatus::Behind);
}

#[test]
fn weekly_pace_excludes_nonworking_days() {
    let start = local(2026, 10, 9, 0);
    let now = local(2026, 10, 11, 12);
    let reset = local(2026, 10, 12, 0);
    let pace = PaceEstimate::estimate(&window(75.0, now, now - start, (reset - start) as u64), &[], 5, now);
    assert_eq!(pace.expected_percent, Some(100.0));
    assert_eq!(pace.status, PaceStatus::OnTrack);
}

#[test]
fn short_window_uses_linear_pace_even_with_five_workdays() {
    let now = 1_780_000_000;
    let pace = PaceEstimate::estimate(&window(20.0, now, 40_000, 80_000), &[], 5, now);
    assert_eq!(pace.expected_percent, Some(50.0));
}

#[test]
fn flat_or_decreasing_samples_do_not_create_a_recent_growth_rate() {
    let now = 1_780_000_000;
    let candidate = window(20.0, now, 50_000, 100_000);
    for samples in [vec![(now, 10.0), (now, 20.0)], vec![(now - 100, 30.0), (now, 20.0)]] {
        let pace = PaceEstimate::estimate(&candidate, &samples, 7, now);
        assert_eq!(pace.projected_exhaustion, None);
    }
}

#[test]
fn window_start_has_no_exhaustion_projection() {
    let now = 1_780_000_000;
    let pace = PaceEstimate::estimate(&window(20.0, now, 0, 100_000), &[], 7, now);
    assert_eq!(pace.projected_exhaustion, None);
    assert_eq!(pace.status, PaceStatus::Unknown);
}
