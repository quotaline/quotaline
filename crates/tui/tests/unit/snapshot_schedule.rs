use super::*;

#[test]
fn schedules_the_next_read_after_the_poll_interval() {
    let now = Instant::now();
    let mut schedule = SnapshotSchedule::new(now);
    assert!(schedule.start(now));
    schedule.finish();
    assert!(!schedule.start(now + Duration::from_secs(1)));
    assert!(schedule.start(now + Duration::from_secs(2)));
}

#[test]
fn defers_reload_until_the_current_read_finishes() {
    let now = Instant::now();
    let mut schedule = SnapshotSchedule::new(now);
    assert!(schedule.start(now));
    let reload_at = now + Duration::from_secs(1);
    schedule.reload(reload_at);
    assert!(!schedule.start(reload_at));
    schedule.finish();
    assert!(schedule.start(reload_at));
}
