//! Exercises copilot keyring in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_secret_service_account_precedes_stale_file_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token keyring-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "copilot_plan":"individual",
                "login":"ada",
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":69}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let home = root.join("home/.copilot");
                fs::create_dir_all(&home).unwrap();
                fs::write(
                    home.join("config.json"),
                    r#"{"loggedInUsers":[{"user":"zoe","host":"https://github.com/","token":"stale-file"},{"user":"ada","host":"https://github.com/","token":"stale-file"}]}"#,
                )
                .unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = copilot_keyring_fixture(
            &fixture,
            &[
                ("https://github.com:x", "wrong-account"),
                (
                    "https://github.com:ada",
                    r#"{"token":"keyring-fixture","host":"https://github.com/","user":"keyring-ada"}"#,
                ),
            ],
        )
        .await;
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
        let accepted: bool = proxy.call("Refresh", &("copilot",)).await.unwrap();
        assert!(accepted);
        let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 31.0);
        assert_eq!(state["providers"]["copilot"]["source"], "Secret Service: copilot-cli");
        assert_eq!(state["providers"]["copilot"]["identity"], "keyring-ada");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_map_account_finds_username_keyring_item() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for (case, config) in [
            ("mapped login", r#"{"loggedInUsers":{"github.com":{"login":"ada"}}}"#),
            (
                "array keychain account",
                r#"{"loggedInUsers":[{"keychainAccount":"https://github.com:ada"}]}"#,
            ),
        ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token map-keyring"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "copilot_plan":"individual",
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":64}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let path = root.join("home/.copilot/config.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, config).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Copilot fixture",
            &[("service", "copilot-cli"), ("username", "https://github.com:ada")],
            br#"{"access_token":"map-keyring","host":"https://github.com/","login":"keyring-ada"}"#,
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
        let accepted: bool = proxy.call("Refresh", &("copilot",)).await.unwrap();
        assert!(accepted);
        let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
        assert_eq!(state["providers"]["copilot"]["source"], "Secret Service: copilot-cli", "{case}");
        assert_eq!(state["providers"]["copilot"]["identity"], "keyring-ada", "{case}");
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 36.0);
        server.verify().await;
        }
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_map_account_uses_oauth_token_file() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token map-file"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "copilot_plan":"individual",
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":63}}
            })))
            .expect(1..=2)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let path = root.join("home/.copilot/config.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"loggedInUsers":{"github.com":{"username":"ada","oauthToken":"map-file"}}}"#)
                    .unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
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
        let accepted: bool = proxy.call("Refresh", &("copilot",)).await.unwrap();
        assert!(accepted);
        let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
        assert!(state["providers"]["copilot"]["source"].as_str().unwrap().ends_with("/.copilot/config.json"));
        assert_eq!(state["providers"]["copilot"]["identity"], "ada");
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 37.0);
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_ignores_unlisted_secret_service_accounts() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "copilot_plan":"individual",
                "login":"ada",
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":68}}
            })))
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let home = root.join("home/.copilot");
                fs::create_dir_all(&home).unwrap();
                fs::write(
                    home.join("config.json"),
                    r#"{"loggedInUsers":[{"user":"ada","host":"github.com","token":"file-fixture"}]}"#,
                )
                .unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
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
        wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        let _keyring = copilot_keyring_fixture(&fixture, &[("https://github.com:z", "wrong-account")]).await;
        let mut changes = proxy.receive_signal("Changed").await.unwrap();
        let accepted: bool = proxy.call("Refresh", &("copilot",)).await.unwrap();
        assert!(accepted);
        async {
            loop {
                changes.next().await.expect("quota service disconnected before Copilot refresh");
                if server.received_requests().await.unwrap().len() >= 2 {
                    break;
                }
            }
        }.await;
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| request.headers.get("authorization")
            .and_then(|header| header.to_str().ok()) == Some("token file-fixture")));
        let state: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert!(state["providers"]["copilot"]["source"].as_str().unwrap().ends_with("/.copilot/config.json"));
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 32.0);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_keyring_survives_malformed_cli_config() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token keyring-raw"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "copilot_plan":"individual",
                "login":"ada",
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":67}}
            })))
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let home = root.join("home/.copilot");
                fs::create_dir_all(&home).unwrap();
                fs::write(home.join("config.json"), "{invalid").unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = copilot_keyring_fixture(&fixture, &[("https://github.com:ada", "keyring-raw")]).await;
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
        let accepted: bool = proxy.call("Refresh", &("copilot",)).await.unwrap();
        assert!(accepted);
        let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
        assert_eq!(state["providers"]["copilot"]["source"], "Secret Service: copilot-cli");
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 33.0);
        assert_eq!(state["providers"]["copilot"]["identity"], "ada");
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_keyring_account_and_json_identity() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for (case, account, secret, identity) in [
            ("bare account", "ada", "keyring-identity", "ada"),
            ("HTTPS account", "https://github.com:ada/", "keyring-identity", "ada"),
            ("HTTP account", "http://github.com:ada/", "keyring-identity", "ada"),
            (
                "OAuth token",
                "https://github.com:ada",
                r#"{"oauth_token":"keyring-identity"}"#,
                "ada",
            ),
            (
                "JSON login",
                "github.com",
                r#"{"access_token":"keyring-identity","login":"ada","host":"https://github.com/"}"#,
                "ada",
            ),
            ("empty account user", "https://github.com:", "keyring-identity", ""),
            (
                "path account user",
                "https://github.com:ada/bob",
                "keyring-identity",
                "",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(matchers::method("GET"))
                .and(matchers::path("/copilot_internal/user"))
                .and(matchers::header("authorization", "token keyring-identity"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "copilot_plan":"individual",
                    "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":69}}
                })))
                .expect(1)
                .mount(&server)
                .await;
            let providers = serde_json::json!({
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true
            });
            let fixture = isolated_service_configured(
                &serde_json::json!({"providers":{
                    "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
                }}),
                |_| {},
                |command| {
                    command.env("TMB_FIXTURE_API_URL", server.uri());
                },
            );
            let _keyring = copilot_keyring_fixture(&fixture, &[(account, secret)]).await;
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
            let _: String = proxy
                .call("SetSetting", &("providers", providers.to_string()))
                .await
                .unwrap();
            let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
            assert_eq!(state["providers"]["copilot"]["source"], "Secret Service: copilot-cli");
            assert_eq!(state["providers"]["copilot"]["identity"], identity, "{case}");
            assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 31.0);
            server.verify().await;
        }
    });
}
