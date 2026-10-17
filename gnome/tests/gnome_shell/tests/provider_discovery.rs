//! Exercises provider discovery in an isolated GNOME session.

use super::super::*;

#[test]
fn service_exits_when_session_bus_closes() {
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
    });
    fixture._bus.0.kill().unwrap();
    fixture._bus.0.wait().unwrap();
    let status = wait_for_exit(&mut fixture._service.0, Duration::from_secs(3))
        .expect("service kept running after its session bus closed");
    assert!(status.success());
}

#[test]
fn model_activity_reports_last_increase_or_reset_within_retention() {
    let now = chrono::Utc::now().timestamp();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":providers,"retention_days":7,"hide_unused_models":false}),
        |root| {
            let data = root.join("data/quotaline");
            fs::create_dir_all(&data).unwrap();
            let db = rusqlite::Connection::open(data.join("history.sqlite3")).unwrap();
            db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id))").unwrap();
            for (offset, id, used, reset) in [
                (600, "session", 0.0, "2026-10-10T10:00:00Z"),
                (500, "session", 10.0, "2026-10-10T10:00:00Z"),
                (400, "session", 10.0, "2026-10-10T10:00:00Z"),
                (300, "session", 10.0, "2026-10-10T15:00:00Z"),
                (200, "session", 15.0, "2026-10-10T15:00:00Z"),
                (100, "session", 15.0, "2026-10-10T15:00:00Z"),
                (500, "weekly", 10.0, "2026-10-10T10:00:00Z"),
                (300, "weekly", 10.0, "2026-10-10T15:00:00Z"),
                (100, "weekly", 10.0, "2026-10-10T15:00:00Z"),
                (100, "monthly", 0.0, "2026-10-10T10:00:00Z"),
                (8 * 86_400, "old", 20.0, "2026-10-10T10:00:00Z"),
            ] {
                db.execute(
                    "INSERT INTO samples VALUES(?1,'claude',?2,?3,?4,?5)",
                    rusqlite::params![now - offset, id, id, used, reset],
                )
                .unwrap();
            }
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            let windows = ["session", "weekly", "monthly", "old"]
                .map(|id| serde_json::json!({"id":id,"label":id,"used":0.0,"reset":null,"group":"weekly"}));
            fs::write(
                cache.join("snapshot.json"),
                serde_json::json!({"claude":{"id":"claude","name":"Claude","status":"connected","plan":"",
                "identity":"","source":"","notice":"","updated":now,"windows":windows}})
                .to_string(),
            )
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
        let raw: String = proxy.call("GetModelGroups", &("", "")).await.unwrap();
        let groups: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let activity = groups[0]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["key"].as_str().unwrap(), row["last_used_at"].as_f64()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(
            activity,
            std::collections::BTreeMap::from([
                ("claude:monthly", None),
                ("claude:old", None),
                ("claude:session", Some((now - 200) as f64)),
                ("claude:weekly", Some((now - 300) as f64)),
            ])
        );
    });
}

#[test]
fn model_groups_use_shared_search_and_reveal_policy() {
    let fixture = isolated_service(&serde_json::json!({"demo": true}));
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
        for (case, query, revealed, expected) in [
            ("search", "CURRENT SESSION", "", vec!["claude:session"]),
            ("no match", "absent model", "", vec![]),
            ("revealed", "absent model", "claude:session", vec!["claude:session"]),
        ] {
            let raw: String = proxy.call("GetModelGroups", &(query, revealed)).await.unwrap();
            let groups: serde_json::Value = serde_json::from_str(&raw).unwrap();
            let keys = groups
                .as_array()
                .unwrap()
                .iter()
                .flat_map(|group| group["rows"].as_array().unwrap())
                .map(|row| row["key"].as_str().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(keys, expected, "{case}");
        }
    });
}

