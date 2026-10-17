//! Exercises antigravity credentials in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_refresh_uses_oauth_client_from_service_environment() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .and(matchers::body_string_contains("client_id=fixture-client"))
            .and(matchers::body_string_contains("client_secret=fixture-secret"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"fresh-antigravity","expires_in":3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:retrieveUserQuotaSummary"))
            .and(matchers::header("authorization", "Bearer fresh-antigravity"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.6}]}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let path = root.join("home/.gemini/antigravity-cli/antigravity-oauth-token");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"token":{"access_token":"expired","refresh_token":"refresh-me","expiry":1}}"#).unwrap();
            },
            |command| {
                command
                    .env("TMB_FIXTURE_API_URL", server.uri())
                    .env("ANTIGRAVITY_OAUTH_CLIENT_ID", "fixture-client")
                    .env("ANTIGRAVITY_OAUTH_CLIENT_SECRET", "fixture-secret");
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
        wait_for_state(&proxy, |state| state["providers"]["antigravity"]["status"] == "sign-in").await;
        // Turning token refresh on refreshes every provider, so no explicit refresh is needed.
        let _: String = proxy.call("SetSetting", &("allow_token_refresh", "true")).await.unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["antigravity"]["status"] == "connected").await;
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 40.0);
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_keyring_precedes_stale_file_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:retrieveUserQuotaSummary"))
            .and(matchers::header("authorization", "Bearer keyring-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.64}]}]}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer keyring-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"name":"Google AI Ultra"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers,"notifications":false}),
            |root| {
                let credentials = root.join("home/.gemini/antigravity-cli/antigravity-oauth-token");
                fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                fs::write(credentials, r#"{"token":{"access_token":"stale-file"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        let encoded = base64::engine::general_purpose::STANDARD.encode(
            br#"{"token":{"access_token":"keyring-fixture"},"email":"keyring@example.com"}"#,
        );
        store_keyring_item(
            &fixture,
            "Antigravity fixture",
            &[("service", "gemini"), ("username", "antigravity")],
            format!("go-keyring-base64:{encoded}").as_bytes(),
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
        let accepted: bool = proxy.call("Refresh", &("antigravity",)).await.unwrap();
        assert!(accepted);
        let state = provider_with_status(&fixture, "antigravity", "connected").await;
        assert_eq!(state["providers"]["antigravity"]["source"], "Secret Service: gemini/antigravity");
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 36.0);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_invalid_keyring_falls_back_to_cli_file() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:retrieveUserQuotaSummary"))
            .and(matchers::header("authorization", "Bearer file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.73}]}]}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"name":"Google AI Pro"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            },"notifications":false}),
            |root| {
                let credentials = root.join("home/.gemini/antigravity-cli/antigravity-oauth-token");
                fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                fs::write(credentials, r#"{"token":{"access_token":"file-fixture"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Antigravity invalid fixture",
            &[("service", "gemini"), ("username", "antigravity")],
            b"go-keyring-base64:!",
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
        let _: String = proxy.call("SetSetting", &("providers", providers.to_string().as_str())).await.unwrap();
        let state = provider_with_status(&fixture, "antigravity", "connected").await;
        assert!(state["providers"]["antigravity"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/.gemini/antigravity-cli/antigravity-oauth-token"));
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 27.0);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_invalid_keyring_without_file_reports_unreadable_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_with(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            },"notifications":false}),
            |_| {},
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Antigravity invalid fixture",
            &[("service", "gemini"), ("username", "antigravity")],
            b"go-keyring-base64:!",
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
        let _: String = proxy.call("SetSetting", &("providers", providers.to_string().as_str())).await.unwrap();
        let state = provider_with_status(&fixture, "antigravity", "unreadable").await;
        assert_eq!(
            state["providers"]["antigravity"]["notice"],
            "Antigravity keyring item is not base64"
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_locked_keyring_reports_unreadable_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_with(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            },"notifications":false}),
            |_| {},
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Antigravity locked fixture",
            &[("service", "gemini"), ("username", "antigravity")],
            br#"{"token":{"access_token":"locked-fixture"}}"#,
        );
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let collection = secrets.get_default_collection().await.unwrap();
        collection.lock().await.unwrap();
        assert!(collection.is_locked().await.unwrap());
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let _: String = proxy.call("SetSetting", &("providers", providers.to_string().as_str())).await.unwrap();
        let state = provider_with_status(&fixture, "antigravity", "unreadable").await;
        assert_eq!(state["providers"]["antigravity"]["notice"], "Antigravity keyring is locked");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_app_credentials_survive_a_malformed_cli_file() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:retrieveUserQuotaSummary"))
            .and(matchers::header("authorization", "Bearer app-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let cli = root.join("home/.gemini/antigravity-cli/antigravity-oauth-token");
                fs::create_dir_all(cli.parent().unwrap()).unwrap();
                fs::write(cli, "{").unwrap();
                let app = root.join("config/Antigravity/auth.json");
                fs::create_dir_all(app.parent().unwrap()).unwrap();
                fs::write(app, r#"{"access_token":"app-token"}"#).unwrap();
            },
            |command| {
                command
                    .env("TMB_FIXTURE_API_URL", server.uri())
                    .env_remove("ANTIGRAVITY_AUTH_FILE");
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
        let state = wait_for_state(&proxy, |state| state["providers"]["antigravity"]["status"] == "connected").await;
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 75.0);
        assert!(state["providers"]["antigravity"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/Antigravity/auth.json"));
    });
}
