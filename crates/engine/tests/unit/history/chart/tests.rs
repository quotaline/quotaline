//! Shapes stored usage samples into Shell chart series.

use super::*;
use std::sync::Arc;

#[test]
fn history_selection_keeps_only_selected_series() {
    let mut history = History {
        metric: "usage".into(),
        start: 0,
        end: 1,
        series: ["claude:session", "codex:weekly"]
            .into_iter()
            .map(|id| Series {
                id: id.into(),
                provider: id.split_once(':').unwrap().0.into(),
                label: id.into(),
                points: vec![Point(0, 25.0, None, false)],
            })
            .collect(),
        canonical_total: None,
    };
    history.retain_selected(None);
    assert_eq!(history.series.len(), 2);
    history.retain_selected(Some(&BTreeSet::from(["codex:weekly".into()])));
    assert_eq!(history.series[0].id, "codex:weekly");
    history.retain_selected(Some(&BTreeSet::new()));
    assert!(history.series.is_empty());
}

#[test]
fn chart_history_filters_parallel_totals_and_preserves_canonical_sum() {
    let history = History {
        metric: "turns".into(),
        start: 0,
        end: 3,
        canonical_total: None,
        series: vec![
            Series {
                id: "codex:total".into(),
                provider: "codex".into(),
                label: "total".into(),
                points: vec![Point(1, 10.0, None, false), Point(2, 20.0, None, false)],
            },
            Series {
                id: "codex:surface:cli".into(),
                provider: "codex".into(),
                label: "surface:cli".into(),
                points: vec![Point(1, 4.0, None, false)],
            },
            Series {
                id: "codex:surface:web".into(),
                provider: "codex".into(),
                label: "surface:web".into(),
                points: vec![Point(1, 6.0, None, false)],
            },
            Series {
                id: "claude:other".into(),
                provider: "claude".into(),
                label: "other".into(),
                points: vec![Point(1, 3.0, None, false)],
            },
        ],
    }
    .prepared();
    assert_eq!(history.canonical_total, Some(33.0));
    assert_eq!(history.series[0].points.len(), 1);
    assert_eq!(history.series[0].points[0].0, 2);
    assert_eq!(history.series.len(), 4);
}

#[test]
fn chart_history_uses_surfaces_and_drops_empty_series() {
    let history = History {
        metric: "credits".into(),
        start: 0,
        end: 3,
        canonical_total: None,
        series: vec![
            Series {
                id: "cli".into(),
                provider: "codex".into(),
                label: "surface:cli".into(),
                points: vec![Point(1, 4.0, None, false)],
            },
            Series {
                id: "web".into(),
                provider: "codex".into(),
                label: "surface:web".into(),
                points: vec![Point(1, 6.0, None, false)],
            },
            Series {
                id: "other".into(),
                provider: "codex".into(),
                label: "other".into(),
                points: vec![Point(1, 2.0, None, false), Point(2, 5.0, None, false)],
            },
            Series {
                id: "empty".into(),
                provider: "codex".into(),
                label: "total".into(),
                points: vec![],
            },
        ],
    }
    .prepared();
    assert_eq!(history.canonical_total, Some(15.0));
    assert_eq!(history.series.len(), 3);
    let usage = History {
        metric: "usage".into(),
        start: 0,
        end: 1,
        canonical_total: None,
        series: vec![Series {
            id: "empty".into(),
            provider: "codex".into(),
            label: "usage".into(),
            points: vec![],
        }],
    }
    .prepared();
    assert!(usage.series.is_empty());
    assert_eq!(usage.canonical_total, None);
}

