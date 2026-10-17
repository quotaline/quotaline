//! Exercises stale provider actions in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_last_reading_stays_visible_in_shell_after_api_failure() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with({
                let requests = requests.clone();
                move |_: &wiremock::Request| {
                    if requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        ResponseTemplate::new(200).set_body_json(serde_json::json!({
                            "individualUsage":{
                                "plan":{"used":2500,"limit":10000},
                                "onDemand":{"enabled":true,"used":200,"limit":1000}
                            }
                        }))
                    } else {
                        ResponseTemplate::new(503)
                    }
                }
            })
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/aiserver.v1.DashboardService/GetCurrentPeriodUsage"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
    });
    let fixture = cursor_live_service(&server);
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
        wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
    });
    run_shell_script(&fixture, "extension/tests/integration/cursor-stale.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn stale_claude_card_starts_sign_in_after_auth_failure() {
    use std::os::unix::fs::symlink;

    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        },"notifications":false}),
        |root| {
            let bin = root.join("home/.local/bin");
            fs::create_dir_all(&bin).unwrap();
            symlink("/usr/bin/true", bin.join("claude")).unwrap();
            symlink("/usr/bin/true", bin.join("ptyxis")).unwrap();
            let credentials = root.join("home/.claude/.credentials.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(
                credentials,
                r#"{"claudeAiOauth":{"accessToken":"service-fixture","subscriptionType":"Pro"}}"#,
            )
            .unwrap();
            cached_claude_snapshot(root);
        },
        |command| {
            command
                .env("TMB_FIXTURE_API_URL", server.uri())
                .env("PATH", "/nonexistent");
        },
    );
    runtime.block_on(assert_stale_claude_recovery(&fixture, "sign-in"));
    run_shell_script(
        &fixture,
        "extension/tests/integration/stale-recovery.test.js",
        &[
            ("TMB_RECOVERY_ACTION", "Sign in to Claude"),
            ("TMB_RECOVERY_CAPTURE", "sign-in"),
            ("TMB_RECOVERY_SUMMARY", "Sign-in needs renewal"),
            ("TMB_RECOVERY_DETAIL", "HTTP 401. Run claude"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn stale_claude_card_opens_setup_for_unreadable_credentials_in_light_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{
            "claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        },"notifications":false}),
        |root| {
            let credentials = root.join("home/.claude/.credentials.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, "{").unwrap();
            cached_claude_snapshot(root);
        },
    );
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(assert_stale_claude_recovery(&fixture, "unreadable"));
    run_shell_script(
        &fixture,
        "extension/tests/integration/stale-recovery.test.js",
        &[
            ("TMB_RECOVERY_ACTION", "Set up Claude"),
            ("TMB_RECOVERY_CAPTURE", "setup-light"),
            ("TMB_RECOVERY_SUMMARY", "Credentials could not be read"),
            ("TMB_RECOVERY_DETAIL", "is not valid JSON"),
            ("TMB_THEME_MODE", "light"),
            ("GSETTINGS_BACKEND", "memory"),
        ],
    );
}
