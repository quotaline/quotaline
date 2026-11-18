//! Spend aggregation cases.

use super::*;

fn row(day: &str, provider: &str, model: &str, value: f64) -> SpendRow {
    SpendRow {
        day: day.into(),
        provider: provider.into(),
        model: model.into(),
        value,
    }
}

fn now() -> Timestamp {
    "2026-10-13T12:00:00Z".parse().unwrap()
}

#[test]
fn spend_summary_groups_models_and_excludes_outside_days() {
    let summary = spend_summary(
        &[
            row("2026-10-13", "claude", "opus", 3.0),
            row("2026-10-13", "codex", "gpt", 1.5),
            row("2026-10-12", "claude", "opus", 2.0),
            row("2026-09-14", "claude", "haiku", 0.5),
            row("2026-09-13", "claude", "opus", 9.0),
            row("2026-10-14", "codex", "gpt", 7.0),
            row("invalid", "claude", "opus", 20.0),
        ],
        now(),
    );
    assert_eq!((summary.today, summary.yesterday, summary.last_window), (4.5, 2.0, 7.0));
    assert_eq!(summary.providers, ["claude", "codex"]);
    assert_eq!(
        summary.models.iter().map(|model| model.cost).collect::<Vec<_>>(),
        [5.0, 1.5, 0.5]
    );
}

#[test]
fn spend_summary_keeps_zero_models_and_sorts_ties_by_identity() {
    let summary = spend_summary(
        &[
            row("2026-10-13", "codex", "gpt", 0.0),
            row("2026-10-13", "claude", "opus", 0.0),
        ],
        now(),
    );
    assert_eq!(
        summary
            .models
            .iter()
            .map(|model| model.provider.as_str())
            .collect::<Vec<_>>(),
        ["claude", "codex"]
    );
}

#[test]
fn spend_report_returns_per_provider_summaries() {
    let report = spend_report(
        &[
            row("2026-10-13", "claude", "opus", 3.0),
            row("2026-10-12", "codex", "gpt", 2.0),
        ],
        now(),
    );
    assert_eq!(report.total.last_window, 5.0);
    assert_eq!(report.by_provider["claude"].today, 3.0);
    assert_eq!(report.by_provider["codex"].yesterday, 2.0);
}

#[test]
fn spend_summary_without_rows_has_no_providers() {
    let summary = spend_summary(&[], now());
    assert_eq!((summary.today, summary.yesterday, summary.last_window), (0.0, 0.0, 0.0));
    assert!(summary.models.is_empty());
    assert!(summary.providers.is_empty());
}
