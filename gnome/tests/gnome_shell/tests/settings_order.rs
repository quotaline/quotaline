//! Exercises ordering through the GNOME service.

use super::super::*;

#[test]
fn reorder_rejects_unknown_kind_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let result: zbus::Result<String> = proxy.call("Reorder", &("unknown", "claude", "codex", false)).await;
        assert!(result.is_err());
    });
}

#[test]
fn reorder_persists_provider_order_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let provider: String = proxy
            .call("Reorder", &("provider", "codex", "claude", false))
            .await
            .unwrap();
        let provider: serde_json::Value = serde_json::from_str(&provider).unwrap();
        assert_eq!(provider["ordering"]["providers"][0], "codex");
    });
}

#[test]
fn reorder_persists_model_order_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let model: String = proxy
            .call("Reorder", &("model", "claude:weekly", "claude:session", false))
            .await
            .unwrap();
        let model: serde_json::Value = serde_json::from_str(&model).unwrap();
        assert_eq!(model["ordering"]["models"][0], "claude:weekly");
    });
}

#[test]
fn swapping_providers_preserves_intervening_order_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
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
        let state: String = proxy.call("SwapProvider", &("claude", "gemini")).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(
            state["ordering"]["providers"].as_array().unwrap()[..3],
            serde_json::json!(["gemini", "codex", "claude"]).as_array().unwrap()[..]
        );
    });
}
