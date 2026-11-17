//! macOS pace bridge cases.

use super::*;
use jiff::civil::date;

#[test]
fn bridge_exposes_shared_pace_configuration() {
    assert_eq!(
        pace_configuration(),
        PaceConfigurationRecord {
            sample_horizon_seconds: 3600,
            min_workdays: 4,
            max_workdays: 7,
        }
    );
}

fn window(used: f64, start: i64, duration: i64) -> PaceWindowRecord {
    PaceWindowRecord {
        used,
        reset_at: Some((start + duration) as f64),
        duration: Some(duration as f64),
    }
}

#[test]
fn bridge_exposes_all_statuses_and_projection() {
    let now = 1_780_000_000;
    let start = now - 50_000;
    let cases = [
        (0.0, PaceStatusRecord::Behind),
        (50.0, PaceStatusRecord::OnTrack),
        (80.0, PaceStatusRecord::Ahead),
        (100.0, PaceStatusRecord::Exhausted),
    ];
    for (used, status) in cases {
        let result = pace_estimate(window(used, start, 100_000), Vec::new(), 7, now as f64, "UTC".into(), 0);
        assert_eq!(result.status, status);
    }
    let result = pace_estimate(
        window(50.0, start, 100_000),
        vec![
            PaceSampleRecord {
                at: (now - 1800) as f64,
                used: 25.0,
            },
            PaceSampleRecord {
                at: now as f64,
                used: 50.0,
            },
        ],
        7,
        now as f64,
        "UTC".into(),
        0,
    );
    assert_eq!(result.projected_exhaustion, Some((now + 3600) as f64));
    assert_eq!(result.expected_percent, Some(50.0));
    assert_eq!(result.ratio, Some(1.0));
}

#[test]
fn bridge_uses_named_timezone_across_dst() {
    let zone = TimeZone::get("America/Los_Angeles").unwrap();
    let start = zone.to_timestamp(date(2026, 10, 31).into()).unwrap().as_second();
    let now = zone.to_timestamp(date(2026, 11, 2).at(9, 0, 0, 0)).unwrap().as_second();
    let result = pace_estimate(
        window(30.0, start, 7 * 86_400),
        Vec::new(),
        5,
        now as f64,
        "America/Los_Angeles".into(),
        0,
    );
    assert!(result.expected_percent.unwrap() < 10.0);
    assert_eq!(result.status, PaceStatusRecord::Ahead);
}

#[test]
fn bridge_falls_back_to_fixed_offset_and_handles_missing_reset() {
    let now = 1_780_000_000;
    let result = pace_estimate(
        window(10.0, now - 1000, 100_000),
        Vec::new(),
        5,
        now as f64,
        "GMT+0530".into(),
        19_800,
    );
    assert!(result.expected_percent.is_some());
    let unknown = pace_estimate(
        PaceWindowRecord {
            used: 10.0,
            reset_at: None,
            duration: None,
        },
        Vec::new(),
        7,
        now as f64,
        "invalid".into(),
        i32::MAX,
    );
    assert_eq!(unknown.status, PaceStatusRecord::Unknown);
    assert_eq!(unknown.projected_exhaustion, None);
}

#[test]
fn bridge_ignores_out_of_range_reset() {
    let result = pace_estimate(
        PaceWindowRecord {
            used: 10.0,
            reset_at: Some(f64::INFINITY),
            duration: Some(100_000.0),
        },
        Vec::new(),
        7,
        1_780_000_000.0,
        "UTC".into(),
        0,
    );
    assert_eq!(result.status, PaceStatusRecord::Unknown);
}
