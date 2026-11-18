//! Review aggregation cases.

use super::*;

fn now() -> Timestamp {
    "2026-10-13T12:00:00Z".parse().unwrap()
}

#[test]
fn no_review_rows_has_no_summary() {
    assert_eq!(review_counts(&[], now()), None);
}

#[test]
fn review_summary_counts_today_and_six_previous_days() {
    let rows = [
        ReviewRow {
            day: "2026-10-13".into(),
            value: 2.9,
        },
        ReviewRow {
            day: "2026-10-07".into(),
            value: 3.0,
        },
        ReviewRow {
            day: "2026-10-06".into(),
            value: 7.0,
        },
        ReviewRow {
            day: "2026-10-14".into(),
            value: 8.0,
        },
        ReviewRow {
            day: "bad".into(),
            value: 9.0,
        },
    ];
    assert_eq!(review_counts(&rows, now()), Some(ReviewCounts { today: 2, week: 5 }));
}

#[test]
fn old_review_rows_still_show_zero_counts() {
    let rows = [ReviewRow {
        day: "2026-09-01".into(),
        value: 10.0,
    }];
    assert_eq!(review_counts(&rows, now()), Some(ReviewCounts { today: 0, week: 0 }));
}
