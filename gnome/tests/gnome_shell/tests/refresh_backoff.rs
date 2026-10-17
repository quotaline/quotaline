//! Exercises refresh backoff in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn second_provider_refresh_staggers_after_startup() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let (request_sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(move |_: &wiremock::Request| {
                request_sender.send(Instant::now()).unwrap();
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "individualUsage":{"plan":{"used":25,"limit":100}}
                }))
            })
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "rate_limit":{"primary_window":{"used_percent":25}}
            })))
            .mount(&server)
            .await;
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":true,"copilot":false},
            "interval_minutes":{"codex":30,"cursor":5}
        }),
        |root| {
            for (path, value) in [
                (
                    "home/.codex/auth.json",
                    r#"{"tokens":{"access_token":"codex-fixture"}}"#,
                ),
                ("home/.cursor/auth.json", r#"{"accessToken":"cursor-fixture"}"#),
            ] {
                let path = root.join(path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, value).unwrap();
            }
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    );
    runtime.block_on(async {
        let first = requests.recv().await.unwrap();
        let second = requests.recv().await.unwrap();
        assert!(second.duration_since(first) >= Duration::from_millis(5_350));
        drop(fixture);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn power_profile_changes_reschedule_provider_refresh() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let (request_sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(move |_: &wiremock::Request| {
                request_sender.send(()).unwrap();
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "individualUsage":{"plan":{"used":25,"limit":100}}
                }))
            })
            .mount(&server)
            .await;
    });
    let power_directory = tempfile::tempdir().unwrap();
    let (mut power_bus, power_address) = private_bus(power_directory.path(), "power-bus");
    let power_connection = runtime.block_on(async {
        zbus::connection::Builder::address(power_address.trim())
            .name("org.freedesktop.UPower.PowerProfiles")
            .serve_at(
                "/org/freedesktop/UPower/PowerProfiles",
                PowerProfiles {
                    active: "power-saver".into(),
                    degraded: String::new(),
                },
            )
            .build()
            .await
            .unwrap()
    });
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":true,"copilot":false},
            "interval_minutes":{"cursor":2}
        }),
        |root| {
            let path = root.join("home/.cursor/auth.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
            command.env("DBUS_SYSTEM_BUS_ADDRESS", power_address.trim());
        },
    );
    runtime.block_on(async {
        let power = zbus::Proxy::new(
            &power_connection,
            "org.freedesktop.UPower.PowerProfiles",
            "/org/freedesktop/UPower/PowerProfiles",
            "org.freedesktop.UPower.PowerProfiles",
        )
        .await
        .unwrap();
        requests.recv().await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(2_500), requests.recv())
                .await
                .is_err()
        );
        power.set_property("ActiveProfile", "balanced").await.unwrap();
        requests.recv().await.unwrap();
        power
            .set_property("PerformanceDegraded", "high-operating-temperature")
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(2_500), requests.recv())
                .await
                .is_err()
        );
        power.set_property("PerformanceDegraded", "").await.unwrap();
        requests.recv().await.unwrap();
        drop(power);
        drop(power_connection);
        power_bus.0.kill().unwrap();
        assert!(wait_for_exit(&mut power_bus.0, Duration::from_secs(3)).is_some());
        assert!(
            tokio::time::timeout(Duration::from_secs(1), requests.recv())
                .await
                .is_err()
        );
        drop(fixture);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn power_saver_keeps_claude_transcript_scans_to_known_files() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let mut power_connection = None;
    let fixture = isolated_service_configured_with_bus(
        &serde_json::json!({
            "providers":{"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false}
        }),
        |root| {
            let folder = root.join("home/.claude/projects/app");
            fs::create_dir_all(&folder).unwrap();
            let first = serde_json::json!({
                "type":"assistant","timestamp":chrono::Utc::now().to_rfc3339(),
                "message":{"id":"first","model":"claude-sonnet-5-5","usage":{"input_tokens":1}}
            });
            fs::write(folder.join("known.jsonl"), format!("{first}\n{{\"type\":\"assistant\"")).unwrap();
        },
        |address| {
            power_connection = Some(runtime.block_on(async {
                zbus::connection::Builder::address(address)
                    .name("org.freedesktop.UPower.PowerProfiles")
                    .serve_at(
                        "/org/freedesktop/UPower/PowerProfiles",
                        PowerProfiles {
                            active: "power-saver".into(),
                            degraded: String::new(),
                        },
                    )
                    .build()
                    .await
                    .unwrap()
            }));
        },
        |command| {
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    );
    let _power_connection = power_connection.unwrap();
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
        wait_for_state(&proxy, |state| state["claude_local_usage"]["today_messages"] == 1).await;
        let folder = fixture.scratch.path().join("home/.claude/projects/app");
        let next = |id: &str| {
            serde_json::json!({
                "type":"assistant","timestamp":chrono::Utc::now().to_rfc3339(),
                "message":{"id":id,"model":"claude-sonnet-5-5","usage":{"input_tokens":1}}
            })
        };
        fs::write(folder.join("new.jsonl"), format!("{}\n", next("new"))).unwrap();
        use std::io::Write;
        writeln!(
            fs::OpenOptions::new()
                .append(true)
                .open(folder.join("known.jsonl"))
                .unwrap(),
            "\n{}",
            next("known-next")
        )
        .unwrap();
        let state = async {
            let mut changes = proxy.receive_signal("Changed").await.unwrap();
            loop {
                let raw: String = proxy.call("GetState", &()).await.unwrap();
                let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
                if state["claude_local_usage"]["today_messages"] == 2 {
                    break state;
                }
                changes.next().await.unwrap();
            }
        }
        .await;
        assert_eq!(state["claude_local_usage"]["today_messages"], 2);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn service_runs_without_a_system_power_bus() {
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false}
        }),
        |_| {},
        |command| {
            command.env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent/quotaline-system-bus");
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
        let state: String = proxy.call("GetState", &()).await.unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&state).unwrap()["settings"]["demo"],
            false
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn scheduled_provider_refresh_doubles_rate_limit_backoff() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        for (method, path) in [
            ("GET", "/api/usage-summary"),
            ("POST", "/aiserver.v1.DashboardService/GetCurrentPeriodUsage"),
        ] {
            Mock::given(matchers::method(method))
                .and(matchers::path(path))
                .respond_with(ResponseTemplate::new(429))
                .mount(&server)
                .await;
        }
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
            state["providers"]["cursor"]["retry_after"].as_u64() >= Some(5)
        })
        .await;
        let first_count = server.received_requests().await.unwrap().len();
        let refreshed: bool = proxy.call("Refresh", &("cursor",)).await.unwrap();
        assert!(refreshed);
        assert_eq!(server.received_requests().await.unwrap().len(), first_count);
        // A scheduled retry can double it again before the state is read, so wait for at least the doubled backoff.
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["retry_after"].as_u64() >= Some(10)
        })
        .await;
        assert!(server.received_requests().await.unwrap().len() > first_count);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn manual_refresh_doubles_provider_failure_backoff() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        for (method, path) in [
            ("GET", "/api/usage-summary"),
            ("POST", "/aiserver.v1.DashboardService/GetCurrentPeriodUsage"),
        ] {
            Mock::given(matchers::method(method))
                .and(matchers::path(path))
                .respond_with(ResponseTemplate::new(500))
                .mount(&server)
                .await;
        }
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
            state["providers"]["cursor"]["retry_after"].as_u64() >= Some(1)
        })
        .await;
        let refreshed: bool = proxy.call("Refresh", &("cursor",)).await.unwrap();
        assert!(refreshed);
        wait_for_state(&proxy, |state| {
            state["providers"]["cursor"]["retry_after"].as_u64() >= Some(2)
        })
        .await;
    });
}
