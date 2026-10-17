//! Provides isolated Shell test fixtures for shell.

use super::*;

pub(crate) fn run_shell_script(fixture: &IsolatedService, script: &str, variables: &[(&str, &str)]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let zip = packed_extension(root, fixture.scratch.path());
    let complete = fixture.scratch.path().join("shell-scenario-complete");
    fs::write(&complete, "").unwrap();
    let mut command = Command::new("gnome-shell-test-tool");
    for (key, value) in variables {
        command.env(key, value);
    }
    let mut output = environment(&mut command, fixture.scratch.path(), fixture.address.as_str())
        .current_dir(root)
        .env("DBUS_SYSTEM_BUS_ADDRESS", fixture.address.as_str())
        .env("TMB_SCREENSHOT_DIR", root.join("../target/shell-screenshots"))
        .env("TMB_SHELL_SCENARIO_DONE", &complete)
        .env("GDK_DEBUG", "no-portals")
        .env("GTK_IM_MODULE", "gtk-im-context-simple")
        .args([
            "--headless",
            "--wrap",
            env!("CARGO_BIN_EXE_shell_no_x11"),
            "--extension",
        ])
        .arg(&zip)
        .arg(shell_test_script(script))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("headless GNOME Shell");
    let stdout = follow(output.stdout.take().unwrap());
    let stderr = follow(output.stderr.take().unwrap());
    let status = output.wait().unwrap();
    let logs = format!("{}\n{}", stdout.join().unwrap(), stderr.join().unwrap());
    let completed = fs::read(&complete).unwrap() == b"done";
    assert!(
        shell_script_succeeded(status.success(), script, completed, &logs),
        "{script} failed: exit={status:?}, complete={completed}\n{logs}",
    );
    assert!(!extension_error(&logs), "{script} failed:\n{logs}");
}

/// Collects a Shell output stream, echoing progress and errors as they happen so a run stopped for hanging shows
/// what the script was waiting for.
fn follow(stream: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut logs = String::new();
        for line in std::io::BufRead::lines(std::io::BufReader::new(stream)).map_while(Result::ok) {
            if ["Waiting until:", "JS ERROR", "Shell scenario failed"]
                .iter()
                .any(|marker| line.contains(marker))
            {
                eprintln!("{line}");
            }
            logs.push_str(&line);
            logs.push('\n');
        }
        logs
    })
}

// GNOME Shell's own panels can throw while the headless Shell tears down; only errors whose stack passes through
// the extension or a test script come from this project.
pub(crate) fn extension_error(logs: &str) -> bool {
    logs.split("JS ERROR").skip(1).any(|error| {
        let stack = error.split("\n\n").next().unwrap_or_default();
        stack.contains("quotaline@bernat.tech") || stack.contains("/extension/tests/")
    })
}

pub(crate) fn shell_script_succeeded(exited_successfully: bool, script: &str, completed: bool, logs: &str) -> bool {
    exited_successfully
        || (script == "extension/tests/integration/shell-smoke.js"
            && completed
            && logs.contains("Failed to exit helper:")
            && logs.contains("GNOME Shell exited with exit status: 1"))
}

#[path = "../tests/shell_result.rs"]
mod shell_result_tests;

