//! Runs the GNOME D-Bus service and publishes panel state.

use chrono::Utc;
use futures_util::{StreamExt, TryStreamExt};
use quotaline_analytics as analytics;
use quotaline_analytics::project;
use quotaline_catalog::IDS;
use quotaline_catalog::REFRESH_POLICY;
use quotaline_demo::demo;
use quotaline_export as export;
use quotaline_gnome_adapter::{claude_transcripts, logging, providers, store};
use quotaline_http::ProviderFailureKind;
use quotaline_notifications as notifications;
use quotaline_quota::Provider;
use quotaline_refresh as refresh;
use quotaline_settings::Settings;
use quotaline_status_bar::OrderDraft;
use quotaline_status_bar::preview_short_label;
use quotaline_status_bar::short_label_conflicts;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    path::PathBuf,
    process::Command,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify, Semaphore};
use zbus::{connection::Builder, fdo, object_server::SignalEmitter};

const NAME: &str = "tech.bernat.Quotaline.Gnome";
const PATH: &str = "/tech/bernat/Quotaline/Gnome";

#[zbus::proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait PortalSettings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::Result<zbus::OwnedVariant>;
}

struct Inner {
    settings: Settings,
    history_session: analytics::HistorySession,
    history_visibility: analytics::HistoryVisibility,
    first_run_pending: bool,
    providers: BTreeMap<String, Provider>,
    /// The shared refresh status behind each provider's notice, retry time, and warnings.
    statuses: BTreeMap<String, refresh::ProviderStatus>,
    running: BTreeSet<String>,
    cycle_running: bool,
    session: refresh::RefreshSession,
    last_refresh: Option<i64>,
    panel_open: bool,
    power_saver: bool,
    thermal_pressure: bool,
    last_analytics: Option<Instant>,
    last_claude_scan: Option<Instant>,
    claude_scan_pending: bool,
    claude_scanning: bool,
    claude_scanned: bool,
    claude_local_usage: Option<claude_transcripts::LocalUsage>,
    recent_samples: store::RecentSamples,
    notified: notifications::NotificationLedger,
    pending_notifications: notification_queue::NotificationQueue<4096>,
    // The session's last known clock format; refresh results read it rather than wait on the portal.
    twelve_hour_clock: Option<bool>,
    notification_providers: notifications::NotificationRoutes,
    #[cfg(feature = "fixture-api")]
    notification_handoffs: u32,
    connection: Option<zbus::Connection>,
}

#[derive(Clone)]
struct App {
    inner: Arc<Mutex<Inner>>,
    first_run_candidate: bool,
    demo_requested: bool,
    persistence: Arc<Mutex<()>>,
    client: providers::Api,
    slots: Arc<Semaphore>,
    schedule_changed: Arc<Notify>,
    shutdown: Arc<Notify>,
    power_stop: Arc<Notify>,
    minute_seconds: u64,
    idle_seconds: u64,
    home: PathBuf,
    logs: Arc<logging::LogStore>,
    claude_reader: Arc<std::sync::Mutex<claude_transcripts::Scanner>>,
}

