//! Exercises service history and profile in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn history_project_labels_disambiguate_short_names_in_shell() {
    project_history_shell(
        false,
        &[("/work/secret-project", 1.25), ("/other/secret-project", 2.5)],
        &["work/secret-project", "other/secret-project"],
        "public",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_project_labels_hide_paths_in_shell() {
    project_history_shell(
        true,
        &[("/work/secret-project", 1.25), ("/other/secret-project", 2.5)],
        &["Project 1f57fcdf0259", "Project 8abae2271ca8"],
        "private",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_project_labels_disambiguate_shared_parents_in_shell() {
    project_history_shell(
        false,
        &[
            ("/team-a/work/secret-project", 1.25),
            ("/team-b/work/secret-project", 2.5),
        ],
        &["team-a/work/secret-project", "team-b/work/secret-project"],
        "public-shared-parent",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_project_labels_preserve_root_path_in_shell() {
    project_history_shell(false, &[("/", 1.25)], &["/"], "public-root");
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_value_formats_reuse_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({
        "demo":true,"last_tab":"History","history_metric":"costUSD","history_days":7
    }));
    run_shell_cases(
        &fixture,
        &[
            ("history-value-format.test.js", &[("TMB_HISTORY_FORMAT", "costUSD")]),
            ("history-value-format.test.js", &[("TMB_HISTORY_FORMAT", "inputTokens")]),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cached_provider_sources_render_in_settings() {
    let _shell = shell_lock();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"last_tab":"Settings","show_all_providers":true}),
        |root| {
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            let now = chrono::Utc::now().timestamp();
            let entries = [
                ("claude", "Claude", "", "/fixture/auth.json"),
                ("codex", "Codex", "Plus", "Keychain"),
                ("gemini", "Gemini", "", ""),
            ]
            .into_iter()
            .map(|(id, name, plan, source)| {
                (
                    id.to_string(),
                    serde_json::json!({
                        "id":id,"name":name,"status":"connected","plan":plan,"identity":"",
                        "source":source,"windows":[],"notice":"","updated":now,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
            fs::write(
                cache.join("snapshot.json"),
                serde_json::Value::Object(entries).to_string(),
            )
            .unwrap();
        },
        |_| {},
    );
    run_shell_script(&fixture, "extension/tests/integration/provider-source.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn expired_claude_cloud_credit_is_labeled_in_shell() {
    cloud_credit_shell_case(
        "expired",
        serde_json::json!({"limit":250,"remaining":83,"expires_at":"2020-01-01T00:00:00Z"}),
        "connected",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn locked_claude_cloud_credit_is_unavailable_in_shell() {
    cloud_credit_shell_case(
        "locked",
        serde_json::json!({"limit":250,"remaining":83,"expires_at":"2030-01-01T00:00:00Z",
            "locked_reason":"provider_locked"}),
        "connected",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn stale_claude_cloud_credit_without_expiry_is_muted_in_shell() {
    cloud_credit_shell_case(
        "stale",
        serde_json::json!({"limit":250,"remaining":83,"expires_at":null}),
        "stale",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cached_quota_without_pace_or_currency_remains_readable_in_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{"claude":true},"last_tab":"Usage","show_all_providers":true}),
        |root| {
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            let now = chrono::Utc::now().timestamp();
            let reset = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
            fs::write(
                cache.join("snapshot.json"),
                serde_json::json!({
                    "claude": {
                        "id":"claude","name":"Claude","status":"connected","plan":"","identity":"",
                        "source":"Local","notice":"","updated":now,
                        "windows":[
                            {"id":"emoji","label":"✶","used":20.0,"reset":reset,"group":"weekly"},
                            {"id":"plain","label":"Claude","used":35.0,"reset":null,"group":"weekly",
                                "duration_seconds":604800}
                        ],
                        "spend": {
                            "enabled":true,"used":12.4,"limit":50.0,"percent":24.8,"currency":null,
                            "reset":null,"limit_reached":false,"balance":1.3
                        }
                    }
                })
                .to_string(),
            )
            .unwrap();
        },
        |_| {},
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/cached-quota.test.js",
        &[("TMB_CENTERED", "1")],
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/cached-quota.test.js",
        &[("TMB_VIRTUAL_MONITOR", "760x720")],
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/cached-quota.test.js",
        &[("TMB_VIRTUAL_MONITOR", "420x720")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn hide_unused_omits_an_empty_provider_group_in_shell() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":0}
            })))
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&server)
            .await;
    });
    let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"hide_unused_models":true}),
        |root| {
            let credentials = root.join("home/.claude/.credentials.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"empty-fixture"}}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "claude", "connected"));
    assert_eq!(state["providers"]["claude"]["windows"].as_array().unwrap().len(), 1);
    assert_eq!(state["providers"]["claude"]["windows"][0]["used"], 0.0);
    run_shell_script(&fixture, "extension/tests/integration/hide-unused.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_legacy_weekly_limit_keeps_its_duration() {
    let state = claude_usage_state(serde_json::json!({"seven_day":{"utilization":33}}), "connected");
    assert_eq!(state["providers"]["claude"]["windows"][0]["duration_seconds"], 604_800);
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_empty_quota_response_reports_an_error() {
    let state = claude_usage_state(serde_json::json!({}), "error");
    assert_eq!(
        state["providers"]["claude"]["notice"],
        "Unexpected response: claude.usage: invalid payload"
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_unknown_limit_without_scope_remains_visible() {
    let state = claude_usage_state(
        serde_json::json!({
            "limits":[{"percent":12,"kind":"monthly_total","group":"monthly"}]
        }),
        "connected",
    );
    assert_eq!(state["providers"]["claude"]["windows"][0]["id"], "monthly_total");
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_uses_its_config_directory_override() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":42}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&server)
            .await;
    });
    let alternate = tempfile::tempdir().unwrap();
    fs::write(
        alternate.path().join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"alternate-fixture"}}"#,
    )
    .unwrap();
    let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("CLAUDE_CONFIG_DIR", alternate.path());
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "claude", "connected"));
    assert_eq!(
        state["providers"]["claude"]["source"],
        alternate.path().join(".credentials.json").display().to_string()
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_reports_an_unreachable_quota_endpoint() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let alternate = tempfile::tempdir().unwrap();
    fs::write(
        alternate.path().join(".credentials.json"),
        r#"{"claudeAiOauth":{"accessToken":"offline-fixture"}}"#,
    )
    .unwrap();
    let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("CLAUDE_CONFIG_DIR", alternate.path());
            command.env("TMB_FIXTURE_API_URL", endpoint);
        },
    );
    let state = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(provider_with_status(&fixture, "claude", "offline"));
    assert!(
        state["providers"]["claude"]["notice"]
            .as_str()
            .is_some_and(|notice| notice.starts_with("error sending request")),
        "{state}"
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_keeps_credentials_when_token_refresh_cannot_connect() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let alternate = tempfile::tempdir().unwrap();
    let credentials = alternate.path().join(".credentials.json");
    let document =
        serde_json::json!({"claudeAiOauth":{"accessToken":"expired","refreshToken":"old-refresh","expiresAt":1}});
    fs::write(&credentials, document.to_string()).unwrap();
    let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"allow_token_refresh":true}),
        |_| {},
        |command| {
            command.env("CLAUDE_CONFIG_DIR", alternate.path());
            command.env("TMB_FIXTURE_API_URL", endpoint);
        },
    );
    let state = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(provider_with_status(&fixture, "claude", "offline"));
    assert!(
        state["providers"]["claude"]["notice"]
            .as_str()
            .is_some_and(|notice| notice.starts_with("Claude token refresh failed: Network error: ")),
        "{state}"
    );
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&fs::read(credentials).unwrap()).unwrap(),
        document
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_profile_organization_reaches_shell_and_privacy_chips() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":42,"resets_at":
                    (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339()}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "account":{"email":"profile@example.com"},
                "organization":{"name":"Example Team","organization_type":"claude_pro"}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = claude_live_service(&server);
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
            state["providers"]["claude"]["organization"] == "Example Team"
        })
        .await;
    });
    run_shell_cases_with_reset(
        &fixture,
        &[
            ("organization.test.js", &[]),
            ("organization.test.js", &[("TMB_CENTERED", "1")]),
        ],
        -1,
        None,
        &[],
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/organization.test.js",
        &[("TMB_VIRTUAL_MONITOR", "420x720")],
    );
}