#[cfg(feature = "fixture-api")]
pub(crate) fn cursor_live_service(server: &MockServer) -> IsolatedService {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":true,"copilot":false});
    isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let credentials = root.join("home/.cursor/auth.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(credentials, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    )
}

#[cfg(feature = "fixture-api")]
pub(crate) fn cursor_shell_fixture(summary: serde_json::Value, script: &str, variables: &[(&str, &str)]) {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/usage-summary"))
            .respond_with(ResponseTemplate::new(200).set_body_json(summary))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = cursor_live_service(&server);
    runtime.block_on(async {
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
        wait_for_state(&proxy, |state| state["providers"]["cursor"]["status"] == "connected").await;
    });
    run_shell_script(&fixture, script, variables);
}

#[cfg(feature = "fixture-api")]
pub(crate) fn cached_claude_snapshot(root: &Path) {
    let snapshot = root.join("cache/quotaline/snapshot.json");
    fs::create_dir_all(snapshot.parent().unwrap()).unwrap();
    fs::write(
        snapshot,
        serde_json::json!({"claude":{
            "id":"claude","name":"Claude","status":"connected","plan":"Pro","identity":"",
            "source":"Claude Code","windows":[{"id":"session","label":"Current session","used":42.0,
                "reset":null,"group":"session"}],"notice":"","updated":chrono::Utc::now().timestamp()
        }})
        .to_string(),
    )
    .unwrap();
}

#[cfg(feature = "fixture-api")]
pub(crate) async fn assert_stale_claude_recovery(fixture: &IsolatedService, expected: &str) {
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
    let state = wait_for_state(&proxy, |state| state["providers"]["claude"]["status"] == "stale").await;
    assert_eq!(state["providers"]["claude"]["recovery_status"], expected);
    assert_eq!(state["providers"]["claude"]["windows"][0]["used"], 42.0);
}

#[cfg(feature = "fixture-api")]
pub(crate) async fn start_keyring(fixture: &IsolatedService) -> Process {
    let mut keyring = environment(
        &mut Command::new("gnome-keyring-daemon"),
        fixture.scratch.path(),
        &fixture.address,
    )
    .args(["--foreground", "--components=secrets", "--unlock"])
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .unwrap();
    keyring.stdin.take().unwrap().write_all(b"fixture-password").unwrap();
    BufReader::new(keyring.stdout.take().unwrap())
        .read_to_end(&mut Vec::new())
        .unwrap();
    let connection = zbus::connection::Builder::address(fixture.address.as_str())
        .unwrap()
        .build()
        .await
        .unwrap();
    let secrets =
        secret_service::SecretService::connect_with_existing(secret_service::EncryptionType::Dh, connection.clone())
            .await
            .unwrap();
    let collection = secrets.get_default_collection().await.unwrap();
    collection.ensure_unlocked().await.unwrap();
    Process(keyring)
}

#[cfg(feature = "fixture-api")]
pub(crate) fn store_keyring_item(fixture: &IsolatedService, label: &str, attributes: &[(&str, &str)], secret: &[u8]) {
    store_keyring_item_at(fixture.scratch.path(), &fixture.address, label, attributes, secret);
}

#[cfg(feature = "fixture-api")]
pub(crate) fn store_keyring_item_at(
    root: &Path,
    address: &str,
    label: &str,
    attributes: &[(&str, &str)],
    secret: &[u8],
) {
    let mut command = Command::new("secret-tool");
    environment(&mut command, root, address)
        .arg("store")
        .arg(format!("--label={label}"));
    for (name, value) in attributes {
        command.arg(name).arg(value);
    }
    let mut store = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    store.stdin.take().unwrap().write_all(secret).unwrap();
    let output = store.wait_with_output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[cfg(feature = "fixture-api")]
pub(crate) async fn copilot_keyring_fixture(fixture: &IsolatedService, entries: &[(&str, &str)]) -> Process {
    let keyring = start_keyring(fixture).await;
    for (account, secret) in entries {
        store_keyring_item(
            fixture,
            "Copilot fixture",
            &[("service", "copilot-cli"), ("account", account)],
            secret.as_bytes(),
        );
    }
    keyring
}

pub(crate) fn write_codex_rollout(root: &Path, used: f64) {
    write_codex_rollout_at(root, used, "2026-10-10T10:00:00Z");
}

pub(crate) fn write_codex_rollout_at(root: &Path, used: f64, timestamp: &str) {
    let sessions = root.join("home/.codex/sessions/2026/10/10");
    fs::create_dir_all(&sessions).unwrap();
    let line = serde_json::json!({"timestamp":timestamp,"type":"event_msg","payload":{"type":"token_count","rate_limits":{"plan_type":"pro","primary":{"used_percent":used,"window_minutes":300,"resets_in_seconds":600},"secondary":{"used_percent":7.0,"window_minutes":10080,"resets_in_seconds":3600},"credits":{"balance":"12.50"}}}});
    fs::write(
        sessions.join("rollout-test.jsonl"),
        format!("unrelated event\n{line}\n"),
    )
    .unwrap();
}

pub(crate) async fn wait_for_service(connection: &zbus::Connection) {
    let dbus = zbus::fdo::DBusProxy::new(connection).await.unwrap();
    let name = zbus::names::BusName::try_from("tech.bernat.Quotaline.Gnome").unwrap();
    let mut owners = dbus
        .receive_name_owner_changed_with_args(&[(0, name.as_str())])
        .await
        .unwrap();
    if dbus.name_has_owner(name.clone()).await.unwrap() {
        return;
    }
    while !dbus.name_has_owner(name.clone()).await.unwrap() {
        owners.next().await.unwrap();
    }
}

pub(crate) async fn wait_for_state(
    proxy: &zbus::Proxy<'_>,
    matches: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    wait_for_reply(proxy, "GetState", &(), matches).await
}

/// Applies a History navigation action in UTC and returns the view, history, and error envelope.
pub(crate) async fn history_view(proxy: &zbus::Proxy<'_>, action: serde_json::Value) -> serde_json::Value {
    let raw: String = proxy
        .call("GetHistoryView", &(action.to_string(), "UTC", 0_i32))
        .await
        .unwrap();
    serde_json::from_str(&raw).unwrap()
}

/// Calls `method` again after every `Changed` signal until its JSON reply matches.
pub(crate) async fn wait_for_reply<B>(
    proxy: &zbus::Proxy<'_>,
    method: &str,
    body: &B,
    matches: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let mut changes = proxy.receive_signal("Changed").await.unwrap();
    loop {
        let raw: String = proxy.call(method, body).await.unwrap();
        let state: serde_json::Value = serde_json::from_str(&raw).unwrap();
        if matches(&state) {
            return state;
        }
        // Shown only when the test fails, so a run stopped for hanging still shows the last reply.
        eprintln!("{method} reply did not match yet: {raw}");
        changes
            .next()
            .await
            .expect("quota service disconnected before state update");
    }
}

#[cfg(feature = "fixture-api")]
pub(crate) async fn provider_with_status(fixture: &IsolatedService, id: &str, status: &str) -> serde_json::Value {
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
    wait_for_state(&proxy, |state| state["providers"][id]["status"] == status).await
}

pub(crate) fn write_private_snapshot(root: &Path) {
    let cache = root.join("cache/quotaline");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        cache.join("snapshot.json"),
        serde_json::json!({"claude":{"id":"claude","name":"Claude","status":"connected","plan":"Pro",
            "identity":"private@example.invalid","organization":"Private Organization",
            "source":"/private/credential-file","windows":[],"notice":"","updated":1_780_000_000},
            "gemini":{"id":"gemini","name":"Gemini CLI","status":"connected","plan":"Workspace",
            "identity":"owner@example.invalid","organization":"owner@example.invalid's Organization",
            "source":"Secret Service","windows":[],"notice":"","updated":1_780_000_000}})
        .to_string(),
    )
    .unwrap();
}