#[test]
fn model_selection_actions_use_shared_policy() {
    let fixture = isolated_service(&serde_json::json!({"demo": true}));
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
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let selected = state["model_groups"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|group| group["rows"].as_array().unwrap())
            .find(|row| row["selected"] == true)
            .unwrap()["key"]
            .as_str()
            .unwrap()
            .to_owned();
        let raw: String = proxy.call("ToggleModel", &(selected.as_str(),)).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(state["settings"]["has_custom_selection"].as_bool().unwrap());
        assert!(
            !state["settings"]["selected_windows"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(selected))
        );
        let raw: String = proxy.call("ToggleProviderModels", &("claude",)).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let claude = state["model_groups"]
            .as_array()
            .unwrap()
            .iter()
            .find(|group| group["provider"] == "claude")
            .unwrap();
        assert_eq!(claude["selected_count"], claude["total_count"]);
        assert!(
            proxy
                .call::<_, _, String>("ToggleModel", &("unknown:model",))
                .await
                .is_err()
        );
    });
}

#[test]
fn removed_model_cannot_allow_clearing_the_last_visible_selection() {
    let fixture = isolated_service(&serde_json::json!({
        "demo": true,
        "has_custom_selection": true,
        "selected_windows": ["claude:session", "removed:window"],
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
        let raw: String = proxy.call("ToggleModel", &("claude:session",)).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            state["settings"]["selected_windows"],
            serde_json::json!(["claude:session", "removed:window"])
        );
        assert_eq!(state["selected_model_count"], 1);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_app_session_precedes_agent_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .and(matchers::header(
                "cookie",
                "WorkosCursorSessionToken=%3A%3Aapp-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "individualUsage":{"plan":{"used":40,"limit":100}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":true,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let state = root.join("config/Cursor/User/globalStorage/state.vscdb");
                fs::create_dir_all(state.parent().unwrap()).unwrap();
                let db = rusqlite::Connection::open(&state).unwrap();
                db.execute_batch("CREATE TABLE ItemTable (key TEXT, value TEXT); INSERT INTO ItemTable VALUES ('cursorAuth/accessToken','app-token'),('cursorAuth/cachedEmail','app@example.com'),('cursorAuth/stripeMembershipType','Pro');").unwrap();
                let agent = root.join("home/.cursor/auth.json");
                fs::create_dir_all(agent.parent().unwrap()).unwrap();
                fs::write(agent, r#"{"accessToken":"agent-token"}"#).unwrap();
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
        let state = wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
        assert_eq!(state["providers"]["cursor"]["plan"], "Pro");
        assert_eq!(state["providers"]["cursor"]["identity"], "app@example.com");
        assert_eq!(state["providers"]["cursor"]["windows"][0]["used"], 40.0);
        assert!(state["providers"]["cursor"]["source"]
            .as_str()
            .unwrap()
            .ends_with("state.vscdb"));
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_agent_survives_an_unreadable_app_database() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .and(matchers::header(
                "cookie",
                "WorkosCursorSessionToken=%3A%3Aagent-token",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "individualUsage":{"plan":{"used":25,"limit":100}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":true,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let state = root.join("config/Cursor/User/globalStorage/state.vscdb");
                fs::create_dir_all(state.parent().unwrap()).unwrap();
                fs::write(state, b"invalid database").unwrap();
                let agent = root.join("home/.cursor/auth.json");
                fs::create_dir_all(agent.parent().unwrap()).unwrap();
                fs::write(agent, r#"{"accessToken":"agent-token"}"#).unwrap();
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
        let state = wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
        assert_eq!(state["providers"]["cursor"]["windows"][0]["used"], 25.0);
        assert!(state["providers"]["cursor"]["source"]
            .as_str()
            .unwrap()
            .ends_with("auth.json"));
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn empty_secret_service_keeps_unsigned_providers_in_sign_in_state() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let disabled = serde_json::json!({"claude":false,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service(&serde_json::json!({"providers":disabled}));
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        let dbus = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
        let name = zbus::names::BusName::try_from("org.freedesktop.secrets").unwrap();
        assert!(!dbus.name_has_owner(name.clone()).await.unwrap());
        let mut owners = dbus
            .receive_name_owner_changed_with_args(&[(0, name.as_str())])
            .await
            .unwrap();
        let _keyring = start_keyring(&fixture).await;
        owners.next().await.unwrap();
        assert!(dbus.name_has_owner(name).await.unwrap());
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let enabled = serde_json::json!({"claude":false,"codex":true,"gemini":true,
            "antigravity":true,"cursor":false,"copilot":true});
        let _: String = proxy
            .call("SetSetting", &("providers", enabled.to_string()))
            .await
            .unwrap();
        let state = wait_for_state(&proxy, |state| {
            ["codex", "gemini", "antigravity", "copilot"]
                .iter()
                .all(|id| state["providers"][id]["status"] == "sign-in")
        })
        .await;
        for id in ["codex", "gemini", "antigravity", "copilot"] {
            assert_eq!(state["providers"][id]["status"], "sign-in", "{id}");
        }
    });
}

#[test]
fn codex_rollout_fallback_is_visible_without_api_credentials() {
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), |root| {
        write_codex_rollout(root, 42.0);
    });
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
        let state = wait_for_state(&proxy, |state| state["providers"]["codex"]["status"] == "stale").await;
        assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 42.0);
        assert_eq!(state["providers"]["codex"]["windows"][1]["used"], 7.0);
        assert_eq!(state["providers"]["codex"]["credit_balance"], 12.5);
        assert_eq!(state["providers"]["codex"]["updated"], 1_791_626_400_i64);
        let db = rusqlite::Connection::open(fixture.scratch.path().join("data/quotaline/history.sqlite3")).unwrap();
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM samples WHERE key GLOB 'codex:*'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_rollout_uses_the_config_directory_override() {
    let alternate = tempfile::tempdir().unwrap();
    let sessions = alternate.path().join("sessions/2026/10/10");
    fs::create_dir_all(&sessions).unwrap();
    let path = sessions.join("rollout-alternate.jsonl");
    let line = serde_json::json!({"timestamp":"2026-10-10T10:00:00Z","rate_limits":{"primary":{"used_percent":63}}});
    fs::write(&path, line.to_string()).unwrap();
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("CODEX_HOME", alternate.path());
        },
    );
    let state = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(provider_with_status(&fixture, "codex", "stale"));
    assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 63.0);
    assert_eq!(state["providers"]["codex"]["source"], path.display().to_string());
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_uses_default_home_when_the_override_has_no_credentials() {
    let alternate = tempfile::tempdir().unwrap();
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let codex = root.join("home/.codex");
            fs::create_dir_all(&codex).unwrap();
            fs::write(
                codex.join("auth.json"),
                r#"{"tokens":{"access_token":"default-fixture"}}"#,
            )
            .unwrap();
            write_codex_rollout(root, 67.0);
        },
        |command| {
            command.env("CODEX_HOME", alternate.path());
        },
    );
    let state = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(provider_with_status(&fixture, "codex", "stale"));
    assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 67.0);
    assert!(
        state["providers"]["codex"]["source"]
            .as_str()
            .unwrap()
            .contains("home/.codex/sessions/2026/10/10/rollout-test.jsonl")
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_override_is_named_when_multiple_accounts_are_found() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"rate_limit":{}})))
            .mount(&server)
            .await;
    });
    let alternate = tempfile::tempdir().unwrap();
    fs::write(
        alternate.path().join("auth.json"),
        r#"{"tokens":{"access_token":"configured-fixture"}}"#,
    )
    .unwrap();
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let path = root.join("home/.codex/auth.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, r#"{"tokens":{"access_token":"default-fixture"}}"#).unwrap();
        },
        |command| {
            command.env("CODEX_HOME", alternate.path());
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "codex", "connected"));
    assert_eq!(
        state["providers"]["codex"]["account_selection"],
        serde_json::json!({"count": 2, "label": alternate.path().display().to_string()})
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn privacy_masks_a_multiple_account_source() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"rate_limit":{}})))
            .mount(&server)
            .await;
    });
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"hide_identity":true}),
        |root| {
            for home in [".codex", ".config/codex"] {
                let path = root.join("home").join(home).join("auth.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"tokens":{"access_token":"fixture"}}"#).unwrap();
            }
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "codex", "connected"));
    assert_eq!(state["providers"]["codex"]["source"], "Local credential");
    assert!(state["providers"]["codex"]["account_selection"].is_null());
}
