//! Checks calendar grouping and shared chart helpers through their public APIs.

use super::*;

#[path = "tests/pipeline.rs"]
mod pipeline;

fn sample(at: f64, used: f64, reset_at: Option<f64>) -> WindowSample {
    WindowSample {
        key: "claude:session".into(),
        at,
        used,
        reset_at,
    }
}

fn point(at: f64, value: f64) -> WindowChartPoint {
    WindowChartPoint::sample(&sample(at, value, None))
}

fn bucketed(samples: &[WindowSample], rollup: WindowRollup, time_zone: &str, fallback_offset: i32) -> Vec<f64> {
    let times = samples.iter().map(|sample| sample.at);
    let start = times.clone().fold(f64::INFINITY, f64::min);
    let end = times.fold(f64::NEG_INFINITY, f64::max);
    let request = WindowChartRequest {
        keys: vec!["claude:session".into()],
        visible: vec!["claude:session".into()],
        start,
        end,
        now: end,
        rollup,
        stacked: false,
        time_zone: time_zone.into(),
        fallback_offset,
        includes_end: true,
    };
    window_chart(samples, &request).series[0]
        .points
        .iter()
        .map(|point| point.value)
        .collect()
}

fn utc(text: &str) -> f64 {
    text.parse::<jiff::Timestamp>().unwrap().as_second() as f64
}

#[test]
fn minute_buckets_keep_the_latest_sample() {
    let samples = [
        sample(10.0, 1.0, None),
        sample(20.0, 2.0, None),
        sample(80.0, 3.0, None),
    ];
    assert_eq!(bucketed(&samples, WindowRollup::Minute, "UTC", 0), [2.0, 3.0]);
}

#[test]
fn hour_buckets_keep_the_latest_sample() {
    let samples = [
        sample(10.0, 1.0, None),
        sample(20.0, 2.0, None),
        sample(80.0, 3.0, None),
    ];
    assert_eq!(bucketed(&samples, WindowRollup::Hour, "UTC", 0), [3.0]);
}

#[test]
fn bucket_ignores_an_older_sample_read_later() {
    let samples = [sample(20.0, 2.0, None), sample(10.0, 1.0, None)];
    assert_eq!(bucketed(&samples, WindowRollup::Minute, "UTC", 0), [2.0]);
}

#[test]
fn bucket_prefers_the_later_of_equal_timestamps() {
    let samples = [sample(20.0, 1.0, None), sample(20.0, 2.0, None)];
    assert_eq!(bucketed(&samples, WindowRollup::Minute, "UTC", 0), [2.0]);
}

#[test]
fn day_bucket_uses_local_date_across_dst_transition() {
    let samples = [
        sample(utc("2026-03-07T06:30:00Z"), 1.0, None),
        sample(utc("2026-03-07T07:30:00Z"), 2.0, None),
    ];
    assert_eq!(bucketed(&samples, WindowRollup::Day, "America/Los_Angeles", 0), [2.0]);
}

#[test]
fn day_bucket_falls_back_to_the_fixed_offset_for_an_unknown_zone() {
    let samples = [
        sample(utc("2026-03-07T06:30:00Z"), 1.0, None),
        sample(utc("2026-03-07T07:30:00Z"), 2.0, None),
    ];
    assert_eq!(bucketed(&samples, WindowRollup::Day, "missing-zone", -28_800), [2.0]);
}

#[test]
fn repeated_local_hour_remains_two_calendar_buckets() {
    let samples = [
        sample(utc("2026-11-01T08:30:00Z"), 1.0, None),
        sample(utc("2026-11-01T09:30:00Z"), 2.0, None),
    ];
    assert_eq!(
        bucketed(&samples, WindowRollup::Hour, "America/Los_Angeles", 0),
        [1.0, 2.0]
    );
}

#[test]
fn bucket_saturates_an_out_of_range_timestamp() {
    assert_eq!(
        bucketed(
            &[sample(f64::MAX, 1.0, None)],
            WindowRollup::Hour,
            "America/Los_Angeles",
            0
        ),
        [1.0]
    );
}

