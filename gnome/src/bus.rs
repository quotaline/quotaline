//! Serves the engine's state, settings, history, and actions to GNOME Shell over D-Bus.

use super::*;
use quotaline_engine::service::{self, step_history_reading};
use zbus::fdo;

pub(super) struct Bus(pub(super) Engine);

fn fdo(error: service::Error) -> fdo::Error {
    match error {
        service::Error::InvalidArgs(message) => fdo::Error::InvalidArgs(message),
        service::Error::Failed(message) => fdo::Error::Failed(message),
    }
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

    async fn deliver_notifications(&self, #[zbus(connection)] connection: &zbus::Connection) {
        deliver_notifications(&self.0, connection).await;
    }

    #[cfg(feature = "fixture-api")]
    async fn test_queue_notification(&self) {
        self.0
            .queue_notification(service::Planned {
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
            })
            .await;
    }

    #[cfg(feature = "fixture-api")]
    async fn test_pending_notification_count(&self) -> u32 {
        self.0.pending_notification_count().await
    }

    #[cfg(feature = "fixture-api")]
    async fn test_notification_handoff_count(&self) -> u32 {
        self.0.notification_handoff_count().await
    }

    async fn get_history_view(&self, action: &str, time_zone: &str, offset: i32) -> fdo::Result<String> {
        self.0.history_view(action, time_zone, offset).await.map_err(fdo)
    }

    fn step_history_reading(&self, count: i32, selected: i32, directions: Vec<i32>) -> fdo::Result<i32> {
        step_history_reading(count, selected, &directions).map_err(fdo)
    }

    async fn update_history_visibility(&self, action: &str) -> fdo::Result<String> {
        self.0.history_visibility(action).await.map_err(fdo)
    }

    async fn get_spend_summary(&self) -> fdo::Result<String> {
        self.0.spend_summary().await.map_err(fdo)
    }

    async fn get_review_summary(&self) -> fdo::Result<String> {
        self.0.review_summary().await.map_err(fdo)
    }

    async fn preview_short_label(&self, key: &str, draft: &str) -> fdo::Result<String> {
        self.0.preview_short_label(key, draft).await.map_err(fdo)
    }

    async fn get_model_groups(&self, query: &str, revealed: &str) -> fdo::Result<String> {
        self.0.model_groups(query, revealed).await.map_err(fdo)
    }

    async fn toggle_model(&self, key: &str) -> fdo::Result<String> {
        self.0.toggle_model(key).await.map_err(fdo)
    }

    async fn toggle_provider_models(&self, provider: &str) -> fdo::Result<String> {
        self.0.toggle_provider_models(provider).await.map_err(fdo)
    }

    async fn set_setting(&self, key: &str, raw: &str) -> fdo::Result<String> {
        self.0.set_setting(key, raw).await.map_err(fdo)
    }

    async fn toggle_notification_threshold(&self, threshold: i32) -> fdo::Result<String> {
        self.0.toggle_notification_threshold(threshold).await.map_err(fdo)
    }

    async fn step_provider_interval(&self, provider: &str, delta: i32) -> fdo::Result<String> {
        self.0.step_provider_interval(provider, delta).await.map_err(fdo)
    }

    async fn reorder(&self, kind: &str, source: &str, target: &str, after: bool) -> fdo::Result<String> {
        self.0.reorder(kind, source, target, after).await.map_err(fdo)
    }

    async fn swap_provider(&self, source: &str, target: &str) -> fdo::Result<String> {
        self.0.swap_provider(source, target).await.map_err(fdo)
    }

    async fn refresh(&self, provider: &str) -> bool {
        self.0.refresh(provider).await
    }

    async fn set_panel_open(&self, open: bool) {
        self.0.set_panel_open(open).await;
    }

    fn quit(&self) {
        self.0.quit();
    }

    async fn start_sign_in(&self, provider: &str) -> bool {
        start_sign_in(&self.0, provider).await
    }

    #[cfg(feature = "fixture-api")]
    async fn test_request_sign_in(
        &self,
        provider: &str,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        Self::sign_in_requested(&emitter, provider)
            .await
            .map_err(|error| fdo::Error::Failed(error.to_string()))
    }

    async fn get_diagnostics(&self) -> String {
        self.0.diagnostics().await
    }

    async fn get_issue_url(&self) -> fdo::Result<String> {
        self.0.issue_url().await.map_err(fdo)
    }

    async fn get_logs(&self, full: bool) -> fdo::Result<String> {
        self.0.logs(full).map_err(fdo)
    }

    async fn record_shell_diagnostic(&self, category: &str, message: &str) -> bool {
        self.0.record_diagnostic(category, message).await
    }

    async fn clear_logs(&self) -> fdo::Result<bool> {
        self.0.clear_logs().await.map_err(fdo)
    }

    async fn reset_settings(&self) -> fdo::Result<String> {
        self.0.reset_settings().await.map_err(fdo)
    }

    async fn clear_history(&self) -> fdo::Result<bool> {
        self.0.clear_history().await.map_err(fdo)
    }

    async fn export_metric_history(&self, metric: &str, start: i64, end: i64, path: &str) -> fdo::Result<bool> {
        self.0
            .export_metric_history(metric, start, end, path)
            .await
            .map_err(fdo)
    }

    async fn export_all_history(&self, path: &str) -> fdo::Result<bool> {
        self.0.export_all_history(path).await.map_err(fdo)
    }
}
