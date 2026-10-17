//! Provides isolated Shell test fixtures for portal.

use super::*;

pub(crate) fn fresh_install_fixture() -> IsolatedService {
    isolated_service_with(&serde_json::json!({}), |root| {
        fs::remove_file(root.join("config/quotaline/settings.json")).unwrap();
    })
}

#[cfg(feature = "fixture-api")]
pub(crate) fn portal_shell_fixture(script: &'static str, response: u32, uri_override: Option<&str>) {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    let path = if response == 5 {
        fixture.scratch.path().join("missing/history-export.csv")
    } else {
        fixture.scratch.path().join("home/history-export.csv")
    };
    let marker = fixture.scratch.path().join("portal-requested");
    let reply_marker = fixture.scratch.path().join("portal-replied");
    let signal_marker = fixture.scratch.path().join("portal-signal-received");
    let await_reply = matches!(response, 2 | 5 | 8) || uri_override.is_some();
    let disabled = fixture.scratch.path().join("portal-disabled");
    let closed = fixture.scratch.path().join("portal-closed");
    let gate = (response == 6 || response == 7).then(|| std::sync::Arc::new(tokio::sync::Notify::new()));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let (sender, mut requests) = tokio::sync::mpsc::unbounded_channel();
        let (changes, mut change_events) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = changes.send(event);
        })
        .unwrap();
        notify::Watcher::watch(
            &mut watcher,
            fixture.scratch.path(),
            notify::RecursiveMode::NonRecursive,
        )
        .unwrap();
        let portal = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .name("org.freedesktop.portal.Desktop")
            .unwrap()
            .serve_at(
                "/org/freedesktop/portal/desktop",
                PortalChooser(
                    sender,
                    response == 3,
                    (response == 4 || response == 7).then_some(marker.clone()),
                    gate.clone(),
                ),
            )
            .unwrap()
            .serve_at(
                "/org/freedesktop/portal/desktop/request/test/tmb",
                PortalRequest(closed.clone()),
            )
            .unwrap()
            .build()
            .await
            .unwrap();
        let path = path.to_str().unwrap().to_owned();
        let uri = uri_override
            .map(str::to_owned)
            .unwrap_or_else(|| format!("file://{path}"));
        let expected = if response == 5 {
            "Export directory does not exist"
        } else if response == 3 {
            "Save dialog unavailable"
        } else if response == 2 {
            "Save dialog closed without a file"
        } else if uri_override.is_some() {
            "Save dialog did not return a local file"
        } else {
            ""
        };
        let disabled_for_shell = disabled.clone();
        let closed_for_shell = closed.clone();
        let reply_marker_for_shell = reply_marker.clone();
        let signal_marker_for_shell = signal_marker.clone();
        let shell = tokio::task::spawn_blocking(move || {
            run_shell_script(
                &fixture,
                script,
                &[
                    ("TMB_EXPORT_PATH", &path),
                    ("TMB_EXPECT_ERROR", expected),
                    ("TMB_PORTAL_MARKER", marker.to_str().unwrap()),
                    (
                        "TMB_PORTAL_REPLY_MARKER",
                        if await_reply {
                            reply_marker_for_shell.to_str().unwrap()
                        } else {
                            ""
                        },
                    ),
                    (
                        "TMB_PORTAL_SIGNAL_MARKER",
                        if response == 6 {
                            signal_marker_for_shell.to_str().unwrap()
                        } else {
                            ""
                        },
                    ),
                    ("TMB_PORTAL_DISABLED", disabled_for_shell.to_str().unwrap()),
                    ("TMB_PORTAL_CLOSED", closed_for_shell.to_str().unwrap()),
                    ("TMB_PORTAL_WAIT_CLOSE", if response == 7 { "1" } else { "0" }),
                ],
            )
        });
        requests.recv().await.expect("Save dialog sender closed");
        if await_reply {
            while !reply_marker.exists() {
                change_events
                    .recv()
                    .await
                    .expect("portal marker watcher stopped")
                    .unwrap();
            }
            portal
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/portal/desktop/request/test/other",
                    "org.freedesktop.portal.Request",
                    "Response",
                    &(
                        0_u32,
                        std::collections::HashMap::<String, zbus::zvariant::Value<'_>>::new(),
                    ),
                )
                .await
                .unwrap();
        }
        if response != 3 && response != 4 && response != 7 {
            let mut results = std::collections::HashMap::new();
            results.insert("uris", zbus::zvariant::Value::new(vec![uri]));
            portal
                .emit_signal(
                    None::<&str>,
                    "/org/freedesktop/portal/desktop/request/test/tmb",
                    "org.freedesktop.portal.Request",
                    "Response",
                    &(
                        if response == 5 || response == 6 || response == 8 {
                            0
                        } else {
                            response
                        },
                        results,
                    ),
                )
                .await
                .unwrap();
        }
        if response == 6 {
            while !signal_marker.exists() {
                change_events
                    .recv()
                    .await
                    .expect("portal signal watcher stopped")
                    .unwrap();
            }
        }
        if response == 7 {
            while !disabled.exists() {
                change_events
                    .recv()
                    .await
                    .expect("portal marker watcher stopped")
                    .unwrap();
            }
        }
        if let Some(gate) = &gate {
            gate.notify_one();
        }
        shell.await.unwrap();
    });
}
