//! Answers front-end requests for state, settings, history, and actions.

use super::*;
use serde::Serialize;

fn json(value: &impl Serialize) -> Result<String, Error> {
    serde_json::to_string(value).map_err(failed)
}

impl Engine {
    pub async fn spend_summary(&self) -> Result<String, Error> {
        let settings = self.inner.lock().await.settings.clone();
        json(&blocking(|| store::spend_report(&settings, Timestamp::now())).map_err(failed)?)
    }

    pub async fn review_summary(&self) -> Result<String, Error> {
        let settings = self.inner.lock().await.settings.clone();
        let counts = blocking(|| store::review_counts(&settings, Timestamp::now())).map_err(failed)?;
        counts.map_or(Ok(String::new()), |counts| json(&counts))
    }

    pub async fn preview_short_label(&self, key: &str, draft: &str) -> Result<String, Error> {
        let inner = self.inner.lock().await;
        json(
            &preview_short_label(&inner.providers, &inner.settings.short_labels, key, draft)
                .ok_or_else(|| Error::InvalidArgs("Unknown model".into()))?,
        )
    }

    pub async fn model_groups(&self, query: &str, revealed: &str) -> Result<String, Error> {
        let inner = self.inner.lock().await;
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

    pub async fn toggle_model(&self, key: &str) -> Result<String, Error> {
        let inner = self.inner.lock().await;
        let exists = inner
            .providers
            .iter()
            .any(|(id, provider)| provider.windows.iter().any(|row| key == format!("{id}:{}", row.id)));
        if !exists {
            return Err(Error::InvalidArgs("Unknown model".into()));
        }
        let selected = quotaline_status_bar::available_selected_window_keys(&inner.settings, &inner.providers);
        drop(inner);
        match quotaline_status_bar::toggle_window(&selected, key) {
            Some(next) => self.set_setting("selected_windows", &json!(next).to_string()).await,
            None => Ok(self.state().await),
        }
    }

    pub async fn toggle_provider_models(&self, provider: &str) -> Result<String, Error> {
        let inner = self.inner.lock().await;
        let keys = inner
            .providers
            .get(provider)
            .ok_or_else(|| Error::InvalidArgs("Unknown provider".into()))?
            .windows
            .iter()
            .map(|row| format!("{provider}:{}", row.id))
            .collect::<Vec<_>>();
        let selected = quotaline_status_bar::available_selected_window_keys(&inner.settings, &inner.providers);
        drop(inner);
        match quotaline_status_bar::toggle_provider_windows(&selected, &keys) {
            Some(next) => self.set_setting("selected_windows", &json!(next).to_string()).await,
            None => Ok(self.state().await),
        }
    }

    pub async fn set_setting(&self, key: &str, raw: &str) -> Result<String, Error> {
        self.apply_setting(key, raw).await
    }

    pub async fn toggle_notification_threshold(&self, threshold: i32) -> Result<String, Error> {
        self.set_setting("toggle_notification_threshold", &threshold.to_string())
            .await
    }

    pub async fn step_provider_interval(&self, provider: &str, delta: i32) -> Result<String, Error> {
        self.set_setting("step_provider_interval", &json!([provider, delta]).to_string())
            .await
    }

    pub async fn reorder(&self, kind: &str, source: &str, target: &str, after: bool) -> Result<String, Error> {
        let inner = self.inner.lock().await;
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
            _ => return Err(Error::InvalidArgs("Unknown order kind".into())),
        };
        drop(inner);
        self.set_setting(key, &json(&values)?).await
    }

    pub async fn swap_provider(&self, source: &str, target: &str) -> Result<String, Error> {
        let inner = self.inner.lock().await;
        let mut order = OrderDraft::from_state(&inner.settings, &inner.providers);
        order.swap_provider(source, target);
        drop(inner);
        self.set_setting("provider_order", &json(&order.providers)?).await
    }

