//! Exercises service demo and metric in an isolated GNOME session.

use super::super::*;

#[test]
fn service_exposes_shared_provider_and_metric_metadata() {
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
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["provider_definitions"][0]["id"], "claude");
        assert_eq!(state["provider_definitions"][0]["name"], "Claude");
        assert_eq!(state["metric_groups"][0], serde_json::json!(["Windows", ["usage"]]));
        assert_eq!(state["short_label_limit"], quotaline_status_bar::SHORT_LABEL_LIMIT);
        let reviews: String = proxy.call("GetReviewSummary", &()).await.unwrap();
        let counts: serde_json::Value = serde_json::from_str(&reviews).unwrap();
        assert!(counts["week"].as_i64().unwrap() >= counts["today"].as_i64().unwrap());
    });
}

#[test]
fn demo_launch_requests_use_generated_data_without_persisting_the_request() {
    for argument in [true, false] {
        let scratch = tempfile::tempdir().unwrap();
        let (_bus, address) = private_bus(scratch.path(), "session-bus");
        let mut command = Command::new(env!("CARGO_BIN_EXE_quotaline-gnome"));
        environment(&mut command, scratch.path(), address.trim());
        if argument {
            command.arg("--demo");
        } else {
            command.env("QUOTALINE_DEMO", "1");
        }
        let _service = Process(command.stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let connection = zbus::connection::Builder::address(address.trim())
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
            assert_eq!(state["settings"]["demo"], true);
            assert!(!state["providers"]["claude"]["windows"].as_array().unwrap().is_empty());
            let _: String = proxy.call("SetSetting", &("hide_zero", "false")).await.unwrap();
            let stored: serde_json::Value =
                serde_json::from_slice(&fs::read(scratch.path().join("config/quotaline/settings.json")).unwrap())
                    .unwrap();
            assert_eq!(stored["demo"], false);
            assert_eq!(stored["hide_zero"], false);
        });
    }
}

#[test]
fn explicit_demo_choice_overrides_saved_demo_and_launch_request() {
    let fixture = isolated_service_configured(
        &serde_json::json!({"demo":true,"demo_choice":false}),
        |_| {},
        |command| {
            command.arg("--demo");
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
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["demo"], false);
        assert_eq!(state["settings"]["demo_choice"], false);
    });
}

#[test]
fn service_exposes_default_panel_selection_for_settings() {
    let fixture = default_selection_service();
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
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["gemini"]["windows"]
                .as_array()
                .is_some_and(|rows| rows.len() == 3)
        })
        .await;
        assert_eq!(
            state["default_selected_windows"],
            serde_json::json!(["gemini:model:a", "gemini:model:b"])
        );
        assert_eq!(state["status_entries"].as_array().unwrap().len(), 2);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn default_model_selection_matches_settings_in_shell() {
    let fixture = default_selection_service();
    run_shell_script(&fixture, "extension/tests/integration/default-selection.test.js", &[]);
}

#[test]
fn history_response_keeps_raw_series_for_native_labels() {
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
        let history =
            history_view(&proxy, serde_json::json!({"kind":"metric","metric":"inputTokens"})).await["history"].take();
        assert!(
            history["series"]
                .as_array()
                .unwrap()
                .iter()
                .all(|series| series["label"].is_string())
        );
        assert!(history["project_labels"].as_object().unwrap().is_empty());
        assert!(history.get("display_labels").is_none());
    });
}
