//! Provides isolated Shell test fixtures for fixture.

use super::*;

pub(crate) struct IsolatedService {
    _echo: ServiceEcho,
    pub(crate) scratch: tempfile::TempDir,
    pub(crate) _service: Process,
    pub(crate) _bus: Process,
    pub(crate) address: String,
}

pub(crate) fn isolated_service(settings: &serde_json::Value) -> IsolatedService {
    isolated_service_with(settings, |_| {})
}

pub(crate) fn isolated_service_with(settings: &serde_json::Value, prepare: impl FnOnce(&Path)) -> IsolatedService {
    isolated_service_configured(settings, prepare, |_| {})
}

pub(crate) fn isolated_service_configured(
    settings: &serde_json::Value,
    prepare: impl FnOnce(&Path),
    configure: impl FnOnce(&mut Command),
) -> IsolatedService {
    isolated_service_configured_with_bus(settings, prepare, |_| {}, configure)
}

pub(crate) fn isolated_service_configured_with_bus(
    settings: &serde_json::Value,
    prepare: impl FnOnce(&Path),
    setup_bus: impl FnOnce(&str),
    configure: impl FnOnce(&mut Command),
) -> IsolatedService {
    let scratch = tempfile::tempdir().unwrap();
    let config = scratch.path().join("config/quotaline");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(scratch.path().join("home")).unwrap();
    fs::create_dir_all(scratch.path().join("cache")).unwrap();
    fs::create_dir_all(scratch.path().join("data")).unwrap();
    fs::write(config.join("settings.json"), settings.to_string()).unwrap();
    prepare(scratch.path());
    let (bus, address) = private_bus(scratch.path(), "session-bus");
    setup_bus(address.trim());
    let mut command = Command::new(env!("CARGO_BIN_EXE_quotaline-gnome"));
    environment(&mut command, scratch.path(), address.trim());
    configure(&mut command);
    let service_log = scratch.path().join("service.log");
    let service = Process(
        command
            .stdout(Stdio::null())
            .stderr(Stdio::from(fs::File::create(&service_log).unwrap()))
            .spawn()
            .unwrap(),
    );
    IsolatedService {
        _echo: echo_service_log(&service_log),
        scratch,
        _service: service,
        _bus: bus,
        address: address.trim().into(),
    }
}

/// Copies the service log to the test's output as it grows, so a test stopped for hanging shows what the service
/// was doing; the file stays for tests that read it.
struct ServiceEcho(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl Drop for ServiceEcho {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn echo_service_log(path: &Path) -> ServiceEcho {
    let stopped = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut log = fs::File::open(path).unwrap();
    std::thread::spawn({
        let stopped = stopped.clone();
        move || {
            let mut chunk = Vec::new();
            while !stopped.load(std::sync::atomic::Ordering::Relaxed) {
                chunk.clear();
                std::io::Read::read_to_end(&mut log, &mut chunk).unwrap();
                eprint!("{}", String::from_utf8_lossy(&chunk));
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    ServiceEcho(stopped)
}

pub(crate) fn database_events(
    path: &Path,
) -> (
    notify::RecommendedWatcher,
    std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
) {
    let (sender, events) = std::sync::mpsc::channel();
    let mut watcher = notify::recommended_watcher(sender).unwrap();
    notify::Watcher::watch(
        &mut watcher,
        path.parent().unwrap(),
        notify::RecursiveMode::NonRecursive,
    )
    .unwrap();
    (watcher, events)
}

#[cfg(feature = "fixture-api")]
pub(crate) fn fast_cursor_service(server: &MockServer, minutes: u32) -> IsolatedService {
    fast_cursor_service_with(server, minutes, |_| {})
}

#[cfg(feature = "fixture-api")]
pub(crate) fn fast_cursor_service_with(
    server: &MockServer,
    minutes: u32,
    configure: impl FnOnce(&mut Command),
) -> IsolatedService {
    isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":true,"copilot":false},
            "interval_minutes":{"cursor":minutes}
        }),
        |root| {
            let path = root.join("home/.cursor/auth.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, r#"{"accessToken":"cursor-fixture"}"#).unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
            configure(command);
        },
    )
}

#[cfg(feature = "fixture-api")]
pub(crate) fn missing_cursor_service(server: &MockServer) -> IsolatedService {
    isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":true,"copilot":false},
            "interval_minutes":{"cursor":1},
            "detailed_logging":true
        }),
        |_| {},
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
            command.env("TMB_FIXTURE_FAST_SCHEDULE", "1");
        },
    )
}

#[cfg(feature = "fixture-api")]
pub(crate) async fn cursor_log_count(proxy: &zbus::Proxy<'_>) -> usize {
    let raw: String = proxy.call("GetLogs", &(false,)).await.unwrap();
    serde_json::from_str::<Vec<serde_json::Value>>(&raw)
        .unwrap()
        .iter()
        .filter(|entry| {
            entry["message"]
                .as_str()
                .is_some_and(|message| message.contains("cursor refresh completed in"))
        })
        .count()
}
