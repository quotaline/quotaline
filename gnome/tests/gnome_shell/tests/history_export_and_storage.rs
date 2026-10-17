//! Exercises history export and storage in an isolated GNOME session.

use super::super::*;

#[test]
fn selected_model_filters_retained_quota_csv() {
    let now = chrono::Utc::now().timestamp();
    let fixture = isolated_service_with(&serde_json::json!({"selected_windows":["codex:session"]}), |root| {
        let path = root.join("data/quotaline/history.sqlite3");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id)); CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
        for key in ["session", "weekly"] {
            db.execute(
                "INSERT INTO samples VALUES(?1,'codex',?2,?2,50,NULL)",
                rusqlite::params![now, key],
            )
            .unwrap();
        }
    });
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let path = fixture.scratch.path().join("selected-live.csv");
        let exported: bool = proxy
            .call(
                "ExportMetricHistory",
                &("usage", now - 60, now + 60, path.to_str().unwrap()),
            )
            .await
            .unwrap();
        assert!(exported);
        let csv = fs::read_to_string(path).unwrap();
        assert!(csv.contains(",codex:session,"));
        assert!(!csv.contains("codex:weekly"));
    });
}

#[test]
fn csv_exports_protect_spreadsheet_formula_prefixes() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":false,"codex":true,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id)); CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
            for (index, value) in ["=1+1", "+1+1", "-1+1", "@SUM(1)", "\t=1", "\r=1"]
                .into_iter()
                .enumerate()
            {
                db.execute(
                    "INSERT INTO samples VALUES(?1,'codex',?2,?3,50,NULL)",
                    rusqlite::params![now.timestamp(), format!("window-{index}"), value],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO daily_analytics VALUES(?1,'codex','turns',?2,1)",
                    rusqlite::params![now.format("%Y-%m-%d").to_string(), value],
                )
                .unwrap();
            }
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let start = now.timestamp() - 86_400;
        let end = now.timestamp() + 86_400;
        let path = fixture.scratch.path().join("quotas.csv");
        let exported: bool = proxy
            .call("ExportMetricHistory", &("usage", start, end, path.to_str().unwrap()))
            .await
            .unwrap();
        assert!(exported);
        let quota = fs::read_to_string(path).unwrap();
        let path = fixture.scratch.path().join("turns.csv");
        let exported: bool = proxy
            .call("ExportMetricHistory", &("turns", start, end, path.to_str().unwrap()))
            .await
            .unwrap();
        assert!(exported);
        let analytics = fs::read_to_string(path).unwrap();
        for value in ["=1+1", "+1+1", "-1+1", "@SUM(1)", "\t=1", "\r=1"] {
            let escaped = format!("'{value}");
            assert!(quota.contains(&escaped), "{value:?}");
            assert!(analytics.contains(&escaped), "{value:?}");
        }
    });
}

#[test]
fn metric_history_ignores_invalid_dates_and_excludes_boundary_days() {
    let today = chrono::Utc::now().date_naive();
    let tomorrow = today.succ_opt().unwrap();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":false,"codex":true,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
            for (day, value) in [
                (today.to_string(), 1.0),
                (format!("{today}x"), 2.0),
                (tomorrow.to_string(), 3.0),
            ] {
                db.execute(
                    "INSERT INTO daily_analytics VALUES(?1,'codex','turns','total',?2)",
                    rusqlite::params![day, value],
                )
                .unwrap();
            }
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let view = history_view(&proxy, serde_json::json!({"kind":"metric","metric":"turns"})).await;
        let points = view["history"]["series"][0]["points"].as_array().unwrap();
        assert_eq!(
            points
                .iter()
                .map(|point| point[1].as_f64().unwrap())
                .collect::<Vec<_>>(),
            [1.0]
        );
    });
}

#[test]
fn reducing_retention_prunes_old_quota_and_analytics_history() {
    let now = chrono::Utc::now();
    let old = now - chrono::Duration::days(10);
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":false,"codex":true,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id)); CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
            for (stamp, used) in [(old, 25.0), (now, 50.0)] {
                db.execute(
                    "INSERT INTO samples VALUES(?1,'codex','session','Session',?2,NULL)",
                    rusqlite::params![stamp.timestamp(), used],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO daily_analytics VALUES(?1,'codex','turns','total',?2)",
                    rusqlite::params![stamp.format("%Y-%m-%d").to_string(), used],
                )
                .unwrap();
            }
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let readings = |view: serde_json::Value| {
            view["history"]["series"][0]["points"]
                .as_array()
                .unwrap()
                .iter()
                .map(|point| point[1].as_f64().unwrap())
                .collect::<Vec<_>>()
        };
        let usage = serde_json::json!({"kind":"metric","metric":"usage"});
        let turns = serde_json::json!({"kind":"metric","metric":"turns"});
        history_view(&proxy, serde_json::json!({"kind":"period","days":30})).await;
        assert_eq!(readings(history_view(&proxy, usage.clone()).await)[..2], [25.0, 50.0]);
        assert_eq!(readings(history_view(&proxy, turns.clone()).await).len(), 2);
        let _: String = proxy.call("SetSetting", &("retention_days", "7")).await.unwrap();
        assert!(
            readings(history_view(&proxy, usage).await)
                .iter()
                .all(|used| *used == 50.0)
        );
        assert_eq!(readings(history_view(&proxy, turns).await).len(), 1);
    });
}

