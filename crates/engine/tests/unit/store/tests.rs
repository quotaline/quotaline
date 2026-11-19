//! Checks store behavior through the public API.

use super::*;
use quotaline_quota::Window;
use serde::Serialize;
use std::os::unix::fs::PermissionsExt;

fn turns(day: &str, metric: &str, value: f64) -> analytics::Point {
    analytics::Point {
        day: day.into(),
        provider: "codex".into(),
        metric: metric.into(),
        series: "total".into(),
        value,
    }
}

fn day_start(day: &str) -> i64 {
    analytics::utc_day::midnight(day).unwrap() as i64
}

#[test]
fn recording_uses_shared_history_sampling_policy() {
    let store = Store::open(None, 60).unwrap();
    let mut provider = Provider::connected(
        "claude",
        "Pro".into(),
        "network".into(),
        "Local".into(),
        vec![Window::new("session", "Session", 25.0, None, "usage")],
    );
    for (at, used, reset, count) in [
        (1000.0, 25.0, None, 1),
        (1001.0, 27.0, None, 1),
        (1002.0, 30.0, None, 2),
        (1003.0, 30.0, Some("2026-10-10T10:00:00Z"), 3),
        (1303.0, 30.0, Some("2026-10-10T10:00:00Z"), 4),
    ] {
        provider.windows[0].used = used;
        provider.windows[0].reset = reset.map(str::to_owned);
        record_into(&store, &provider, at).unwrap();
        assert_eq!(store.stats().unwrap().sample_count, count, "at {at}");
    }
}

#[test]
fn recording_keeps_reset_times_with_the_sample() {
    let store = Store::open(None, 60).unwrap();
    let mut provider = Provider::connected(
        "claude",
        "Pro".into(),
        "network".into(),
        "Local".into(),
        vec![Window::new("session", "Session", 25.0, None, "usage")],
    );
    provider.windows[0].reset = Some("2026-10-10T09:30:00.250+00:00".into());
    record_into(&store, &provider, 1000.0).unwrap();
    let dates = store.last_usage_dates(None, 0.0, 2000.0).unwrap();
    assert_eq!(dates, [("claude:session".into(), 1000.0)]);
    let rows = store.samples(quotaline_history::SampleQuery {
        keys: None,
        start: 0.0,
        end: 2000.0,
        rollup: None,
        time_zone: "UTC",
        fallback_offset: 0,
        includes_end: true,
    });
    assert_eq!(rows.unwrap()[0].reset_at, Some(1_791_624_600.25));
}

#[test]
fn codex_credit_balance_is_recorded_only_from_network_fetches() {
    let store = Store::open(None, 60).unwrap();
    let mut provider = Provider::connected("codex", "Pro".into(), "network".into(), "Local".into(), Vec::new());
    provider.credit_balance = Some(12.5);
    provider.updated = Some(day_start("2026-08-01"));
    provider.status = "stale".into();
    record_into(&store, &provider, provider.updated.unwrap() as f64).unwrap();
    assert!(store.analytics("codex", "2026-08-01", "2026-08-01").unwrap().is_empty());
    provider.status = "connected".into();
    record_into(&store, &provider, provider.updated.unwrap() as f64).unwrap();
    assert_eq!(
        store.analytics("codex", "2026-08-01", "2026-08-01").unwrap()[0].value,
        12.5
    );
}

