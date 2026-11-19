use super::*;

fn pair() -> (tempfile::TempDir, Live, Live) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("live.sqlite3");
    let first = Live::open(&path, "first".into()).unwrap();
    let second = Live::open(&path, "second".into()).unwrap();
    (directory, first, second)
}

#[test]
fn one_engine_holds_the_lease_until_it_expires_or_resigns() {
    let (_directory, first, second) = pair();
    assert!(first.lead(100.0, 30.0).unwrap());
    assert!(!second.lead(100.0, 30.0).unwrap());
    assert!(first.lead(120.0, 30.0).unwrap());
    assert!(!second.lead(149.0, 30.0).unwrap());
    assert!(second.lead(150.0, 30.0).unwrap());
    assert!(!first.lead(151.0, 30.0).unwrap());
    second.resign().unwrap();
    assert!(first.lead(152.0, 30.0).unwrap());
}

#[test]
fn published_state_reaches_the_other_engine_once() {
    let (_directory, first, second) = pair();
    assert_eq!(second.published::<Vec<u32>>().unwrap(), None);
    assert!(!second.changed().unwrap());
    first.publish(&vec![1, 2]).unwrap();
    assert!(!first.changed().unwrap());
    assert!(second.changed().unwrap());
    assert!(!second.changed().unwrap());
    first.publish(&vec![3]).unwrap();
    assert!(second.changed().unwrap());
    assert_eq!(second.published::<Vec<u32>>().unwrap(), Some(vec![3]));
}

#[test]
fn refresh_requests_reach_the_holder_in_order_once() {
    let (_directory, first, second) = pair();
    second.request(&"codex").unwrap();
    second.request(&"claude").unwrap();
    assert_eq!(first.take_requests::<String>().unwrap(), ["codex", "claude"]);
    assert!(first.take_requests::<String>().unwrap().is_empty());
    second.request(&1).unwrap();
    assert!(matches!(
        first.take_requests::<String>().unwrap_err(),
        LiveError::Json(_)
    ));
}

#[test]
fn a_visible_panel_counts_until_it_hides_or_goes_quiet() {
    let (_directory, first, second) = pair();
    assert!(!first.visible(100.0, 30.0).unwrap());
    second.present(true, 100.0).unwrap();
    assert!(!second.visible(100.0, 30.0).unwrap(), "an engine counted its own panel");
    assert!(first.visible(129.0, 30.0).unwrap());
    assert!(!first.visible(131.0, 30.0).unwrap());
    second.present(true, 131.0).unwrap();
    assert!(first.visible(131.0, 30.0).unwrap());
    second.present(false, 132.0).unwrap();
    assert!(!first.visible(132.0, 30.0).unwrap());
    second.present(true, 133.0).unwrap();
    second.resign().unwrap();
    assert!(!first.visible(133.0, 30.0).unwrap());
}

#[test]
fn each_planned_notice_is_claimed_once_in_order_unless_it_expired() {
    let (_directory, first, second) = pair();
    first.post(&["stale"], 100.0).unwrap();
    first.post(&["reset", "threshold"], 200.0).unwrap();
    assert_eq!(second.pending(250.0, 60.0).unwrap(), 2);
    assert_eq!(second.claim::<String>(250.0, 60.0).unwrap(), ["reset", "threshold"]);
    assert!(first.claim::<String>(250.0, 60.0).unwrap().is_empty());
}

#[test]
fn malformed_rows_report_json_errors() {
    let (_directory, first, second) = pair();
    first.publish(&"text").unwrap();
    assert_eq!(
        second.published::<Vec<u32>>().unwrap_err().to_string(),
        "invalid type: string \"text\", expected a sequence at line 1 column 6"
    );
    first.post(&["text"], 100.0).unwrap();
    assert!(matches!(
        second.claim::<u32>(100.0, 60.0).unwrap_err(),
        LiveError::Json(_)
    ));
}

#[test]
fn a_damaged_store_reports_sql_errors() {
    let (directory, first, _second) = pair();
    let raw = Connection::open(directory.path().join("live.sqlite3")).unwrap();
    raw.execute_batch("DROP TABLE lease; DROP TABLE published; DROP TABLE presence; DROP TABLE outbox;")
        .unwrap();
    assert_eq!(first.lead(100.0, 30.0).unwrap_err().to_string(), "no such table: lease");
    assert!(matches!(first.resign().unwrap_err(), LiveError::Sql(_)));
    assert!(matches!(first.pending(100.0, 60.0).unwrap_err(), LiveError::Sql(_)));
    assert!(matches!(first.publish(&1).unwrap_err(), LiveError::Sql(_)));
    assert!(matches!(first.present(true, 100.0).unwrap_err(), LiveError::Sql(_)));
    assert!(matches!(first.visible(100.0, 30.0).unwrap_err(), LiveError::Sql(_)));
    assert!(matches!(
        Live::open(directory.path(), "holder".into()).err().unwrap(),
        LiveError::Sql(_)
    ));
}
