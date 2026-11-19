//! Runs provider refreshes, scheduling, and settings for any front end that embeds the engine.

use crate::{claude_transcripts, logging, providers, store};
use futures_util::future::BoxFuture;
use jiff::Timestamp;
use quotaline_analytics as analytics;
use quotaline_analytics::project;
use quotaline_catalog::IDS;
use quotaline_catalog::REFRESH_POLICY;
use quotaline_demo::demo;
use quotaline_export as export;
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
    env, fmt,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, Notify, Semaphore};

mod commands;
mod coordination;
mod history_selection;
mod history_session;
mod history_visibility;
mod notification_queue;
mod refresh_ops;
mod scheduler;
mod settings;
mod state;

pub use history_selection::step as step_history_reading;
pub use notification_queue::Planned;

/// Receives the engine's change signals for the front end that embeds it.
pub trait Host: Send + Sync {
    fn changed(&self, state: String) -> BoxFuture<'_, ()>;
    fn log_changed(&self) -> BoxFuture<'_, ()>;
    fn notification_planned(&self) -> BoxFuture<'_, ()>;
    /// Whether the session shows a 12-hour clock, read when a rate limit names its retry time.
    fn twelve_hour_clock(&self) -> BoxFuture<'_, bool>;
}

#[derive(Debug)]
pub enum Error {
    InvalidArgs(String),
    Failed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgs(message) | Self::Failed(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

fn failed(error: impl fmt::Display) -> Error {
    Error::Failed(error.to_string())
}

fn invalid(error: impl fmt::Display) -> Error {
    Error::InvalidArgs(error.to_string())
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
    /// Whether another engine's front end shows its panel, which keeps polling at the visible pace.
    remote_visible: bool,
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
    notification_providers: notifications::NotificationRoutes,
    #[cfg(feature = "fixture-api")]
    notification_handoffs: u32,
}

#[derive(Clone)]
pub struct Engine {
    inner: Arc<Mutex<Inner>>,
    host: Arc<dyn Host>,
    first_run_candidate: bool,
    demo_requested: bool,
    persistence: Arc<Mutex<()>>,
    client: providers::Api,
    slots: Arc<Semaphore>,
    schedule_changed: Arc<Notify>,
    shutdown: Arc<Notify>,
    minute_seconds: u64,
    idle_seconds: u64,
    home: PathBuf,
    logs: Arc<logging::LogStore>,
    claude_reader: Arc<std::sync::Mutex<claude_transcripts::Scanner>>,
    coordination: Option<Arc<coordination::Coordination>>,
}

impl Engine {
    pub fn new(demo_requested: bool, host: Arc<dyn Host>) -> Result<Self, Box<dyn std::error::Error>> {
        let adopted = store::adopt_legacy_directories();
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
            Timestamp::now().as_second() as f64,
        );
        let mut history_visibility = analytics::HistoryVisibility::default();
        history_visibility.set_hidden("global", settings.history_hidden_keys.clone());
        let providers = if settings.demo { demo() } else { store::read_snapshot() };
        let last_refresh = providers.values().filter_map(|provider| provider.updated).max();
        let client = providers::Api::new()?;
        let recent_samples =
            blocking(|| store::recent_samples_all(Timestamp::now().as_second())).unwrap_or_else(|error| {
                eprintln!("Recent history read failed: {error}");
                BTreeMap::new()
            });
        let home = crate::platform::home();
        let claude_root = env::var_os("CLAUDE_CONFIG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude"))
            .join("projects");
        let claude_checkpoint = store::claude_checkpoint();
        let claude_reader = Arc::new(std::sync::Mutex::new(claude_transcripts::Scanner::new(
            claude_root,
            claude_checkpoint,
        )));
        // Written before any front end announces itself, so a reader that sees it running also sees these lines.
        let logs = Arc::new(logging::LogStore::new(store::log()));
        for (level, message) in [("info", "Service started".to_owned())].into_iter().chain(adopted) {
            if let Err(error) = logs.record(level, "app", &message) {
                eprintln!("Service log write failed: {error}");
            }
        }
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
                remote_visible: false,
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
                notification_providers: notifications::NotificationRoutes::default(),
                #[cfg(feature = "fixture-api")]
                notification_handoffs: 0,
            })),
            host,
            first_run_candidate: first_run_pending,
            demo_requested,
            persistence: Arc::new(Mutex::new(())),
            client,
            slots: Arc::new(Semaphore::new(2)),
            schedule_changed: Arc::new(Notify::new()),
            shutdown: Arc::new(Notify::new()),
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
            logs,
            coordination: if demo_requested {
                None
            } else {
                coordination::Coordination::open().map(Arc::new)
            },
        })
    }

    pub fn home(&self) -> &std::path::Path {
        &self.home
    }

    pub async fn is_demo(&self) -> bool {
        self.inner.lock().await.settings.demo
    }

    pub async fn log(&self, level: &str, category: &str, message: &str) {
        if let Err(error) = self.logs.record(level, category, message) {
            eprintln!("Service log write failed: {error}");
        }
        self.host.log_changed().await;
    }

    async fn emit(&self) {
        self.publish().await;
        let state = self.state().await;
        self.host.changed(state).await;
    }

    pub async fn take_notifications(&self) -> Vec<Planned> {
        let mut planned = self.inner.lock().await.pending_notifications.take();
        planned.extend(self.claim_notifications().await);
        planned
    }

    /// Whether notices show, and with sound; read per notice so switching them off stops a batch midway.
    pub async fn notification_preferences(&self) -> (bool, bool) {
        let inner = self.inner.lock().await;
        (inner.settings.notifications, inner.settings.notify_sound)
    }

    pub async fn route_notification(&self, notification_id: u32, provider: &str) {
        self.inner
            .lock()
            .await
            .notification_providers
            .insert(notification_id, provider);
    }

    /// The provider a clicked notice asks to sign in to, unless that provider was switched off since.
    pub async fn notification_sign_in(&self, notification_id: u32) -> Option<String> {
        let mut inner = self.inner.lock().await;
        inner
            .notification_providers
            .take(notification_id)
            .filter(|provider| inner.settings.providers.get(provider) != Some(&false))
    }

    /// Applies the session's power profile, which stretches polling while the machine saves power or runs hot.
    pub async fn set_power_state(&self, power_saver: Option<bool>, thermal_pressure: Option<bool>) {
        let mut inner = self.inner.lock().await;
        let next = (
            power_saver.unwrap_or(inner.power_saver),
            thermal_pressure.unwrap_or(inner.thermal_pressure),
        );
        if next == (inner.power_saver, inner.thermal_pressure) {
            return;
        }
        (inner.power_saver, inner.thermal_pressure) = next;
        self.client.set_local_scan_throttled(next.0 || next.1);
        self.schedule_changed.notify_one();
    }

    /// Loads current readings, then keeps refreshing on schedule until `quit` is called.
    pub async fn run(&self) {
        if self.is_demo().await {
            self.warm_demo();
        }
        let leading = self.contest().await;
        if leading || self.is_demo().await {
            // Everything is due in a new session, and a provider a front end refreshed while this engine started is not.
            self.request(refresh::RefreshIntent {
                reason: refresh::RefreshReason::Scheduled,
                usage: refresh::RefreshPolicy::IfDue,
                analytics: refresh::RefreshPolicy::Skip,
                providers: None,
            })
            .await;
        } else {
            self.follow().await;
        }
        if self.first_run_candidate {
            self.inner.lock().await.first_run_pending = true;
            self.emit().await;
        }
        if leading {
            self.scan_claude().await;
        }
        match self.coordination.clone() {
            Some(coordination) => {
                self.spawn_coordinator(coordination);
                self.shutdown.notified().await;
            }
            None => {
                let poller = self.spawn_poller();
                self.shutdown.notified().await;
                poller.abort();
            }
        }
        self.stop_coordinating().await;
    }

    pub fn quit(&self) {
        self.shutdown.notify_one();
    }

    // Seeding the demo history takes long enough on slow machines to delay the first chart a menu opens. A failed
    // seed is retried, and reported, by the first history request that needs it.
    fn warm_demo(&self) {
        tokio::task::spawn_blocking(|| store::warm_demo().ok());
    }

    #[cfg(feature = "fixture-api")]
    pub async fn queue_notification(&self, planned: Planned) {
        self.inner.lock().await.pending_notifications.extend([planned]);
        self.host.notification_planned().await;
    }

    #[cfg(feature = "fixture-api")]
    pub async fn pending_notification_count(&self) -> u32 {
        let local = self.inner.lock().await.pending_notifications.len() as u32;
        local + self.posted_notification_count().await
    }

    #[cfg(feature = "fixture-api")]
    pub async fn record_notification_handoff(&self) {
        self.inner.lock().await.notification_handoffs += 1;
    }

    #[cfg(feature = "fixture-api")]
    pub async fn notification_handoff_count(&self) -> u32 {
        self.inner.lock().await.notification_handoffs
    }
}

fn epoch() -> f64 {
    Timestamp::now().as_millisecond() as f64 / 1000.0
}

// SQLite history calls block; running them in place lets the two runtime workers hand off requests meanwhile.
fn blocking<T>(work: impl FnOnce() -> T) -> T {
    tokio::task::block_in_place(work)
}
