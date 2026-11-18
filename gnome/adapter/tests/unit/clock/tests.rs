//! Formats times in a fixed zone the way a GNOME session would.

use super::*;
use jiff::tz;

const NOON: i64 = 1_792_004_400;

fn clock(twelve_hour: bool) -> SessionClock {
    SessionClock::new(TimeZone::fixed(tz::offset(-7)), twelve_hour)
}

#[test]
fn times_follow_the_session_clock_preference() {
    assert_eq!(clock(false).time(NOON + 3600), "13:00");
    assert_eq!(clock(true).time(NOON + 3600), "1:00 PM");
}

#[test]
fn weekdays_and_dates_carry_the_time() {
    assert_eq!(clock(false).weekday_time(NOON + 86_400), "Thu 12:00");
    assert_eq!(clock(true).date_time(NOON + 7 * 86_400), "Oct 21 12:00 PM");
}

#[test]
fn day_precision_reads_the_utc_date() {
    assert_eq!(clock(false).utc_day(NOON - 13 * 3600), "Oct 14");
}

#[test]
fn local_days_change_at_local_midnight() {
    let clock = clock(false);
    assert!(clock.same_local_day(NOON + 3600, NOON));
    assert!(!clock.same_local_day(NOON + 19 * 3600, NOON));
}
