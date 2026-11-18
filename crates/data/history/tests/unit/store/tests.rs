//! Exercises the shared store against real SQLite databases.

use std::fs;

use super::*;

#[path = "tests/accounts.rs"]
mod accounts;
#[path = "tests/demo.rs"]
mod demo;
#[path = "tests/export.rs"]
mod export;
#[path = "tests/legacy.rs"]
mod legacy;
#[path = "tests/locking.rs"]
mod locking;

fn sample(at: f64, used: f64, reset_at: Option<f64>) -> SampleWrite {
    SampleWrite {
        sample: Sample {
            at,
            key: "claude:session".into(),
            used,
            reset_at,
        },
        label: "Current session".into(),
    }
}

fn at(value: &str) -> f64 {
    value.parse::<jiff::Timestamp>().unwrap().as_second() as f64
}

fn query<'a>(start: f64, end: f64, keys: Option<&'a [String]>) -> SampleQuery<'a> {
    SampleQuery {
        keys,
        start,
        end,
        rollup: None,
        time_zone: "UTC",
        fallback_offset: 0,
        includes_end: true,
    }
}

fn snapshot<'a>(
    provider: &'a str,
    fingerprint: Option<&'a str>,
    now: f64,
    windows: &'a [SampleWrite],
) -> SnapshotWrite<'a> {
    SnapshotWrite {
        provider,
        fingerprint,
        now,
        fetched_at: now,
        network: true,
        credit_balance: None,
        credit_unlimited: false,
        windows,
    }
}

#[test]
fn snapshots_follow_sampling_and_account_switches() {
    let store = Store::open(None, 60).unwrap();
    let rows = [sample(1000.0, 10.0, None)];
    assert_eq!(
        store
            .record_snapshot(snapshot("claude", Some("first"), 1000.0, &rows))
            .unwrap(),
        1
    );
    assert_eq!(
        store
            .record_snapshot(snapshot("claude", Some("first"), 1060.0, &rows))
            .unwrap(),
        0
    );
    assert_eq!(store.stats().unwrap().sample_count, 1);
    store.activate_account("claude", "second").unwrap();
    assert_eq!(store.stats().unwrap().sample_count, 0);
    store.activate_account("claude", "first").unwrap();
    assert_eq!(store.stats().unwrap().sample_count, 1);
}

#[test]
fn analytics_scope_replaces_only_covered_rows() {
    let store = Store::open(None, 60).unwrap();
    let point = AnalyticsPoint {
        provider: "codex".into(),
        day: "2026-08-29".into(),
        metric: "turns".into(),
        series: "total".into(),
        value: 3.0,
    };
    let at = at("2026-08-29T12:00:00Z");
    store
        .record_analytics("codex", None, at, std::slice::from_ref(&point), &[])
        .unwrap();
    let scope = CoverageScope {
        start_day: "2026-08-29".into(),
        end_day: "2026-08-29".into(),
        metrics: vec!["turns".into()],
    };
    store.record_analytics("codex", None, at, &[], &[scope]).unwrap();
    assert!(store.analytics("codex", "2026-08-01", "2026-08-31").unwrap().is_empty());
}

