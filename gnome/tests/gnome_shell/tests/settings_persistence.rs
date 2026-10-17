//! Exercises settings persistence in an isolated GNOME session.

use super::super::*;

#[test]
fn short_label_preview_rejects_unknown_models_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let result: zbus::Result<String> = proxy.call("PreviewShortLabel", &("missing", "NEW")).await;
        assert!(result.is_err());
    });
}

#[test]
fn invalid_settings_are_rejected_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        for (key, raw) in [
            ("format", "\"unsupported\""),
            ("order", "\"random\""),
            ("provider_order", "[\"claude\",\"claude\"]"),
            ("provider_order", "[\"unknown\"]"),
            ("model_order", "[\"claude:session\",\"claude:session\"]"),
            ("show_as", "\"both\""),
            ("widget_size", "\"extra-large\""),
            ("last_tab", "\"Charts\""),
            ("history_days", "2"),
            ("history_metric", "\"unknown\""),
            ("history_rollup", "\"week\""),
            ("short_labels", "{\"claude:session\":\"toolong\"}"),
            ("short_labels", "{\"claude:session\":\"A\",\"codex:session\":\"A\"}"),
            ("short_labels", "{\"claude:session\":\"CX 5h\"}"),
            ("short_label:claude:session", "true"),
            ("short_label:missing", "\"NEW\""),
            ("decimals", "\"two\""),
            ("missing", "true"),
            ("demo", "{"),
        ] {
            let result: zbus::Result<String> = proxy.call("SetSetting", &(key, raw)).await;
            assert!(result.is_err(), "{key} accepted an invalid value");
        }
        let state: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(state["settings"]["demo"], true);
    });
}

#[test]
fn legacy_short_label_conflict_allows_an_unrelated_edit_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({
        "demo": true,
        "short_labels": {"claude:session": "CX 5h"}
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
        let state: String = proxy
            .call(
                "SetSetting",
                &(
                    "short_labels",
                    "{\"claude:session\":\"CX 5h\",\"codex:weekly\":\"WEEK\"}",
                ),
            )
            .await
            .unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(state["settings"]["short_labels"]["codex:weekly"], "WEEK");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn conflicting_persisted_short_label_shows_derived_default_in_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({
        "demo": true,
        "short_labels": {"claude:session": "CX 5h"}
    }));
    run_shell_script(&fixture, "extension/tests/integration/legacy-label.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn derived_short_labels_include_unselected_models_in_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service_with(
        &serde_json::json!({
            "providers": {"claude": false, "codex": true, "gemini": false, "antigravity": false,
                "cursor": false, "copilot": false},
            "selected_windows": ["codex:additional:spark-beta"]
        }),
        |root| {
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            fs::write(
                cache.join("snapshot.json"),
                serde_json::json!({"codex": {
                    "id": "codex", "name": "Codex", "status": "connected", "plan": "Plus",
                    "identity": "", "source": "fixture", "notice": "", "updated": chrono::Utc::now().timestamp(),
                    "windows": [
                        {"id": "additional:spark-alpha", "label": "Spark Alpha", "used": 20,
                            "reset": null, "group": "other"},
                        {"id": "additional:spark-beta", "label": "Spark Beta", "used": 30,
                            "reset": null, "group": "other"}
                    ]
                }})
                .to_string(),
            )
            .unwrap();
        },
    );
    run_shell_script(&fixture, "extension/tests/integration/derived-label.test.js", &[]);
}

#[test]
fn dbus_settings_persist_and_reset() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let changed: String = proxy.call("SetSetting", &("show_all_providers", "true")).await.unwrap();
        let changed: serde_json::Value = serde_json::from_str(&changed).unwrap();
        assert_eq!(changed["settings"]["show_all_providers"], true);
        let changed: String = proxy.call("SetSetting", &("history_days", "30")).await.unwrap();
        let changed: serde_json::Value = serde_json::from_str(&changed).unwrap();
        assert_eq!(changed["settings"]["history_days"], 30);
        let path = fixture.scratch.path().join("config/quotaline/settings.json");
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["show_all_providers"], true);
        assert_eq!(saved["history_days"], 30);
        let reset: String = proxy.call("ResetSettings", &()).await.unwrap();
        let reset: serde_json::Value = serde_json::from_str(&reset).unwrap();
        assert_eq!(reset["settings"]["show_all_providers"], false);
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["show_all_providers"], false);
        assert_eq!(saved["history_days"], 1);
    });
}

#[test]
fn failed_setting_write_keeps_the_previous_live_value() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let config = fixture.scratch.path().join("config/quotaline");
        fs::set_permissions(&config, fs::Permissions::from_mode(0o500)).unwrap();
        let changed: zbus::Result<String> = proxy.call("SetSetting", &("show_all_providers", "true")).await;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(changed.is_err());
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["show_all_providers"], false);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(config.join("settings.json")).unwrap()).unwrap(),
            serde_json::json!({"demo":true})
        );
    });
}

