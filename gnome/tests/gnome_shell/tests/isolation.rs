//! Exercises isolation in an isolated GNOME session.

use super::super::*;

#[test]
fn process_exit_wait_handles_an_already_reaped_child() {
    let mut child = Command::new("true").spawn().unwrap();
    let status = child.wait().unwrap();
    assert_eq!(wait_for_exit(&mut child, Duration::from_secs(1)), Some(status));
}

#[test]
fn process_cleanup_kills_and_reaps_a_stopped_child() {
    let child = Command::new("cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let process = Process(child);
    let pid = process.0.id() as i32;
    assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
    let mut status = 0;
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) }, pid);
    assert!(libc::WIFSTOPPED(status));
    drop(process);
    assert_eq!(unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) }, -1);
    assert_eq!(std::io::Error::last_os_error().raw_os_error(), Some(libc::ECHILD));
}

#[test]
fn shell_test_process_cannot_inherit_desktop_service_addresses() {
    let scratch = tempfile::tempdir().unwrap();
    let mut command = Command::new("env");
    for (key, value) in [
        ("DISPLAY", ":0"),
        ("WAYLAND_DISPLAY", "wayland-0"),
        ("SESSION_MANAGER", "desktop-session"),
        ("AT_SPI_BUS_ADDRESS", "unix:path=/run/user/1000/at-spi/bus"),
        ("PULSE_SERVER", "unix:/run/user/1000/pulse/native"),
        ("GNOME_KEYRING_CONTROL", "/run/user/1000/keyring"),
        ("SSH_AUTH_SOCK", "/run/user/1000/keyring/ssh"),
    ] {
        command.env(key, value);
    }
    let output = environment(&mut command, scratch.path(), "unix:path=/private/test-bus")
        .output()
        .unwrap();
    assert!(output.status.success());
    let variables = String::from_utf8(output.stdout).unwrap();
    assert!(variables.contains("DBUS_SESSION_BUS_ADDRESS=unix:path=/private/test-bus\n"));
    assert!(variables.contains("DBUS_SYSTEM_BUS_ADDRESS=unix:path=/private/test-bus\n"));
    assert!(variables.contains(&format!(
        "XDG_RUNTIME_DIR={}\n",
        scratch.path().join("runtime").display()
    )));
    assert!(variables.contains("GSETTINGS_BACKEND=memory\n"));
    for key in [
        "DISPLAY",
        "WAYLAND_DISPLAY",
        "SESSION_MANAGER",
        "AT_SPI_BUS_ADDRESS",
        "PULSE_SERVER",
        "GNOME_KEYRING_CONTROL",
        "SSH_AUTH_SOCK",
    ] {
        assert!(!variables.lines().any(|line| line.starts_with(&format!("{key}="))));
    }
}

#[test]
fn shell_test_bus_cannot_activate_desktop_services() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let scratch = tempfile::tempdir().unwrap();
        let (_bus, address) = private_bus(scratch.path(), "session-bus");
        let connection = zbus::connection::Builder::address(address.as_str())
            .build()
            .await
            .unwrap();
        let proxy = zbus::fdo::DBusProxy::new(&connection).await.unwrap();
        let names = proxy.list_activatable_names().await.unwrap();
        assert!(!names.iter().any(|name| name.as_str() == "org.freedesktop.secrets"));
        assert!(
            !names
                .iter()
                .any(|name| name.as_str() == "org.freedesktop.portal.Desktop")
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn fixture_api_rejects_remote_hosts() {
    fixture_api_rejection("http://example.com:8080", "Fixture API must use loopback HTTP");
}

#[cfg(feature = "fixture-api")]
#[test]
fn fixture_api_rejects_tls_on_loopback() {
    fixture_api_rejection("https://127.0.0.1:8080", "Fixture API must use loopback HTTP");
}

#[cfg(feature = "fixture-api")]
#[test]
fn fixture_api_rejects_malformed_urls() {
    fixture_api_rejection("not-a-url", "RelativeUrlWithoutBase");
}

#[test]
fn shell_wrapper_requires_a_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell_no_x11")).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("GNOME Shell command"));
}

#[test]
fn shell_wrapper_reports_an_unavailable_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell_no_x11"))
        .arg("/no/such/gnome-shell")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot start GNOME Shell"));
}

#[test]
fn shell_wrapper_reports_a_failing_command() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell_no_x11"))
        .arg("/usr/bin/false")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("GNOME Shell exited with"));
}

#[test]
fn shell_wrapper_resizes_the_virtual_monitor() {
    let output = Command::new(env!("CARGO_BIN_EXE_shell_no_x11"))
        .env("TMB_VIRTUAL_MONITOR", "760x720")
        .args(["/usr/bin/echo", "--virtual-monitor", "1280x720"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "--virtual-monitor 760x720 --no-x11"
    );
}