impl App {
    fn new(demo_requested: bool) -> Result<Self, Box<dyn std::error::Error>> {
        let mut settings = store::read_settings();
        let first_run_pending = !store::config().exists();
        if first_run_pending {
            store::atomic_json(&store::config(), &settings)?;
        }
        settings.request_demo(demo_requested);
        let history_session = quotaline_settings::history_session_from_settings(
            &settings,
            "UTC".into(),
            0,
            Utc::now().timestamp() as f64,
        );
        let mut history_visibility = analytics::HistoryVisibility::default();
        history_visibility.set_hidden("global", settings.history_hidden_keys.clone());
        let providers = if settings.demo { demo() } else { store::read_snapshot() };
        let last_refresh = providers.values().filter_map(|provider| provider.updated).max();
        let client = providers::Api::new()?;
        let recent_samples = blocking(|| store::recent_samples_all(Utc::now().timestamp())).unwrap_or_else(|error| {
            eprintln!("Recent history read failed: {error}");
            BTreeMap::new()
        });
        let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let claude_root = env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"))
            .join("projects");
        let claude_checkpoint = store::xdg("XDG_DATA_HOME", ".local/share").join("claude-transcripts.json");
        let claude_reader = Arc::new(std::sync::Mutex::new(claude_transcripts::Scanner::new(
            claude_root,
            claude_checkpoint,
        )));
        let minute_seconds = if cfg!(feature = "fixture-api") && env::var_os("TMB_FIXTURE_FAST_SCHEDULE").is_some() {
            1
        } else {
            60
        };
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner {
                settings,
                history_session,
                history_visibility,
                first_run_pending: false,
                providers,
                statuses: BTreeMap::new(),
                running: BTreeSet::new(),
                cycle_running: false,
                session: refresh::RefreshSession::with_minute(epoch(), minute_seconds as f64),
                last_refresh,
                panel_open: false,
                power_saver: false,
                thermal_pressure: false,
                last_analytics: None,
                last_claude_scan: None,
                claude_scan_pending: false,
                claude_scanning: false,
                claude_scanned: false,
                claude_local_usage: None,
                recent_samples,
                notified: notifications::NotificationLedger::default(),
                pending_notifications: notification_queue::NotificationQueue::default(),
                twelve_hour_clock: None,
                notification_providers: notifications::NotificationRoutes::default(),
                #[cfg(feature = "fixture-api")]
                notification_handoffs: 0,
                connection: None,
            })),
            first_run_candidate: first_run_pending,
            demo_requested,
            persistence: Arc::new(Mutex::new(())),
            client,
            slots: Arc::new(Semaphore::new(2)),
            schedule_changed: Arc::new(Notify::new()),
            shutdown: Arc::new(Notify::new()),
            power_stop: Arc::new(Notify::new()),
            minute_seconds,
            idle_seconds: {
                #[cfg(feature = "fixture-api")]
                {
                    env::var("TMB_FIXTURE_IDLE_SECONDS")
                        .ok()
                        .and_then(|value| value.parse().ok())
                        .unwrap_or(3_600)
                }
                #[cfg(not(feature = "fixture-api"))]
                {
                    3_600
                }
            },
            home,
            claude_reader,
            logs: Arc::new(logging::LogStore::new(
                store::xdg("XDG_DATA_HOME", ".local/share").join("log.txt"),
            )),
        })
    }

    async fn log(&self, level: &str, category: &str, message: &str) {
        if let Err(error) = self.logs.record(level, category, message) {
            eprintln!("Service log write failed: {error}");
        }
        self.emit_log_changed().await;
    }

    async fn emit_log_changed(&self) {
        let connection = self.inner.lock().await.connection.clone();
        if let Some(connection) = connection {
            let _: zbus::Result<()> = Bus::log_changed(&emitter(&connection)).await;
        }
    }

    async fn emit(&self) {
        let connection = self
            .inner
            .lock()
            .await
            .connection
            .clone()
            .expect("service is registered");
        let state = self.state().await;
        let _: zbus::Result<()> = Bus::changed(&emitter(&connection), &state).await;
    }

    async fn twelve_hour_clock(&self, connection: &zbus::Connection) -> bool {
        let format = async {
            PortalSettingsProxy::new(connection)
                .await?
                .read_one("org.gnome.desktop.interface", "clock-format")
                .await
        }
        .await;
        let twelve_hour = format.ok().and_then(|value| String::try_from(value).ok()).as_deref() == Some("12h");
        self.inner.lock().await.twelve_hour_clock = Some(twelve_hour);
        twelve_hour
    }

    async fn deliver_notifications(&self) {
        let (planned, connection) = {
            let mut inner = self.inner.lock().await;
            (inner.pending_notifications.take(), inner.connection.clone())
        };
        let Some(connection) = connection.filter(|_| !planned.is_empty()) else {
            return;
        };
        let clock =
            quotaline_gnome_adapter::clock::SessionClock::new(chrono::Local, self.twelve_hour_clock(&connection).await);
        let now = Utc::now().timestamp();
        for item in planned {
            // GNOME plans sign-in notices with the provider's own message, so no login hint is needed here.
            let copy = notifications::copy(
                &item.event.facts,
                &item.provider,
                "",
                notifications::Moments {
                    now,
                    fetched_at: item.fetched_at,
                },
                &clock,
            );
            self.notify(&copy.title, &copy.body, item.sign_in.as_deref()).await;
            #[cfg(feature = "fixture-api")]
            {
                self.inner.lock().await.notification_handoffs += 1;
            }
        }
    }

    async fn notify(&self, title: &str, body: &str, provider: Option<&str>) {
        let (enabled, sound, connection) = {
            let inner = self.inner.lock().await;
            (
                inner.settings.notifications,
                inner.settings.notify_sound,
                inner.connection.clone(),
            )
        };
        if !enabled {
            return;
        }
        let connection = connection.expect("service is registered");
        let mut hints = BTreeMap::<String, zbus::Variant<'_>>::new();
        if sound {
            hints.insert("sound-name".into(), zbus::Variant::from("message-new-instant"));
        } else {
            hints.insert("suppress-sound".into(), zbus::Variant::from(true));
        }
        let actions = if provider.is_some() {
            vec!["default", "Sign in"]
        } else {
            Vec::new()
        };
        let result: zbus::Result<u32> = async {
            let reply = connection
                .call_method(
                    Some("org.freedesktop.Notifications"),
                    "/org/freedesktop/Notifications",
                    Some("org.freedesktop.Notifications"),
                    "Notify",
                    &(
                        "Quotaline",
                        0_u32,
                        "dialog-information-symbolic",
                        title,
                        body,
                        actions,
                        hints,
                        7000_i32,
                    ),
                )
                .await?;
            reply.body().deserialize()
        }
        .await;
        if let (Ok(notification_id), Some(provider)) = (result, provider) {
            let mut inner = self.inner.lock().await;
            inner.notification_providers.insert(notification_id, provider);
        }
    }

    async fn watch_notification_actions(&self, connection: &zbus::Connection) -> zbus::Result<()> {
        #[cfg(feature = "fixture-api")]
        let interface =
            env::var("TMB_FIXTURE_NOTIFICATION_INTERFACE").unwrap_or_else(|_| "org.freedesktop.Notifications".into());
        #[cfg(not(feature = "fixture-api"))]
        let interface = String::from("org.freedesktop.Notifications");
        let proxy = zbus::Proxy::new(
            connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            interface.as_str(),
        )
        .await?;
        let mut actions = proxy.receive_signal("ActionInvoked").await?;
        while let Some(message) = tokio::select! {
            message = actions.next() => message,
            _ = connection.closed() => None,
        } {
            let Ok((notification_id, action)) = message.body().deserialize::<(u32, String)>() else {
                continue;
            };
            if action != "default" {
                continue;
            }
            let provider = {
                let mut inner = self.inner.lock().await;
                inner
                    .notification_providers
                    .take(notification_id)
                    .filter(|provider| inner.settings.providers.get(provider) != Some(&false))
            };
            if let Some(provider) = provider {
                let _ = Bus::sign_in_requested(&emitter(connection), &provider).await;
            }
        }
        Ok(())
    }

    // Seeding the demo history takes long enough on slow machines to delay the first chart a menu opens. A failed
    // seed is retried, and reported, by the first history request that needs it.
    fn warm_demo(&self) {
        tokio::task::spawn_blocking(|| store::warm_demo().ok());
    }

    async fn start_sign_in(&self, provider: &str) -> bool {
        if self.inner.lock().await.settings.demo {
            return false;
        }
        let command: &[&str] = match provider {
            "claude" => &["claude", "auth", "login"],
            "codex" => &["codex", "login"],
            "gemini" => &["gemini"],
            "cursor" => &["cursor-agent", "login"],
            "copilot" => &["copilot", "login"],
            _ => return false,
        };
        let mut search = env::var_os("PATH").unwrap_or_default();
        search.push(":");
        search.push(self.home.join(".local/bin"));
        let Some(executable) = env::split_paths(&search)
            .map(|directory| directory.join(command[0]))
            .find(|path| path.is_file())
        else {
            return false;
        };
        for terminal in ["ptyxis", "kgx", "gnome-terminal"] {
            let Some(program) = env::split_paths(&search)
                .map(|directory| directory.join(terminal))
                .find(|path| path.is_file())
            else {
                continue;
            };
            let mut process = Command::new(program);
            if terminal == "ptyxis" {
                process.arg("--new-window").arg("--");
            } else {
                process.arg("--");
            }
            match process
                .arg(&executable)
                .args(&command[1..])
                .env("PATH", &search)
                .spawn()
            {
                Ok(_) => return true,
                Err(error) => {
                    self.log("warning", "app", &format!("{provider} sign-in launch failed: {error}"))
                        .await
                }
            }
        }
        false
    }
}

