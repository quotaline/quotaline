//! Checks the Swift history boundary against real SQLite stores.

use super::*;
use quotaline_analytics::{CoverageScope, Point, WindowRollup};
use quotaline_history::{Sample, SampleWrite};

#[path = "tests/analytics.rs"]
mod analytics;
#[path = "tests/charts.rs"]
mod charts;
#[path = "tests/export.rs"]
mod export;
#[path = "tests/samples.rs"]
mod samples;

fn at(value: &str) -> f64 {
    value.parse::<jiff::Timestamp>().unwrap().as_second() as f64
}

fn sample(at: f64, used: f64) -> SampleWrite {
    SampleWrite {
        sample: Sample {
            at,
            key: "claude:session".into(),
            used,
            reset_at: None,
        },
        label: "Current session".into(),
    }
}

fn query(start: f64, end: f64) -> StoredSampleQuery {
    StoredSampleQuery {
        keys: None,
        start,
        end,
        rollup: None,
        time_zone: "UTC".into(),
        fallback_offset: 0,
        includes_end: true,
    }
}

#[test]
fn store_metadata_matches_the_shared_history_store() {
    assert_eq!(
        history_store_metadata(),
        HistoryStoreMetadata {
            schema_version: 2,
            default_retention_days: 60,
            demo_days: 30,
        }
    );
}

#[test]
fn demo_seed_fills_samples_and_analytics() {
    let store = RustHistoryStore::new(None, 60).unwrap();
    store.seed_demo(vec!["codex".into()], 100_000.0, false).unwrap();
    let stats = store.stats().unwrap();
    assert!(stats.sample_count > 0);
    assert!(stats.analytics_count > 0);
}

#[test]
fn opening_newer_schema_reports_its_version() {
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("history.sqlite3");
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("PRAGMA user_version=3").unwrap();
    drop(db);
    assert!(matches!(
        RustHistoryStore::new(Some(path.to_string_lossy().into_owned()), 60),
        Err(HistoryStoreError::Schema { version: 3 })
    ));
}

#[test]
fn opening_below_a_regular_file_reports_a_database_error() {
    let folder = tempfile::tempdir().unwrap();
    let parent = folder.path().join("file");
    std::fs::write(&parent, b"file").unwrap();
    let path = parent.join("history.sqlite3");
    assert!(matches!(
        RustHistoryStore::new(Some(path.to_string_lossy().into_owned()), 60),
        Err(HistoryStoreError::Database { .. })
    ));
}
