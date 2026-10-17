//! Exercises history failure and analytics in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn quota_remains_visible_when_history_storage_cannot_open() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .and(matchers::header("authorization", "Bearer claude-storage-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":42}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let path = root.join("home/.claude/.credentials.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"claudeAiOauth":{"accessToken":"claude-storage-fixture"}}"#).unwrap();
            },
            |command| {
                command
                    .env("XDG_DATA_HOME", "/dev/null")
                    .env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let state = provider_with_status(&fixture, "claude", "connected").await;
        assert_eq!(state["providers"]["claude"]["windows"][0]["used"], 42.0);
        server.verify().await;
    });
}

#[test]
fn malformed_existing_history_schema_returns_an_error_without_stopping_the_service() {
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":false,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            rusqlite::Connection::open(path)
                .unwrap()
                .execute_batch("CREATE TABLE samples(ts INTEGER, provider TEXT, window_id TEXT, label TEXT, reset TEXT); CREATE TABLE daily_analytics(day TEXT, provider TEXT, metric TEXT, series TEXT)")
                .unwrap();
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
        assert!(history["error"].as_str().unwrap().contains("no such column"));
        let analytics = history_view(&proxy, serde_json::json!({"kind":"metric","metric":"costUSD"})).await;
        assert!(analytics["error"].as_str().unwrap().contains("no such column"));
        let state: String = proxy.call("GetState", &()).await.unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&state).unwrap()["version"], 1);
    });
}

#[test]
fn history_schema_creation_reports_an_incompatible_existing_table() {
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            rusqlite::Connection::open(path)
                .unwrap()
                .execute_batch("CREATE TABLE samples(ts INTEGER)")
                .unwrap();
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
        assert!(history["error"].as_str().unwrap().contains("no such column"));
        let state: String = proxy.call("GetState", &()).await.unwrap();
        assert_eq!(serde_json::from_str::<serde_json::Value>(&state).unwrap()["version"], 1);
    });
}

#[test]
fn retention_prune_reports_a_database_failure_without_stopping_the_service() {
    for table in ["samples", "analytics", "archived_samples", "archived_analytics"] {
        let fixture = isolated_service_with(
            &serde_json::json!({"providers":{"claude":false,"codex":false,"gemini":false,
                "antigravity":false,"cursor":false,"copilot":false}}),
            |root| {
                let path = root.join("data/quotaline/history.sqlite3");
                drop(quotaline_history::Store::open(Some(&path), 60).unwrap());
                rusqlite::Connection::open(path)
                    .unwrap()
                    .execute_batch(&format!("INSERT INTO samples VALUES(1,'codex:session','Session',1,NULL); INSERT INTO analytics VALUES('codex','2020-01-01','turns','total',1); INSERT INTO archived_samples VALUES('old',1,'codex:session','Session',1,NULL); INSERT INTO archived_analytics VALUES('old','codex','2020-01-01','turns','total',1); CREATE TRIGGER refuse_prune BEFORE DELETE ON {table} BEGIN SELECT RAISE(FAIL,'{table} prune denied'); END"))
                    .unwrap();
            },
        );
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
            let result: zbus::Result<String> = proxy.call("SetSetting", &("retention_days", "7")).await;
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains(&format!("{table} prune denied"))
            );
            let state: String = proxy.call("GetState", &()).await.unwrap();
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&state).unwrap()["settings"]["retention_days"],
                7
            );
        });
    }
}

#[test]
fn history_recreates_schema_after_database_replacement() {
    let fixture = isolated_service(&serde_json::json!({
        "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false}
    }));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let before = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
        assert_eq!(before["history"]["series"], serde_json::json!([]));
        let path = fixture.scratch.path().join("data/quotaline/history.sqlite3");
        fs::rename(&path, path.with_extension("old")).unwrap();
        fs::write(path.with_file_name("history.sqlite3-wal"), b"obsolete sidecar").unwrap();
        let _ = fs::remove_file(path.with_file_name("history.sqlite3-shm"));
        for suffix in ["-wal", "-shm"] {
            let sidecar = path.with_file_name(format!("history.sqlite3{suffix}"));
            if let Err(error) = fs::remove_file(sidecar) {
                assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
            }
        }
        let after = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
        assert_eq!(after["history"]["series"], serde_json::json!([]));
        let db = rusqlite::Connection::open(path).unwrap();
        let tables: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='samples'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn scheduled_codex_analytics_reaches_daily_history() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let requests = std::sync::atomic::AtomicUsize::new(0);
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":25}}
            })))
            .mount(&server)
            .await;
        for path in [
            "/backend-api/wham/usage/daily-token-usage-breakdown",
            "/backend-api/wham/analytics/daily-skill-usage-metrics",
            "/backend-api/wham/analytics/daily-plugin-usage-metrics",
            "/backend-api/wham/analytics/daily-code-review-metrics",
        ] {
            Mock::given(matchers::method("GET"))
                .and(matchers::path(path))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data":[]})))
                .expect(2)
                .mount(&server)
                .await;
        }
        Mock::given(matchers::method("GET"))
            .and(matchers::path(
                "/backend-api/wham/analytics/daily-workspace-usage-counts",
            ))
            .respond_with(move |_: &wiremock::Request| {
                let turns = if requests.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                    1
                } else {
                    3
                };
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "data":[
                        {"date":"2020-01-01","totals":{"turns":100}},
                        {"date":today,"totals":{"turns":turns}}
                    ]
                }))
            })
            .expect(2)
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
            "analytics_interval_minutes":5
        }),
        |root| {
            let credential = root.join("home/.codex/auth.json");
            fs::create_dir_all(credential.parent().unwrap()).unwrap();
            fs::write(credential, r#"{"tokens":{"access_token":"analytics-fixture"}}"#).unwrap();
            let database = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(database.parent().unwrap()).unwrap();
            rusqlite::Connection::open(database)
                .unwrap()
                .execute_batch("CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))")
                .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    );
    runtime.block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
    });
    let path = fixture.scratch.path().join("data/quotaline/history.sqlite3");
    let (_watcher, events) = database_events(&path);
    let db = rusqlite::Connection::open(path).unwrap();
    loop {
        let value = db.query_row(
            "SELECT value FROM analytics WHERE provider='codex' AND metric='turns' AND series='total'",
            [],
            |row| row.get::<_, f64>(0),
        );
        if value == Ok(3.0) {
            let rows: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM analytics WHERE provider='codex' AND metric='turns'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(rows, 1);
            return;
        }
        events
            .recv()
            .expect("Scheduled Codex analytics did not reach daily history")
            .expect("database watcher failed");
    }
}