#[test]
fn exports_and_queries_read_retained_rows() {
    let folder = tempfile::tempdir().unwrap();
    let store = Store::open(Some(&folder.path().join("history.sqlite3")), 60).unwrap();
    let at = at("2026-08-29T12:00:00Z");
    store
        .seed(&[sample(at, 20.0, None), sample(at + 60.0, 30.0, None)])
        .unwrap();
    assert_eq!(store.samples(query(at, at + 60.0, None)).unwrap().len(), 2);
    assert_eq!(
        store
            .samples(SampleQuery {
                rollup: Some(3600.0),
                ..query(at, at + 60.0, None)
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.summaries().unwrap()[0].last_percent, 30.0);
    assert_eq!(store.earliest_sample(None).unwrap(), Some(at));
    assert_eq!(store.stats().unwrap().sample_count, 2);
    let path = folder.path().join("history.csv");
    store.export_all(&path, false).unwrap();
    assert!(fs::read_to_string(&path).unwrap().contains("claude:session"));
    store.export_samples(&path, at, at + 60.0, Some(&[]), true).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 1);
    assert_eq!(store.clear().unwrap(), 2);
}

#[test]
fn usage_dates_count_increases_and_reset_changes() {
    let store = Store::open(None, 60).unwrap();
    let start = at("2026-08-29T12:00:00Z");
    store
        .seed(&[
            sample(start, 10.0, None),
            sample(start + 60.0, 10.0, None),
            sample(start + 120.0, 20.0, None),
            sample(start + 180.0, 0.0, Some(start + 3600.0)),
        ])
        .unwrap();
    let dates = store
        .last_usage_dates(Some(&["claude:session".into()]), start, start + 180.0)
        .unwrap();
    assert_eq!(dates, [("claude:session".into(), start + 120.0)]);
}

#[test]
fn daily_rollup_keeps_last_sample_across_dst_change() {
    let store = Store::open(None, 60).unwrap();
    let before = at("2026-03-08T09:30:00Z");
    let after = at("2026-03-08T10:30:00Z");
    store
        .seed(&[sample(before, 10.0, None), sample(after, 20.0, None)])
        .unwrap();
    let rows = store
        .samples(SampleQuery {
            rollup: Some(86_400.0),
            time_zone: "America/Los_Angeles",
            fallback_offset: -28_800,
            ..query(at("2026-03-08T08:00:00Z"), at("2026-03-08T12:00:00Z"), None)
        })
        .unwrap();
    assert_eq!(rows, [sample(after, 20.0, None).sample]);
}

#[test]
fn hourly_rollup_at_dst_boundary_keeps_the_last_sample() {
    let store = Store::open(None, 60).unwrap();
    let before = at("2026-03-08T09:59:58Z");
    let last = at("2026-03-08T09:59:59Z");
    store
        .seed(&[sample(before, 10.0, None), sample(last, 20.0, None)])
        .unwrap();
    let rows = store
        .samples(SampleQuery {
            rollup: Some(3600.0),
            time_zone: "America/Los_Angeles",
            fallback_offset: -28_800,
            ..query(before, last, None)
        })
        .unwrap();
    assert_eq!(rows, [sample(last, 20.0, None).sample]);
}

#[test]
fn retention_removes_archived_and_active_rows() {
    let store = Store::open(None, 60).unwrap();
    let old = at("2026-01-01T12:00:00Z");
    let now = at("2026-08-29T12:00:00Z");
    store.seed(&[sample(old, 10.0, None)]).unwrap();
    store.activate_account("claude", "first").unwrap();
    store.activate_account("claude", "second").unwrap();
    store.seed(&[sample(old, 20.0, None)]).unwrap();
    assert_eq!(store.apply_retention_days(7, now).unwrap().samples, 2);
    assert_eq!(store.stats().unwrap().sample_count, 0);
}

#[test]
fn opening_newer_schema_preserves_the_database() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("history.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=3").unwrap();
    drop(db);
    assert!(matches!(Store::open(Some(&path), 60), Err(HistoryError::Schema(3))));
}

#[test]
fn existing_mac_schemas_preserve_samples_when_upgraded() {
    for version in [1, 2] {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("history.sqlite3");
        let db = Connection::open(&path).unwrap();
        db.execute_batch(&format!(
            "CREATE TABLE samples (ts REAL NOT NULL, key TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, resets_at REAL, PRIMARY KEY (key, ts));
             CREATE TABLE analytics (provider TEXT NOT NULL, day TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY (provider, day, metric, series));
             CREATE TABLE analytics_accounts (provider TEXT PRIMARY KEY, fingerprint TEXT NOT NULL);
             INSERT INTO samples VALUES (1000,'claude:session','Session',42,NULL);
             PRAGMA user_version={version};"
        ))
        .unwrap();
        if version == 2 {
            db.execute_batch(
                "CREATE TABLE archived_samples (fingerprint TEXT NOT NULL, ts REAL NOT NULL, key TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, resets_at REAL, PRIMARY KEY(fingerprint, key, ts));
                 CREATE TABLE archived_analytics (fingerprint TEXT NOT NULL, provider TEXT NOT NULL, day TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(fingerprint, provider, day, metric, series));",
            )
            .unwrap();
        }
        drop(db);
        let store = Store::open(Some(&path), 60).unwrap();
        assert_eq!(store.stats().unwrap().sample_count, 1, "version {version}");
        assert_eq!(store.earliest_sample(None).unwrap(), Some(1000.0));
    }
}

#[test]
fn retention_configuration_clamps_and_invalidates_pruning() {
    let store = Store::open(None, 0).unwrap();
    assert_eq!(store.retention_days(), 7);
    let old = at("2026-08-01T12:00:00Z");
    let now = at("2026-08-29T12:00:00Z");
    store.seed(&[sample(old, 20.0, None)]).unwrap();
    store.apply_retention_days(365, now).unwrap();
    store.set_retention_days(365);
    assert_eq!(store.retention_days(), 365);
    store.set_retention_days(0);
    assert_eq!(store.retention_days(), 7);
    store.record_snapshot(snapshot("claude", None, now, &[])).unwrap();
    assert_eq!(store.stats().unwrap().sample_count, 0);
    store.interrupt();
}

