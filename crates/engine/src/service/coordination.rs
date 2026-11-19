//! Lets every engine on the machine share one poller through the live store.
//!
//! The engine holding the lease runs the scheduler and publishes each state change; the others show what it published,
//! hand their refresh requests to it, and take turns delivering the notices it plans.

use super::*;
use crate::live::Live;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    sync::atomic::{AtomicBool, Ordering},
    time::SystemTime,
};
use tokio::task::JoinHandle;

const TICK: Duration = Duration::from_secs(1);
const RENEW_TICKS: u32 = 10;
const LEASE_SECONDS: f64 = 30.0;
const NOTICE_MAX_AGE_SECONDS: f64 = 3_600.0;

pub(super) struct Coordination {
    live: Live,
    leading: AtomicBool,
    /// Set when this engine takes the lease, so it serves the requests queued while nobody polled.
    promoted: AtomicBool,
    started: AtomicBool,
    /// Set once the engine stops, so nothing it still runs takes the lease back.
    // Held across each lease write, so a renewal already past the check cannot land after the release.
    released: std::sync::Mutex<bool>,
    poller: std::sync::Mutex<Option<JoinHandle<()>>>,
    coordinator: std::sync::Mutex<Option<JoinHandle<()>>>,
    settings_seen: std::sync::Mutex<Option<SystemTime>>,
}

impl Coordination {
    pub(super) fn open() -> Option<Self> {
        let path = store::live();
        let holder = format!("{}-{}", std::process::id(), Timestamp::now().as_nanosecond());
        let opened = path
            .parent()
            .map_or(Ok(()), fs::create_dir_all)
            .map_err(|error| error.to_string())
            .and_then(|()| Live::open(&path, holder).map_err(|error| error.to_string()));
        match opened {
            Ok(live) => Some(Self {
                live,
                leading: AtomicBool::new(false),
                promoted: AtomicBool::new(false),
                started: AtomicBool::new(false),
                released: std::sync::Mutex::new(false),
                poller: std::sync::Mutex::new(None),
                coordinator: std::sync::Mutex::new(None),
                settings_seen: std::sync::Mutex::new(None),
            }),
            Err(error) => {
                eprintln!("Live store open failed, polling alone: {error}");
                None
            }
        }
    }

    /// Runs one store call; a failure is logged and leaves the caller on its fallback.
    fn store<T>(&self, action: &str, work: impl FnOnce(&Live) -> crate::live::Result<T>) -> Option<T> {
        blocking(|| work(&self.live))
            .inspect_err(|error| eprintln!("Live store {action} failed: {error}"))
            .ok()
    }

