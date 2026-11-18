use super::*;

#[test]
fn observation_distinguishes_fresh_readings_elapsed_time_and_clock_skew() {
    let clock = ReadingTime {
        now: "2026-11-18T12:00:00Z".parse().unwrap(),
        today: "2026-11-18".parse().unwrap(),
    };
    for (stamp, expected) in [
        ("2026-11-18T12:00:04.999Z", "Observed just now"),
        ("2026-11-18T12:00:05Z", "Observed < 1 min ahead"),
        ("2026-11-18T12:05:00Z", "Observed 5 min ahead"),
        ("2026-11-18T11:59:55.001Z", "Observed just now"),
        ("2026-11-18T11:59:55Z", "Observed 5s ago"),
        ("2026-11-18T11:55:00Z", "Observed 5 min ago"),
        ("2026-11-18T10:00:00Z", "Observed 2 hr ago"),
        ("2026-11-17T12:00:00Z", "Observed 1 d ago"),
    ] {
        assert_eq!(clock.observed(stamp.parse().unwrap()), expected);
    }
}

#[test]
fn precise_resets_distinguish_future_due_and_elapsed_deadlines() {
    let clock = ReadingTime {
        now: "2026-11-18T12:00:00Z".parse().unwrap(),
        today: "2026-11-18".parse().unwrap(),
    };
    for (stamp, expected) in [
        ("2026-11-18T12:00:00.001Z", "Resets in < 1 min"),
        ("2026-11-18T12:05:00Z", "Resets in 5 min"),
        ("2026-11-18T14:30:00Z", "Resets in 2 hr 30 min"),
        ("2026-11-19T15:00:00Z", "Resets in 1d 3h"),
        ("2026-11-18T12:00:00Z", "Reset due now"),
        ("2026-11-18T11:59:59.999Z", "Reset was due just now"),
        ("2026-11-18T11:55:00Z", "Reset was due 5 min ago"),
    ] {
        assert_eq!(clock.reset(stamp.parse().unwrap(), false), expected);
    }
}

#[test]
fn day_precision_compares_the_reported_date_to_the_local_calendar() {
    let clock = ReadingTime {
        now: "2027-01-01T03:00:00Z".parse().unwrap(),
        today: "2026-12-31".parse().unwrap(),
    };
    for (stamp, expected) in [
        ("2026-12-29T00:00:00Z", "Reset was due 2 days ago"),
        ("2026-12-30T00:00:00Z", "Reset was due yesterday"),
        ("2026-12-31T00:00:00Z", "Reset due today"),
        ("2027-01-01T00:00:00Z", "Resets tomorrow"),
        ("2027-01-03T00:00:00Z", "Resets in 3 days"),
    ] {
        assert_eq!(clock.reset(stamp.parse().unwrap(), true), expected);
    }
}

#[test]
fn day_precision_counts_month_boundaries_in_leap_years() {
    let clock = ReadingTime {
        now: "2028-02-28T12:00:00Z".parse().unwrap(),
        today: "2028-02-28".parse().unwrap(),
    };
    assert_eq!(
        clock.reset("2028-03-01T00:00:00Z".parse().unwrap(), true),
        "Resets in 2 days"
    );
}
