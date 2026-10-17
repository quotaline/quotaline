//! Checks shared History visibility through the public service interface.

use super::super::*;

#[test]
fn history_visibility_toggle_and_isolation_persist() {
    let fixture = isolated_service(&serde_json::json!({"demo": true}));
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
        let series = ["claude:session", "codex:session", "gemini:session"];
        for (kind, id, hidden, shown) in [
            ("toggle", series[0], vec![series[0]], vec![series[1], series[2]]),
            ("toggle", series[1], vec![series[0], series[1]], vec![series[2]]),
            ("toggle", series[2], vec![series[0], series[1]], vec![series[2]]),
            ("isolate", series[0], vec![series[1], series[2]], vec![series[0]]),
            ("isolate", series[0], vec![series[0], series[1]], vec![series[2]]),
        ] {
            let action = serde_json::json!({"kind": kind, "metric": "usage", "id": id, "series": series});
            let raw: String = proxy
                .call("UpdateHistoryVisibility", &(action.to_string(),))
                .await
                .unwrap();
            let response: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(response["hidden"], serde_json::json!(hidden), "{kind} {id}");
            assert_eq!(response["shown"], serde_json::json!(shown), "{kind} {id}");
            let raw: String = proxy.call("GetState", &()).await.unwrap();
            let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(state["settings"]["history_hidden_keys"], serde_json::json!(hidden));
        }
    });
}

#[test]
fn history_visibility_double_click_undoes_the_first_toggle() {
    let fixture = isolated_service(&serde_json::json!({"demo": true}));
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
        let series = ["claude:session", "codex:session"];
        for kind in ["toggle", "isolate_undo"] {
            let action = serde_json::json!({"kind": kind, "metric": "usage", "id": series[0], "series": series});
            let _: String = proxy
                .call("UpdateHistoryVisibility", &(action.to_string(),))
                .await
                .unwrap();
        }
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["history_hidden_keys"], serde_json::json!([series[1]]));
    });
}

#[test]
fn history_visibility_rejects_invalid_actions_and_syncs_external_settings() {
    let fixture = isolated_service(&serde_json::json!({"demo": true}));
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
        let invalid: zbus::Result<String> = proxy.call("UpdateHistoryVisibility", &(r#"{"kind":"missing"}"#,)).await;
        assert!(invalid.is_err());
        let _: String = proxy
            .call("SetSetting", &("history_hidden_keys", r#"["claude:session"]"#))
            .await
            .unwrap();
        let action = serde_json::json!({"kind": "toggle", "metric": "usage", "id": "missing",
            "series": ["claude:session", "codex:session"]});
        let raw: String = proxy
            .call("UpdateHistoryVisibility", &(action.to_string(),))
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(response["hidden"], serde_json::json!(["claude:session"]));
        assert_eq!(response["shown"], serde_json::json!(["codex:session"]));
    });
}
