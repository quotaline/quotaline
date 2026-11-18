//! GNOME review storage cases.

use super::*;
use crate::history::{Point, Series};

#[test]
fn demo_review_counts_use_shared_aggregation() {
    let now = Timestamp::now();
    let store = Store::open(None, 60).unwrap();
    store
        .seed_demo(&["codex".into()], now.as_second() as f64, false)
        .unwrap();
    let counts = review_counts(&store, &BTreeMap::new(), 60, now).unwrap();
    let counts = counts.unwrap();
    assert!(counts.today > 0);
    assert!(counts.week >= counts.today);
}

#[test]
fn history_review_counts_ignore_other_providers() {
    let now = "2026-10-13T12:00:00Z".parse::<Timestamp>().unwrap();
    let at = "2026-10-13T00:00:00Z".parse::<Timestamp>().unwrap().as_second();
    let history = History {
        metric: "codeReviews".into(),
        start: at,
        end: at,
        series: vec![
            Series {
                id: "claude:reviews".into(),
                provider: "claude".into(),
                label: "reviews".into(),
                points: vec![Point(at, 20.0, None, false)],
            },
            Series {
                id: "codex:reviews".into(),
                provider: "codex".into(),
                label: "reviews".into(),
                points: vec![Point(at, 3.0, None, false)],
            },
        ],
        canonical_total: None,
    };
    assert_eq!(
        counts_from_history(&history, now),
        Some(analytics::ReviewCounts { today: 3, week: 3 })
    );
}

#[test]
fn history_without_codex_reviews_has_no_summary() {
    let now = "2026-10-13T12:00:00Z".parse::<Timestamp>().unwrap();
    let history = History {
        metric: "codeReviews".into(),
        start: 0,
        end: 0,
        series: Vec::new(),
        canonical_total: None,
    };
    assert_eq!(counts_from_history(&history, now), None);
}
