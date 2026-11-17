//! Checks chart series, totals, and point budgets through the public projection.

use super::*;

fn point(metric: &str, provider: &str, day: &str, series: &str, value: f64) -> Point {
    Point {
        metric: metric.into(),
        provider: provider.into(),
        day: day.into(),
        series: series.into(),
        value,
    }
}

#[test]
fn parallel_chart_prefers_detail_without_double_counting_totals() {
    let rows = [
        point("turns", "codex", "2026-08-29", "total", 10.0),
        point("turns", "codex", "2026-08-29", "surface:cli", 4.0),
        point("turns", "codex", "2026-08-29", "surface:web", 6.0),
        point("turns", "codex", "2026-08-30", "total", 3.0),
        point("messages", "claude", "2026-08-29", "total", 100.0),
    ];
    let chart = analytics_chart(&rows, "turns", &[], false).unwrap();
    assert_eq!(chart.data_point_count, 3);
    assert_eq!(chart.series.len(), 3);
    assert_eq!(chart.summary_total, 13.0);
    assert_eq!(chart.timeline.len(), 2);
    assert_eq!(
        chart
            .series
            .iter()
            .find(|series| series.series == "surface:cli")
            .unwrap()
            .summary_value,
        Some(4.0)
    );
}

#[test]
fn nonparallel_chart_stacks_visible_series_and_sums_their_values() {
    let rows = [
        point("inputTokens", "claude", "2026-08-29", "a", 10.0),
        point("inputTokens", "claude", "2026-08-29", "b", 20.0),
        point("inputTokens", "claude", "2026-08-30", "a", 5.0),
    ];
    let chart = analytics_chart(&rows, "inputTokens", &[], true).unwrap();
    assert_eq!(chart.summary_total, 35.0);
    assert!((chart.y_max - 32.4).abs() < 1e-9);
    let hidden = [("claude".into(), "b".into())];
    let selected = analytics_chart(&rows, "inputTokens", &hidden, true).unwrap();
    assert_eq!(selected.summary_total, 15.0);
    assert!((selected.y_max - 10.8).abs() < 1e-9);
    assert!(
        !selected
            .series
            .iter()
            .find(|series| series.series == "b")
            .unwrap()
            .visible
    );
}

#[test]
fn daily_line_segments_across_gaps_and_uses_latest_summary() {
    let rows = [
        point("surfaceUsagePercent", "codex", "2026-08-29", "cli", 20.0),
        point("surfaceUsagePercent", "codex", "2026-09-02", "cli", 30.0),
        point("surfaceUsagePercent", "codex", "2026-09-03", "cli", 35.0),
    ];
    let chart = analytics_chart(&rows, "surfaceUsagePercent", &[], true).unwrap();
    assert_eq!(
        chart.series[0]
            .points
            .iter()
            .map(|point| point.segment)
            .collect::<Vec<_>>(),
        [0, 1, 1]
    );
    assert_eq!(chart.series[0].summary_value, Some(35.0));
    assert_eq!(chart.y_max, 100.0);
}

#[test]
fn invalid_days_and_unknown_metrics_do_not_produce_series() {
    assert_eq!(analytics_chart(&[], "unknown", &[], false), None);
    let rows = [point("messages", "claude", "bad", "total", 5.0)];
    let chart = analytics_chart(&rows, "messages", &[], false).unwrap();
    assert_eq!(chart.data_point_count, 1);
    assert!(chart.series.is_empty());
    assert!(chart.timeline.is_empty());
    assert_eq!(chart.y_max, 1.0);
}

#[test]
fn hidden_and_negative_values_have_an_empty_axis() {
    let rows = [point("costUSD", "claude", "2026-08-29", "model", -3.0)];
    let chart = analytics_chart(&rows, "costUSD", &[], false).unwrap();
    assert_eq!(chart.y_max, 1.0);
    let chart = analytics_chart(&rows, "costUSD", &[("claude".into(), "model".into())], false).unwrap();
    assert!(chart.timeline.is_empty());
    assert_eq!(chart.y_max, 1.0);
    assert_eq!(chart.summary_total, 0.0);
}

#[test]
fn daily_balance_uses_padded_axis_and_latest_value() {
    let rows = [point("creditBalance", "codex", "2026-08-29", "Available credits", 7.0)];
    let chart = analytics_chart(&rows, "creditBalance", &[], true).unwrap();
    assert!((chart.y_max - 7.56).abs() < 1e-9);
    assert_eq!(chart.series[0].summary_value, Some(7.0));
}

#[test]
fn chart_limits_the_total_number_of_points() {
    let first = jiff::civil::date(2024, 1, 1);
    let rows = (0..4)
        .flat_map(|series| {
            (0..420).map(move |day| {
                point(
                    "inputTokens",
                    "claude",
                    &(first + jiff::ToSpan::days(day)).to_string(),
                    &format!("model-{series}"),
                    day as f64,
                )
            })
        })
        .collect::<Vec<_>>();
    let chart = analytics_chart(&rows, "inputTokens", &[], false).unwrap();
    assert!(chart.series.iter().all(|series| series.points.len() <= 400));
    assert!(chart.series.iter().map(|series| series.points.len()).sum::<usize>() <= 1_200);
}
