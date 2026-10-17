//! Exercises credit balances and processes in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_credit_balance_reaches_shell() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"standard-tier","availableCredits":[
                    {"creditType":"GOOGLE_ONE_AI","creditAmount":2.5}
                ]}
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
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let gemini = root.join("home/.gemini");
            fs::create_dir_all(&gemini).unwrap();
            fs::write(
                gemini.join("oauth_creds.json"),
                r#"{"access_token":"gemini-credit-fixture"}"#,
            )
            .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    runtime.block_on(provider_with_status(&fixture, "gemini", "connected"));
    run_shell_script(&fixture, "extension/tests/integration/gemini-credit.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn expired_codex_keyring_token_refreshes_in_the_same_item() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"keyring-fresh","refresh_token":"keyring-rotated"
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .and(matchers::header("authorization", "Bearer keyring-fresh"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":33,"limit_window_seconds":18000}},
                "rate_limit_reset_credits":[]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers,"allow_token_refresh":true}),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(codex.join("config.toml"), "cli_auth_credentials_store = 'keyring'\n").unwrap();
                fs::write(codex.join("auth.json"), r#"{"tokens":{"access_token":"stale-file"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        let codex = fixture.scratch.path().join("home/.codex");
        let digest = sha2::Sha256::digest(codex.canonicalize().unwrap().to_string_lossy().as_bytes());
        let account = format!("cli|{}", hex::encode(&digest[..8]));
        let expired = format!("h.{}.s", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#));
        store_keyring_item(
            &fixture,
            "Codex fixture",
            &[("service", "Codex Auth"), ("username", &account)],
            serde_json::json!({"tokens":{"access_token":expired,"refresh_token":"keyring-old"}})
                .to_string()
                .as_bytes(),
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
        let mut enabled = providers;
        enabled["codex"] = true.into();
        let _: String = proxy.call("SetSetting", &("providers", enabled.to_string())).await.unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["codex"]["status"] == "connected").await;
        assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 33.0);
        let lookup = environment(&mut Command::new("secret-tool"), fixture.scratch.path(), &fixture.address)
            .args(["lookup", "service", "Codex Auth", "username", &account])
            .output()
            .unwrap();
        assert!(lookup.status.success());
        let saved: serde_json::Value = serde_json::from_slice(&lookup.stdout).unwrap();
        assert_eq!(saved["tokens"]["access_token"], "keyring-fresh");
        assert_eq!(saved["tokens"]["refresh_token"], "keyring-rotated");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_refresh_preserves_a_keyring_item_changed_during_oauth() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
                "allow_token_refresh":true
            }),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(codex.join("config.toml"), "cli_auth_credentials_store = 'keyring'\n").unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        let codex = fixture.scratch.path().join("home/.codex");
        let digest = sha2::Sha256::digest(codex.canonicalize().unwrap().to_string_lossy().as_bytes());
        let account = format!("cli|{}", hex::encode(&digest[..8]));
        let expired = format!("h.{}.s", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#));
        store_keyring_item(
            &fixture,
            "Codex fixture",
            &[("service", "Codex Auth"), ("username", account.as_str())],
            serde_json::json!({"tokens":{"access_token":expired,"refresh_token":"shared"}})
                .to_string()
                .as_bytes(),
        );
        let root = fixture.scratch.path().to_path_buf();
        let address = fixture.address.clone();
        let changed_account = account.clone();
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/oauth/token"))
            .respond_with(move |_: &wiremock::Request| {
                store_keyring_item_at(
                    &root,
                    &address,
                    "Codex replacement",
                    &[("service", "Codex Auth"), ("username", changed_account.as_str())],
                    br#"{"tokens":{"access_token":"new-session"}}"#,
                );
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token":"refreshed"}))
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
            "claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        })
        .to_string();
        let _: String = proxy.call("SetSetting", &("providers", providers.as_str())).await.unwrap();
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["notice"] == "Codex keyring item changed during refresh"
        })
        .await;
        assert_eq!(state["providers"]["codex"]["status"], "sign-in");
        let lookup = environment(&mut Command::new("secret-tool"), fixture.scratch.path(), &fixture.address)
            .args(["lookup", "service", "Codex Auth", "username", &account])
            .output()
            .unwrap();
        assert!(lookup.status.success());
        let saved: serde_json::Value = serde_json::from_slice(&lookup.stdout).unwrap();
        assert_eq!(saved["tokens"]["access_token"], "new-session");
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn codex_refresh_reports_a_keyring_item_locked_during_oauth() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
                "allow_token_refresh":true
            }),
            |root| {
                let codex = root.join("home/.codex");
                fs::create_dir_all(&codex).unwrap();
                fs::write(codex.join("config.toml"), "cli_auth_credentials_store = 'keyring'\n").unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        let codex = fixture.scratch.path().join("home/.codex");
        let digest = sha2::Sha256::digest(codex.canonicalize().unwrap().to_string_lossy().as_bytes());
        let account = format!("cli|{}", hex::encode(&digest[..8]));
        let expired = format!("h.{}.s", base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#));
        store_keyring_item(
            &fixture,
            "Codex fixture",
            &[("service", "Codex Auth"), ("username", account.as_str())],
            serde_json::json!({"tokens":{"access_token":expired,"refresh_token":"shared"}})
                .to_string()
                .as_bytes(),
        );
        let (started_sender, started) = tokio::sync::oneshot::channel();
        let started_sender = std::sync::Mutex::new(Some(started_sender));
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = std::sync::Mutex::new(released);
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/oauth/token"))
            .respond_with(move |_: &wiremock::Request| {
                started_sender.lock().unwrap().take().unwrap().send(()).unwrap();
                released.lock().unwrap().recv().unwrap();
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"access_token":"fresh"}))
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
                ("service", "Codex Auth"),
                ("username", account.as_str()),
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
            "claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        })
        .to_string();
        let _: String = proxy.call("SetSetting", &("providers", providers.as_str())).await.unwrap();
        started.await.unwrap();
        item.lock().await.unwrap();
        release.send(()).unwrap();
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["notice"] == "Codex keyring item locked during refresh"
        })
        .await;
        assert_eq!(state["providers"]["codex"]["status"], "unreadable");
        assert!(item.is_locked().await.unwrap());
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_discovers_same_user_language_server() {
    assert_antigravity_discovers_local_process(
        "language_server",
        &["--app_data_dir=/tmp/antigravity", "--csrf_token=fixture-csrf"],
        "Antigravity language server",
        "Fixture Pro",
    );
}
