//! Provides isolated Shell test fixtures for claude.

use super::*;

pub(crate) fn claude_live_service(server: &MockServer) -> IsolatedService {
    let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let credentials = root.join("home/.claude/.credentials.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"profile-fixture"}}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    )
}

#[cfg(feature = "fixture-api")]
pub(crate) fn claude_usage_state(usage: serde_json::Value, status: &str) -> serde_json::Value {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(usage))
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
    let fixture = claude_live_service(&server);
    runtime.block_on(provider_with_status(&fixture, "claude", status))
}

#[cfg(feature = "fixture-api")]
pub(crate) fn claude_reset_shell_fixture(reset: &str, expected: &str, screenshot: &str) {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":42,"resets_at":reset}
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
    let fixture = claude_live_service(&server);
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
        wait_for_state(&proxy, |state| {
            state["providers"]["claude"]["windows"][0]["reset"] == reset
        })
        .await;
    });
    run_shell_script(
        &fixture,
        "extension/tests/integration/claude-reset.test.js",
        &[
            ("TMB_EXPECT_RESET_TEXT", expected),
            ("TMB_RESET_SCREENSHOT", screenshot),
        ],
    );
}

pub(crate) fn default_selection_service() -> IsolatedService {
    isolated_service_with(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false}
        }),
        |root| {
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            fs::write(
                cache.join("snapshot.json"),
                serde_json::json!({
                    "gemini":{
                        "id":"gemini","name":"Gemini","status":"connected","plan":"Pro","identity":"",
                        "source":"fixture","notice":"","updated":1780000000,
                        "windows":[
                            {"id":"model:a","label":"A","used":10,"reset":null,"group":"other"},
                            {"id":"model:b","label":"B","used":20,"reset":null,"group":"other"},
                            {"id":"model:c","label":"C","used":30,"reset":null,"group":"other","active":false}
                        ]
                    }
                })
                .to_string(),
            )
            .unwrap();
        },
    )
}
