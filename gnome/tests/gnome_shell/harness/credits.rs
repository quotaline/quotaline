//! Provides isolated Shell test fixtures for credits.

use super::*;

pub(crate) fn copilot_locked_keyring_state(account: &str, configured: bool, status: &str) -> serde_json::Value {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_with(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            },"notifications":false}),
            |root| {
                if !configured {
                    return;
                }
                let config = root.join("home/.copilot/config.json");
                fs::create_dir_all(config.parent().unwrap()).unwrap();
                fs::write(config, r#"{"loggedInUsers":[{"keychainAccount":"https://github.com:ada"}]}"#).unwrap();
            },
        );
        let _keyring = copilot_keyring_fixture(&fixture, &[(account, "locked-fixture")]).await;
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
        provider_with_status(&fixture, "copilot", status).await
    })
}

#[cfg(feature = "fixture-api")]
pub(crate) fn codex_keyring_failure_state(file: Option<&str>, locked: bool) -> serde_json::Value {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .and(matchers::header("authorization", "Bearer codex-file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":32,"limit_window_seconds":18000}},
                "rate_limit_reset_credits":[]
            })))
            .expect(u64::from(file.is_some()))
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            },"notifications":false}),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(codex.join("config.toml"), "cli_auth_credentials_store = 'keyring'\n").unwrap();
                if let Some(file) = file {
                    fs::write(codex.join("auth.json"), file).unwrap();
                }
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        let codex = fixture.scratch.path().join("home/.codex");
        let digest = sha2::Sha256::digest(codex.canonicalize().unwrap().to_string_lossy().as_bytes());
        let account = format!("cli|{}", hex::encode(&digest[..8]));
        store_keyring_item(
            &fixture,
            "Codex invalid fixture",
            &[("service", "Codex Auth"), ("username", &account)],
            if locked { br#"{"tokens":{"access_token":"locked"}}"# } else { b"not JSON" },
        );
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        if locked {
            let secrets = secret_service::SecretService::connect_with_existing(
                secret_service::EncryptionType::Dh,
                connection.clone(),
            )
            .await
            .unwrap();
            let collection = secrets.get_default_collection().await.unwrap();
            collection.lock().await.unwrap();
            assert!(collection.is_locked().await.unwrap());
        }
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let _: String = proxy.call("SetSetting", &("providers", providers.to_string().as_str())).await.unwrap();
        let state = provider_with_status(&fixture, "codex", if file.is_some() { "connected" } else { "unreadable" }).await;
        server.verify().await;
        state
    })
}

#[cfg(feature = "fixture-api")]
pub(crate) fn codex_credits_shell_case(case: &str, credits: serde_json::Value, resets: serde_json::Value) {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":31,"limit_window_seconds":18000}},
                "credits":credits,
                "rate_limit_reset_credits":resets
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let codex = root.join("home/.codex");
            fs::create_dir_all(&codex).unwrap();
            fs::write(
                codex.join("auth.json"),
                r#"{"tokens":{"access_token":"credit-fixture"}}"#,
            )
            .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    runtime.block_on(async {
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
        wait_for_state(&proxy, |state| state["providers"]["codex"]["status"] == "connected").await;
    });
    run_shell_script(
        &fixture,
        "extension/tests/integration/codex-credits.test.js",
        &[("TMB_CREDIT_CASE", case)],
    );
}
