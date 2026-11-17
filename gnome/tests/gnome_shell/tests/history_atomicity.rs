//! Exercises history atomicity in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
fn codex_analytics_server(runtime: &tokio::runtime::Runtime) -> MockServer {
    let server = runtime.block_on(MockServer::start());
    let today = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":25}},
                "rate_limit_reset_credits":[]
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
                .mount(&server)
                .await;
        }
        Mock::given(matchers::method("GET"))
            .and(matchers::path(
                "/backend-api/wham/analytics/daily-workspace-usage-counts",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data":[{"date":today,"totals":{"turns":3}}]
            })))
            .mount(&server)
            .await;
    });
    server
}

#[cfg(feature = "fixture-api")]
#[test]
fn failed_analytics_insert_is_reported_and_leaves_no_partial_history() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = codex_analytics_server(&runtime);
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
            "analytics_interval_minutes":5
        }),
        |root| {
            let credential = root.join("home/.codex/auth.json");
            fs::create_dir_all(credential.parent().unwrap()).unwrap();
            fs::write(
                credential,
                r#"{"tokens":{"access_token":"analytics-rejection-fixture"}}"#,
            )
            .unwrap();
            let database = root.join("data/quotaline/history.sqlite3");
            drop(quotaline_history::Store::open(Some(&database), 60).unwrap());
            rusqlite::Connection::open(database)
                .unwrap()
                .execute_batch("DROP TABLE analytics; CREATE TABLE analytics(provider TEXT NOT NULL, day TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL CHECK(value < 0), PRIMARY KEY(provider, day, metric, series))")
                .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    );
    let log = fixture.scratch.path().join("service.log");
    let (_watcher, events) = database_events(&log);
    loop {
        if fs::read_to_string(&log)
            .unwrap()
            .contains("Codex analytics write failed: CHECK constraint failed")
        {
            break;
        }
        events
            .recv()
            .expect("Scheduled Codex analytics failure was not logged")
            .expect("service log watcher failed");
    }
    let database = fixture.scratch.path().join("data/quotaline/history.sqlite3");
    let rows: i64 = rusqlite::Connection::open(database)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM analytics", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    runtime.block_on(server.verify());
}

#[cfg(feature = "fixture-api")]
#[test]
fn failed_analytics_progress_save_is_reported() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = codex_analytics_server(&runtime);
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
            "analytics_interval_minutes":5
        }),
        |root| {
            let credential = root.join("home/.codex/auth.json");
            fs::create_dir_all(credential.parent().unwrap()).unwrap();
            fs::write(
                credential,
                r#"{"tokens":{"access_token":"analytics-progress-fixture"}}"#,
            )
            .unwrap();
            fs::create_dir_all(root.join("data/quotaline/codex-analytics-watermarks.json")).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    );
    let log = fixture.scratch.path().join("service.log");
    let (_watcher, events) = database_events(&log);
    loop {
        if fs::read_to_string(&log)
            .unwrap()
            .contains("Codex analytics progress write failed")
        {
            break;
        }
        events
            .recv()
            .expect("Codex analytics progress failure was not logged")
            .expect("service log watcher failed");
    }
}

#[cfg(feature = "fixture-api")]
#[test]
fn rejected_analytics_do_not_advance_download_progress() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = codex_analytics_server(&runtime);
    runtime.block_on(
        Mock::given(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(503))
            .with_priority(1)
            .mount(&server),
    );
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |root| {
            let credential = root.join("home/.codex/auth.json");
            fs::create_dir_all(credential.parent().unwrap()).unwrap();
            fs::write(credential, r#"{"tokens":{"access_token":"new-account"}}"#).unwrap();
            quotaline_history::Store::open(Some(&root.join("data/quotaline/history.sqlite3")), 60)
                .unwrap()
                .activate_account("codex", "previous-account")
                .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    );
    let log = fixture.scratch.path().join("service.log");
    let (_watcher, events) = database_events(&log);
    while !fs::read_to_string(&log)
        .unwrap()
        .contains("Codex analytics discarded after account change")
    {
        events
            .recv_timeout(Duration::from_secs(10))
            .expect("Analytics did not finish")
            .unwrap();
    }
    assert!(
        !fixture
            .scratch
            .path()
            .join("data/quotaline/codex-analytics-watermarks.json")
            .exists()
    );
}