#[test]
fn clip_carries_recent_value_and_honors_open_end() {
    let points = [point(-60.0, 30.0), point(300.0, 40.0), point(1_200.0, 50.0)];
    assert_eq!(
        window_clip(&points, 0.0, 600.0, 300.0, true)
            .iter()
            .map(|point| point.value)
            .collect::<Vec<_>>(),
        [30.0, 40.0]
    );
    assert_eq!(
        window_clip(&[point(0.0, 1.0), point(600.0, 2.0)], 0.0, 600.0, 300.0, false).len(),
        1
    );
    assert_eq!(window_clip(&[point(-1_000.0, 1.0)], 0.0, 600.0, 300.0, true).len(), 0);
    assert_eq!(
        window_clip(&[point(0.0, 1.0), point(30.0, 2.0)], 0.0, 600.0, 300.0, true).len(),
        2
    );
    assert_eq!(window_clip(&[], 0.0, 600.0, 300.0, true).len(), 0);
}

#[test]
fn change_points_keep_run_ends_resets_and_gaps() {
    assert_eq!(
        window_change_points(&[point(0.0, 1.0), point(60.0, 1.0)], None).len(),
        2
    );
    let values = [1.0, 1.0, 1.0, 2.0, 0.0, 0.0, 3.0];
    let points = values
        .iter()
        .enumerate()
        .map(|(index, value)| point(index as f64 * 60.0, *value))
        .collect::<Vec<_>>();
    assert_eq!(
        window_change_points(&points, None)
            .iter()
            .map(|point| point.value)
            .collect::<Vec<_>>(),
        [1.0, 1.0, 2.0, 0.0, 0.0, 3.0]
    );
    let points = [
        point(0.0, 20.0),
        point(300.0, 20.0),
        point(600.0, 20.0),
        point(1_800.0, 20.0),
        point(2_100.0, 20.0),
    ];
    assert_eq!(
        window_change_points(&points, Some(300.0))
            .iter()
            .map(|point| point.at)
            .collect::<Vec<_>>(),
        [0.0, 600.0, 1_800.0, 2_100.0]
    );
    let mut reset = point(60.0, 1.0);
    reset.is_reset = true;
    assert_eq!(
        window_change_points(&[point(0.0, 1.0), reset, point(120.0, 1.0)], None).len(),
        3
    );
}

#[test]
fn extension_keeps_fresh_values_only() {
    assert_eq!(window_extend_to(&[], 120.0, None).len(), 0);
    assert_eq!(window_extend_to(&[point(300.0, 4.0)], 120.0, None).len(), 1);
    assert_eq!(
        window_extend_to(&[point(0.0, 4.0)], 120.0, None).last().unwrap().at,
        120.0
    );
    assert_eq!(window_extend_to(&[point(0.0, 4.0)], 120.0, Some(60.0)).len(), 1);
    assert_eq!(window_extend_to(&[point(0.0, 4.0)], 90.0, Some(60.0)).len(), 2);
    let mut reset = point(0.0, 0.0);
    reset.is_reset = true;
    assert!(!window_extend_to(&[reset], 60.0, None)[1].is_reset);
}

#[test]
fn nearest_picks_the_closer_date_and_the_earlier_tie() {
    assert_eq!(window_nearest(&[], 2.0), None);
    assert_eq!(window_nearest(&[1.0, 3.0], 0.0), Some(1.0));
    assert_eq!(window_nearest(&[1.0, 3.0], 4.0), Some(3.0));
    assert_eq!(window_nearest(&[1.0, 3.0], 2.0), Some(1.0));
    assert_eq!(window_nearest(&[1.0, 3.0], 2.5), Some(3.0));
}

#[test]
fn stack_carries_values_across_other_series_dates() {
    let lower = WindowChartSeries {
        key: "a".into(),
        points: vec![point(0.0, 10.0), point(60.0, 20.0)],
        visible: true,
        summary_value: 20.0,
    };
    let upper = WindowChartSeries {
        key: "b".into(),
        points: vec![point(30.0, 5.0)],
        visible: true,
        summary_value: 5.0,
    };
    let stacked = window_stack(&[lower.clone(), upper]);
    assert_eq!(
        stacked[0].points.iter().map(|point| point.value).collect::<Vec<_>>(),
        [10.0, 10.0, 20.0]
    );
    assert_eq!(
        stacked[1]
            .points
            .iter()
            .map(|point| point.stack_base)
            .collect::<Vec<_>>(),
        [10.0, 10.0, 20.0]
    );
    assert_eq!(
        stacked[1].points.iter().map(|point| point.value).collect::<Vec<_>>(),
        [0.0, 5.0, 5.0]
    );
    let hidden = WindowChartSeries {
        key: "c".into(),
        points: vec![point(15.0, 7.0)],
        visible: false,
        summary_value: 7.0,
    };
    assert_eq!(window_stack(&[hidden.clone(), lower])[0], hidden);
    assert_eq!(window_stack(&[]), []);
}
