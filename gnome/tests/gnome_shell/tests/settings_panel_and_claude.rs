//! Exercises settings panel and claude in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn claude_model_names_render_as_compact_panel_labels() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "limits":[
                    {"percent":87,"kind":"weekly_scoped","scope":{"model":{"display_name":"Opus 4"}}},
                    {"percent":72,"kind":"weekly_scoped","scope":{"model":{"display_name":"Nova"}}},
                    {"percent":66,"kind":"weekly_scoped","scope":{"model":{"display_name":"Two Parts"}}},
                    {"percent":48,"kind":"weekly_scoped","scope":{"model":{"display_name":"Claude Model"}}},
                    {"percent":35,"kind":"weekly_scoped","scope":{"model":{"display_name":"Flash"}}}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
            "selected_windows":["claude:weekly:opus-4","claude:weekly:nova","claude:weekly:two-parts",
                "claude:weekly:claude-model","claude:weekly:flash"],
            "fit_to_space":false
        }),
        |root| {
            let path = root.join("home/.claude/.credentials.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                r#"{"claudeAiOauth":{"accessToken":"label-fixture","subscriptionType":"Pro"}}"#,
            )
            .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-labels.test.js",
        &[
            ("TMB_EXPECT_LABELS", "OP4,NOV,TP,CLA,FLA"),
            ("TMB_LABEL_SCREENSHOT", "claude-panel-labels.png"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn hidden_claude_limit_survives_adaptive_panel_formats() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let reset = (chrono::Utc::now() + chrono::Duration::days(2) + chrono::Duration::minutes(90)).to_rfc3339();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "limits":[
                    {"kind":"session","percent":36},
                    {"kind":"weekly_all","percent":100,"resets_at":reset},
                    {"kind":"weekly_scoped","percent":20,"scope":{"model":{"display_name":"Opus 4"}}},
                    {"kind":"weekly_scoped","percent":30,"scope":{"model":{"display_name":"Sonnet"}}},
                    {"kind":"weekly_scoped","percent":40,"scope":{"model":{"display_name":"Fable"}}},
                    {"kind":"weekly_scoped","percent":50,"scope":{"model":{"display_name":"Nova"}}},
                    {"kind":"weekly_scoped","percent":60,"scope":{"model":{"display_name":"Flash"}}},
                    {"kind":"weekly_scoped","percent":70,"scope":{"model":{"display_name":"Haiku"}}}
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
            "selected_windows":["claude:session","claude:weekly:opus-4","claude:weekly:sonnet","claude:weekly:fable",
                "claude:weekly:nova","claude:weekly:flash","claude:weekly:haiku"],
            "has_custom_selection":true,"format":"countdown","fit_to_space":true,"notifications":false
        }),
        |root| {
            let credentials = root.join("home/.claude/.credentials.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"restricted-fixture"}}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "claude", "connected"));
    assert_eq!(state["status_entries"].as_array().unwrap().len(), 7);
    assert!(
        state["status_entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["restrictions"].as_array().is_some_and(|facts| !facts.is_empty()))
    );
    runtime.block_on(server.verify());
    run_shell_script(&fixture, "extension/tests/integration/panel-restriction.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_quota_categories_render_as_compact_panel_labels() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "quota_snapshots":{
                    "chat":{"percent_remaining":60},
                    "completions":{"percent_remaining":25}
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true},
            "selected_windows":["copilot:chat","copilot:completions"],
            "fit_to_space":false
        }),
        |root| {
            let path = root.join("home/.copilot/config.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                r#"{"loggedInUsers":[{"token":"copilot-fixture","host":"github.com","user":"octocat"}]}"#,
            )
            .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-labels.test.js",
        &[
            ("TMB_EXPECT_LABELS", "GHC,GHX"),
            ("TMB_LABEL_SCREENSHOT", "copilot-panel-labels.png"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_online_refresh_reaches_state_and_history() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .and(matchers::header("authorization", "Bearer service-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":42,"resets_at":(chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let credentials = root.join("home/.claude/.credentials.json");
                fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"service-fixture","subscriptionType":"Pro"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
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
        let state = wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 42.0)
        .await;
        assert_eq!(state["providers"]["claude"]["status"], "connected");
        assert_eq!(state["providers"]["claude"]["windows"][0]["used"], 42.0);
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await["history"].take();
        let service_log = fs::read_to_string(fixture.scratch.path().join("service.log")).unwrap();
        assert_eq!(
            history["series"][0]["points"][0][1],
            42.0,
            "history={history}; service={service_log}"
        );
    });
}