#[test]
fn concurrent_atomic_writes_keep_a_complete_private_document() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    std::thread::scope(|scope| {
        for index in 0..24 {
            let path = &path;
            scope.spawn(move || atomic_json(path, &serde_json::json!({"index": index})).unwrap());
        }
    });
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert!(value["index"].as_u64().unwrap() < 24);
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn atomic_write_removes_a_temp_file_after_encoding_failure() {
    struct Unserializable;

    impl Serialize for Unserializable {
        fn serialize<Serializer: serde::Serializer>(
            &self,
            _serializer: Serializer,
        ) -> std::result::Result<Serializer::Ok, Serializer::Error> {
            Err(serde::ser::Error::custom("cannot encode"))
        }
    }

    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    assert_eq!(
        atomic_json(&path, &Unserializable).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn atomic_write_removes_a_temp_file_after_rename_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("snapshot.json");
    fs::create_dir(&path).unwrap();
    assert!(atomic_json(&path, &serde_json::json!({"safe":true})).is_err());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn atomic_write_reports_a_non_directory_parent() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("blocked");
    fs::write(&parent, b"file").unwrap();
    assert_eq!(
        atomic_json(&parent.join("snapshot.json"), &serde_json::json!({}))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn analytics_corrections_replace_only_the_covered_metric_and_days() {
    let store = Store::open(None, 60).unwrap();
    let now = day_start("2026-08-03") as f64;
    let coverage = Coverage {
        metrics: &["turns", "credits"],
        start: "2026-08-01",
        end: "2026-08-02",
    };
    let points = [
        turns("2026-08-01", "turns", 3.0),
        turns("2026-08-02", "turns", 4.0),
        turns("2026-08-01", "credits", 9.0),
        turns("2026-07-31", "turns", 12.0),
        turns("2026-08-03", "turns", 13.0),
        turns("2026-08-01", "ignored", 14.0),
        analytics::Point {
            provider: "claude".into(),
            ..turns("2026-08-01", "turns", 15.0)
        },
    ];
    replace_analytics_into(&store, "codex", None, &coverage, &points, now).unwrap();
    let correction = Coverage {
        metrics: &["turns"],
        start: "2026-08-01",
        end: "2026-08-01",
    };
    replace_analytics_into(
        &store,
        "codex",
        None,
        &correction,
        &[turns("2026-08-01", "turns", 7.0)],
        now,
    )
    .unwrap();
    let start = day_start("2026-08-01");
    let end = start + 3 * 86_400;
    let turns = crate::history::history_analytics(&store, "turns", start, end, &BTreeMap::new()).unwrap();
    assert_eq!(
        turns.series[0].points.iter().map(|point| point.1).collect::<Vec<_>>(),
        vec![7.0, 4.0]
    );
    let credits = crate::history::history_analytics(&store, "credits", start, end, &BTreeMap::new()).unwrap();
    assert_eq!(credits.series[0].points[0].1, 9.0);
    let disabled = BTreeMap::from([("codex".into(), false)]);
    let hidden = crate::history::history_analytics(&store, "turns", start, end, &disabled).unwrap();
    assert!(hidden.series.is_empty());
}

#[test]
fn late_analytics_from_another_account_keep_the_active_history() {
    let store = Store::open(None, 60).unwrap();
    let now = day_start("2026-08-02") as f64;
    let coverage = Coverage {
        metrics: &["turns"],
        start: "2026-08-01",
        end: "2026-08-01",
    };
    replace_analytics_into(
        &store,
        "codex",
        Some("active"),
        &coverage,
        &[turns("2026-08-01", "turns", 7.0)],
        now,
    )
    .unwrap();
    replace_analytics_into(
        &store,
        "codex",
        Some("old"),
        &coverage,
        &[turns("2026-08-01", "turns", 3.0)],
        now,
    )
    .unwrap();
    assert_eq!(store.active_account("codex").unwrap().as_deref(), Some("active"));
    assert_eq!(
        store.analytics("codex", "2026-08-01", "2026-08-01").unwrap()[0].value,
        7.0
    );
}

fn snapshot(fingerprint: &str, value: f64) -> analytics::AnalyticsSnapshot {
    analytics::AnalyticsSnapshot {
        provider: "codex".into(),
        points: vec![turns("2026-08-01", "turns", value)],
        credit_events: Vec::new(),
        fetched_at: day_start("2026-08-02") as f64,
        account_fingerprint: Some(fingerprint.into()),
        covered_scopes: vec![analytics::CoverageScope {
            metrics: vec!["turns".into()],
            start_day: "2026-08-01".into(),
            end_day: "2026-08-02".into(),
        }],
    }
}

#[test]
fn snapshot_activates_its_account_and_records_its_points() {
    let store = Store::open(None, 60).unwrap();
    assert!(record_snapshot_into(&store, &snapshot("active", 7.0)).unwrap());
    assert_eq!(store.active_account("codex").unwrap().as_deref(), Some("active"));
    assert_eq!(
        store.analytics("codex", "2026-08-01", "2026-08-01").unwrap()[0].value,
        7.0
    );
}

#[test]
fn late_snapshot_from_another_account_keeps_the_active_history() {
    let store = Store::open(None, 60).unwrap();
    assert!(record_snapshot_into(&store, &snapshot("active", 7.0)).unwrap());
    assert!(!record_snapshot_into(&store, &snapshot("old", 3.0)).unwrap());
    assert_eq!(
        store.analytics("codex", "2026-08-01", "2026-08-01").unwrap()[0].value,
        7.0
    );
}

#[test]
fn analytics_exports_cover_only_enabled_providers() {
    let store = Store::open(None, 60).unwrap();
    store
        .seed_demo(&["claude".into(), "codex".into()], 100_000.0, false)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("cost.csv");
    let settings = Settings {
        providers: BTreeMap::from([("codex".into(), false)]),
        ..Settings::default()
    };
    export_analytics_from(&store, &settings, &file, "turns", (0, 100_000)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap().lines().count(), 1);
    export_analytics_from(&store, &Settings::default(), &file, "turns", (0, 100_000)).unwrap();
    assert!(fs::read_to_string(&file).unwrap().lines().count() > 1);
}

#[test]
fn usage_exports_keep_selected_windows_of_enabled_providers() {
    let store = Store::open(None, 60).unwrap();
    store
        .seed_demo(&["claude".into(), "codex".into()], 100_000.0, false)
        .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("usage.csv");
    let selected = BTreeSet::from(["claude:session".to_string(), "codex:session".to_string()]);
    let settings = Settings {
        providers: BTreeMap::from([("codex".into(), false)]),
        ..Settings::default()
    };
    export_from(&store, &settings, &file, (0, 100_000), Some(&selected)).unwrap();
    let csv = fs::read_to_string(&file).unwrap();
    assert!(csv.lines().skip(1).all(|line| line.contains(",claude:session,")));
    assert!(csv.lines().count() > 1);
    export_from(&store, &settings, &file, (0, 100_000), None).unwrap();
    let all = fs::read_to_string(&file).unwrap();
    assert!(all.contains(",claude:weekly,"));
    assert!(!all.contains(",codex:"));
}

#[test]
fn demo_settings_read_seeded_history() {
    let settings = Settings {
        demo: true,
        ..Settings::default()
    };
    let now = Timestamp::now().as_second();
    let history = rolled_history(&settings, now - 86_400, now, WindowRollup::Hour, true).unwrap();
    assert!(history.series.iter().any(|series| series.id == "claude:session"));
    let cost = history_analytics(&settings, "costUSD", now - 8 * 86_400, now).unwrap();
    assert!(!cost.series.is_empty());
    assert!(spend_report(&settings, Timestamp::now()).unwrap().total.last_window > 0.0);
    assert!(review_counts(&settings, Timestamp::now()).unwrap().is_some());
    assert!(!model_activity(&settings, now).unwrap().is_empty());
}

#[test]
fn demo_model_activity_follows_the_retention_period() {
    let now = Timestamp::now().as_second();
    let week = Settings {
        demo: true,
        retention_days: 7,
        ..Settings::default()
    };
    let month = Settings {
        retention_days: 30,
        ..week.clone()
    };
    let recent = model_activity(&week, now).unwrap();
    assert_eq!(model_activity(&week, now).unwrap(), recent);
    let longer = model_activity(&month, now).unwrap();
    assert!(longer.len() >= recent.len());
    assert!(recent.values().all(|at| *at >= now - 7 * 86_400));
}

#[test]
fn demo_history_provides_each_advertised_metric() {
    let settings = Settings {
        demo: true,
        ..Settings::default()
    };
    let now = Timestamp::now().as_second();
    for (metric, _, _, _) in analytics::METRICS {
        let history = history_analytics(&settings, metric, now - 8 * 86_400, now + 86_400).unwrap();
        assert!(!history.series.is_empty(), "{metric}");
    }
}

#[cfg(target_os = "linux")]
fn legacy_base(legacy: bool, current: bool) -> tempfile::TempDir {
    let base = tempfile::tempdir().unwrap();
    if legacy {
        fs::create_dir(base.path().join("token-menu-bar-gnome")).unwrap();
        fs::write(base.path().join("token-menu-bar-gnome/settings.json"), b"legacy").unwrap();
    }
    if current {
        fs::create_dir(base.path().join("quotaline")).unwrap();
        fs::write(base.path().join("quotaline/settings.json"), b"current").unwrap();
    }
    base
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_directory_moves_to_the_current_name() {
    let base = legacy_base(true, false);
    let (level, message) = adopt_legacy_directory(base.path()).unwrap();
    assert_eq!((level, message.starts_with("Moved ")), ("info", true));
    assert_eq!(
        fs::read(base.path().join("quotaline/settings.json")).unwrap(),
        b"legacy"
    );
    assert!(!base.path().join("token-menu-bar-gnome").exists());
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_directory_stays_when_the_current_one_exists() {
    let base = legacy_base(true, true);
    assert_eq!(adopt_legacy_directory(base.path()), None);
    assert_eq!(
        fs::read(base.path().join("quotaline/settings.json")).unwrap(),
        b"current"
    );
    assert_eq!(
        fs::read(base.path().join("token-menu-bar-gnome/settings.json")).unwrap(),
        b"legacy"
    );
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_directory_absent_leaves_the_base_untouched() {
    let base = legacy_base(false, false);
    assert_eq!(adopt_legacy_directory(base.path()), None);
    assert!(!base.path().join("quotaline").exists());
}

#[test]
#[cfg(target_os = "linux")]
fn legacy_directory_move_failure_is_a_warning() {
    let base = legacy_base(true, false);
    fs::set_permissions(base.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let (level, message) = adopt_legacy_directory(base.path()).unwrap();
    fs::set_permissions(base.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!((level, message.starts_with("Could not move ")), ("warning", true));
    assert!(base.path().join("token-menu-bar-gnome/settings.json").exists());
}
