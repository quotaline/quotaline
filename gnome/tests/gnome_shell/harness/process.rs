//! Provides isolated Shell test fixtures for process.

use super::*;

// Nextest runs each test in a separate process, so a mutex cannot protect the shared Shell session.
pub(crate) fn shell_lock() -> fs::File {
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(std::env::temp_dir().join("quotaline-gnome-shell.lock"))
        .unwrap();
    lock.lock().unwrap();
    lock
}

pub(crate) struct Process(pub(crate) Child);

pub(crate) fn wait_for_exit(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let descriptor = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id(), 0) as i32 };
    if descriptor < 0 {
        return child.try_wait().ok().flatten();
    }
    let descriptor = unsafe { OwnedFd::from_raw_fd(descriptor) };
    let mut event = libc::pollfd {
        fd: descriptor.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    if unsafe { libc::poll(&mut event, 1, timeout.as_millis() as i32) } > 0 {
        child.wait().ok()
    } else {
        None
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_some() {
            return;
        }
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGTERM);
        }
        if wait_for_exit(&mut self.0, Duration::from_secs(3)).is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

// gnome-shell-test-tool replaces the XDG home directories but keeps the search paths, so a handler placed first in
// them stops a test Shell from opening links in the desktop's real browser.
fn url_handler(root: &Path) -> (String, String) {
    let data = root.join("xdg-data");
    let config = root.join("xdg-config");
    fs::create_dir_all(data.join("applications")).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::write(
        data.join("applications/tmb-test-open.desktop"),
        "[Desktop Entry]\nType=Application\nName=Test link handler\nExec=true %u\nNoDisplay=true\n\
         MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;x-scheme-handler/file;\n",
    )
    .unwrap();
    fs::write(
        config.join("mimeapps.list"),
        "[Default Applications]\ntext/html=tmb-test-open.desktop\nx-scheme-handler/http=tmb-test-open.desktop\n\
         x-scheme-handler/https=tmb-test-open.desktop\nx-scheme-handler/file=tmb-test-open.desktop\n",
    )
    .unwrap();
    (
        format!("{}:/usr/local/share:/usr/share", data.display()),
        format!("{}:/etc/xdg", config.display()),
    )
}

pub(crate) fn environment<'a>(command: &'a mut Command, root: &Path, address: &str) -> &'a mut Command {
    let runtime = root.join("runtime");
    fs::create_dir_all(&runtime).unwrap();
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700)).unwrap();
    let (data_dirs, config_dirs) = url_handler(root);
    let address = if address.is_empty() {
        "unix:path=/nonexistent/quotaline-test-bus"
    } else {
        address
    };
    command
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_RUNTIME_DIR", runtime)
        .env("XDG_DATA_DIRS", data_dirs)
        .env("XDG_CONFIG_DIRS", config_dirs)
        .env("DBUS_SESSION_BUS_ADDRESS", address)
        .env("DBUS_SYSTEM_BUS_ADDRESS", address)
        .env("GSETTINGS_BACKEND", "memory")
        .env("NO_AT_BRIDGE", "1")
        .env("GIO_USE_VFS", "local")
        .env("GIO_USE_VOLUME_MONITOR", "unix")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("GNOME_SETUP_DISPLAY")
        .env_remove("SESSION_MANAGER")
        .env_remove("AT_SPI_BUS_ADDRESS")
        .env_remove("XAUTHORITY")
        .env_remove("DBUS_STARTER_ADDRESS")
        .env_remove("DBUS_STARTER_BUS_TYPE")
        .env_remove("XDG_SESSION_ID")
        .env_remove("PULSE_SERVER")
        .env_remove("PIPEWIRE_REMOTE")
        .env_remove("PIPEWIRE_RUNTIME_DIR")
        .env_remove("GNOME_KEYRING_CONTROL")
        .env_remove("GNOME_KEYRING_PID")
        .env_remove("SSH_AUTH_SOCK")
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("GEMINI_CLI_HOME")
        .env_remove("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE")
        .env_remove("ANTIGRAVITY_AUTH_FILE")
        .env_remove("COPILOT_GITHUB_TOKEN")
        .env_remove("GH_TOKEN")
        .env_remove("GITHUB_TOKEN")
        .env_remove("QUOTALINE_DEMO")
        .env_remove("GH_HOST")
        .env_remove("COPILOT_HOME")
        .env("TMB_EXPECT_DEMO", "1")
}

pub(crate) fn private_bus(root: &Path, socket: &str) -> (Process, String) {
    for name in ["home", "config", "cache", "data"] {
        fs::create_dir_all(root.join(name)).unwrap();
    }
    let configuration = root.join("bus.conf");
    fs::write(
        &configuration,
        format!(
            "<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
            root.join(socket).display()
        ),
    )
    .unwrap();
    let mut command = Command::new("dbus-daemon");
    let mut bus = Process(
        environment(&mut command, root, "unix:path=/nonexistent/quotaline-test-bus")
            .args(["--nofork", "--print-address=1"])
            .arg(format!("--config-file={}", configuration.display()))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    (bus, address.trim().into())
}

#[cfg(feature = "fixture-api")]
pub(crate) fn fixture_api_rejection(url: &str, message: &str) {
    let scratch = tempfile::tempdir().unwrap();
    let output = environment(
        &mut Command::new(env!("CARGO_BIN_EXE_quotaline-gnome")),
        scratch.path(),
        "",
    )
    .env("TMB_FIXTURE_API_URL", url)
    .output()
    .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(message), "{stderr}");
}

/// `just extension-build` compiles the TypeScript sources here; coverage swaps in an instrumented copy.
pub(crate) fn extension_source() -> String {
    std::env::var("TMB_EXTENSION_DIR").unwrap_or_else(|_| "../target/extension".into())
}

/// Scripts run from the plain build, so modules they import directly never count toward Shell coverage.
pub(crate) fn shell_test_script(script: &str) -> std::path::PathBuf {
    Path::new("../target").join(script)
}

#[cfg(feature = "fixture-api")]
pub(crate) fn packed_extension(root: &Path, output_dir: &Path) -> std::path::PathBuf {
    let source = extension_source();
    let stage = output_dir.join("extension-stage");
    let copy = Command::new("cp")
        .current_dir(root)
        .arg("-LR")
        .arg(&source)
        .arg(&stage)
        .output()
        .expect("copy extension sources");
    let stderr = String::from_utf8_lossy(&copy.stderr);
    assert!(copy.status.success(), "extension staging failed: {stderr}");
    let pack = Command::new("gnome-extensions")
        .current_dir(root)
        .arg("pack")
        .arg(&stage)
        .arg("--out-dir")
        .arg(output_dir)
        .args([
            "--force",
            "--extra-source=src",
            "--extra-source=icons",
            "--extra-source=privacy.html",
            "--extra-source=notices.html",
            "--extra-source=provider-setup.html",
        ])
        .output()
        .expect("gnome-extensions pack");
    let stderr = String::from_utf8_lossy(&pack.stderr);
    assert!(pack.status.success(), "extension pack failed: {stderr}");
    output_dir.join("quotaline@bernat.tech.shell-extension.zip")
}

#[cfg(feature = "fixture-api")]
pub(crate) fn request_service_quit(fixture: &IsolatedService) {
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
        proxy.call_noreply("Quit", &()).await.unwrap();
    });
}
