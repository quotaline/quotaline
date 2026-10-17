//! Exercises refresh intervals in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn disabling_a_provider_drops_its_in_flight_result() {
    interrupted_cursor_refresh(false);
}

#[cfg(feature = "fixture-api")]
#[test]
fn entering_demo_mode_drops_an_in_flight_provider_result() {
    interrupted_cursor_refresh(true);
}

#[cfg(feature = "fixture-api")]
#[test]
fn scheduled_provider_refresh_updates_cursor_usage() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with({
                let requests = requests.clone();
                move |_: &wiremock::Request| {
                    let used = if requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        2500
                    } else {
                        5000
                    };
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "individualUsage":{"plan":{"used":used,"limit":10000}}
                    }))
                }
            })
            .mount(&server)
            .await;
    });
    let fixture = fast_cursor_service(&server, 1);
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
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["windows"][0]["used"] == 25.0
        })
        .await;
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["windows"][0]["used"] == 50.0 && state["is_refreshing"] == false
        })
        .await;
        assert_eq!(state["is_refreshing"], false);
        assert!(
            state["last_refresh"]
                .as_i64()
                .is_some_and(|stamp| stamp <= chrono::Utc::now().timestamp())
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn missing_credentials_wait_for_panel_activation() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "individualUsage":{"plan":{"used":25,"limit":100}}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = missing_cursor_service(&server);
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
        let signals = proxy.receive_signal("LogChanged").await.unwrap();
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["credential_health"] == "missing"
        })
        .await;
        Box::pin(
            futures_util::stream::once(async {})
                .chain(signals.map(|_| ()))
                .then(|_| cursor_log_count(&proxy))
                .filter(|count| futures_util::future::ready(*count == 1)),
        )
        .next()
        .await
        .expect("Log signal stream ended before the credential check");
        tokio::time::sleep(Duration::from_secs(3)).await;
        assert_eq!(
            cursor_log_count(&proxy).await,
            1,
            "Missing Cursor credentials were checked on the quota schedule"
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        let path = fixture.scratch.path().join("home/.cursor/auth.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
        let _: () = proxy.call("SetPanelOpen", &true).await.unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
        assert_eq!(state["providers"]["cursor"]["credential_health"], "valid");
        assert_eq!(state["providers"]["cursor"]["windows"][0]["used"], 25.0);
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn previously_connected_provider_keeps_retrying_after_credential_loss() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "individualUsage":{"plan":{"used":25,"limit":100}}
            })))
            .expect(2)
            .mount(&server)
            .await;
    });
    let fixture = fast_cursor_service(&server, 1);
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
        wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
        let credential = fixture.scratch.path().join("home/.cursor/auth.json");
        fs::remove_file(&credential).unwrap();
        let stale = wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["status"] == "stale"
                && state["providers"]["cursor"]["credential_health"] == "missing"
        })
        .await;
        assert_eq!(stale["providers"]["cursor"]["windows"][0]["used"], 25.0);
        fs::write(credential, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
        wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn open_panel_uses_provider_floor_and_close_restores_background_interval() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let (request_sender, mut request_events) = tokio::sync::mpsc::unbounded_channel();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with({
                let requests = requests.clone();
                move |_: &wiremock::Request| {
                    request_sender.send(()).unwrap();
                    let used = (requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1) * 2500;
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "individualUsage":{"plan":{"used":used,"limit":10000}}
                    }))
                }
            })
            .mount(&server)
            .await;
    });
    let fixture = fast_cursor_service(&server, 30);
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
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["windows"][0]["used"] == 25.0
        })
        .await;
        let _: () = proxy.call("SetPanelOpen", &true).await.unwrap();
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["windows"][0]["used"] == 50.0
        })
        .await;
        let _: () = proxy.call("SetPanelOpen", &false).await.unwrap();
        while request_events.try_recv().is_ok() {}
        assert!(
            tokio::time::timeout(Duration::from_secs(2), request_events.recv())
                .await
                .is_err(),
            "Cursor refreshed again before its background interval"
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 2);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn scheduled_provider_refresh_honors_retry_after() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let (request_sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with({
                let request_sender = request_sender.clone();
                move |_: &wiremock::Request| {
                    request_sender.send(()).unwrap();
                    ResponseTemplate::new(429).insert_header("Retry-After", "30")
                }
            })
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/aiserver.v1.DashboardService/GetCurrentPeriodUsage"))
            .respond_with(move |_: &wiremock::Request| {
                request_sender.send(()).unwrap();
                ResponseTemplate::new(429).insert_header("Retry-After", "30")
            })
            .mount(&server)
            .await;
    });
    let fixture = fast_cursor_service(&server, 1);
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
        wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "rate-limited").await;
        while requests.try_recv().is_ok() {}
        let refreshed: bool = proxy.call("Refresh", &("cursor",)).await.unwrap();
        assert!(refreshed);
        assert!(
            tokio::time::timeout(Duration::from_secs(3), requests.recv())
                .await
                .is_err(),
            "Cursor retried before Retry-After elapsed"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn idle_refresh_interval_expands_until_panel_opens() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with({
                let requests = requests.clone();
                move |_: &wiremock::Request| {
                    let used = (requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1) * 25;
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({
                        "individualUsage":{"plan":{"used":used,"limit":100}}
                    }))
                }
            })
            .mount(&server)
            .await;
    });
    let fixture = fast_cursor_service_with(&server, 2, |command| {
        command.env("TMB_FIXTURE_IDLE_SECONDS", "1");
    });
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
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["windows"][0]["used"] == 25.0
        })
        .await;
        assert!(
            tokio::time::timeout(
                Duration::from_millis(2_500),
                wait_for_state(&proxy, |state| {
                    state["providers"]["cursor"]["windows"][0]["used"] == 50.0
                })
            )
            .await
            .is_err()
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::SeqCst), 1);
        let _: () = proxy.call("SetPanelOpen", &true).await.unwrap();
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["windows"][0]["used"] == 50.0
        })
        .await;
    });
}