    fn replace_poller(&self, poller: Option<JoinHandle<()>>) {
        let previous = std::mem::replace(
            &mut *self.poller.lock().unwrap_or_else(std::sync::PoisonError::into_inner),
            poller,
        );
        if let Some(previous) = previous {
            previous.abort();
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Published {
    providers: BTreeMap<String, Provider>,
    statuses: BTreeMap<String, refresh::ProviderStatus>,
    session: refresh::Handover,
    running: BTreeSet<String>,
    last_refresh: Option<i64>,
    claude_local_usage: Option<claude_transcripts::LocalUsage>,
    recent_samples: store::RecentSamples,
}

/// What a front end asks of the polling engine when another engine polls.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Request {
    Refresh {
        provider: String,
    },
    ClearHistory,
    /// Drops readings and schedule state after a settings reset, which the next refresh rebuilds.
    ForgetReadings,
}

impl Engine {
    /// The live store while this engine shares polling; demo readings stay local to the engine showing them.
    pub(super) async fn shared(&self) -> Option<&Coordination> {
        let coordination = self.coordination.as_deref()?;
        (!self.is_demo().await).then_some(coordination)
    }

    /// Takes or renews the lease, starting or stopping the scheduler when leadership changes.
    pub(super) async fn contest(&self) -> bool {
        let Some(coordination) = self.coordination.as_deref() else {
            return true;
        };
        let demo = self.is_demo().await;
        let leading = {
            let released = coordination
                .released
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if *released {
                false
            } else if demo {
                coordination.store("lease release", Live::resign);
                false
            } else {
                // A store that cannot be written must not leave every engine waiting on a poller that never comes.
                coordination
                    .store("lease renewal", |live| live.lead(epoch(), LEASE_SECONDS))
                    .unwrap_or(true)
            }
        };
        let was = coordination.leading.swap(leading, Ordering::Relaxed);
        if leading && !was {
            coordination.promoted.store(true, Ordering::Relaxed);
            let mut inner = self.inner.lock().await;
            // The previous poller's fetches in flight never finish here, so they must not hold the cycle open.
            if !inner.cycle_running {
                inner.running.clear();
            }
        }
        if coordination.started.load(Ordering::Relaxed) && leading != was {
            coordination.replace_poller(leading.then(|| self.spawn_poller()));
        }
        leading
    }

    pub(super) fn spawn_poller(&self) -> JoinHandle<()> {
        let timer = self.clone();
        tokio::spawn(async move {
            // Checking the lease before each round stops a poller that lost it, as after a suspend, before it fetches.
            while timer.contest().await {
                timer.scheduled().await;
                if let Some(deadline) = timer.next_scheduled().await {
                    tokio::select! {
                        _ = tokio::time::sleep_until(deadline.into()) => {},
                        _ = timer.schedule_changed.notified() => {},
                    }
                } else {
                    timer.schedule_changed.notified().await;
                }
            }
        })
    }

    /// Runs the shared side of the engine: the lease, settings written elsewhere, published state, and requests.
    pub(super) fn spawn_coordinator(&self, coordination: Arc<Coordination>) {
        coordination.started.store(true, Ordering::Relaxed);
        // Read after marking the coordinator started, since a refresh during startup may have changed leadership.
        if coordination.leading.load(Ordering::Relaxed) {
            coordination.replace_poller(Some(self.spawn_poller()));
        }
        let app = self.clone();
        let owner = coordination.clone();
        let coordinator = tokio::spawn(async move {
            let mut ticker = tokio::time::interval(TICK);
            ticker.tick().await;
            let mut tick = 0_u32;
            loop {
                ticker.tick().await;
                tick = tick.wrapping_add(1);
                app.coordinate(&coordination, tick.is_multiple_of(RENEW_TICKS)).await;
            }
        });
        *owner
            .coordinator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(coordinator);
    }

    async fn coordinate(&self, coordination: &Coordination, renew: bool) {
        self.reload_settings(coordination).await;
        if self.is_demo().await {
            if renew {
                self.contest().await;
            }
            return;
        }
        if renew {
            self.announce_presence(coordination).await;
            if self.contest().await {
                self.track_visibility(coordination).await;
            }
        }
        let changed = coordination.store("change check", Live::changed).unwrap_or(false);
        let leading = coordination.leading.load(Ordering::Relaxed);
        if leading && (changed || coordination.promoted.swap(false, Ordering::Relaxed)) {
            self.serve_requests(coordination).await;
        }
        if !changed {
            return;
        }
        if leading {
            self.track_visibility(coordination).await;
        } else {
            self.follow().await;
        }
        self.host.notification_planned().await;
    }

    /// Gives up the lease, so another engine polls at once instead of when it expires.
    pub async fn release(&self) {
        self.stop_coordinating().await;
    }

    pub(super) async fn stop_coordinating(&self) {
        let Some(coordination) = self.coordination.as_deref() else {
            return;
        };
        let mut released = coordination
            .released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *released = true;
        coordination.store("lease release", Live::resign);
        drop(released);
        coordination.leading.store(false, Ordering::Relaxed);
        if let Some(coordinator) = coordination
            .coordinator
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            coordinator.abort();
        }
        coordination.replace_poller(None);
    }

    /// Shows the state the polling engine published last.
    pub(super) async fn follow(&self) {
        let Some(coordination) = self.shared().await else {
            return;
        };
        let Some(Some(published)) = coordination.store("state read", Live::published::<Published>) else {
            return;
        };
        {
            let mut inner = self.inner.lock().await;
            inner.providers = published.providers;
            inner.statuses = published.statuses;
            inner.session.adopt(published.session);
            inner.running = published.running;
            inner.last_refresh = published.last_refresh;
            inner.claude_local_usage = published.claude_local_usage;
            inner.recent_samples = published.recent_samples;
        }
        self.emit().await;
    }

    pub(super) async fn publish(&self) {
        let Some(coordination) = self.shared().await else {
            return;
        };
        if !coordination.leading.load(Ordering::Relaxed) {
            return;
        }
        let published = {
            let inner = self.inner.lock().await;
            Published {
                providers: inner.providers.clone(),
                statuses: inner.statuses.clone(),
                session: inner.session.handover(),
                running: inner.running.clone(),
                last_refresh: inner.last_refresh,
                claude_local_usage: inner.claude_local_usage.clone(),
                recent_samples: inner.recent_samples.clone(),
            }
        };
        coordination.store("state write", |live| live.publish(&published));
    }

    /// Hands a refresh to the polling engine; false when this engine polls and should refresh itself.
    pub(super) async fn delegate_refresh(&self, requested: &str) -> bool {
        if self.shared().await.is_none() || self.contest().await {
            return false;
        }
        self.forward(Request::Refresh {
            provider: requested.into(),
        })
        .await
    }

    /// Sends a request to the polling engine; false when this engine polls, so nothing needs sending.
    pub(super) async fn forward(&self, request: Request) -> bool {
        let Some(coordination) = self.shared().await else {
            return false;
        };
        if coordination.leading.load(Ordering::Relaxed) {
            return false;
        }
        coordination.store("request", |live| live.request(&request));
        true
    }

    async fn serve_requests(&self, coordination: &Coordination) {
        let mut requests = Vec::new();
        for request in coordination
            .store("requests read", Live::take_requests::<Request>)
            .unwrap_or_default()
        {
            if !requests.contains(&request) {
                requests.push(request);
            }
        }
        for request in requests {
            match request {
                Request::Refresh { provider } => {
                    self.refresh(&provider).await;
                }
                Request::ClearHistory => {
                    if let Err(error) = self.clear_history().await {
                        eprintln!("Forwarded history clear failed: {error}");
                    }
                }
                Request::ForgetReadings => self.forget_readings().await,
            }
        }
    }

    /// Posts planned notices for whichever engine can show them; false when this engine keeps them itself.
    pub(super) async fn post_notifications(&self, planned: &[Planned]) -> bool {
        let Some(coordination) = self.shared().await else {
            return false;
        };
        coordination
            .store("notification post", |live| live.post(planned, epoch()))
            .is_some()
    }

    pub(super) async fn claim_notifications(&self) -> Vec<Planned> {
        let Some(coordination) = self.shared().await else {
            return Vec::new();
        };
        coordination
            .store("notification claim", |live| live.claim(epoch(), NOTICE_MAX_AGE_SECONDS))
            .unwrap_or_default()
    }

    #[cfg(feature = "fixture-api")]
    pub(super) async fn posted_notification_count(&self) -> u32 {
        let Some(coordination) = self.shared().await else {
            return 0;
        };
        coordination
            .store("notification count", |live| {
                live.pending(epoch(), NOTICE_MAX_AGE_SECONDS)
            })
            .unwrap_or(0)
    }

    pub(super) async fn share_panel_open(&self, open: bool) {
        if let Some(coordination) = self.shared().await {
            coordination.store("presence write", |live| live.present(open, epoch()));
        }
    }

    async fn announce_presence(&self, coordination: &Coordination) {
        if self.inner.lock().await.panel_open {
            coordination.store("presence write", |live| live.present(true, epoch()));
        }
    }

    /// Polls at the visible pace while any engine's panel is open, not only this one's.
    async fn track_visibility(&self, coordination: &Coordination) {
        let now = epoch();
        let visible = coordination
            .store("presence read", |live| live.visible(now, LEASE_SECONDS))
            .unwrap_or(false);
        let mut inner = self.inner.lock().await;
        if inner.remote_visible == visible {
            return;
        }
        inner.remote_visible = visible;
        if visible {
            inner.session.mark_visible(now);
        }
        self.schedule_changed.notify_one();
    }

    /// Adopts settings another engine wrote, so every front end shows and polls with the same choices.
    async fn reload_settings(&self, coordination: &Coordination) {
        {
            // Held while reading, so a setting this engine saves meanwhile cannot be replaced by the older file.
            let mut inner = self.inner.lock().await;
            let Ok(modified) = fs::metadata(store::config()).and_then(|metadata| metadata.modified()) else {
                return;
            };
            {
                let mut seen = coordination
                    .settings_seen
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if *seen == Some(modified) {
                    return;
                }
                *seen = Some(modified);
            }
            let mut settings = blocking(store::read_settings);
            // Demo readings belong to the engine showing them, so another engine's demo switch does not apply.
            settings.demo = inner.settings.demo;
            settings.demo_choice = inner.settings.demo_choice;
            if serde_json::to_value(&inner.settings).ok() == serde_json::to_value(&settings).ok() {
                return;
            }
            inner.history_session = quotaline_settings::history_session_from_settings(
                &settings,
                "UTC".into(),
                0,
                Timestamp::now().as_second() as f64,
            );
            inner
                .history_visibility
                .set_hidden("global", settings.history_hidden_keys.clone());
            inner.settings = settings;
        }
        self.schedule_changed.notify_one();
        self.emit().await;
    }
}

#[cfg(all(test, feature = "fixture-api"))]
#[path = "../../tests/unit/service/coordination/tests.rs"]
mod tests;
