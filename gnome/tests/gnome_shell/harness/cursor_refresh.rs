//! Provides isolated Shell test fixtures for cursor refresh.

use super::*;

pub(crate) fn interrupted_cursor_refresh(demo: bool) {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let (started_sender, started) = tokio::sync::oneshot::channel();
        let started_sender = std::sync::Mutex::new(Some(started_sender));
        let (release, released) = std::sync::mpsc::channel::<()>();
        let released = std::sync::Mutex::new(released);
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(move |_: &wiremock::Request| {
                started_sender.lock().unwrap().take().unwrap().send(()).unwrap();
                released.lock().unwrap().recv().unwrap();
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "individualUsage":{"plan":{"used":5000,"limit":10000}}
                }))
            })
            .expect(1)
            .mount(&server)
            .await;
        let disabled = serde_json::json!({"claude":false,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":disabled}),
            |root| {
                let path = root.join("home/.cursor/auth.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
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
        let mut enabled = disabled.clone();
        enabled["cursor"] = true.into();
        let _: String = proxy
            .call("SetSetting", &("providers", enabled.to_string()))
            .await
            .unwrap();
        started.await.unwrap();
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["is_refreshing"], true);
        assert_eq!(state["refreshing_providers"], serde_json::json!(["cursor"]));
        assert!(state["last_refresh"].is_null());
        if demo {
            let _: String = proxy.call("SetSetting", &("demo", "true")).await.unwrap();
        } else {
            let _: String = proxy
                .call("SetSetting", &("providers", disabled.to_string()))
                .await
                .unwrap();
        }
        let mut changes = proxy.receive_signal("Changed").await.unwrap();
        release.send(()).unwrap();
        if demo {
            changes.next().await.unwrap();
            let raw: String = proxy.call("GetState", &()).await.unwrap();
            let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(state["settings"]["demo"], true);
            assert_eq!(state["providers"]["cursor"]["status"], "connected");
            assert_eq!(state["is_refreshing"], false);
            assert_eq!(state["refreshing_providers"], serde_json::json!([]));
            assert!(state["last_refresh"].as_i64().is_some());
        } else {
            let state = wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "sign-in").await;
            assert_eq!(state["settings"]["providers"]["cursor"], false);
            assert!(state["providers"]["cursor"]["windows"].as_array().unwrap().is_empty());
            let view = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
            assert_eq!(view["history"]["series"], serde_json::json!([]));
        }
        server.verify().await;
    });
}
