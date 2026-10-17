//! Exercises provider routes and rollouts in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_routes_enterprise_hosts_to_their_expected_api_path() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for (host, path) in [
            ("octocorp.ghe.com", "/copilot_internal/user"),
            ("api.octocorp.ghe.com", "/copilot_internal/user"),
            ("github.example.com", "/api/v3/copilot_internal/user"),
        ] {
            let server = MockServer::start().await;
            Mock::given(matchers::method("GET"))
                .and(matchers::path(path))
                .and(matchers::header("authorization", "token enterprise-token"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":20}}
                })))
                .expect(1)
                .mount(&server)
                .await;
            let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,
                "antigravity":false,"cursor":false,"copilot":true});
            let fixture = isolated_service_configured(
                &serde_json::json!({"providers":providers}),
                |_| {},
                |command| {
                    command
                        .env("TMB_FIXTURE_API_URL", server.uri())
                        .env("COPILOT_GITHUB_TOKEN", "enterprise-token")
                        .env("GH_HOST", host);
                },
            );
            let state = provider_with_status(&fixture, "copilot", "connected").await;
            assert_eq!(state["providers"]["copilot"]["windows"][0]["used"], 80.0, "{host}");
            server.verify().await;
        }
    });
}

#[test]
fn codex_rollout_refresh_detects_a_changed_file() {
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), |root| {
        write_codex_rollout(root, 10.0);
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
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["windows"][0]["used"] == 10.0
        })
        .await;
        write_codex_rollout(fixture.scratch.path(), 20.0);
        let accepted: bool = proxy.call("Refresh", &("codex",)).await.unwrap();
        assert!(accepted);
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["windows"][0]["used"] == 20.0
        })
        .await;
    });
}

#[test]
fn codex_keeps_a_newer_reading_over_an_older_local_rollout() {
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), |root| {
        write_codex_rollout(root, 10.0);
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
        wait_for_state(&proxy, |state| {
            state["providers"]["codex"]["windows"][0]["used"] == 10.0
        })
        .await;
        let log = fixture.scratch.path().join("data/quotaline/log.txt");
        let refreshes = || {
            fs::read_to_string(&log)
                .unwrap_or_default()
                .matches("codex refresh: ")
                .count()
        };
        let (_watcher, events) = database_events(&log);
        let before = refreshes();
        write_codex_rollout_at(fixture.scratch.path(), 30.0, "2026-10-10T09:00:00Z");
        let accepted: bool = proxy.call("Refresh", &("codex",)).await.unwrap();
        assert!(accepted);
        while refreshes() == before {
            events.recv().unwrap().unwrap();
        }
        let state: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(state["providers"]["codex"]["windows"][0]["used"], 10.0);
        assert_eq!(state["providers"]["codex"]["status"], "stale");
    });
}
