//! Checks shared demo analytics through the public generator.

use super::*;

#[test]
fn demo_analytics_covers_claude_and_codex() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let claude = demo_analytics_at("claude", now, 3, false).unwrap();
    assert_eq!(claude.len(), 72);
    assert_eq!(claude[0].day, "2026-10-10");
    assert_eq!(claude[0].metric, "inputTokens");
    assert_eq!(claude[0].series, "fable");
    assert_eq!(claude[21].metric, "sessions");
    assert_eq!(claude[0].provider, "claude");
    let codex = demo_analytics_at("codex", now, 2, false).unwrap();
    assert_eq!(codex.len(), 32);
    assert_eq!(codex[0].metric, "surfaceUsagePercent");
    assert_eq!(codex[14].metric, "codeReviews");
    assert_eq!(codex[0].provider, "codex");
}

#[test]
fn demo_analytics_days_keep_matching_calendar_values() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let short = demo_analytics_at("claude", now, 3, false).unwrap();
    let long = demo_analytics_at("claude", now, 60, false).unwrap();
    assert_eq!(short.as_slice(), &long[long.len() - short.len()..]);
}

#[test]
fn long_text_fixture_changes_only_series_names() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let standard = demo_analytics_at("claude", now, 1, false).unwrap();
    let expanded = demo_analytics_at("claude", now, 1, true).unwrap();
    assert_eq!(standard[0].day, expanded[0].day);
    assert_eq!(standard[0].value, expanded[0].value);
    assert!(expanded[0].series.len() > 50);
    assert_eq!(demo_analytics_at("codex", now, 0, true), Some(Vec::new()));
}

#[test]
fn unsupported_providers_have_no_demo_analytics() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    for id in ["gemini", "antigravity", "cursor", "copilot", "unknown"] {
        assert!(demo_analytics_at(id, now, 1, false).is_none());
    }
}

#[test]
fn demo_analytics_cover_every_history_metric() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let points = ["claude", "codex"]
        .into_iter()
        .flat_map(|id| demo_analytics_at(id, now, 1, false).unwrap())
        .collect::<Vec<_>>();
    for (metric, ..) in quotaline_analytics::METRICS {
        assert!(points.iter().any(|point| point.metric == *metric), "{metric}");
    }
}

#[test]
fn demo_credit_history_mirrors_model_credits() {
    let now = "2026-10-12T14:00:00Z".parse::<Timestamp>().unwrap();
    let codex = demo_analytics_at("codex", now, 1, false).unwrap();
    let values = |metric: &str| {
        codex
            .iter()
            .filter(|point| point.metric == metric)
            .map(|point| (point.series.clone(), point.value))
            .collect::<Vec<_>>()
    };
    assert_eq!(values("credits"), values("modelCredits"));
}
