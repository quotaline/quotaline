//! Provides isolated Shell test fixtures for notifications.

use super::*;

pub(crate) async fn pending_notifications(proxy: &zbus::Proxy<'_>) -> u32 {
    proxy.call("TestPendingNotificationCount", &()).await.unwrap()
}

pub(crate) async fn deliver_notifications(proxy: &zbus::Proxy<'_>) {
    let _: () = proxy.call("DeliverNotifications", &()).await.unwrap();
}

#[derive(Clone, Copy, Default)]
pub(crate) struct ThresholdCase {
    pub(crate) sound: bool,
    pub(crate) enabled: bool,
    pub(crate) desktop: bool,
    pub(crate) repeat: bool,
    pub(crate) clock_format: Option<&'static str>,
}

pub(crate) fn threshold_notification(case: ThresholdCase) {
    let ThresholdCase {
        sound,
        enabled,
        desktop,
        repeat,
        clock_format,
    } = case;
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reset = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with({
                let requests = requests.clone();
                move |_: &wiremock::Request| {
                    let used = if requests
                        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
                        .is_multiple_of(2)
                    {
                        74
                    } else {
                        76
                    };
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "five_hour":{"utilization":used,"resets_at":reset.as_str()}
                    }))
                }
            })
            .expect(if repeat { 4 } else { 2 })
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers,"notify_thresholds":[75],"notify_sound":sound,
                "notifications":enabled,
                "notify_pace":false,
                "notify_resets":false,"notify_expiring_credits":false}),
            |root| {
                let credentials = root.join("home/.claude/.credentials.json");
                fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"service-fixture"}}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
            },
        );
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let _desktop = if desktop {
            Some(
                zbus::connection::Builder::address(fixture.address.as_str())
                    .name("org.freedesktop.Notifications")
                    .serve_at("/org/freedesktop/Notifications", NotificationSink(sender))
                    .build()
                    .await
                    .unwrap(),
            )
        } else {
            None
        };
        let _portal = match clock_format {
            Some(format) => Some(
                zbus::connection::Builder::address(fixture.address.as_str())
                    .name("org.freedesktop.portal.Desktop")
                    .serve_at("/org/freedesktop/portal/desktop", ClockSettings(format))
                    .build()
                    .await
                    .unwrap(),
            ),
            None => None,
        };
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 74.0).await;
        let mut planned_signal = proxy.receive_signal("NotificationPlanned").await.unwrap();
        let refreshed: bool = proxy.call("Refresh", &(String::from("claude"),)).await.unwrap();
        assert!(refreshed);
        wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 76.0).await;
        if enabled {
            planned_signal.next().await.unwrap();
        }
        assert_eq!(pending_notifications(&proxy).await, u32::from(enabled));
        deliver_notifications(&proxy).await;
        if !enabled {
            assert!(receiver.try_recv().is_err());
            return;
        }
        if !desktop {
            assert!(receiver.try_recv().is_err());
            return;
        }
        let (title, body, actions, sound_hint, suppress_sound) = receiver.recv().await
            .unwrap();
        assert_eq!(title, "Claude Current session at 76%");
        assert!(body.starts_with("Crossed 75% of the current session limit. Resets "));
        let twelve_hour = [" AM.", " PM."].iter().any(|suffix| body.ends_with(suffix));
        assert_eq!(twelve_hour, clock_format == Some("12h"), "{body}");
        assert!(actions.is_empty());
        assert_eq!(sound_hint, sound);
        assert_eq!(suppress_sound, !sound);
        if repeat {
            let refreshed: bool = proxy.call("Refresh", &(String::from("claude"),)).await.unwrap();
            assert!(refreshed);
            wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 74.0).await;
            let refreshed: bool = proxy.call("Refresh", &(String::from("claude"),)).await.unwrap();
            assert!(refreshed);
            wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 76.0).await;
            assert_eq!(pending_notifications(&proxy).await, 0);
            deliver_notifications(&proxy).await;
            assert!(receiver.try_recv().is_err());
        }
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
pub(crate) fn sign_in_notification_action(disable_before_click: bool) {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), |root| {
        let cache = root.join("cache/quotaline");
        fs::create_dir_all(&cache).unwrap();
        fs::write(
            cache.join("snapshot.json"),
            serde_json::json!({"claude":{"id":"claude","name":"Claude","status":"connected",
                "plan":"Pro","identity":"","source":"","windows":[],"notice":"","updated":1_780_000_000}})
            .to_string(),
        )
        .unwrap();
        let credentials = root.join("home/.claude/.credentials.json");
        fs::create_dir_all(credentials.parent().unwrap()).unwrap();
        fs::write(
            credentials,
            r#"{"claudeAiOauth":{"accessToken":"expired","expiresAt":1}}"#,
        )
        .unwrap();
    });
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let _desktop = zbus::connection::Builder::address(fixture.address.as_str())
            .name("org.freedesktop.Notifications")
            .serve_at("/org/freedesktop/Notifications", NotificationSink(sender))
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
        let enabled = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let mut planned_signal = proxy.receive_signal("NotificationPlanned").await.unwrap();
        let _: String = proxy.call("SetSetting", &("providers", enabled.to_string().as_str())).await.unwrap();
        wait_for_state(&proxy, |state| state["providers"]["claude"]["status"] == "sign-in").await;
        planned_signal.next().await.unwrap();
        assert_eq!(pending_notifications(&proxy).await, 1);
        deliver_notifications(&proxy).await;
        let (title, body, actions, _, _) = receiver.recv().await
            .unwrap();
        assert_eq!(title, "Claude sign-in needed");
        assert!(body.contains("token expired"));
        assert_eq!(actions, ["default", "Sign in"]);
        let mut sign_in = proxy.receive_signal("SignInRequested").await.unwrap();
        for (id, action) in [(1_u32, "dismiss"), (9_u32, "default")] {
            _desktop
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/Notifications",
                    "org.freedesktop.Notifications",
                    "ActionInvoked",
                    &(id, action),
                )
                .await
                .unwrap();
            assert!(tokio::time::timeout(Duration::from_millis(200), sign_in.next()).await.is_err());
        }
        _desktop
            .emit_signal(
                None::<&str>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &("malformed", "default"),
            )
            .await
            .unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(200), sign_in.next()).await.is_err());
        if disable_before_click {
            let disabled = serde_json::json!({"claude":false,"codex":false,"gemini":false,
                "antigravity":false,"cursor":false,"copilot":false});
            let _: String = proxy.call("SetSetting", &("providers", disabled.to_string())).await.unwrap();
        }
        _desktop
            .emit_signal(
                None::<&str>,
                "/org/freedesktop/Notifications",
                "org.freedesktop.Notifications",
                "ActionInvoked",
                &(1_u32, "default"),
            )
            .await
            .unwrap();
        if disable_before_click {
            assert!(tokio::time::timeout(Duration::from_millis(200), sign_in.next()).await.is_err());
            return;
        }
        let request = sign_in.next().await
            .unwrap();
        assert_eq!(request.body().deserialize::<(String,)>().unwrap().0, "claude");
    });
}
