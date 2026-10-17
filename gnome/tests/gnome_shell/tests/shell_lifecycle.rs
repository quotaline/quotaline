//! Exercises shell lifecycle in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn demo_usage_and_history_reach_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({
        "demo":true,"write_usage_json":true,"history_days":7,"notifications":false
    }));
    run_shell_script(
        &fixture,
        "extension/tests/integration/shell-smoke.js",
        &[("TMB_DEMO_PHASE", "usage")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn demo_settings_reach_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true,"write_usage_json":true,"history_days":7}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/shell-smoke.js",
        &[("TMB_DEMO_PHASE", "settings")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn demo_formats_and_lifecycle_reach_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service_with(
        &serde_json::json!({"demo":true,"write_usage_json":true,"history_days":7}),
        |root| {
            fs::create_dir_all(root.join("home/.claude")).unwrap();
            fs::write(root.join("home/.claude/.credentials.json"), "{").unwrap();
        },
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/shell-smoke.js",
        &[("TMB_DEMO_PHASE", "formats")],
    );
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.scratch.path().join("config/quotaline/settings.json")).unwrap())
            .unwrap();
    assert_eq!(settings["demo"], false);
    assert_eq!(settings["show_all_providers"], false);
    let usage_path = fixture.scratch.path().join("data/quotaline/usage.json");
    let usage: serde_json::Value = serde_json::from_slice(&fs::read(&usage_path).unwrap()).unwrap();
    assert_eq!(usage["providers"].as_array().unwrap().len(), 6);
    assert!(usage["providers"][0]["windows"][0]["usedPercent"].is_number());
    assert_eq!(fs::metadata(usage_path).unwrap().permissions().mode() & 0o777, 0o600);
    let diagnostics = environment(&mut Command::new("gdbus"), fixture.scratch.path(), &fixture.address)
        .args([
            "call",
            "--session",
            "--dest",
            "tech.bernat.Quotaline.Gnome",
            "--object-path",
            "/tech/bernat/Quotaline/Gnome",
            "--method",
            "tech.bernat.Quotaline.Gnome.GetDiagnostics",
        ])
        .output()
        .unwrap();
    assert!(diagnostics.status.success());
    let report = String::from_utf8_lossy(&diagnostics.stdout);
    assert!(report.contains("window_count"));
    assert!(!report.contains("private@example.com"));
    let restored = environment(&mut Command::new("gdbus"), fixture.scratch.path(), &fixture.address)
        .args([
            "call",
            "--session",
            "--dest",
            "tech.bernat.Quotaline.Gnome",
            "--object-path",
            "/tech/bernat/Quotaline/Gnome",
            "--method",
            "tech.bernat.Quotaline.Gnome.SetSetting",
            "demo",
            "true",
        ])
        .output()
        .unwrap();
    assert!(restored.status.success());
}

#[cfg(feature = "fixture-api")]
#[test]
fn service_owner_loss_and_recovery_reach_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    let scratch = fixture.scratch.path().to_path_buf();
    let address = fixture.address.clone();
    let service_id = fixture._service.0.id();
    let ready = scratch.join("service-ready");
    let lost = scratch.join("service-lost");
    let (sender, events) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(sender).unwrap();
    notify::Watcher::watch(&mut watcher, &scratch, notify::RecursiveMode::NonRecursive).unwrap();
    let ready_for_shell = ready.clone();
    let lost_for_shell = lost.clone();
    let shell = std::thread::spawn(move || {
        run_shell_script(
            &fixture,
            "extension/tests/integration/service-restart.test.js",
            &[
                ("TMB_SERVICE_READY", ready_for_shell.to_str().unwrap()),
                ("TMB_SERVICE_LOST", lost_for_shell.to_str().unwrap()),
            ],
        );
    });
    let wait_for_marker = |marker: &Path| {
        while !marker.exists() {
            events
                .recv()
                .unwrap_or_else(|error| panic!("{} was not created: {error}", marker.display()))
                .unwrap();
        }
    };
    wait_for_marker(&ready);
    assert_eq!(unsafe { libc::kill(service_id as i32, libc::SIGTERM) }, 0);
    wait_for_marker(&lost);
    let _restarted = Process(
        environment(
            &mut Command::new(env!("CARGO_BIN_EXE_quotaline-gnome")),
            &scratch,
            &address,
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap(),
    );
    shell.join().unwrap();
}

#[cfg(feature = "fixture-api")]
#[test]
fn quit_method_stops_service() {
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    request_service_quit(&fixture);
    assert!(
        wait_for_exit(&mut fixture._service.0, Duration::from_secs(3))
            .expect("quota service did not exit after Quit")
            .success()
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn quit_footer_hides_panel_and_stops_service() {
    let _shell = shell_lock();
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/quit.test.js",
        &[("TMB_QUIT_ACTION", "button"), ("TMB_QUIT_LIFECYCLE", "1")],
    );
    assert!(
        wait_for_exit(&mut fixture._service.0, Duration::from_secs(3))
            .expect("quota service did not exit after footer Quit")
            .success()
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn quit_shortcut_hides_panel_and_stops_service() {
    let _shell = shell_lock();
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/quit.test.js",
        &[("TMB_QUIT_ACTION", "upper")],
    );
    assert!(
        wait_for_exit(&mut fixture._service.0, Duration::from_secs(3))
            .expect("quota service did not exit after Ctrl+Q")
            .success()
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn quit_works_in_small_menu() {
    let _shell = shell_lock();
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/quit.test.js",
        &[("TMB_QUIT_ACTION", "button"), ("TMB_VIRTUAL_MONITOR", "320x720")],
    );
    assert!(
        wait_for_exit(&mut fixture._service.0, Duration::from_secs(3))
            .expect("quota service did not exit after compact Quit")
            .success()
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn quit_without_service_hides_panel() {
    let _shell = shell_lock();
    let mut fixture = isolated_service(&serde_json::json!({"demo":true}));
    request_service_quit(&fixture);
    assert!(wait_for_exit(&mut fixture._service.0, Duration::from_secs(3)).is_some());
    run_shell_script(
        &fixture,
        "extension/tests/integration/quit.test.js",
        &[("TMB_QUIT_ACTION", "lower")],
    );
}
