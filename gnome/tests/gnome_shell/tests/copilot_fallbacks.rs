//! Exercises copilot fallbacks in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_keyring_item_without_a_token_uses_cli_file() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token file-fallback"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":58}}
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
            |root| {
                let home = root.join("home/.copilot");
                fs::create_dir_all(&home).unwrap();
                fs::write(
                    home.join("config.json"),
                    r#"{"loggedInUsers":[{"user":"ada","host":"github.com","token":"file-fallback"}]}"#,
                )
                .unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let _keyring = copilot_keyring_fixture(&fixture, &[("https://github.com:ada", "{}")]).await;
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
        assert!(
            state["providers"]["copilot"]["source"]
                .as_str()
                .unwrap()
                .ends_with("/.copilot/config.json")
        );
        assert_eq!(state["providers"]["copilot"]["identity"], "ada");
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 42.0);
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn malformed_copilot_keyring_item_reports_unreadable_credentials() {
    for (secret, error) in [
        (b"{invalid".as_slice(), "Copilot keyring item is not JSON"),
        (b"\xff".as_slice(), "Copilot keyring item is not text"),
        (
            br#"{"token":"x","host":"bad host"}"#.as_slice(),
            "Invalid Copilot keyring host",
        ),
    ] {
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service(&serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}));
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _keyring = runtime.block_on(start_keyring(&fixture));
        store_keyring_item(
            &fixture,
            "Copilot fixture",
            &[("service", "copilot-cli"), ("account", "https://github.com:ada")],
            if secret == b"\xff" { b"temporary" } else { secret },
        );
        runtime.block_on(async {
            let connection = zbus::connection::Builder::address(fixture.address.as_str())
                .build()
                .await
                .unwrap();
            wait_for_service(&connection).await;
            if secret == b"\xff" {
                let secrets = secret_service::SecretService::connect_with_existing(
                    secret_service::EncryptionType::Dh,
                    connection.clone(),
                )
                .await
                .unwrap();
                let found = secrets
                    .search_items(std::collections::HashMap::from([
                        ("service", "copilot-cli"),
                        ("account", "https://github.com:ada"),
                    ]))
                    .await
                    .unwrap();
                found.unlocked[0]
                    .set_secret(secret, "application/octet-stream")
                    .await
                    .unwrap();
            }
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
            let accepted: bool = proxy.call("Refresh", &("copilot",)).await.unwrap();
            assert!(accepted);
            let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "unreadable").await;
            assert_eq!(state["providers"]["copilot"]["notice"], error);
        });
    }
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_locked_matching_keyring_account_reports_unreadable_credentials() {
    let state = copilot_locked_keyring_state("https://github.com:ada", true, "unreadable");
    assert_eq!(state["providers"]["copilot"]["notice"], "Copilot keyring is locked");
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_locked_unlisted_keyring_account_does_not_block_sign_in() {
    let state = copilot_locked_keyring_state("https://github.com:zoe", true, "sign-in");
    assert!(
        state["providers"]["copilot"]["notice"]
            .as_str()
            .unwrap()
            .contains("No sign-in found")
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_locked_keyring_without_a_configured_account_reports_unreadable_credentials() {
    let state = copilot_locked_keyring_state("https://github.com:ada", false, "unreadable");
    assert_eq!(state["providers"]["copilot"]["notice"], "Copilot keyring is locked");
}

#[cfg(feature = "fixture-api")]
#[test]
fn unusable_copilot_keyring_item_falls_back_to_local_session() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for secret in ["{invalid", ""] {
            let server = MockServer::start().await;
            Mock::given(matchers::method("GET"))
                .and(matchers::path("/copilot_internal/user"))
                .and(matchers::header("authorization", "token file-fixture"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "copilot_plan":"individual",
                    "login":"ada",
                    "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":58}}
                })))
                .expect(1)
                .mount(&server)
                .await;
            let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
            let fixture = isolated_service_configured(
                &serde_json::json!({"providers":{
                    "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
                }}),
                |_| {},
                |command| {
                    command.env("TMB_FIXTURE_API_URL", server.uri());
                },
            );
            let _keyring = copilot_keyring_fixture(&fixture, &[("https://github.com:ada", secret)]).await;
            let home = fixture.scratch.path().join("home/.copilot");
            fs::create_dir_all(&home).unwrap();
            fs::write(
                home.join("config.json"),
                r#"{"loggedInUsers":[{"user":"ada","host":"https://github.com/","token":"file-fixture"}]}"#,
            )
            .unwrap();
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
            let _: String = proxy.call("SetSetting", &("providers", providers.to_string())).await.unwrap();
            let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
            assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 42.0);
            server.verify().await;
        }
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_legacy_apps_survives_malformed_higher_priority_files() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .and(matchers::header("authorization", "token legacy-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "copilot_plan":"individual",
                "login":"ada",
                "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":66}}
            })))
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let home = root.join("home/.copilot");
                let legacy = root.join("config/github-copilot");
                fs::create_dir_all(&home).unwrap();
                fs::create_dir_all(&legacy).unwrap();
                fs::write(home.join("config.json"), "{invalid").unwrap();
                fs::write(legacy.join("hosts.json"), "{invalid").unwrap();
                fs::write(legacy.join("apps.json"), r#"{"github.com:ada":{"oauth_token":"legacy-fixture"}}"#)
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
        let state = wait_for_state(&proxy, |state| state["providers"]["copilot"]["status"] == "connected").await;
        assert!(state["providers"]["copilot"]["source"].as_str().unwrap().ends_with("/github-copilot/apps.json"));
        assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 34.0);
    });
}