#[test]
fn queries_respect_selected_keys_endpoints_and_missing_rows() {
    let store = Store::open(None, 60).unwrap();
    let start = at("2026-08-29T12:00:00Z");
    assert!(
        store
            .samples(SampleQuery {
                rollup: Some(60.0),
                ..query(start, start + 60.0, None)
            })
            .unwrap()
            .is_empty()
    );
    store
        .seed(&[sample(start, 10.0, None), sample(start + 60.0, 20.0, None)])
        .unwrap();
    let keys = ["claude:session".to_string()];
    assert!(store.samples(query(start, start + 60.0, Some(&[]))).unwrap().is_empty());
    assert_eq!(
        store
            .samples(SampleQuery {
                includes_end: false,
                ..query(start, start + 60.0, Some(&keys))
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        store
            .samples(SampleQuery {
                rollup: Some(60.0),
                time_zone: "Bad/Zone",
                includes_end: false,
                ..query(start, start + 60.0, Some(&keys))
            })
            .unwrap()
            .len(),
        1
    );
    assert_eq!(store.earliest_sample(Some(&keys)).unwrap(), Some(start));
    assert_eq!(store.earliest_sample(Some(&[])).unwrap(), None);
    assert_eq!(store.earliest_sample(Some(&["missing".into()])).unwrap(), None);
    assert!(
        store
            .last_usage_dates(Some(&[]), start, start + 60.0)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn analytics_queries_filter_metric_provider_and_endpoint() {
    let store = Store::open(None, 60).unwrap();
    let first = AnalyticsPoint {
        provider: "codex".into(),
        day: "2026-08-29".into(),
        metric: "turns".into(),
        series: "cli".into(),
        value: 3.0,
    };
    let mut second = first.clone();
    second.day = "2026-08-30".into();
    second.value = 5.0;
    let now = at("2026-08-30T12:00:00Z");
    assert_eq!(
        store
            .record_analytics("codex", Some("first"), now, &[first.clone(), second.clone()], &[])
            .unwrap(),
        2
    );
    let providers = ["codex".to_string()];
    assert_eq!(
        store.analytics("codex", "2026-08-29", "2026-08-30").unwrap(),
        [first.clone(), second.clone()]
    );
    assert_eq!(
        store
            .analytics_rows("turns", &providers, "2026-08-29", "2026-08-30", false)
            .unwrap(),
        std::slice::from_ref(&first)
    );
    assert_eq!(
        store
            .analytics_rows("turns", &providers, "2026-08-29", "2026-08-30", true)
            .unwrap(),
        [first, second]
    );
    assert!(
        store
            .analytics_rows("turns", &[], "2026-08-29", "2026-08-30", true)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store.earliest_analytics("turns", &providers).unwrap(),
        Some(at("2026-08-29T00:00:00Z"))
    );
    assert_eq!(store.earliest_analytics("turns", &[]).unwrap(), None);
    assert_eq!(store.earliest_analytics("missing", &providers).unwrap(), None);
    assert_eq!(store.stats().unwrap().analytics_count, 2);
}

#[test]
fn analytics_rejects_invalid_days_when_finding_earliest() {
    let store = Store::open(None, 60).unwrap();
    let now = at("2026-08-29T12:00:00Z");
    store
        .record_analytics(
            "codex",
            None,
            now,
            &[AnalyticsPoint {
                provider: "codex".into(),
                day: "2026-99-99".into(),
                metric: "turns".into(),
                series: "total".into(),
                value: 1.0,
            }],
            &[],
        )
        .unwrap();
    assert_eq!(store.earliest_analytics("turns", &["codex".into()]).unwrap(), None);
}

#[test]
fn exports_filter_rows_and_hide_project_identity() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("history.csv");
    let store = Store::open(None, 60).unwrap();
    let now = at("2026-08-29T12:00:00Z");
    store
        .seed(&[sample(now, 20.0, Some(now + 3600.0)), sample(now + 60.0, 30.0, None)])
        .unwrap();
    let point = AnalyticsPoint {
        provider: "codex".into(),
        day: "2026-08-29".into(),
        metric: "turns".into(),
        series: "total".into(),
        value: 7.0,
    };
    store.record_analytics("codex", None, now, &[point], &[]).unwrap();
    store.export_all(&path, true).unwrap();
    let all = fs::read_to_string(&path).unwrap();
    assert!(all.contains("analytics,,"));
    assert!(all.contains("2026-08-29"));
    assert!(all.contains("7.0000"));
    store
        .export_samples(&path, now, now + 60.0, Some(&["claude:session".into()]), false)
        .unwrap();
    let selected = fs::read_to_string(&path).unwrap();
    assert_eq!(selected.lines().count(), 2);
    assert!(selected.contains("20.00"));
    store.export_samples(&path, now, now + 60.0, None, true).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 3);
    store
        .export_analytics(&path, "turns", &["codex".into()], now, now, true)
        .unwrap();
    let analytics = fs::read_to_string(&path).unwrap();
    assert_eq!(analytics.lines().count(), 2);
    assert!(analytics.contains("7.0000"));
    store.export_analytics(&path, "turns", &[], now, now, false).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap().lines().count(), 1);
    assert!(matches!(
        store.export_all(&folder.path().join("missing/history.csv"), false),
        Err(HistoryError::ExportDirectory)
    ));
}

#[test]
fn codex_credit_snapshots_require_a_network_balance() {
    let store = Store::open(None, 60).unwrap();
    let now = at("2026-08-29T12:00:00Z");
    for (provider, network, balance, unlimited) in [
        ("claude", true, Some(5.0), false),
        ("codex", false, Some(5.0), false),
        ("codex", true, Some(5.0), true),
        ("codex", true, None, false),
    ] {
        assert_eq!(
            store
                .record_snapshot(SnapshotWrite {
                    network,
                    credit_balance: balance,
                    credit_unlimited: unlimited,
                    ..snapshot(provider, None, now, &[])
                })
                .unwrap(),
            0
        );
    }
    assert_eq!(
        store
            .record_snapshot(SnapshotWrite {
                credit_balance: Some(5.0),
                ..snapshot("codex", None, now, &[])
            })
            .unwrap(),
        1
    );
    assert_eq!(
        store.analytics("codex", "2026-08-29", "2026-08-29").unwrap()[0].value,
        5.0
    );
}

#[test]
fn analytics_scope_skips_expired_days_and_combines_matching_series() {
    let store = Store::open(None, 60).unwrap();
    let now = at("2026-08-29T12:00:00Z");
    let point = AnalyticsPoint {
        provider: "codex".into(),
        day: "2026-08-29".into(),
        metric: "turns".into(),
        series: "total".into(),
        value: 3.0,
    };
    let expired = AnalyticsPoint {
        day: "2026-01-01".into(),
        ..point.clone()
    };
    let scopes = [CoverageScope {
        start_day: "2026-01-01".into(),
        end_day: "2026-01-31".into(),
        metrics: vec!["turns".into()],
    }];
    assert_eq!(
        store
            .record_analytics("codex", Some("first"), now, &[point.clone(), point, expired], &scopes)
            .unwrap(),
        1
    );
    assert_eq!(
        store.analytics("codex", "2026-08-29", "2026-08-29").unwrap()[0].value,
        6.0
    );
    store.activate_account("codex", "second").unwrap();
    assert!(store.analytics("codex", "2026-08-29", "2026-08-29").unwrap().is_empty());
    store.activate_account("codex", "first").unwrap();
    assert_eq!(
        store.analytics("codex", "2026-08-29", "2026-08-29").unwrap()[0].value,
        6.0
    );
}

#[test]
fn read_only_store_queries_and_rejects_writes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite");
    let writer = Store::open(Some(&path), 60).unwrap();
    writer.seed(&[sample(1000.0, 24.0, None)]).unwrap();
    let reader = Store::open_read_only(&path, 60).unwrap();
    assert_eq!(
        reader.samples(query(900.0, 1100.0, None)).unwrap(),
        [sample(1000.0, 24.0, None).sample]
    );
    assert!(reader.seed(&[sample(1060.0, 50.0, None)]).is_err());
    assert_eq!(writer.stats().unwrap().sample_count, 1);
}

#[test]
fn read_only_store_does_not_create_or_migrate_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite");
    assert!(matches!(
        Store::open_read_only(&path, 60),
        Err(HistoryError::Database(_))
    ));
    assert!(!path.exists());
    let db = Connection::open(&path).unwrap();
    assert!(matches!(Store::open_read_only(&path, 60), Err(HistoryError::Schema(0))));
    let version: i32 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, 0);
}

#[test]
fn read_only_store_can_read_pre_archive_history() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("history.sqlite");
    Store::open(Some(&path), 60)
        .unwrap()
        .seed(&[sample(1000.0, 24.0, None)])
        .unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("DROP TABLE archived_samples; DROP TABLE archived_analytics; PRAGMA user_version=1;")
        .unwrap();
    let reader = Store::open_read_only(&path, 60).unwrap();
    assert_eq!(reader.stats().unwrap().sample_count, 1);
    let version: i32 = db.pragma_query_value(None, "user_version", |row| row.get(0)).unwrap();
    assert_eq!(version, 1);
}
