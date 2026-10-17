//! Exercises claude transcript history in an isolated GNOME session.

use super::super::*;

#[test]
fn claude_local_analytics_reaches_history_without_network_credentials() {
    let _shell = shell_lock();
    let scratch = tempfile::tempdir().unwrap();
    let config = scratch.path().join("config/quotaline");
    let projects = scratch.path().join("home/.claude/projects/demo");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&projects).unwrap();
    fs::create_dir_all(scratch.path().join("cache")).unwrap();
    fs::create_dir_all(scratch.path().join("data")).unwrap();
    let log_path = scratch.path().join("data/quotaline/log.txt");
    fs::create_dir_all(log_path.parent().unwrap()).unwrap();
    let timestamp = chrono::Utc::now().timestamp_millis();
    let entries = (0..205)
        .map(|index| serde_json::json!({"timestamp":timestamp,"level":"info","category":"fixture","message":format!("Log fixture {index}")}).to_string())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(log_path, format!("{entries}\n")).unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    fs::write(
        config.join("settings.json"),
        serde_json::json!({"providers":providers}).to_string(),
    )
    .unwrap();
    let today = chrono::Utc::now().date_naive().format("%Y-%m-%d").to_string();
    let line = serde_json::json!({"type":"assistant","timestamp":chrono::Utc::now().to_rfc3339(),"sessionId":"session", "message":{"id":"message","model":"claude-sonnet-5-5","usage":{"input_tokens":100_000,"output_tokens":100_000}}});
    let (_bus, address) = private_bus(scratch.path(), "session-bus");
    let service_log = scratch.path().join("service.log");
    let _service = Process(
        environment(
            &mut Command::new(env!("CARGO_BIN_EXE_quotaline-gnome")),
            scratch.path(),
            address.trim(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::from(fs::File::create(&service_log).unwrap()))
        .spawn()
        .unwrap(),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(address.trim())
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
        let mut changes = proxy.receive_signal("Changed").await.unwrap();
        let cost = serde_json::json!({"kind":"metric","metric":"costUSD"});
        let empty = history_view(&proxy, cost.clone()).await["history"].to_string();
        assert!(!empty.contains("claude-sonnet-5-5"));
        fs::write(
            projects.join("session.jsonl"),
            format!("{line}\nprivate transcript text must never be stored\n"),
        )
        .unwrap();
        let enabled = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
        let _: String = proxy.call("SetSetting", &(String::from("providers"), enabled.to_string())).await.unwrap();
        let mut history = empty;
        while !history.contains("claude-sonnet-5-5") {
            changes
                .next()
                .await
                .expect("quota service disconnected before transcript scan");
            history = history_view(&proxy, cost.clone()).await["history"].to_string();
        }
        history_view(&proxy, serde_json::json!({"kind":"metric","metric":"usage"})).await;
    });
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let pack = Command::new("gnome-extensions")
        .current_dir(root)
        .arg("pack")
        .arg(extension_source())
        .arg("--out-dir")
        .arg(scratch.path())
        .args([
            "--force",
            "--extra-source=src",
            "--extra-source=icons",
            "--extra-source=privacy.html",
            "--extra-source=notices.html",
            "--extra-source=provider-setup.html",
        ])
        .output()
        .unwrap();
    assert!(pack.status.success(), "{}", String::from_utf8_lossy(&pack.stderr));
    let complete = scratch.path().join("shell-scenario-complete");
    fs::write(&complete, "").unwrap();
    let output = environment(
        &mut Command::new("gnome-shell-test-tool"),
        scratch.path(),
        address.trim(),
    )
    .env("TMB_SHELL_SCENARIO_DONE", &complete)
    .env("TMB_EXPECT_DEMO", "0")
    .env("TMB_EXPECT_LOCAL", "1")
    .env("TMB_SCREENSHOT_DIR", root.join("../target/shell-screenshots/local"))
    .env("GDK_DEBUG", "no-portals")
    .current_dir(root)
    .args([
        "--headless",
        "--wrap",
        env!("CARGO_BIN_EXE_shell_no_x11"),
        "--extension",
    ])
    .arg(scratch.path().join("quotaline@bernat.tech.shell-extension.zip"))
    .arg(shell_test_script("extension/tests/integration/shell-smoke.js"))
    .output()
    .unwrap();
    let logs = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let completed = fs::read(&complete).unwrap() == b"done";
    assert!(
        shell_script_succeeded(
            output.status.success(),
            "extension/tests/integration/shell-smoke.js",
            completed,
            &logs
        ),
        "Claude Shell smoke test failed:\n{logs}"
    );
    assert!(!extension_error(&logs), "Claude Shell smoke test failed:\n{logs}");
    let checkpoint = fs::read(scratch.path().join("data/quotaline/claude-transcripts.json")).unwrap();
    let state: serde_json::Value = serde_json::from_slice(&checkpoint).unwrap();
    assert_eq!(state["days"][&today]["costs"]["claude-sonnet-5-5"], 1.2);
    assert_eq!(state["days"][&today]["messages"], 1);
    assert!(
        !checkpoint
            .windows(b"private transcript text".len())
            .any(|span| span == b"private transcript text")
    );
    let raw = fs::read(scratch.path().join("data/quotaline/history.sqlite3")).unwrap();
    assert!(
        !raw.windows(b"private transcript text".len())
            .any(|span| span == b"private transcript text")
    );
}