#[test]
fn usage_history_drops_to_zero_where_a_window_reset() {
    let store = Store::open(None, 60).unwrap();
    let row = |at: f64, used: f64, reset_at: f64| quotaline_history::SampleWrite {
        sample: quotaline_history::Sample {
            at,
            key: "claude:session".into(),
            used,
            reset_at: Some(reset_at),
        },
        label: "5h".into(),
    };
    store
        .seed(&[row(6_060.0, 80.0, 6_120.0), row(6_300.0, 20.0, 9_600.0)])
        .unwrap();
    let history = history(
        Arc::new(store),
        6_000,
        6_400,
        &BTreeMap::new(),
        WindowRollup::Minute,
        true,
    )
    .unwrap();
    assert_eq!(
        history.series[0].points,
        [
            Point(6_060, 80.0, Some(6_120.0), false),
            Point(6_120, 0.0, Some(6_120.0), false),
            Point(6_300, 20.0, Some(9_600.0), false),
            Point(6_400, 20.0, Some(9_600.0), true),
        ]
    );
}

#[test]
fn usage_history_is_bounded_and_draws_the_latest_reading_to_the_period_end() {
    let store = Store::open(None, 60).unwrap();
    let rows = (0..2000)
        .map(|index| quotaline_history::SampleWrite {
            sample: quotaline_history::Sample {
                at: f64::from(index),
                key: "claude:session".into(),
                used: f64::from(index) / 20.0,
                reset_at: None,
            },
            label: "Session".into(),
        })
        .collect::<Vec<_>>();
    store.seed(&rows).unwrap();
    let history = history(Arc::new(store), 0, 2000, &BTreeMap::new(), WindowRollup::Minute, true).unwrap();
    assert!(history.series[0].points.len() <= 801);
    assert_eq!(
        history.series[0].points.last().unwrap(),
        &Point(2000, 1999.0 / 20.0, None, true)
    );
    assert_eq!(history.series[0].label, "Session");
}

#[test]
fn usage_history_starts_with_the_reading_before_the_period() {
    let store = Store::open(None, 60).unwrap();
    let row = |at: f64, used: f64| quotaline_history::SampleWrite {
        sample: quotaline_history::Sample {
            at,
            key: "claude:session".into(),
            used,
            reset_at: None,
        },
        label: "Session".into(),
    };
    store.seed(&[row(10_000.0, 30.0), row(10_500.0, 40.0)]).unwrap();
    let history = history(
        Arc::new(store),
        10_200,
        11_000,
        &BTreeMap::new(),
        WindowRollup::Minute,
        true,
    )
    .unwrap();
    assert_eq!(history.series[0].points.first().unwrap().1, 30.0);
}

#[test]
fn usage_history_skips_samples_without_a_provider_key() {
    let store = Store::open(None, 60).unwrap();
    let row = |key: &str| quotaline_history::SampleWrite {
        sample: quotaline_history::Sample {
            at: 100.0,
            key: key.into(),
            used: 5.0,
            reset_at: None,
        },
        label: "Session".into(),
    };
    store.seed(&[row("claude:session"), row("invalid")]).unwrap();
    let history = history(Arc::new(store), 0, 200, &BTreeMap::new(), WindowRollup::Minute, true).unwrap();
    let ids = history
        .series
        .iter()
        .map(|series| series.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["claude:session"]);
}

#[test]
fn usage_history_leaves_out_disabled_providers() {
    let store = Store::open(None, 60).unwrap();
    store
        .seed_demo(&["claude".into(), "codex".into()], 100_000.0, false)
        .unwrap();
    let disabled = BTreeMap::from([("codex".into(), false)]);
    let history = history(Arc::new(store), 0, 100_000, &disabled, WindowRollup::Hour, false).unwrap();
    assert!(!history.series.is_empty());
    assert!(history.series.iter().all(|series| series.provider == "claude"));
}

#[test]
fn usage_history_reports_an_unreadable_store() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("history.sqlite3");
    let store = Store::open(Some(&path), 60).unwrap();
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE samples")
        .unwrap();
    assert!(history(Arc::new(store), 0, 200, &BTreeMap::new(), WindowRollup::Minute, true).is_err());
}
