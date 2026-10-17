//! Exercises codex history and credits in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn codex_history_tracks_each_account_separately() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        for (account, used) in [("first", 31), ("second", 62)] {
            Mock::given(matchers::method("GET"))
                .and(matchers::path("/backend-api/wham/usage"))
                .and(matchers::header("authorization", format!("Bearer {account}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rate_limit":{"primary_window":{"used_percent":used,"limit_window_seconds":18000}}
                })))
                .mount(&server)
                .await;
        }
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{
                "claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            }}),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(
                    codex.join("auth.json"),
                    r#"{"tokens":{"access_token":"first","account_id":"first"}}"#,
                )
                .unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
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
        assert!(proxy.call::<_, _, bool>("Refresh", &("codex",)).await.unwrap());
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["windows"][0]["used"] == 31.0
        })
        .await;
        let database = fixture.scratch.path().join("data/quotaline/history.sqlite3");
        let db = rusqlite::Connection::open(database).unwrap();
        assert_eq!(
            db.query_row("SELECT used FROM samples WHERE key GLOB 'codex:*'", [], |row| row
                .get::<_, f64>(0))
                .unwrap(),
            31.0
        );
        let day = chrono::Utc::now().format("%Y-%m-%d").to_string();
        db.execute("INSERT INTO analytics VALUES('codex',?1,'turns','total',3)", [&day])
            .unwrap();

        let credential = fixture.scratch.path().join("home/.codex/auth.json");
        fs::write(
            &credential,
            r#"{"tokens":{"access_token":"second","account_id":"second"}}"#,
        )
        .unwrap();
        assert!(proxy.call::<_, _, bool>("Refresh", &("codex",)).await.unwrap());
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["windows"][0]["used"] == 62.0
        })
        .await;
        assert_eq!(
            db.query_row("SELECT used FROM samples WHERE key GLOB 'codex:*'", [], |row| row
                .get::<_, f64>(0))
                .unwrap(),
            62.0
        );
        assert_eq!(
            db.query_row(
                "SELECT used FROM archived_samples WHERE key GLOB 'codex:*'",
                [],
                |row| { row.get::<_, f64>(0) }
            )
            .unwrap(),
            31.0
        );
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM analytics WHERE provider='codex'", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            0
        );
        db.execute("INSERT INTO analytics VALUES('codex',?1,'turns','total',7)", [&day])
            .unwrap();

        fs::write(
            credential,
            r#"{"tokens":{"access_token":"first","account_id":"first"}}"#,
        )
        .unwrap();
        assert!(proxy.call::<_, _, bool>("Refresh", &("codex",)).await.unwrap());
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["windows"][0]["used"] == 31.0
        })
        .await;
        assert_eq!(
            db.query_row("SELECT used FROM samples WHERE key GLOB 'codex:*'", [], |row| row
                .get::<_, f64>(0))
                .unwrap(),
            31.0
        );
        assert_eq!(
            db.query_row(
                "SELECT used FROM archived_samples WHERE key GLOB 'codex:*'",
                [],
                |row| { row.get::<_, f64>(0) }
            )
            .unwrap(),
            62.0
        );
        assert_eq!(
            db.query_row("SELECT value FROM analytics WHERE provider='codex'", [], |row| {
                row.get::<_, f64>(0)
            })
            .unwrap(),
            3.0
        );
        assert_eq!(
            db.query_row(
                "SELECT value FROM archived_analytics WHERE provider='codex'",
                [],
                |row| { row.get::<_, f64>(0) }
            )
            .unwrap(),
            7.0
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_secret_service_auth_precedes_stale_file_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .and(matchers::header("authorization", "Bearer keyring-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":31,"limit_window_seconds":18000}},
                "rate_limit_reset_credits":[]
            })))
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            }}),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(codex.join("config.toml"), "cli_auth_credentials_store = 'keyring'\n").unwrap();
                fs::write(codex.join("auth.json"), r#"{"tokens":{"access_token":"stale-file"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        let codex = fixture.scratch.path().join("home/.codex");
        let digest = sha2::Sha256::digest(codex.canonicalize().unwrap().to_string_lossy().as_bytes());
        let account = format!("cli|{}", hex::encode(&digest[..8]));
        store_keyring_item(
            &fixture,
            "Codex fixture",
            &[("service", "Codex Auth"), ("username", &account)],
            br#"{"tokens":{"access_token":"keyring-fixture"}}"#,
        );
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
        let _: String = proxy.call("SetSetting", &("providers", providers.to_string())).await.unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["codex"]["status"] == "connected").await;
        assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 31.0);
        assert_eq!(state["providers"]["codex"]["source"], "Secret Service: Codex Auth");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_invalid_keyring_falls_back_to_auth_file() {
    let state = codex_keyring_failure_state(Some(r#"{"tokens":{"access_token":"codex-file-fixture"}}"#), false);
    assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 32.0);
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_invalid_keyring_without_file_reports_unreadable_credentials() {
    let state = codex_keyring_failure_state(None, false);
    assert_eq!(state["providers"]["codex"]["notice"], "Codex keyring item is not JSON");
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_locked_keyring_without_file_reports_unreadable_credentials() {
    let state = codex_keyring_failure_state(None, true);
    assert_eq!(state["providers"]["codex"]["notice"], "Codex keyring is locked");
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_credit_balance_reaches_daily_history() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let (started_sender, started) = tokio::sync::oneshot::channel();
        let started_sender = std::sync::Mutex::new(Some(started_sender));
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = std::sync::Mutex::new(released);
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .and(matchers::header("authorization", "Bearer balance-fixture"))
            .respond_with(move |_: &wiremock::Request| {
                started_sender.lock().unwrap().take().unwrap().send(()).unwrap();
                released.lock().unwrap().recv().unwrap();
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "rate_limit":{"primary_window":{"used_percent":31,"limit_window_seconds":18000}},
                    "credits":{"balance":42.5,"unlimited":false},
                    "rate_limit_reset_credits":[]
                }))
            })
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(codex.join("auth.json"), r#"{"tokens":{"access_token":"balance-fixture"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
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
        let mut changes = proxy.receive_signal("Changed").await.unwrap();
        started.await.unwrap();
        let mut release = Some(release);
        async {
            loop {
                let history =
                    history_view(&proxy, serde_json::json!({"kind":"metric","metric":"creditBalance"})).await
                        ["history"]
                        .take();
                if history["series"].as_array().is_some_and(|series| !series.is_empty()) {
                    assert_eq!(history["series"][0]["provider"], "codex");
                    assert_eq!(history["series"][0]["label"], "Available credits");
                    assert_eq!(history["series"][0]["points"][0][1], 42.5);
                    break;
                }
                let _ = release.take().map(|sender| sender.send(()).unwrap());
                changes
                    .next()
                    .await
                    .expect("quota service disconnected before Codex credit balance was recorded");
            }
        }.await;
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_credit_allowance_reaches_shell() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .and(matchers::header("authorization", "Bearer allowance-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":31,"limit_window_seconds":18000}},
                "spend_control":{"individual_limit":{
                    "used":"2.125","limit":"10","remaining":"7.875","source":"workspace_owner"
                }}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let codex = root.join("home/.codex");
            fs::create_dir_all(&codex).unwrap();
            fs::write(
                codex.join("auth.json"),
                r#"{"tokens":{"access_token":"allowance-fixture"}}"#,
            )
            .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    runtime.block_on(async {
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
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["spend"]["credit_allowance"]["source"] == "workspace_owner"
        })
        .await;
    });
    run_shell_script(&fixture, "extension/tests/integration/codex-allowance.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_unlimited_usage_credits_reach_shell() {
    codex_credits_shell_case(
        "unlimited",
        serde_json::json!({"unlimited":true,"overage_limit_reached":true}),
        serde_json::json!([]),
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_missing_balance_and_textual_reset_expiry_reach_shell() {
    codex_credits_shell_case(
        "missing",
        serde_json::json!({}),
        serde_json::json!({"available_count":1,"credits":[{"title":"Near reset","status":"available","expires_at":"soon"}]}),
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_reset_credits_without_usage_credits_reach_shell() {
    codex_credits_shell_case(
        "reset-only",
        serde_json::Value::Null,
        serde_json::json!({"available_count":2,"total_earned_count":3,"credits":[]}),
    );
}
