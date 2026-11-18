//! GNOME spend storage cases.

use super::*;
use crate::history::{Point, Series};

#[test]
fn demo_spend_report_uses_shared_daily_totals() {
    let now = "2026-10-13T12:00:00Z".parse::<Timestamp>().unwrap();
    let store = Store::open(None, 60).unwrap();
    store
        .seed_demo(&["claude".into()], now.as_second() as f64, false)
        .unwrap();
    let report = spend_report(&store, &BTreeMap::new(), now).unwrap();
    assert!(report.total.last_window > 0.0);
    assert!(report.by_provider.contains_key("claude"));
}

#[test]
fn history_spend_report_groups_series_by_provider() {
    let now = "2026-10-13T12:00:00Z".parse::<Timestamp>().unwrap();
    let history = History {
        metric: "costUSD".into(),
        start: 0,
        end: 0,
        canonical_total: None,
        series: vec![Series {
            id: "claude:opus".into(),
            provider: "claude".into(),
            label: "opus".into(),
            points: vec![
                Point(
                    "2026-10-13T00:00:00Z".parse::<Timestamp>().unwrap().as_second(),
                    4.0,
                    None,
                    false,
                ),
                Point(i64::MAX, 3.0, None, false),
            ],
        }],
    };
    let report = report_from_history(&history, now);
    assert_eq!(report.by_provider["claude"].today, 4.0);
}