#[test]
fn failed_settings_reset_keeps_the_previous_live_value() {
    let fixture = isolated_service(&serde_json::json!({"demo":true,"show_all_providers":true}));
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
        let config = fixture.scratch.path().join("config/quotaline");
        fs::set_permissions(&config, fs::Permissions::from_mode(0o500)).unwrap();
        let reset: zbus::Result<String> = proxy.call("ResetSettings", &()).await;
        fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(reset.is_err());
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["show_all_providers"], true);
    });
}

#[test]
fn service_normalizes_saved_provider_refresh_intervals() {
    let fixture = isolated_service(&serde_json::json!({
        "demo":true,
        "interval_minutes":{"claude":1,"codex":0,"cursor":40,"unknown":4}
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
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let intervals = &state["settings"]["interval_minutes"];
        assert_eq!(intervals["claude"], 2);
        assert_eq!(intervals["codex"], 1);
        assert_eq!(intervals["gemini"], 2);
        assert_eq!(intervals["cursor"], 30);
        assert!(intervals.get("unknown").is_none());
    });
}

#[test]
fn provider_refresh_interval_changes_persist_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let raw: String = proxy
            .call("SetSetting", &("interval_minutes", r#"{"claude":1,"codex":3}"#))
            .await
            .unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["interval_minutes"]["claude"], 2);
        assert_eq!(state["settings"]["interval_minutes"]["codex"], 3);
        let saved: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.scratch.path().join("config/quotaline/settings.json")).unwrap())
                .unwrap();
        assert_eq!(saved["interval_minutes"]["claude"], 2);
        assert_eq!(saved["interval_minutes"]["codex"], 3);
    });
}

#[test]
fn provider_interval_steps_preserve_other_provider_changes() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let _: String = proxy
            .call("SetSetting", &("interval_minutes", r#"{"claude":3,"codex":7}"#))
            .await
            .unwrap();
        let stepped: String = proxy.call("StepProviderInterval", &("claude", -1_i32)).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&stepped).unwrap();
        assert_eq!(state["settings"]["interval_minutes"]["claude"], 2);
        assert_eq!(state["settings"]["interval_minutes"]["codex"], 7);
        assert!(
            proxy
                .call::<_, _, String>("StepProviderInterval", &("claude", 2_i32))
                .await
                .is_err()
        );
        assert!(
            proxy
                .call::<_, _, String>("StepProviderInterval", &("missing", 1_i32))
                .await
                .is_err()
        );
        assert!(
            proxy
                .call::<_, _, String>("SetSetting", &("step_provider_interval", "{}"))
                .await
                .is_err()
        );
    });
}

#[test]
fn hide_identity_masks_account_details_in_state() {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), write_private_snapshot);
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
        let before: String = proxy.call("GetState", &()).await.unwrap();
        let before: serde_json::Value = serde_json::from_str(&before).unwrap();
        assert_eq!(before["providers"]["claude"]["identity"], "private@example.invalid");
        assert_eq!(before["providers"]["claude"]["organization"], "Private Organization");
        assert_eq!(before["providers"]["claude"]["show_organization"], true);
        let after: String = proxy.call("SetSetting", &("hide_identity", "true")).await.unwrap();
        let after: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(after["providers"]["claude"]["identity"], "account");
        assert_eq!(after["providers"]["claude"]["organization"], "workspace");
        assert_eq!(after["providers"]["claude"]["show_organization"], true);
        assert_eq!(after["providers"]["claude"]["source"], "Local credential");
        assert_eq!(after["providers"]["gemini"]["identity"], "account");
        assert_eq!(after["providers"]["gemini"]["organization"], "");
        assert_eq!(after["providers"]["gemini"]["show_organization"], false);
    });
}

#[test]
fn diagnostics_omit_credentials_and_account_identity() {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), write_private_snapshot);
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
        let raw: String = proxy.call("GetDiagnostics", &()).await.unwrap();
        assert!(!raw.contains("private@example.invalid"));
        assert!(!raw.contains("/private/credential-file"));
        assert!(!raw.contains(fixture.scratch.path().to_string_lossy().as_ref()));
        let document: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(document["demo"], false);
        assert_eq!(document["history_path"], "$XDG_DATA_HOME/quotaline/history.sqlite3");
        assert!(
            document["recent_log"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| { entry["message"].as_str() == Some("Service started") })
        );
        assert_eq!(document["providers"]["claude"]["status"], "connected");
    });
}

#[test]
fn refresh_rejects_unknown_provider() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let accepted: bool = proxy.call("Refresh", &("unknown",)).await.unwrap();
        assert!(!accepted);
    });
}

#[test]
fn snapshot_save_failure_is_logged() {
    let fixture = isolated_service_with(
        &serde_json::json!({
            "providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false}
        }),
        |root| {
            // A directory where the snapshot file belongs makes every save fail.
            fs::create_dir_all(root.join("cache/quotaline/snapshot.json")).unwrap();
        },
    );
    let log = fixture.scratch.path().join("service.log");
    let (_watcher, events) = database_events(&log);
    while !fs::read_to_string(&log).unwrap().contains("Snapshot save failed") {
        events
            .recv()
            .expect("service log watcher stopped")
            .expect("service log watcher failed");
    }
}
