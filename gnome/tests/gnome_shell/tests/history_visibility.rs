//! Exercises history visibility in an isolated GNOME session.

use super::super::*;

#[test]
fn state_filters_only_selected_full_limit_notices() {
    let fixture = isolated_service_with(
        &serde_json::json!({
            "providers":{"claude":true},
            "has_custom_selection":true,
            "selected_windows":["claude:session"]
        }),
        |root| {
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            fs::write(
                cache.join("snapshot.json"),
                serde_json::json!({"claude":{
                    "id":"claude","name":"Claude","status":"connected","plan":"Pro",
                    "identity":"","source":"Local","notice":"",
                    "updated":chrono::Utc::now().timestamp(),
                    "windows":[
                        {"id":"session","label":"Current session","used":100,"reset":null,"group":"session"},
                        {"id":"weekly","label":"All models","used":100,"reset":null,"group":"weekly"}
                    ],
                    "notices":[
                        {"kind":"limitReached","windowId":"session","text":"Selected limit"},
                        {"kind":"limitReached","windowId":"weekly","text":"Hidden limit"},
                        {"kind":"limitReached","text":"General limit"},
                        {"kind":"promotion","windowId":"session","text":"Promotion"}
                    ]
                }})
                .to_string(),
            )
            .unwrap();
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
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
        let raw: String = proxy.call("GetState", &()).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(state["settings"]["has_custom_selection"], true);
        assert_eq!(
            state["settings"]["selected_windows"],
            serde_json::json!(["claude:session"])
        );
        assert_eq!(state["providers"]["claude"]["windows"][0]["used"], 100.0);
        let texts = state["providers"]["claude"]["notices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|notice| notice["text"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(texts, ["Hidden limit", "General limit", "Promotion"]);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn fully_hidden_history_can_restore_a_series_in_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
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
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await["history"].take();
        let ids = history["series"]
            .as_array()
            .unwrap()
            .iter()
            .map(|series| series["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert!(!ids.is_empty());
        let _: String = proxy
            .call(
                "SetSetting",
                &("history_hidden_keys", serde_json::json!(ids).to_string().as_str()),
            )
            .await
            .unwrap();
    });
    run_shell_script(&fixture, "extension/tests/integration/all-hidden-history.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn light_and_high_contrast_themes_reuse_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases_with_reset(
        &fixture,
        &[
            ("theme.test.js", &[("TMB_THEME_MODE", "light")]),
            ("theme.test.js", &[("TMB_THEME_MODE", "high-contrast")]),
        ],
        -1,
        Some("CC 5h"),
        &[("GSETTINGS_BACKEND", "memory")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn dark_shell_theme_fits_reference_height() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/theme.test.js",
        &[
            ("TMB_THEME_MODE", "dark"),
            ("GSETTINGS_BACKEND", "memory"),
            ("TMB_VIRTUAL_MONITOR", "1920x1080"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn limit_notice_uses_warning_style_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/warning-notice.test.js",
        FaultOptions {
            warning_notice: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn full_visible_limit_does_not_repeat_its_notice_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/limit-notice.test.js",
        FaultOptions {
            duplicate_limit_notice: true,
            ..FaultOptions::default()
        },
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn refreshing_provider_status_identifies_its_retained_reading_in_shell() {
    fault_service_shell_fixture(
        "extension/tests/integration/provider-status.test.js",
        FaultOptions {
            refreshing_provider: true,
            ..FaultOptions::default()
        },
    );
}
