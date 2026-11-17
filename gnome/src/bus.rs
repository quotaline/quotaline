//! Serves GNOME Shell state, settings, history, and actions over D-Bus.

use super::*;
use quotaline_gnome_adapter::history_view::history_view;
use serde::Serialize;

mod history_selection;
mod history_session;
mod history_visibility;
mod settings;

pub(super) struct Bus(pub(super) App);

fn failed(error: impl std::fmt::Display) -> fdo::Error {
    fdo::Error::Failed(error.to_string())
}

fn json(value: &impl Serialize) -> fdo::Result<String> {
    serde_json::to_string(value).map_err(failed)
}

#[zbus::interface(name = "tech.bernat.Quotaline.Gnome")]
impl Bus {
    #[zbus(signal)]
    pub(super) async fn changed(emitter: &SignalEmitter<'_>, state: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn log_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn sign_in_requested(emitter: &SignalEmitter<'_>, provider: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    pub(super) async fn notification_planned(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    async fn get_state(&self) -> String {
        self.0.state().await
    }

    async fn deliver_notifications(&self) {
        self.0.deliver_notifications().await;
    }

    #[cfg(feature = "fixture-api")]
    async fn test_queue_notification(&self, #[zbus(signal_emitter)] emitter: SignalEmitter<'_>) -> fdo::Result<()> {
        self.0.inner.lock().await.pending_notifications.extend([Planned {
            provider: "Claude".into(),
            event: notifications::Event {
                id: "claude:fixture".into(),
                kind: notifications::Kind::Authentication,
                window_id: None,
                facts: notifications::Facts::Authentication {
                    detail: Some("Sign in to Claude".into()),
                },
            },
            sign_in: Some("claude".into()),
            fetched_at: Timestamp::now().as_second(),
        }]);
        Self::notification_planned(&emitter).await.map_err(failed)
    }

    #[cfg(feature = "fixture-api")]
    async fn test_pending_notification_count(&self) -> u32 {
        self.0.inner.lock().await.pending_notifications.len() as u32
    }

    #[cfg(feature = "fixture-api")]
    async fn test_notification_handoff_count(&self) -> u32 {
        self.0.inner.lock().await.notification_handoffs
    }

    async fn get_history_view(&self, action: &str, time_zone: &str, offset: i32) -> fdo::Result<String> {
        self.history_view(action, time_zone, offset).await
    }

    fn step_history_reading(&self, count: i32, selected: i32, directions: Vec<i32>) -> fdo::Result<i32> {
        history_selection::step(count, selected, &directions)
    }

    async fn update_history_visibility(&self, action: &str) -> fdo::Result<String> {
        self.history_visibility(action).await
    }

    async fn get_spend_summary(&self) -> fdo::Result<String> {
        let settings = self.0.inner.lock().await.settings.clone();
        json(&blocking(|| store::spend_report(&settings, Timestamp::now())).map_err(failed)?)
    }

    async fn get_review_summary(&self) -> fdo::Result<String> {
        let settings = self.0.inner.lock().await.settings.clone();
        let counts = blocking(|| store::review_counts(&settings, Timestamp::now())).map_err(failed)?;
        counts.map_or(Ok(String::new()), |counts| json(&counts))
    }

    async fn preview_short_label(&self, key: &str, draft: &str) -> fdo::Result<String> {
        let inner = self.0.inner.lock().await;
        json(
            &preview_short_label(&inner.providers, &inner.settings.short_labels, key, draft)
                .ok_or_else(|| fdo::Error::InvalidArgs("Unknown model".into()))?,
        )
    }

    async fn get_model_groups(&self, query: &str, revealed: &str) -> fdo::Result<String> {
        let inner = self.0.inner.lock().await;
        let settings = inner.settings.clone();
        let providers = inner.providers.clone();
        drop(inner);
        let activity = blocking(|| store::model_activity(&settings, Timestamp::now().as_second())).map_err(failed)?;
        json(&quotaline_status_bar::settings_list_for_providers(
            &settings,
            &providers,
            &activity,
            query,
            (!revealed.is_empty()).then_some(revealed),
            settings.hide_unused_models,
        ))
    }

    async fn toggle_model(&self, key: &str) -> fdo::Result<String> {
        let inner = self.0.inner.lock().await;
        let exists = inner
            .providers
            .iter()
            .any(|(id, provider)| provider.windows.iter().any(|row| key == format!("{id}:{}", row.id)));
        if !exists {
            return Err(fdo::Error::InvalidArgs("Unknown model".into()));
        }
        let selected = quotaline_status_bar::available_selected_window_keys(&inner.settings, &inner.providers);
        drop(inner);
        match quotaline_status_bar::toggle_window(&selected, key) {
            Some(next) => self.set_setting("selected_windows", &json!(next).to_string()).await,
            None => Ok(self.0.state().await),
        }
    }

    async fn toggle_provider_models(&self, provider: &str) -> fdo::Result<String> {
        let inner = self.0.inner.lock().await;
        let keys = inner
            .providers
            .get(provider)
            .ok_or_else(|| fdo::Error::InvalidArgs("Unknown provider".into()))?
            .windows
            .iter()
            .map(|row| format!("{provider}:{}", row.id))
            .collect::<Vec<_>>();
        let selected = quotaline_status_bar::available_selected_window_keys(&inner.settings, &inner.providers);
        drop(inner);
        match quotaline_status_bar::toggle_provider_windows(&selected, &keys) {
            Some(next) => self.set_setting("selected_windows", &json!(next).to_string()).await,
            None => Ok(self.0.state().await),
        }
    }

    async fn set_setting(&self, key: &str, raw: &str) -> fdo::Result<String> {
        self.apply_setting(key, raw).await
    }

    async fn toggle_notification_threshold(&self, threshold: i32) -> fdo::Result<String> {
        self.set_setting("toggle_notification_threshold", &threshold.to_string())
            .await
    }

    async fn step_provider_interval(&self, provider: &str, delta: i32) -> fdo::Result<String> {
        self.set_setting("step_provider_interval", &json!([provider, delta]).to_string())
            .await
    }

    async fn reorder(&self, kind: &str, source: &str, target: &str, after: bool) -> fdo::Result<String> {
        let inner = self.0.inner.lock().await;
        let mut order = OrderDraft::from_state(&inner.settings, &inner.providers);
        let (key, values) = match kind {
            "provider" => {
                order.move_provider(source, target, after);
                ("provider_order", order.providers)
            }
            "model" => {
                order.move_model(source, target, after);
                ("model_order", order.models)
            }
            _ => return Err(fdo::Error::InvalidArgs("Unknown order kind".into())),
        };
        drop(inner);
        self.set_setting(key, &json(&values)?).await
    }

    async fn swap_provider(&self, source: &str, target: &str) -> fdo::Result<String> {
        let inner = self.0.inner.lock().await;
        let mut order = OrderDraft::from_state(&inner.settings, &inner.providers);
        order.swap_provider(source, target);
        drop(inner);
        self.set_setting("provider_order", &json(&order.providers)?).await
    }

    async fn refresh(&self, provider: &str) -> bool {
        self.0.refresh(provider).await
    }

    async fn set_panel_open(&self, open: bool) {
        let missing = {
            let mut inner = self.0.inner.lock().await;
            if inner.panel_open == open {
                return;
            }
            inner.panel_open = open;
            let now = epoch();
            if open {
                inner.session.mark_visible(now);
            }
            self.0.schedule_changed.notify_one();
            if !open {
                Vec::new()
            } else {
                IDS.into_iter()
                    .filter(|id| {
                        refresh::should_check_credentials(
                            inner.providers.get(*id),
                            inner.settings.providers.get(*id) != Some(&false),
                            inner
                                .session
                                .times(id)
                                .last_attempt
                                .map(|last| Duration::from_secs_f64((now - last).max(0.0))),
                            Duration::from_secs(self.0.minute_seconds),
                        )
                    })
                    .collect()
            }
        };
        for id in missing {
            self.0.refresh(id).await;
        }
    }

    fn quit(&self) {
        self.0.shutdown.notify_one();
    }

    async fn start_sign_in(&self, provider: &str) -> bool {
        self.0.start_sign_in(provider).await
    }

    #[cfg(feature = "fixture-api")]
    async fn test_request_sign_in(
        &self,
        provider: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        Self::sign_in_requested(&emitter, provider).await.map_err(failed)
    }

    async fn get_diagnostics(&self) -> String {
        let inner = self.0.inner.lock().await;
        let mut logs = self.0.logs.tail().into_iter().rev().take(80).collect::<Vec<_>>();
        logs.reverse();
        let providers: BTreeMap<_, _> = inner
            .providers
            .iter()
            .map(|(id, provider)| {
                (
                    id,
                    json!({
                        "status": provider.status,
                        "updated": provider.updated,
                        "window_count": provider.windows.len(),
                        "retry_after": provider.retry_after,
                    }),
                )
            })
            .collect();
        serde_json::to_string_pretty(&json!({
            "version": env!("CARGO_PKG_VERSION"),
            "providers": providers,
            "history_path": "$XDG_DATA_HOME/quotaline/history.sqlite3",
            "demo": inner.settings.demo,
            "refresh_minutes": inner.settings.interval_minutes,
            "analytics_minutes": inner.settings.analytics_interval_minutes,
            "format": inner.settings.format,
            "recent_log": logs,
        }))
        .unwrap_or_default()
    }

    async fn get_issue_url(&self) -> fdo::Result<String> {
        quotaline_logging::issue::url(
            quotaline_logging::issue::REPOSITORY_URL,
            "Issue report",
            &self.get_diagnostics().await,
            None,
            quotaline_logging::issue::MAX_ISSUE_URL_LENGTH,
        )
        .map_err(failed)
    }

    async fn get_logs(&self, full: bool) -> fdo::Result<String> {
        json(&if full {
            self.0.logs.retained()
        } else {
            self.0.logs.tail()
        })
    }

    async fn record_shell_diagnostic(&self, category: &str, message: &str) -> bool {
        if !matches!(category, "geometry" | "tabs" | "status") || message.len() > 256 {
            return false;
        }
        if !self.0.inner.lock().await.settings.detailed_logging {
            return false;
        }
        self.0.log("debug", category, message).await;
        true
    }

    async fn clear_logs(&self) -> fdo::Result<bool> {
        self.0.logs.clear().map_err(failed)?;
        self.0.emit_log_changed().await;
        Ok(true)
    }

    async fn reset_settings(&self) -> fdo::Result<String> {
        {
            let mut inner = self.0.inner.lock().await;
            let mut settings = Settings::default();
            store::atomic_json(&store::config(), &settings).map_err(failed)?;
            settings.request_demo(self.0.demo_requested);
            inner.settings = settings;
            inner.history_session = quotaline_settings::history_session_from_settings(
                &inner.settings,
                "UTC".into(),
                0,
                Timestamp::now().as_second() as f64,
            );
            inner.history_visibility.clear();
            inner.providers.clear();
            inner.statuses.clear();
            inner.notified.clear();
            // Results from the running cycle become stale, so they cannot bring back the cleared providers.
            inner.session.reset(epoch());
            inner.last_analytics = None;
            inner.last_claude_scan = None;
            inner.claude_scan_pending = false;
            inner.claude_local_usage = None;
        }
        self.0.schedule_changed.notify_one();
        self.0.refresh("all").await;
        Ok(self.0.state().await)
    }

    async fn clear_history(&self) -> fdo::Result<bool> {
        {
            let mut inner = self.0.inner.lock().await;
            let mut reader = self
                .0
                .claude_reader
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            reader.clear().map_err(failed)?;
            blocking(store::clear).map_err(failed)?;
            inner.recent_samples.clear();
            inner.claude_local_usage = None;
            inner.claude_scanned = false;
            inner.claude_scan_pending = false;
            inner.last_claude_scan = None;
        }
        self.0.emit().await;
        Ok(true)
    }

    async fn export_metric_history(&self, metric: &str, start: i64, end: i64, path: &str) -> fdo::Result<bool> {
        let path = PathBuf::from(path);
        if metric == "usage" {
            let inner = self.0.inner.lock().await;
            let settings = inner.settings.clone();
            let selected = quotaline_status_bar::history_window_selection(&settings, &inner.providers);
            drop(inner);
            blocking(|| store::export(&settings, &path, start, end, selected.as_ref())).map_err(failed)?;
            return Ok(true);
        }
        if !analytics::METRICS.iter().any(|(id, _, _, _)| *id == metric) {
            return Err(fdo::Error::InvalidArgs("Unknown metric".into()));
        }
        let settings = self.0.inner.lock().await.settings.clone();
        blocking(|| store::export_analytics(&settings, &path, metric, start, end)).map_err(failed)?;
        Ok(true)
    }

    async fn export_all_history(&self, path: &str) -> fdo::Result<bool> {
        let settings = self.0.inner.lock().await.settings.clone();
        let path = PathBuf::from(path);
        tokio::task::spawn_blocking(move || store::export_all(&settings, &path))
            .await
            .map_err(failed)?
            .map_err(failed)?;
        Ok(true)
    }
}
