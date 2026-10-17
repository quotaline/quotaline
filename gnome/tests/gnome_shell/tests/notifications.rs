//! Exercises notifications in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn notification_times_follow_the_twelve_hour_clock_setting() {
    threshold_notification(ThresholdCase {
        enabled: true,
        desktop: true,
        clock_format: Some("12h"),
        ..ThresholdCase::default()
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn threshold_notification_reaches_desktop_with_sound_hint() {
    threshold_notification(ThresholdCase {
        sound: true,
        enabled: true,
        desktop: true,
        ..ThresholdCase::default()
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn threshold_notification_suppresses_sound_when_disabled() {
    threshold_notification(ThresholdCase {
        enabled: true,
        desktop: true,
        ..ThresholdCase::default()
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn repeated_threshold_crossing_notifies_once_per_reset() {
    threshold_notification(ThresholdCase {
        enabled: true,
        desktop: true,
        repeat: true,
        ..ThresholdCase::default()
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn unavailable_notification_server_does_not_interrupt_refresh() {
    threshold_notification(ThresholdCase {
        enabled: true,
        ..ThresholdCase::default()
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn notification_action_listener_failure_is_logged() {
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        },"detailed_logging":true}),
        |_| {},
        |command| {
            command.env("TMB_FIXTURE_NOTIFICATION_INTERFACE", "invalid interface");
        },
    );
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
        let mut changes = proxy.receive_signal("LogChanged").await.unwrap();
        let mut logs = String::new();
        let logs = async {
            loop {
                if logs.contains("Notification actions unavailable") {
                    return logs;
                }
                logs = proxy.call("GetLogs", &(false,)).await.unwrap();
                let accepted: bool = proxy
                    .call("RecordShellDiagnostic", &("status", "listener probe"))
                    .await
                    .unwrap();
                assert!(accepted);
                changes
                    .next()
                    .await
                    .expect("service disconnected before logging listener failure");
            }
        }
        .await;
        assert!(logs.contains("Invalid interface name"));
        let state: String = proxy.call("GetState", &()).await.unwrap();
        assert!(state.contains("\"providers\""));
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn reset_notifications_cover_a_large_set_of_distinct_limits() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let old_reset = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        let new_reset = (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with({
                let attempts = attempts.clone();
                move |_: &wiremock::Request| {
                    let (used, reset) = if attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        (80, old_reset.as_str())
                    } else {
                        (0, new_reset.as_str())
                    };
                    let limits = (0..513)
                        .map(|index| serde_json::json!({
                            "kind":"weekly_scoped",
                            "scope":{"model":{"display_name":format!("Model {index}")}},
                            "percent":used,
                            "resets_at":reset
                        }))
                        .collect::<Vec<_>>();
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"limits":limits}))
                }
            })
            .expect(2)
            .mount(&server)
            .await;
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
                "notifications":true,"notify_resets":true,"notify_pace":false,"notify_thresholds":[75]
            }),
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
        wait_for_state(&proxy, |state| {
            state["providers"]["claude"]["windows"].as_array().is_some_and(|windows| windows.len() == 513)
                && state["providers"]["claude"]["windows"][0]["used"] == 80.0
        })
        .await;
        assert!(proxy.call::<_, _, bool>("Refresh", &("claude",)).await.unwrap());
        wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 0.0).await;
        assert_eq!(pending_notifications(&proxy).await, 513);
        deliver_notifications(&proxy).await;
        let notifications = async {
            let mut notifications = Vec::with_capacity(513);
            for _ in 0..513 {
                notifications.push(receiver.recv().await.expect("notification service disconnected"));
            }
            notifications
        }.await;
        assert!(notifications.iter().all(|(title, ..)| title.ends_with(" reset")));
        assert!(notifications.iter().all(|(_, body, ..)| body == "Usage is back to 0%."));
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn disabling_notifications_stops_an_undelivered_reset_batch() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let old_reset = (chrono::Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
        let new_reset = (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with({
                let attempts = attempts.clone();
                move |_: &wiremock::Request| {
                    let (used, reset) = if attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        (80, old_reset.as_str())
                    } else {
                        (0, new_reset.as_str())
                    };
                    let limits = (0..2)
                        .map(|index| serde_json::json!({
                            "kind":"weekly_scoped",
                            "scope":{"model":{"display_name":format!("Model {index}")}},
                            "percent":used,
                            "resets_at":reset
                        }))
                        .collect::<Vec<_>>();
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"limits":limits}))
                }
            })
            .expect(2)
            .mount(&server)
            .await;
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false},
                "notifications":true,"notify_resets":true,"notify_pace":false,"notify_thresholds":[75]
            }),
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
        let (requested_sender, mut requested) = tokio::sync::mpsc::unbounded_channel();
        let (delivered_sender, mut delivered) = tokio::sync::mpsc::unbounded_channel();
        let gate = std::sync::Arc::new(tokio::sync::Semaphore::new(0));
        let _desktop = zbus::connection::Builder::address(fixture.address.as_str())
            .name("org.freedesktop.Notifications")
            .serve_at(
                "/org/freedesktop/Notifications",
                GatedNotificationSink { requested: requested_sender, gate: gate.clone(), delivered: delivered_sender },
            )
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
        wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 80.0).await;
        let mut planned_signal = proxy.receive_signal("NotificationPlanned").await.unwrap();
        assert!(proxy.call::<_, _, bool>("Refresh", &("claude",)).await.unwrap());
        wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 0.0).await;
        planned_signal.next().await.unwrap();
        assert_eq!(pending_notifications(&proxy).await, 2);
        let pending = proxy.call::<_, _, ()>("DeliverNotifications", &());
        tokio::pin!(pending);
        async {
            tokio::select! {
                result = &mut pending => panic!("notification completed before the gate: {result:?}"),
                requested = requested.recv() => requested.expect("notification request missing"),
            }
        }.await;
        let _: String = proxy.call("SetSetting", &("notifications", "false")).await.unwrap();
        gate.add_permits(1);
        pending.await.unwrap();
        let title = delivered.recv().await.unwrap();
        assert!(title.contains("reset"));
        assert!(delivered.try_recv().is_err());
        assert!(requested.try_recv().is_err());
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn disabled_notifications_do_not_reach_the_desktop() {
    threshold_notification(ThresholdCase {
        desktop: true,
        ..ThresholdCase::default()
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn missing_antigravity_credentials_do_not_notify_after_a_cached_session() {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_with(&serde_json::json!({"providers":providers}), |root| {
        let cache = root.join("cache/quotaline");
        fs::create_dir_all(&cache).unwrap();
        fs::write(
            cache.join("snapshot.json"),
            serde_json::json!({"antigravity":{"id":"antigravity","name":"Antigravity","status":"connected",
                "plan":"Pro","identity":"","source":"","windows":[],"notice":"","updated":1_780_000_000}})
            .to_string(),
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
        let enabled = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let _: String = proxy.call("SetSetting", &("providers", enabled.to_string().as_str())).await.unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["antigravity"]["status"] == "sign-in").await;
        assert_eq!(state["providers"]["antigravity"]["notice"], "No Antigravity sign-in found");
        assert_eq!(pending_notifications(&proxy).await, 0);
        assert!(tokio::time::timeout(Duration::from_millis(500), receiver.recv()).await.is_err());
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn expired_claude_credentials_notify_after_a_cached_session() {
    sign_in_notification_action(false);
}

#[cfg(feature = "fixture-api")]
#[test]
fn disabled_provider_ignores_an_old_sign_in_notification() {
    sign_in_notification_action(true);
}

#[cfg(feature = "fixture-api")]
#[test]
fn enabling_claude_token_refresh_recovers_an_expired_session() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1/oauth/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"fresh-service-token","refresh_token":"fresh-refresh-token","expires_in":3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .and(matchers::header("authorization", "Bearer fresh-service-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":27}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers,"allow_token_refresh":false}),
            |root| {
                let credentials = root.join("home/.claude/.credentials.json");
                fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"old","refreshToken":"old-refresh","expiresAt":1}}"#).unwrap();
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
        // A policy change retries a refresh still in flight, which would fetch usage a second time.
        wait_for_state(&proxy, |state| state["is_refreshing"] == false).await;
        let changed: String = proxy.call("SetSetting", &("allow_token_refresh", "true")).await.unwrap();
        let changed: serde_json::Value = serde_json::from_str(&changed).unwrap();
        assert_eq!(changed["settings"]["allow_token_refresh"], true);
        let state = wait_for_state(&proxy, |state| state["providers"]["claude"]["status"] == "connected").await;
        assert_eq!(state["providers"]["claude"]["windows"][0]["used"], 27.0);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn changing_refresh_policy_retries_an_in_flight_provider() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with({
                let attempts = attempts.clone();
                move |_: &wiremock::Request| {
                    let attempt = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    ResponseTemplate::new(200)
                        .set_body_json(serde_json::json!({"five_hour":{"utilization":if attempt == 0 { 10 } else { 20 }}}))
                        .set_delay(if attempt == 0 { Duration::from_secs(2) } else { Duration::ZERO })
                }
            })
            .expect(2)
            .mount(&server)
            .await;
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false}}),
            |root| {
                let credentials = root.join("home/.claude/.credentials.json");
                fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                fs::write(credentials, r#"{"claudeAiOauth":{"accessToken":"active","expiresAt":9999999999999}}"#)
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
        wait_for_state(&proxy, |state| {
            state["refreshing_providers"]
                .as_array()
                .is_some_and(|providers| providers.contains(&serde_json::json!("claude")))
        })
        .await;
        let _: String = proxy.call("SetSetting", &("allow_token_refresh", "true")).await.unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["claude"]["windows"][0]["used"] == 20.0)
            .await;
        assert_eq!(state["providers"]["claude"]["status"], "connected");
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    });
}