#[test]
fn metric_export_reports_a_missing_destination_directory() {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    for demo in [false, true] {
        let fixture = isolated_service(&serde_json::json!({"demo":demo,"providers":providers}));
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let connection = zbus::connection::Builder::address(fixture.address.as_str())
                .unwrap()
                .build()
                .await
                .unwrap();
            wait_for_service(&connection).await;
            let proxy = zbus::Proxy::new(
                &connection,
                "tech.bernat.Quotaline.Gnome",
                "/tech/bernat/Quotaline/Gnome",
                "tech.bernat.Quotaline.Gnome",
            )
            .await
            .unwrap();
            for metric in ["usage", "turns"] {
                let path = fixture.scratch.path().join("missing/turns.csv");
                let result: zbus::Result<bool> = proxy
                    .call("ExportMetricHistory", &(metric, 0_i64, 1_i64, path.to_str().unwrap()))
                    .await;
                assert!(
                    result
                        .unwrap_err()
                        .to_string()
                        .contains("Export directory does not exist"),
                    "{demo} {metric}"
                );
            }
        });
    }
}

#[test]
fn pace_uses_recent_samples_after_service_restart() {
    let _shell = shell_lock();
    let scratch = tempfile::tempdir().unwrap();
    let now = chrono::Utc::now().timestamp();
    let data = scratch.path().join("data/quotaline");
    let cache = scratch.path().join("cache/quotaline");
    let config = scratch.path().join("config/quotaline");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&cache).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(scratch.path().join("home")).unwrap();
    fs::write(
        config.join("settings.json"),
        b"{\"providers\":{\"claude\":false,\"codex\":false,\"gemini\":false,\"antigravity\":false,\"cursor\":false,\"copilot\":false}}",
    )
    .unwrap();
    let reset = chrono::DateTime::from_timestamp(now + 43_200, 0).unwrap().to_rfc3339();
    let provider = serde_json::json!({
        "claude": {
            "id": "claude", "name": "Claude", "status": "connected", "plan": "Pro",
            "identity": "", "source": "", "notice": "", "updated": now,
            "windows": [{"id": "weekly", "label": "Weekly", "used": 50.0,
                "reset": reset, "group": "weekly", "duration_seconds": 86400}]
        }
    });
    fs::write(cache.join("snapshot.json"), provider.to_string()).unwrap();
    let db = rusqlite::Connection::open(data.join("history.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id))").unwrap();
    for (at, used) in [(now - 1800, 25.0), (now - 10, 50.0)] {
        db.execute(
            "INSERT INTO samples VALUES(?1,'claude','weekly','Weekly',?2,NULL)",
            rusqlite::params![at, used],
        )
        .unwrap();
    }
    drop(db);
    let (_bus, address) = private_bus(scratch.path(), "session-bus");
    let _service = Process(
        environment(
            &mut Command::new(env!("CARGO_BIN_EXE_quotaline-gnome")),
            scratch.path(),
            address.trim(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap(),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let state: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        let projected = state["providers"]["claude"]["windows"][0]["pace"]["projected_exhaustion"]
            .as_i64()
            .unwrap();
        assert!((now + 3000..now + 5000).contains(&projected));
        let cleared: bool = proxy.call("ClearHistory", &()).await.unwrap();
        assert!(cleared);
        let state: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert!(state["providers"]["claude"]["windows"][0]["pace"]["projected_exhaustion"].is_null());
    });
}

#[test]
fn service_remains_available_when_data_directory_is_a_file() {
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,
                "antigravity":false,"cursor":false,"copilot":false}
        }),
        |_| {},
        |command| {
            command.env("XDG_DATA_HOME", "/dev/null");
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["providers"]["claude"], false);
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
        assert!(history["error"].is_string());
        let clear: zbus::Result<bool> = proxy.call("ClearHistory", &()).await;
        assert!(clear.is_err());
    });
}
