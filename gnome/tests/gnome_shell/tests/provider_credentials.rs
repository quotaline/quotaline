//! Exercises provider credentials in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_uses_its_home_directory_override() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"standard-tier"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.75}]
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let alternate = tempfile::tempdir().unwrap();
    let credentials = alternate.path().join(".gemini/oauth_creds.json");
    fs::create_dir_all(credentials.parent().unwrap()).unwrap();
    fs::write(&credentials, r#"{"access_token":"alternate-gemini"}"#).unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("GEMINI_CLI_HOME", alternate.path());
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "gemini", "connected"));
    assert_eq!(
        state["providers"]["gemini"]["source"],
        credentials.display().to_string()
    );
    assert_eq!(state["providers"]["gemini"]["windows"][0]["used"], 25.0);
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_refresh_finds_oauth_client_beside_installed_cli() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .and(matchers::body_string_contains("client_id=path-fixture-id"))
            .and(matchers::body_string_contains("client_secret=path-fixture-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"path-fixture-fresh","expires_in":3600
            })))
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer path-fixture-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"standard-tier"}
            })))
            .expect(2)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .and(matchers::header("authorization", "Bearer path-fixture-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.4}]
            })))
            .expect(2)
            .mount(&server)
            .await;
    });
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false});
    let cli = tempfile::tempdir().unwrap();
    let binary = cli.path().join("bin/gemini");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(&binary, b"fixture").unwrap();
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
    let source = cli
        .path()
        .join("lib/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        source,
        "const OAUTH_CLIENT_ID = 'path-fixture-id'; const OAUTH_CLIENT_SECRET = 'path-fixture-secret';",
    )
    .unwrap();
    let prepare = |root: &Path| {
        let credentials = root.join("home/.gemini/oauth_creds.json");
        fs::create_dir_all(credentials.parent().unwrap()).unwrap();
        fs::write(
            credentials,
            r#"{"access_token":"path-fixture-old","refresh_token":"path-fixture-refresh","expiry_date":1}"#,
        )
        .unwrap();
    };
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"allow_token_refresh":true}),
        prepare,
        |command| {
            command
                .env("TMB_FIXTURE_API_URL", server.uri())
                .env("PATH", format!("{}:/usr/bin:/bin", cli.path().join("bin").display()))
                .env_remove("NPM_CONFIG_PREFIX")
                .env_remove("GEMINI_OAUTH_CLIENT_ID")
                .env_remove("GEMINI_OAUTH_CLIENT_SECRET");
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "gemini", "connected"));
    assert_eq!(state["providers"]["gemini"]["windows"][0]["used"], 60.0);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.scratch.path().join("home/.gemini/oauth_creds.json")).unwrap())
            .unwrap();
    assert_eq!(saved["access_token"], "path-fixture-fresh");
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"allow_token_refresh":true}),
        prepare,
        |command| {
            command
                .env("TMB_FIXTURE_API_URL", server.uri())
                .env("PATH", "/usr/bin:/bin")
                .env("NPM_CONFIG_PREFIX", cli.path())
                .env_remove("GEMINI_OAUTH_CLIENT_ID")
                .env_remove("GEMINI_OAUTH_CLIENT_SECRET");
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "gemini", "connected"));
    assert_eq!(state["providers"]["gemini"]["windows"][0]["used"], 60.0);
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.scratch.path().join("home/.gemini/oauth_creds.json")).unwrap())
            .unwrap();
    assert_eq!(saved["access_token"], "path-fixture-fresh");
    runtime.block_on(server.verify());
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_refresh_uses_oauth_client_from_service_environment() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .and(matchers::body_string_contains("client_id=gemini-environment-id"))
            .and(matchers::body_string_contains("client_secret=gemini-environment-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"gemini-environment-fresh","expires_in":3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer gemini-environment-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .and(matchers::header("authorization", "Bearer gemini-environment-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.2}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers,"allow_token_refresh":true}),
            |root| {
                let path = root.join("home/.gemini/oauth_creds.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"access_token":"old","refresh_token":"refresh-me","expiry_date":1}"#).unwrap();
            },
            |command| {
                command
                    .env("TMB_FIXTURE_API_URL", server.uri())
                    .env("GEMINI_OAUTH_CLIENT_ID", "gemini-environment-id")
                    .env("GEMINI_OAUTH_CLIENT_SECRET", "gemini-environment-secret");
            },
        );
        let state = provider_with_status(&fixture, "gemini", "connected").await;
        assert_eq!(state["providers"]["gemini"]["windows"][0]["used"], 80.0);
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn expired_cursor_session_requests_sign_in_before_network_access() {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,
        "antigravity":false,"cursor":true,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), |root| {
        let path = root.join("home/.cursor/auth.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, r#"{"accessToken":"e30.eyJleHAiOjF9.fixture"}"#).unwrap();
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
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["notice"] == "Cursor session expired; open Cursor or run cursor-agent login."
        })
        .await;
        assert_eq!(state["providers"]["cursor"]["status"], "sign-in");
        assert_eq!(
            state["providers"]["cursor"]["notice"],
            "Cursor session expired; open Cursor or run cursor-agent login."
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_uses_its_explicit_auth_file() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("POST"))
            .and(matchers::path(
                "/antigravity-primary/v1internal:retrieveUserQuotaSummary",
            ))
            .and(matchers::header("authorization", "Bearer antigravity-alternate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer antigravity-alternate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"ultra-tier","name":"Google AI Ultra"}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let alternate = tempfile::tempdir().unwrap();
    let credentials = alternate.path().join("auth.json");
    fs::write(&credentials, r#"{"access_token":"antigravity-alternate"}"#).unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("ANTIGRAVITY_AUTH_FILE", &credentials);
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "antigravity", "connected"));
    assert_eq!(
        state["providers"]["antigravity"]["source"],
        credentials.display().to_string()
    );
    assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 75.0);
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_uses_its_home_directory_override() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token alternate-copilot"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "quota_snapshots":{"chat":{"percent_remaining":40}}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let alternate = tempfile::tempdir().unwrap();
    let credentials = alternate.path().join("config.json");
    fs::write(
        &credentials,
        r#"{"loggedInUsers":[{"token":"alternate-copilot","host":"github.com","user":"octocat"}]}"#,
    )
    .unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("COPILOT_HOME", alternate.path());
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(provider_with_status(&fixture, "copilot", "connected"));
    assert_eq!(
        state["providers"]["copilot"]["source"],
        credentials.display().to_string()
    );
    assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 60.0);
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_rejects_an_invalid_credential_host() {
    let alternate = tempfile::tempdir().unwrap();
    fs::write(
        alternate.path().join("config.json"),
        r#"{"loggedInUsers":[{"token":"copilot-fixture","host":"github.com/other"}]}"#,
    )
    .unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("COPILOT_HOME", alternate.path());
        },
    );
    let state =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(provider_with_status(&fixture, "copilot", "unreadable"));
    assert_eq!(state["providers"]["copilot"]["notice"], "Invalid GitHub host");
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_uses_each_supported_environment_token() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for variable in ["COPILOT_GITHUB_TOKEN", "GH_TOKEN", "GITHUB_TOKEN"] {
            let server = MockServer::start().await;
            Mock::given(matchers::method("GET"))
                .and(matchers::path("/copilot_internal/user"))
                .and(matchers::header("authorization", "token environment-token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":25}}
                })))
                .expect(1)
                .mount(&server)
                .await;
            let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,
                "antigravity":false,"cursor":false,"copilot":true});
            let fixture = isolated_service_configured(
                &serde_json::json!({"providers":providers}),
                |root| {
                    let path = root.join("home/.copilot/config.json");
                    fs::create_dir_all(path.parent().unwrap()).unwrap();
                    fs::write(path, "{").unwrap();
                },
                |command| {
                    command
                        .env("TMB_FIXTURE_API_URL", server.uri())
                        .env(variable, "environment-token");
                },
            );
            let state = provider_with_status(&fixture, "copilot", "connected").await;
            assert_eq!(state["providers"]["copilot"]["source"], "environment", "{variable}");
            assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 75.0, "{variable}");
            server.verify().await;
        }
    });
}
