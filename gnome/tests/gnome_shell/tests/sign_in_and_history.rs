//! Exercises sign in and history in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn sign_in_notification_opens_provider_controls_in_shell() {
    use std::os::unix::fs::symlink;

    let _shell = shell_lock();
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        },"show_all_providers":false}),
        |root| {
            let directory = root.join("home/.local/bin");
            fs::create_dir_all(&directory).unwrap();
            symlink("/usr/bin/true", directory.join("claude")).unwrap();
            symlink("/usr/bin/true", directory.join("ptyxis")).unwrap();
        },
        |command| {
            command.env("PATH", "/nonexistent");
        },
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/notification-sign-in.test.js",
        &[],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn sign_in_starts_each_installed_client_in_a_terminal() {
    use std::os::unix::fs::symlink;

    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |root| {
            let directory = root.join("home/.local/bin");
            fs::create_dir_all(&directory).unwrap();
            for client in ["claude", "codex", "gemini", "cursor-agent", "copilot"] {
                symlink("/usr/bin/true", directory.join(client)).unwrap();
            }
            symlink("/usr/bin/true", directory.join("ptyxis")).unwrap();
        },
        |command| {
            command.env("PATH", "/nonexistent");
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for provider in ["claude", "codex", "gemini", "cursor", "copilot"] {
            assert!(sign_in_response(&fixture, provider).await, "{provider}");
        }
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_sign_in_starts_the_interactive_client() {
    use std::os::unix::fs::symlink;

    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |root| {
            let directory = root.join("home/.local/bin");
            fs::create_dir_all(&directory).unwrap();
            symlink("/usr/bin/true", directory.join("gemini")).unwrap();
            symlink(env!("CARGO_BIN_EXE_sign_in_capture"), directory.join("ptyxis")).unwrap();
        },
        |command| {
            command.env("PATH", "/nonexistent");
        },
    );
    let listener =
        std::os::unix::net::UnixListener::bind(fixture.scratch.path().join("terminal-arguments.sock")).unwrap();
    assert!(
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(sign_in_response(&fixture, "gemini"))
    );
    let mut ready = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    assert_eq!(unsafe { libc::poll(&mut ready, 1, 5000) }, 1);
    let (mut terminal, _) = listener.accept().unwrap();
    let mut arguments = String::new();
    terminal.read_to_string(&mut arguments).unwrap();
    let directory = fixture.scratch.path().join("home/.local/bin");
    assert_eq!(
        arguments,
        format!("--new-window\n--\n{}\n", directory.join("gemini").display())
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn sign_in_uses_available_gnome_terminal_fallbacks() {
    use std::os::unix::fs::symlink;

    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |root| {
            let directory = root.join("home/.local/bin");
            fs::create_dir_all(&directory).unwrap();
            symlink("/usr/bin/true", directory.join("claude")).unwrap();
            symlink("/usr/bin/true", directory.join("kgx")).unwrap();
        },
        |command| {
            command.env("PATH", "/nonexistent");
        },
    );
    let directory = fixture.scratch.path().join("home/.local/bin");
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        assert!(sign_in_response(&fixture, "claude").await);
        fs::remove_file(directory.join("kgx")).unwrap();
        symlink("/usr/bin/true", directory.join("gnome-terminal")).unwrap();
        assert!(sign_in_response(&fixture, "claude").await);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn sign_in_reports_a_terminal_launch_failure() {
    use std::os::unix::fs::symlink;

    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |root| {
            let directory = root.join("home/.local/bin");
            fs::create_dir_all(&directory).unwrap();
            symlink("/usr/bin/true", directory.join("claude")).unwrap();
            fs::write(directory.join("ptyxis"), "not executable").unwrap();
        },
        |command| {
            command.env("PATH", "/nonexistent");
        },
    );
    let (started, logged) = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let started = sign_in_response(&fixture, "claude").await;
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let logs: String = proxy.call("GetLogs", &(false,)).await.unwrap();
        (started, logs.contains("claude sign-in launch failed"))
    });
    assert_eq!((started, logged), (false, true));
}