    pub async fn set_panel_open(&self, open: bool) {
        let missing = {
            let mut inner = self.inner.lock().await;
            if inner.panel_open == open {
                return;
            }
            inner.panel_open = open;
            let now = epoch();
            if open {
                inner.session.mark_visible(now);
            }
            self.schedule_changed.notify_one();
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
                            Duration::from_secs(self.minute_seconds),
                        )
                    })
                    .collect()
            }
        };
        self.share_panel_open(open).await;
        for id in missing {
            self.refresh(id).await;
        }
    }

    pub async fn diagnostics(&self) -> String {
        let inner = self.inner.lock().await;
        let mut logs = self.logs.tail().into_iter().rev().take(80).collect::<Vec<_>>();
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

    pub async fn issue_url(&self) -> Result<String, Error> {
        quotaline_logging::issue::url(
            quotaline_logging::issue::REPOSITORY_URL,
            "Issue report",
            &self.diagnostics().await,
            None,
            quotaline_logging::issue::MAX_ISSUE_URL_LENGTH,
        )
        .map_err(failed)
    }

    pub fn logs(&self, full: bool) -> Result<String, Error> {
        json(&if full { self.logs.retained() } else { self.logs.tail() })
    }

    pub async fn record_diagnostic(&self, category: &str, message: &str) -> bool {
        if !matches!(category, "geometry" | "tabs" | "status") || message.len() > 256 {
            return false;
        }
        if !self.inner.lock().await.settings.detailed_logging {
            return false;
        }
        self.log("debug", category, message).await;
        true
    }

    pub async fn clear_logs(&self) -> Result<bool, Error> {
        self.logs.clear().map_err(failed)?;
        self.host.log_changed().await;
        Ok(true)
    }

    pub async fn reset_settings(&self) -> Result<String, Error> {
        {
            let mut inner = self.inner.lock().await;
            let mut settings = Settings::default();
            store::atomic_json(&store::config(), &settings).map_err(failed)?;
            settings.request_demo(self.demo_requested);
            inner.settings = settings;
            inner.history_session = quotaline_settings::history_session_from_settings(
                &inner.settings,
                "UTC".into(),
                0,
                Timestamp::now().as_second() as f64,
            );
            inner.history_visibility.clear();
        }
        self.forget_readings().await;
        self.forward(coordination::Request::ForgetReadings).await;
        self.refresh("all").await;
        Ok(self.state().await)
    }

    pub(super) async fn forget_readings(&self) {
        {
            let mut inner = self.inner.lock().await;
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
        self.schedule_changed.notify_one();
    }

    pub async fn clear_history(&self) -> Result<bool, Error> {
        {
            let mut inner = self.inner.lock().await;
            let mut reader = self
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
        // The polling engine keeps its own copy of recent readings and transcript progress, which it must drop too.
        self.forward(coordination::Request::ClearHistory).await;
        self.emit().await;
        Ok(true)
    }

    pub async fn export_metric_history(&self, metric: &str, start: i64, end: i64, path: &str) -> Result<bool, Error> {
        let path = PathBuf::from(path);
        if metric == "usage" {
            let inner = self.inner.lock().await;
            let settings = inner.settings.clone();
            let selected = quotaline_status_bar::history_window_selection(&settings, &inner.providers);
            drop(inner);
            blocking(|| store::export(&settings, &path, start, end, selected.as_ref())).map_err(failed)?;
            return Ok(true);
        }
        if !analytics::METRICS.iter().any(|(id, _, _, _)| *id == metric) {
            return Err(Error::InvalidArgs("Unknown metric".into()));
        }
        let settings = self.inner.lock().await.settings.clone();
        blocking(|| store::export_analytics(&settings, &path, metric, start, end)).map_err(failed)?;
        Ok(true)
    }

    pub async fn export_all_history(&self, path: &str) -> Result<bool, Error> {
        let settings = self.inner.lock().await.settings.clone();
        let path = PathBuf::from(path);
        tokio::task::spawn_blocking(move || store::export_all(&settings, &path))
            .await
            .map_err(failed)?
            .map_err(failed)?;
        Ok(true)
    }
}