#[test]
fn claude_transcript_scan_skips_oversized_old_and_incomplete_records() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":true,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("home/.claude/projects/app/session.jsonl");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let valid = serde_json::json!({"type":"assistant","timestamp":now.to_rfc3339(),
                "message":{"model":"claude-sonnet-5-5","usage":{"input_tokens":10}}});
            let old = serde_json::json!({"type":"assistant","timestamp":"2020-01-01T00:00:00Z",
                "message":{"model":"claude-sonnet-5-5","usage":{"input_tokens":20}}});
            let future = serde_json::json!({"type":"assistant","timestamp":"2035-01-01T00:00:00Z",
                "message":{"model":"claude-sonnet-5-5","usage":{"input_tokens":30}}});
            fs::write(
                &path,
                format!("{valid}\n{}\n{old}\n{future}\n{valid}", "x".repeat(1_100_000)),
            )
            .unwrap();
            fs::write(path.with_extension("txt"), valid.to_string()).unwrap();
            std::os::unix::fs::symlink(&path, path.with_file_name("linked.jsonl")).unwrap();
            let stale = path.with_file_name("stale.jsonl");
            fs::write(&stale, valid.to_string()).unwrap();
            fs::File::open(stale)
                .unwrap()
                .set_times(fs::FileTimes::new().set_modified(std::time::SystemTime::UNIX_EPOCH))
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
        wait_for_state(&proxy, |state| state["claude_local_usage"]["today_messages"] == 1).await;
        let history =
            history_view(&proxy, serde_json::json!({"kind":"metric","metric":"messages"})).await["history"].take();
        assert_eq!(history["series"][0]["points"][0][1], 1.0);
    });
    let checkpoint = fs::read(fixture.scratch.path().join("data/quotaline/claude-transcripts.json")).unwrap();
    let state: serde_json::Value = serde_json::from_slice(&checkpoint).unwrap();
    let day = now.date_naive().format("%Y-%m-%d").to_string();
    assert_eq!(state["days"][&day]["messages"], 1);
    assert!(state["seenByDay"][&day][0].as_str().unwrap().ends_with(":0"));
}

#[test]
fn unpriced_claude_transcript_keeps_message_metrics_without_cost_points() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":true,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("home/.claude/projects/app/session.jsonl");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                format!(
                    "{}\n",
                    serde_json::json!({
                        "type":"assistant","timestamp":now.to_rfc3339(),"cwd":"/work/app",
                        "message":{"model":"unknown-local-model","usage":{"input_tokens":10}}
                    })
                ),
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
        wait_for_state(&proxy, |state| state["claude_local_usage"]["today_messages"] == 1).await;
        for (metric, points) in [
            ("messages", Some(1.0)),
            ("costUSD", None),
            ("projectCost", None),
            ("projectMessages", Some(1.0)),
        ] {
            let view = history_view(&proxy, serde_json::json!({"kind":"metric","metric":metric})).await;
            let series = view["history"]["series"].as_array().unwrap();
            assert_eq!(
                series.first().map(|series| series["points"][0][1].as_f64().unwrap()),
                points,
                "{metric}"
            );
        }
    });
}

#[test]
fn claude_transcript_scan_removes_deleted_file_history() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":true,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("home/.claude/projects/app/session.jsonl");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                format!(
                    "{}\n",
                    serde_json::json!({
                        "type":"assistant","timestamp":now.to_rfc3339(),
                        "message":{"id":"removed-message","model":"claude-sonnet-5-5","usage":{"input_tokens":10}}
                    })
                ),
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
        let mut changes = proxy.receive_signal("Changed").await.unwrap();
        let messages = serde_json::json!({"kind":"metric","metric":"messages"});
        Box::pin(
            futures_util::stream::once(async {})
                .chain(changes.by_ref().map(|_| ()))
                .then(|_| async { history_view(&proxy, messages.clone()).await["history"].take() })
                .filter(|history| futures_util::future::ready(!history["series"].as_array().unwrap().is_empty())),
        )
        .next()
        .await
        .expect("service disconnected before transcript scan");
        fs::remove_file(fixture.scratch.path().join("home/.claude/projects/app/session.jsonl")).unwrap();
        let _: String = proxy.call("ResetSettings", &()).await.unwrap();
        Box::pin(
            futures_util::stream::once(async {})
                .chain(changes.by_ref().map(|_| ()))
                .then(|_| async { history_view(&proxy, messages.clone()).await["history"].take() })
                .filter(|history| futures_util::future::ready(history["series"].as_array().unwrap().is_empty())),
        )
        .next()
        .await
        .expect("service disconnected before rescan");
    });
}