#[cfg(feature = "fixture-api")]
#[test]
fn shell_diagnostics_require_detail_and_known_category() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let fixture = isolated_service(&serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false}
        }));
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
        let accepted: bool = proxy
            .call("RecordShellDiagnostic", &("geometry", "menu width=420"))
            .await
            .unwrap();
        assert!(!accepted);
        let _: String = proxy.call("SetSetting", &("detailed_logging", "true")).await.unwrap();
        let accepted: bool = proxy
            .call("RecordShellDiagnostic", &("geometry", "menu width=420"))
            .await
            .unwrap();
        assert!(accepted);
        let accepted: bool = proxy
            .call("RecordShellDiagnostic", &("other", "ignored category"))
            .await
            .unwrap();
        assert!(!accepted);
        let accepted: bool = proxy
            .call("RecordShellDiagnostic", &("tabs", "x".repeat(257)))
            .await
            .unwrap();
        assert!(!accepted);
        let _: String = proxy.call("SetSetting", &("detailed_logging", "false")).await.unwrap();
        let accepted: bool = proxy
            .call("RecordShellDiagnostic", &("status", "ignored after disable"))
            .await
            .unwrap();
        assert!(!accepted);
        let logs: String = proxy.call("GetLogs", &(false,)).await.unwrap();
        let entries: Vec<serde_json::Value> = serde_json::from_str(&logs).unwrap();
        assert_eq!(
            entries
                .iter()
                .filter(|entry| entry["category"] == "geometry" && entry["message"] == "menu width=420")
                .count(),
            1
        );
        assert!(
            !entries
                .iter()
                .any(|entry| entry["message"].as_str().unwrap().starts_with("ignored"))
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn sign_in_without_an_installed_client_uses_the_setup_guide() {
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":{
            "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
        }}),
        |_| {},
        |command| {
            command.env("PATH", "/nonexistent");
        },
    );
    assert!(
        !tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(sign_in_response(&fixture, "claude"))
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn unsupported_provider_sign_in_uses_the_setup_guide() {
    let fixture = isolated_service(&serde_json::json!({"providers":{
        "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
    }}));
    assert!(
        !tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(sign_in_response(&fixture, "antigravity"))
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn demo_sign_in_does_not_start_a_client() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    assert!(
        !tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(sign_in_response(&fixture, "claude"))
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn leaving_demo_mode_clears_preview_providers() {
    let fixture = isolated_service(&serde_json::json!({"demo":true,"providers":{
        "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
    }}));
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
        let before: String = proxy.call("GetState", &()).await.unwrap();
        let before: serde_json::Value = serde_json::from_str(&before).unwrap();
        assert!(before["last_refresh"].as_i64().is_some());
        assert_eq!(before["is_refreshing"], false);
        let state: String = proxy.call("SetSetting", &("demo", "false")).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();
        assert_eq!(state["settings"]["demo"], false);
        assert_eq!(state["providers"]["claude"]["status"], "sign-in");
        assert!(state["last_refresh"].is_null());
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_controls_run_in_reused_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases(
        &fixture,
        &[
            ("history-forward.test.js", &[]),
            ("history-previous.test.js", &[]),
            ("history-series-markers.test.js", &[]),
            ("shortcut-propagation.test.js", &[]),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_turns_counts_the_total_once_in_shell() {
    let _shell = shell_lock();
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let yesterday = (chrono::Utc::now() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let providers = serde_json::json!({"claude":false,"codex":true,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"history_metric":"turns","history_days":7}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))")
                .unwrap();
            for (series, value) in [("total", 3.0), ("cli", 2.0), ("web", 1.0)] {
                db.execute(
                    "INSERT INTO daily_analytics VALUES(?1,'codex','turns',?2,?3)",
                    rusqlite::params![today, series, value],
                )
                .unwrap();
            }
            for (series, value) in [("surface:cli", 4.0), ("surface:web", 1.0), ("model:other", 6.0)] {
                db.execute(
                    "INSERT INTO daily_analytics VALUES(?1,'codex','turns',?2,?3)",
                    rusqlite::params![yesterday, series, value],
                )
                .unwrap();
            }
        },
        |_| {},
    );
    run_shell_script(&fixture, "extension/tests/integration/history-total.test.js", &[]);
}