fn emitter(connection: &zbus::Connection) -> SignalEmitter<'static> {
    SignalEmitter::new(connection, PATH).expect("service path is valid")
}

fn epoch() -> f64 {
    Utc::now().timestamp_millis() as f64 / 1000.0
}

// SQLite history calls block; running them in place lets the two runtime workers hand off D-Bus requests meanwhile.
fn blocking<T>(work: impl FnOnce() -> T) -> T {
    tokio::task::block_in_place(work)
}

mod bus;
mod notification_queue;
mod refresh_ops;
mod scheduler;
mod state;

use bus::Bus;
use notification_queue::Planned;

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let demo_requested = match args.as_slice() {
        [] => env::var_os("QUOTALINE_DEMO").is_some(),
        [flag] if flag == "--demo" => true,
        [flag] if flag == "--usage-json" => {
            let snapshot = store::read_snapshot();
            let export = export::UsageExport::from(&snapshot);
            if export.is_empty() {
                eprintln!("usage export failed: no cached usage yet");
                std::process::exit(1);
            }
            println!("{}", serde_json::to_string_pretty(&export)?);
            return Ok(());
        }
        [flag] if flag == "--help" => {
            println!("Usage: quotaline-gnome [--demo | --usage-json | --help]");
            return Ok(());
        }
        _ => {
            eprintln!("Unsupported argument. Run quotaline-gnome --help for usage.");
            std::process::exit(2);
        }
    };
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let adopted = store::adopt_legacy_directories();
    let app = App::new(demo_requested)?;
    app.log("info", "app", "Service started").await;
    for (level, message) in adopted {
        app.log(level, "app", &message).await;
    }
    if app.inner.lock().await.settings.demo {
        app.warm_demo();
    }
    let connection = Builder::session().build().await?;
    app.inner.lock().await.connection = Some(connection.clone());
    connection.object_server().at(PATH, Bus(app.clone())).await?;
    connection.request_name(NAME).await?;
    let notification_app = app.clone();
    let notification_connection = connection.clone();
    let notification_listener = tokio::spawn(async move {
        if let Err(error) = notification_app
            .watch_notification_actions(&notification_connection)
            .await
        {
            notification_app
                .log("warning", "app", &format!("Notification actions unavailable: {error}"))
                .await;
        }
    });
    let power_app = app.clone();
    let power_listener = tokio::spawn(async move {
        let _ = power_app.watch_power_profiles().await;
    });
    app.refresh("all").await;
    if app.first_run_candidate {
        app.inner.lock().await.first_run_pending = true;
        app.emit().await;
    }
    app.scan_claude().await;
    let timer_app = app.clone();
    let timer = tokio::spawn(async move {
        loop {
            timer_app.scheduled().await;
            if let Some(deadline) = timer_app.next_scheduled().await {
                tokio::select! {
                    _ = tokio::time::sleep_until(deadline.into()) => {},
                    _ = timer_app.schedule_changed.notified() => {},
                }
            } else {
                timer_app.schedule_changed.notified().await;
            }
        }
    });
    tokio::select! {
        result = tokio::signal::ctrl_c() => result?,
        _ = terminate.recv() => {},
        _ = app.shutdown.notified() => {},
        _ = connection.closed() => {},
    }
    timer.abort();
    app.power_stop.notify_one();
    if !connection.is_closed() {
        connection.close().await?;
    }
    let _ = power_listener.await;
    let _ = notification_listener.await;
    Ok(())
}
