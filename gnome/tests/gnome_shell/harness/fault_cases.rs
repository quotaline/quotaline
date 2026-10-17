//! Provides isolated Shell test fixtures for fault cases.

use super::*;
use quotaline_quota::notices::notice_visible;

pub(crate) fn fault_service_shell_fixture(script: &'static str, options: FaultOptions) {
    let _shell = shell_lock();
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let state = runtime.block_on(async {
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
        proxy.call::<_, _, String>("GetState", &()).await.unwrap()
    });
    let (state, activity_reply) = if options.empty_metrics
        || options.warning_notice
        || options.duplicate_limit_notice
        || options.refreshing_provider
        || options.first_run_pending
        || options.credential_states
        || options.fail_activity_until_enabled
        || options.provider_visibility.is_some()
    {
        let mut snapshot: serde_json::Value = serde_json::from_str(&state).unwrap();
        if options.empty_metrics {
            snapshot["metric_definitions"] = serde_json::json!([snapshot["metric_definitions"][0]]);
        }
        if options.warning_notice {
            snapshot["providers"]["claude"]["notices"] =
                serde_json::json!([{"kind":"limitReached","text":"Current session limit reached"}]);
            snapshot["providers"]["claude"]["warnings"] = serde_json::json!(["Profile unavailable: HTTP 503"]);
        }
        if options.duplicate_limit_notice {
            let selected = snapshot["default_selected_windows"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(serde_json::Value::as_str)
                .find_map(|value| value.strip_prefix("claude:"))
                .unwrap()
                .to_string();
            let windows = snapshot["providers"]["claude"]["windows"].as_array_mut().unwrap();
            windows.iter_mut().find(|window| window["id"] == selected).unwrap()["used"] = serde_json::json!(100.0);
            snapshot["providers"]["claude"]["notices"] = serde_json::json!([
                {"kind":"limitReached","windowId":selected,"text":"Selected limit reached"},
                {"kind":"limitReached","text":"General limit reached"}
            ]);
            let full = std::collections::BTreeSet::from([selected]);
            snapshot["providers"]["claude"]["notices"]
                .as_array_mut()
                .unwrap()
                .retain(|notice| notice_visible(notice["kind"] == "limitReached", notice["windowId"].as_str(), &full));
        }
        if options.refreshing_provider {
            snapshot["is_refreshing"] = serde_json::json!(true);
            snapshot["refreshing_providers"] = serde_json::json!(["claude"]);
        }
        if options.first_run_pending {
            snapshot["first_run_pending"] = serde_json::json!(true);
            snapshot["settings"]["detailed_logging"] = serde_json::json!(true);
        }
        if options.credential_states {
            snapshot["status_entries"] = serde_json::json!([]);
            snapshot["settings"]["last_tab"] = serde_json::json!("Settings");
            snapshot["settings"]["demo"] = serde_json::json!(false);
            snapshot["settings"]["show_all_providers"] = serde_json::json!(true);
            for (id, health, usable, status) in [
                ("claude", Some("missing"), false, "sign-in"),
                ("codex", Some("expired"), false, "sign-in"),
                ("gemini", Some("unreadable"), false, "unreadable"),
                ("antigravity", Some("unchecked"), false, "sign-in"),
                ("cursor", None, true, "rate-limited"),
                ("copilot", None, false, "sign-in"),
            ] {
                snapshot["providers"][id]["source"] = serde_json::json!("");
                snapshot["providers"][id]["credential_usable"] = serde_json::json!(usable);
                snapshot["providers"][id]["status"] = serde_json::json!(status);
                snapshot["providers"][id]["windows"] = serde_json::json!([]);
                snapshot["providers"][id]["plan"] = serde_json::json!("");
                snapshot["providers"][id]["identity"] = serde_json::json!("");
                snapshot["providers"][id]["notice"] = serde_json::json!("");
                snapshot["providers"][id]["updated"] = serde_json::Value::Null;
                if let Some(health) = health {
                    snapshot["providers"][id]["credential_health"] = serde_json::json!(health);
                } else {
                    snapshot["providers"][id]
                        .as_object_mut()
                        .unwrap()
                        .remove("credential_health");
                }
            }
        }
        if let Some(mode) = options.provider_visibility {
            snapshot["status_entries"] = serde_json::json!([]);
            snapshot["settings"]["last_tab"] = serde_json::json!("Settings");
            snapshot["settings"]["demo"] = serde_json::json!(false);
            snapshot["settings"]["show_all_providers"] = serde_json::json!(false);
            snapshot["claude_local_usage"] = serde_json::Value::Null;
            let gemini_window = snapshot["providers"]["gemini"]["windows"][0].clone();
            for id in ["claude", "codex", "gemini", "antigravity", "cursor", "copilot"] {
                let provider = &mut snapshot["providers"][id];
                provider["credential_usable"] = serde_json::json!(false);
                provider["credential_health"] = serde_json::json!("missing");
                provider["status"] = serde_json::json!("sign-in");
                provider["windows"] = serde_json::json!([]);
                provider["plan"] = serde_json::json!("");
                provider["identity"] = serde_json::json!("");
                provider["source"] = serde_json::json!("");
                provider["updated"] = serde_json::Value::Null;
            }
            if mode == "empty" {
                snapshot["settings"]["notify_expiring_credits"] = serde_json::json!(false);
            }
            if mode == "overrides" {
                snapshot["settings"]["analytics_interval_minutes"] = serde_json::json!(20);
                snapshot["settings"]["workdays"] = serde_json::json!(5);
                snapshot["settings"]["allow_token_refresh"] = serde_json::json!(true);
            }
            if mode == "mixed" {
                snapshot["claude_local_usage"] = serde_json::json!({"sessions":1});
                snapshot["providers"]["claude"]["status"] = serde_json::json!("unreadable");
                snapshot["providers"]["codex"]["updated"] = serde_json::json!(1);
                snapshot["providers"]["codex"]["status"] = serde_json::json!("connected");
                snapshot["providers"]["gemini"]["windows"] = serde_json::json!([gemini_window]);
                snapshot["providers"]["cursor"]["credential_usable"] = serde_json::json!(true);
                snapshot["providers"]["cursor"]["credential_health"] = serde_json::json!("valid");
                snapshot["providers"]["cursor"]["status"] = serde_json::json!("rate-limited");
                snapshot["settings"]["providers"]["gemini"] = serde_json::json!(false);
            }
            let active = if mode == "mixed" {
                ["claude", "codex", "gemini", "cursor"]
                    .into_iter()
                    .map(|id| quotaline_settings::context::Provider {
                        id: id.into(),
                        long_window: false,
                        reset_credits: false,
                        local_auth: false,
                    })
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let settings = &snapshot["settings"];
            let visibility = quotaline_settings::context::visibility(
                &active,
                quotaline_settings::context::Options {
                    analytics_interval_changed: settings["analytics_interval_minutes"]
                        != quotaline_settings::DEFAULT_ANALYTICS_INTERVAL_MINUTES,
                    workdays_changed: settings["workdays"] != quotaline_quota::pace::MAX_WORKDAYS,
                    token_refresh_enabled: settings["allow_token_refresh"] == true,
                    expiring_credits_enabled: settings["notify_expiring_credits"] == true,
                    sandboxed: false,
                },
            );
            snapshot["settings_context"]["visibility"] = serde_json::json!(visibility);
        }
        for id in ["claude", "codex", "gemini", "antigravity", "cursor", "copilot"] {
            let provider = &snapshot["providers"][id];
            let facts = quotaline_settings::context::Discovery {
                snapshot: !provider["updated"].is_null() || !provider["windows"].as_array().unwrap().is_empty(),
                analytics: id == "claude" && !snapshot["claude_local_usage"].is_null(),
                credential_usable: provider["credential_usable"] == true,
            };
            let discovered = quotaline_settings::context::discovered(facts);
            let show_in_settings =
                quotaline_settings::context::visible(facts, snapshot["settings"]["show_all_providers"] == true, false);
            let show_in_usage = quotaline_quota::usage::provider_card_visible(
                &serde_json::from_value::<quotaline_quota::Provider>(provider.clone()).unwrap(),
                snapshot["settings"]["providers"][id] == true,
                id == "claude" && !snapshot["claude_local_usage"].is_null(),
            );
            snapshot["providers"][id]["discovered"] = serde_json::json!(discovered);
            snapshot["providers"][id]["show_in_settings"] = serde_json::json!(show_in_settings);
            snapshot["providers"][id]["show_in_usage"] = serde_json::json!(show_in_usage);
        }
        let available = snapshot["providers"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, provider)| {
                let keys = provider["windows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|row| format!("{id}:{}", row["id"].as_str().unwrap()))
                    .collect::<std::collections::HashSet<_>>();
                (id.clone(), keys)
            })
            .collect::<std::collections::HashMap<_, _>>();
        snapshot["model_groups"].as_array_mut().unwrap().retain_mut(|group| {
            let keys = &available[group["provider"].as_str().unwrap()];
            group["rows"]
                .as_array_mut()
                .unwrap()
                .retain(|row| keys.contains(row["key"].as_str().unwrap()));
            group["total_count"] = serde_json::json!(keys.len());
            group["selected_count"] = serde_json::json!(
                group["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|row| row["selected"] == true)
                    .count()
            );
            !group["rows"].as_array().unwrap().is_empty()
        });
        snapshot["selected_model_count"] = serde_json::json!(
            snapshot["model_groups"]
                .as_array()
                .unwrap()
                .iter()
                .map(|group| group["selected_count"].as_u64().unwrap())
                .sum::<u64>()
        );
        let reply = snapshot["model_groups"].to_string();
        if options.fail_activity_until_enabled {
            snapshot["settings"]["last_tab"] = serde_json::json!("Settings");
            snapshot["model_groups"] = serde_json::Value::Null;
        }
        (snapshot.to_string(), reply)
    } else {
        let reply = serde_json::from_str::<serde_json::Value>(&state).unwrap()["model_groups"].to_string();
        (state, reply)
    };
    assert_eq!(unsafe { libc::kill(fixture._service.0.id() as i32, libc::SIGTERM) }, 0);
    assert!(fixture._service.0.wait().unwrap().success());
    let service = runtime.block_on(async {
        zbus::connection::Builder::address(fixture.address.as_str())
            .name("tech.bernat.Quotaline.Gnome")
            .serve_at(
                "/tech/bernat/Quotaline/Gnome",
                FaultService {
                    state: std::sync::Mutex::new(if options.malformed_state { "{".into() } else { state }),
                    history_session: tokio::sync::Mutex::new(quotaline_analytics::HistorySession::new(
                        quotaline_analytics::HistorySessionConfig {
                            range: quotaline_analytics::HistoryRange::Today,
                            rollup: quotaline_analytics::WindowRollup::Minute,
                            metric: "usage".into(),
                            daily_utc: false,
                            utc: false,
                            local_time_zone: "UTC".into(),
                            local_offset: 0,
                            now: chrono::Utc::now().timestamp() as f64,
                        },
                    )),
                    hold_state: options.hold_state,
                    state_calls: tokio::sync::Semaphore::new(0),
                    state_gate: tokio::sync::Semaphore::new(0),
                    hold_first_activity: options.hold_first_activity,
                    fail_activity_until_enabled: options.fail_activity_until_enabled,
                    activity_reply,
                    activity_enabled: std::sync::atomic::AtomicBool::new(false),
                    activity_calls: tokio::sync::Semaphore::new(0),
                    activity_gate: tokio::sync::Semaphore::new(0),
                    activity_requests: std::sync::atomic::AtomicUsize::new(0),
                    hold_history: options.hold_history,
                    history_gate: tokio::sync::Notify::new(),
                    history_calls: tokio::sync::Semaphore::new(0),
                    history_retry_sequence: options.history_retry_sequence,
                    history_requests: std::sync::atomic::AtomicUsize::new(0),
                    malformed_logs: options.malformed_logs,
                    log_requests: std::sync::atomic::AtomicUsize::new(0),
                    codex_cost: options.codex_cost,
                    code_reviews: options.code_reviews,
                    cost_reply_error: options.cost_reply_error,
                    hold_cost: options.hold_cost,
                    cost_gate: tokio::sync::Semaphore::new(0),
                },
            )
            .build()
            .await
            .unwrap()
    });
    let expected_cost = options.codex_cost.unwrap_or_default().to_string();
    let expected_reviews = options.review_expected.unwrap_or_default();
    runtime.block_on(async {
        tokio::task::spawn_blocking(move || {
            run_shell_script(
                &fixture,
                script,
                &[
                    ("TMB_COST_EXPECTED", &expected_cost),
                    ("TMB_REVIEW_EXPECTED", expected_reviews),
                    (
                        "TMB_FIRST_RUN_DETECTED",
                        if options.first_run_pending { "1" } else { "0" },
                    ),
                    ("TMB_PROVIDER_VISIBILITY", options.provider_visibility.unwrap_or("")),
                ],
            )
        })
        .await
        .unwrap();
    });
    drop(service);
}
