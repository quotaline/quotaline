//! Checks shared Settings context in the service state.

use super::super::*;

#[test]
fn service_exposes_shared_provider_refresh_limits() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
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
        let state = wait_for_state(&proxy, |state| state["maximum_refresh_minutes"] == 30).await;
        assert_eq!(state["provider_definitions"][0]["minimum_minutes"], 2);
        assert_eq!(
            state["notification_threshold_choices"],
            serde_json::json!([50, 75, 90, 100])
        );
        assert_eq!(
            state["notification_summary"],
            "On · 75%, 90%, 100%, resets, sign-in, pace, credit expiry"
        );
    });
}

#[test]
fn threshold_toggle_uses_shared_settings_policy() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
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
        let added: String = proxy.call("ToggleNotificationThreshold", &(50_i32,)).await.unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&added).unwrap()["settings"]["notify_thresholds"],
            serde_json::json!([50, 75, 90, 100])
        );
        let removed: String = proxy.call("ToggleNotificationThreshold", &(50_i32,)).await.unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&removed).unwrap()["settings"]["notify_thresholds"],
            serde_json::json!([75, 90, 100])
        );
        assert!(
            proxy
                .call::<_, _, String>("ToggleNotificationThreshold", &(42_i32,))
                .await
                .is_err()
        );
    });
}

#[test]
fn inactive_providers_keep_optional_settings_hidden() {
    let fixture = isolated_service(&serde_json::json!({
        "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
        "notify_expiring_credits":false
    }));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
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
        let state = wait_for_state(&proxy, |state| state["settings_context"].is_object()).await;
        assert_eq!(
            state["settings_context"]["visibility"],
            serde_json::json!({"analytics":false,"pace":false,"tokenRefresh":false,"expiringCredits":false})
        );
        assert!(state["settings_context"].get("notification_summary").is_none());
    });
}

#[test]
fn cached_windows_keep_pace_setting_visible_without_credentials() {
    let fixture = isolated_service_with(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false}
        }),
        |root| {
            let mut provider = quotaline_quota::Provider::empty("gemini", "stale");
            let mut window = quotaline_quota::Window::new("weekly", "Weekly", 40.0, None, "session");
            window.duration_seconds = Some(604_800);
            provider.windows.push(window);
            let path = root.join("cache/quotaline/snapshot.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, serde_json::json!({"gemini":provider}).to_string()).unwrap();
        },
    );
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
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
        let state = wait_for_state(&proxy, |state| state["settings_context"]["visibility"]["pace"] == true).await;
        assert_eq!(state["providers"]["gemini"]["windows"][0]["id"], "weekly");
        assert_eq!(state["settings_context"]["visibility"]["analytics"], false);
    });
}
