//! Exercises gemini keyring in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_default_storage_ignores_an_unselected_keyring_account() {
    let state = gemini_file_source_state(false, GeminiKeyringFixture::Conflicting);
    assert!(
        state["providers"]["gemini"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/.gemini/oauth_creds.json")
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_encrypted_storage_uses_file_without_a_keyring_service() {
    let state = gemini_file_source_state(true, GeminiKeyringFixture::Missing);
    assert!(
        state["providers"]["gemini"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/.gemini/oauth_creds.json")
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_encrypted_storage_uses_file_when_keyring_is_empty() {
    let state = gemini_file_source_state(true, GeminiKeyringFixture::Empty);
    assert!(
        state["providers"]["gemini"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/.gemini/oauth_creds.json")
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_unsupported_quota_opens_setup_in_shell() {
    let _shell = shell_lock();
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
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error":{"details":[{"reason":"SUBSCRIPTION_REQUIRED"}]}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false
        },"notifications":false}),
        |root| {
            let credentials = root.join("home/.gemini/oauth_creds.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, r#"{"access_token":"gemini-fixture"}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    let state = runtime.block_on(async {
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
        wait_for_state(&proxy, |state| {
            state["providers"]["gemini"]["recovery_status"] == "unsupported"
        })
        .await
    });
    assert_eq!(state["providers"]["gemini"]["status"], "sign-in");
    assert_eq!(state["providers"]["gemini"]["credential_health"], "valid");
    assert!(
        state["providers"]["gemini"]["notice"]
            .as_str()
            .unwrap()
            .contains("Login with Google")
    );
    runtime.block_on(server.verify());
    run_shell_script(&fixture, "extension/tests/integration/gemini-unsupported.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_keyring_refresh_precedes_stale_file_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"gemini-keyring-fresh","expires_in":3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer gemini-keyring-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"name":"Google AI Pro"},
                "cloudaicompanionProject":"fixture-project"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .and(matchers::header("authorization", "Bearer gemini-keyring-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.42}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers,"allow_token_refresh":true}),
            |root| {
                let gemini = root.join("home/.gemini");
                fs::create_dir_all(&gemini).unwrap();
                fs::write(gemini.join("oauth_creds.json"), r#"{"access_token":"stale-file"}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri())
                    .env("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "true")
                    .env("GEMINI_OAUTH_CLIENT_ID", "fixture-id")
                    .env("GEMINI_OAUTH_CLIENT_SECRET", "fixture-secret");
            },
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Gemini fixture",
            &[("service", "gemini-cli-oauth"), ("username", "main-account")],
            br#"{"token":{"accessToken":"gemini-keyring-old","refreshToken":"gemini-keyring-refresh","expiresAt":1}}"#,
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
        let accepted: bool = proxy.call("Refresh", &("gemini",)).await.unwrap();
        assert!(accepted);
        let state = wait_for_state(&proxy, |state| state["providers"]["gemini"]["status"] == "connected").await;
        let used = state["providers"]["gemini"]["windows"][0]["used"].as_f64().unwrap();
        assert!((used - 58.0).abs() < 0.000_001);
        assert_eq!(state["providers"]["gemini"]["source"], "Secret Service: gemini-cli-oauth");
        let lookup = environment(&mut Command::new("secret-tool"), fixture.scratch.path(), &fixture.address)
            .args(["lookup", "service", "gemini-cli-oauth", "username", "main-account"])
            .output()
            .unwrap();
        assert!(lookup.status.success());
        let saved: serde_json::Value = serde_json::from_slice(&lookup.stdout).unwrap();
        assert_eq!(saved["token"]["accessToken"], "gemini-keyring-fresh");
        assert_eq!(saved["token"]["refreshToken"], "gemini-keyring-refresh");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_refresh_preserves_a_keyring_item_changed_during_oauth() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
                "allow_token_refresh":true
            }),
            |_| {},
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri())
                    .env("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "true")
                    .env("GEMINI_OAUTH_CLIENT_ID", "fixture-id")
                    .env("GEMINI_OAUTH_CLIENT_SECRET", "fixture-secret");
            },
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Gemini fixture",
            &[("service", "gemini-cli-oauth"), ("username", "main-account")],
            br#"{"token":{"accessToken":"old","refreshToken":"shared","expiresAt":1}}"#,
        );
        let root = fixture.scratch.path().to_path_buf();
        let address = fixture.address.clone();
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .respond_with(move |_: &wiremock::Request| {
                store_keyring_item_at(
                    &root,
                    &address,
                    "Gemini replacement",
                    &[("service", "gemini-cli-oauth"), ("username", "main-account")],
                    br#"{"token":{"accessToken":"new-session"}}"#,
                );
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token":"refreshed","expires_in":3600
                }))
            })
            .expect(1)
            .mount(&server)
            .await;
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
        let providers = serde_json::json!({
            "claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false
        })
        .to_string();
        let _: String = proxy.call("SetSetting", &("providers", providers.as_str())).await.unwrap();
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["gemini"]["notice"] == "Gemini keyring item changed during refresh"
        })
        .await;
        assert_eq!(state["providers"]["gemini"]["status"], "sign-in");
        let lookup = environment(&mut Command::new("secret-tool"), fixture.scratch.path(), &fixture.address)
            .args(["lookup", "service", "gemini-cli-oauth", "username", "main-account"])
            .output()
            .unwrap();
        assert!(lookup.status.success());
        let saved: serde_json::Value = serde_json::from_slice(&lookup.stdout).unwrap();
        assert_eq!(saved["token"]["accessToken"], "new-session");
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_refresh_reports_a_keyring_item_locked_during_oauth() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
                "allow_token_refresh":true
            }),
            |_| {},
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri())
                    .env("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "true")
                    .env("GEMINI_OAUTH_CLIENT_ID", "fixture-id")
                    .env("GEMINI_OAUTH_CLIENT_SECRET", "fixture-secret");
            },
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Gemini fixture",
            &[("service", "gemini-cli-oauth"), ("username", "main-account")],
            br#"{"token":{"accessToken":"old","refreshToken":"shared","expiresAt":1}}"#,
        );
        let (started_sender, started) = tokio::sync::oneshot::channel();
        let started_sender = std::sync::Mutex::new(Some(started_sender));
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = std::sync::Mutex::new(released);
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .respond_with(move |_: &wiremock::Request| {
                started_sender.lock().unwrap().take().unwrap().send(()).unwrap();
                released.lock().unwrap().recv().unwrap();
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "access_token":"fresh","expires_in":3600
                }))
            })
            .expect(1)
            .mount(&server)
            .await;
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let secrets = secret_service::SecretService::connect_with_existing(
            secret_service::EncryptionType::Dh,
            connection.clone(),
        )
        .await
        .unwrap();
        let found = secrets
            .search_items(std::collections::HashMap::from([
                ("service", "gemini-cli-oauth"),
                ("username", "main-account"),
            ]))
            .await
            .unwrap();
        let item = &found.unlocked[0];
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let providers = serde_json::json!({
            "claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false
        })
        .to_string();
        let _: String = proxy.call("SetSetting", &("providers", providers.as_str())).await.unwrap();
        started.await.unwrap();
        item.lock().await.unwrap();
        release.send(()).unwrap();
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["gemini"]["notice"] == "Gemini keyring item locked during refresh"
        })
        .await;
        assert_eq!(state["providers"]["gemini"]["status"], "unreadable");
        assert!(item.is_locked().await.unwrap());
        server.verify().await;
    });
}
