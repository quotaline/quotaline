//! Exercises shell state in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn malformed_service_state_is_explained_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/malformed-service.test.js",
        FaultOptions {
            malformed_state: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_cost_history_reaches_usage_card() {
    fault_service_shell_fixture(
        "extension/tests/integration/codex-cost.test.js",
        FaultOptions {
            codex_cost: Some(4.5),
            hold_cost: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn zero_cost_history_stays_visible_in_usage_card() {
    fault_service_shell_fixture(
        "extension/tests/integration/codex-cost.test.js",
        FaultOptions {
            codex_cost: Some(0.0),
            hold_cost: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_code_reviews_show_shared_summary_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/codex-review.test.js",
        FaultOptions {
            codex_cost: Some(0.0),
            code_reviews: Some(r#"{"today":5,"week":12}"#),
            review_expected: Some("5 today · 12 this week"),
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_old_code_reviews_show_zero_today_and_this_week() {
    fault_service_shell_fixture(
        "extension/tests/integration/codex-review.test.js",
        FaultOptions {
            codex_cost: Some(0.0),
            code_reviews: Some(r#"{"today":0,"week":0}"#),
            review_expected: Some("0 today · 0 this week"),
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn first_run_opens_usage_and_dismissal_persists() {
    let _shell = shell_lock();
    let fixture = fresh_install_fixture();
    run_shell_script(
        &fixture,
        "extension/tests/integration/first-run.test.js",
        &[("TMB_FIRST_RUN_DETECTED", "0")],
    );
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.scratch.path().join("config/quotaline/settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["first_run_card_dismissed"], true);
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let state: String = proxy.call("ResetSettings", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(state["settings"]["first_run_card_dismissed"], false);
        assert_eq!(state["first_run_pending"], false);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn first_run_empty_layout_fits_narrow_shell() {
    let _shell = shell_lock();
    let fixture = fresh_install_fixture();
    run_shell_script(
        &fixture,
        "extension/tests/integration/first-run.test.js",
        &[
            ("TMB_FIRST_RUN_DETECTED", "0"),
            ("TMB_FIRST_RUN_NARROW", "1"),
            ("TMB_VIRTUAL_MONITOR", "320x720"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn first_run_names_detected_providers_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/first-run.test.js",
        FaultOptions {
            first_run_pending: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn settings_distinguish_credential_health_states() {
    fault_service_shell_fixture(
        "extension/tests/integration/credential-health.test.js",
        FaultOptions {
            credential_states: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn settings_show_discovered_providers_independent_of_status_and_enabled_state() {
    fault_service_shell_fixture(
        "extension/tests/integration/provider-visibility.test.js",
        FaultOptions {
            provider_visibility: Some("mixed"),
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn settings_explain_when_no_providers_are_discovered() {
    fault_service_shell_fixture(
        "extension/tests/integration/provider-visibility.test.js",
        FaultOptions {
            provider_visibility: Some("empty"),
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn saved_advanced_settings_remain_visible_without_providers() {
    fault_service_shell_fixture(
        "extension/tests/integration/provider-visibility.test.js",
        FaultOptions {
            provider_visibility: Some("overrides"),
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn undiscovered_provider_control_persists_explicit_activation() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"show_all_providers":true,"last_tab":"Settings"}));
    run_shell_script(&fixture, "extension/tests/integration/provider-toggle.test.js", &[]);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.scratch.path().join("config/quotaline/settings.json")).unwrap())
            .unwrap();
    assert_eq!(saved["providers"]["copilot"], true);
    assert!(
        saved["configured_providers"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("copilot"))
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn first_run_detects_cursor_when_quota_is_rate_limited() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/aiserver.v1.DashboardService/GetCurrentPeriodUsage"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({}),
        |root| {
            fs::remove_file(root.join("config/quotaline/settings.json")).unwrap();
            let credentials = root.join("home/.cursor/auth.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
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
        // The service holds the first-run card back while any refresh runs, so wait for the cycle to settle.
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["status"] == "rate-limited" && state["first_run_pending"] == true
        })
        .await;
        assert_eq!(state["providers"]["cursor"]["credential_usable"], true);
        assert_eq!(state["providers"]["cursor"]["credential_health"], "valid");
    });
    run_shell_script(
        &fixture,
        "extension/tests/integration/first-run.test.js",
        &[
            ("TMB_FIRST_RUN_DETECTED", "1"),
            ("TMB_FIRST_RUN_EXPECTED", "Detected: Cursor"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn failed_cost_history_request_is_explained_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/malformed-history.test.js",
        FaultOptions {
            cost_reply_error: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn pending_state_reply_is_ignored_after_shell_shutdown() {
    fault_service_shell_fixture(
        "extension/tests/integration/pending-state.test.js",
        FaultOptions {
            hold_state: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn stale_model_activity_reply_does_not_replace_newer_settings() {
    fault_service_shell_fixture(
        "extension/tests/integration/stale-activity.test.js",
        FaultOptions {
            hold_first_activity: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn failed_model_group_request_can_retry_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/model-groups-retry.test.js",
        FaultOptions {
            fail_activity_until_enabled: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn shell_lifecycle_cases_reuse_one_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases_with_reset(
        &fixture,
        &[
            ("late-history-tab.test.js", &[]),
            ("stale-history-error.test.js", &[]),
            ("stale-log-error.test.js", &[]),
            ("export-error-disable.test.js", &[]),
            ("proxy-failure.test.js", &[]),
        ],
        4,
        Some("CC 5h"),
        &[],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn open_history_refreshes_on_its_minute_timer() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases(
        &fixture,
        &[("usage-timer.test.js", &[]), ("history-timer.test.js", &[])],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn malformed_service_history_and_sign_in_are_explained_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/malformed-history.test.js",
        FaultOptions::default(),
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn failed_history_update_keeps_the_chart_and_can_retry_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/history-retry.test.js",
        FaultOptions {
            history_retry_sequence: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_navigation_before_the_first_reply_is_safe_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/delayed-history.test.js",
        FaultOptions {
            hold_history: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn malformed_log_response_does_not_retry_forever_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/malformed-log.test.js",
        FaultOptions {
            malformed_logs: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_picker_omits_unavailable_metric_groups_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/empty-metrics.test.js",
        FaultOptions {
            empty_metrics: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_without_samples_explains_the_empty_state_in_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"providers":{
        "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
    }}));
    run_shell_script(&fixture, "extension/tests/integration/empty-history.test.js", &[]);
}
