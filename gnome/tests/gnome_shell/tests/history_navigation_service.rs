//! Checks the Rust-owned history session over the public D-Bus interface.

use super::super::*;

#[test]
fn history_reading_selection_uses_shared_policy_over_dbus() {
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
        let selected: i32 = proxy
            .call("StepHistoryReading", &(3_i32, -1_i32, vec![1_i32, -1]))
            .await
            .unwrap();
        assert_eq!(selected, 0);
        let rejected: zbus::Result<i32> = proxy.call("StepHistoryReading", &(3_i32, 3_i32, vec![1_i32])).await;
        assert!(rejected.is_err());
    });
}

#[test]
fn history_view_actions_update_range_and_saved_preferences() {
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
        let period: String = proxy
            .call("GetHistoryView", &(r#"{"kind":"period","days":7}"#, "UTC", 0_i32))
            .await
            .unwrap();
        let period: serde_json::Value = serde_json::from_str(&period).unwrap();
        assert_eq!(period["view"]["days"], 7);
        assert_eq!(period["view"]["follow_now"], false);
        assert_eq!(period["history"]["metric"], "usage");
        let rollup: String = proxy
            .call("GetHistoryView", &(r#"{"kind":"rollup","rollup":"day"}"#, "UTC", 0_i32))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&rollup).unwrap()["view"]["rollup"],
            "day"
        );
        let metric: String = proxy
            .call(
                "GetHistoryView",
                &(r#"{"kind":"metric","metric":"inputTokens"}"#, "UTC", 0_i32),
            )
            .await
            .unwrap();
        let metric: serde_json::Value = serde_json::from_str(&metric).unwrap();
        assert_eq!(metric["view"]["metric"], "inputTokens");
        assert_eq!(metric["history"]["metric"], "inputTokens");
        let utc: String = proxy
            .call("GetHistoryView", &(r#"{"kind":"utc","utc":true}"#, "UTC", 0_i32))
            .await
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&utc).unwrap()["view"]["utc"],
            true
        );
        let saved: String = proxy.call("GetState", &()).await.unwrap();
        let saved: serde_json::Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(saved["settings"]["history_days"], 7);
        assert_eq!(saved["settings"]["history_rollup"], "day");
        assert_eq!(saved["settings"]["history_metric"], "inputTokens");
        assert_eq!(saved["settings"]["history_utc"], true);
    });
}

#[test]
fn history_view_rejects_invalid_actions_without_changing_selection() {
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
        for (action, zone, offset) in [
            (r#"{"kind":"load"}"#, "UTC", 90_000_i32),
            ("invalid", "UTC", 0),
            (r#"{"kind":"period","days":2}"#, "UTC", 0),
            (r#"{"kind":"custom","from":"2026-9-1","to":"2026-09-02"}"#, "UTC", 0),
        ] {
            let result: zbus::Result<String> = proxy.call("GetHistoryView", &(action, zone, offset)).await;
            assert!(result.is_err(), "{action}");
        }
        let raw: String = proxy
            .call("GetHistoryView", &(r#"{"kind":"load"}"#, "UTC", 0_i32))
            .await
            .unwrap();
        let view: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(view["view"]["days"], 1);
        assert_eq!(view["view"]["follow_now"], true);
    });
}

#[test]
fn history_view_keeps_navigation_when_the_database_query_fails() {
    let fixture = isolated_service_with(&serde_json::json!({"demo":false}), |root| {
        let path = root.join("data/quotaline/history.sqlite3");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        rusqlite::Connection::open(path)
            .unwrap()
            .execute_batch("CREATE TABLE samples(ts INTEGER, provider TEXT, window_id TEXT, label TEXT, reset TEXT)")
            .unwrap();
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
        let raw: String = proxy
            .call("GetHistoryView", &(r#"{"kind":"period","days":7}"#, "UTC", 0_i32))
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(response["view"]["days"], 7);
        assert!(response["history"].is_null());
        assert!(response["error"].as_str().unwrap().contains("no such column"));
    });
}
