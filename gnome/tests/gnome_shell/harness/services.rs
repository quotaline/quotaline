//! Provides isolated Shell test fixtures for services.

use super::*;

#[cfg(feature = "fixture-api")]
pub(crate) struct NotificationSink(
    pub(crate) tokio::sync::mpsc::UnboundedSender<(String, String, Vec<String>, bool, bool)>,
);

#[cfg(feature = "fixture-api")]
pub(crate) struct ClockSettings(pub(crate) &'static str);

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "org.freedesktop.portal.Settings")]
impl ClockSettings {
    fn read_one(&self, namespace: &str, key: &str) -> zbus::fdo::Result<zbus::OwnedVariant> {
        assert_eq!((namespace, key), ("org.gnome.desktop.interface", "clock-format"));
        Ok(zbus::Variant::from(self.0).try_into().unwrap())
    }
}

#[cfg(feature = "fixture-api")]
pub(crate) struct PowerProfiles {
    pub(crate) active: String,
    pub(crate) degraded: String,
}

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
impl PowerProfiles {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.active.clone()
    }

    #[zbus(property)]
    fn set_active_profile(&mut self, value: String) {
        self.active = value;
    }

    #[zbus(property)]
    fn performance_degraded(&self) -> String {
        self.degraded.clone()
    }

    #[zbus(property)]
    fn set_performance_degraded(&mut self, value: String) {
        self.degraded = value;
    }
}

#[cfg(feature = "fixture-api")]
pub(crate) struct GatedNotificationSink {
    pub(crate) requested: tokio::sync::mpsc::UnboundedSender<()>,
    pub(crate) gate: std::sync::Arc<tokio::sync::Semaphore>,
    pub(crate) delivered: tokio::sync::mpsc::UnboundedSender<String>,
}

#[cfg(feature = "fixture-api")]
pub(crate) struct PortalChooser(
    pub(crate) tokio::sync::mpsc::UnboundedSender<()>,
    pub(crate) bool,
    pub(crate) Option<std::path::PathBuf>,
    pub(crate) Option<std::sync::Arc<tokio::sync::Notify>>,
);

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "org.freedesktop.portal.FileChooser")]
impl PortalChooser {
    async fn save_file(
        &self,
        _parent_window: &str,
        _title: &str,
        _options: std::collections::HashMap<String, zbus::OwnedVariant>,
    ) -> zbus::fdo::Result<zbus::OwnedObjectPath> {
        if let Some(marker) = &self.2 {
            fs::write(marker, "requested").unwrap();
        }
        self.0.send(()).unwrap();
        if let Some(gate) = &self.3 {
            gate.notified().await;
        }
        if self.1 {
            return Err(zbus::fdo::Error::Failed("Save dialog unavailable".into()));
        }
        Ok(zbus::OwnedObjectPath::try_from("/org/freedesktop/portal/desktop/request/test/tmb").unwrap())
    }
}

#[cfg(feature = "fixture-api")]
pub(crate) struct PortalRequest(pub(crate) std::path::PathBuf);

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl PortalRequest {
    fn close(&self) {
        fs::write(&self.0, "closed").unwrap();
    }
}

#[cfg(feature = "fixture-api")]
pub(crate) struct FaultService {
    pub(crate) state: std::sync::Mutex<String>,
    pub(crate) history_session: tokio::sync::Mutex<quotaline_analytics::HistorySession>,
    pub(crate) hold_state: bool,
    pub(crate) state_calls: tokio::sync::Semaphore,
    pub(crate) state_gate: tokio::sync::Semaphore,
    pub(crate) hold_first_activity: bool,
    pub(crate) fail_activity_until_enabled: bool,
    pub(crate) activity_reply: String,
    pub(crate) activity_enabled: std::sync::atomic::AtomicBool,
    pub(crate) activity_calls: tokio::sync::Semaphore,
    pub(crate) activity_gate: tokio::sync::Semaphore,
    pub(crate) activity_requests: std::sync::atomic::AtomicUsize,
    pub(crate) hold_history: bool,
    pub(crate) history_gate: tokio::sync::Notify,
    pub(crate) history_calls: tokio::sync::Semaphore,
    pub(crate) history_retry_sequence: bool,
    pub(crate) history_requests: std::sync::atomic::AtomicUsize,
    pub(crate) malformed_logs: bool,
    pub(crate) log_requests: std::sync::atomic::AtomicUsize,
    pub(crate) codex_cost: Option<f64>,
    pub(crate) code_reviews: Option<&'static str>,
    pub(crate) cost_reply_error: bool,
    pub(crate) hold_cost: bool,
    pub(crate) cost_gate: tokio::sync::Semaphore,
}

#[cfg(feature = "fixture-api")]
#[derive(Default)]
pub(crate) struct FaultOptions {
    pub(crate) malformed_state: bool,
    pub(crate) hold_state: bool,
    pub(crate) hold_first_activity: bool,
    pub(crate) fail_activity_until_enabled: bool,
    pub(crate) hold_history: bool,
    pub(crate) history_retry_sequence: bool,
    pub(crate) malformed_logs: bool,
    pub(crate) empty_metrics: bool,
    pub(crate) warning_notice: bool,
    pub(crate) duplicate_limit_notice: bool,
    pub(crate) refreshing_provider: bool,
    pub(crate) first_run_pending: bool,
    pub(crate) credential_states: bool,
    pub(crate) provider_visibility: Option<&'static str>,
    pub(crate) codex_cost: Option<f64>,
    pub(crate) code_reviews: Option<&'static str>,
    pub(crate) review_expected: Option<&'static str>,
    pub(crate) cost_reply_error: bool,
    pub(crate) hold_cost: bool,
}

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "tech.bernat.Quotaline.Gnome")]
impl FaultService {
    async fn get_state(&self) -> String {
        self.state_calls.add_permits(1);
        if self.hold_state {
            self.state_gate.acquire().await.unwrap().forget();
        }
        self.state.lock().unwrap().clone()
    }

    async fn wait_for_state_call(&self) {
        self.state_calls.acquire().await.unwrap().forget();
    }

    fn release_state(&self) {
        self.state_gate.add_permits(1);
    }

    fn set_retention_days(&self, days: u64) {
        let mut state = self.state.lock().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&state).unwrap();
        value["settings"]["retention_days"] = serde_json::json!(days);
        *state = value.to_string();
    }

    async fn wait_for_activity_call(&self) {
        self.activity_calls.acquire().await.unwrap().forget();
    }

    fn release_activity(&self) {
        self.activity_gate.add_permits(1);
    }

    fn enable_activity(&self) {
        self.activity_enabled.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn set_panel_open(&self, _open: bool) {}

    fn set_setting(&self, key: &str, raw: &str) -> String {
        let mut state = self.state.lock().unwrap();
        let mut value: serde_json::Value = serde_json::from_str(&state).unwrap();
        value["settings"][key] = serde_json::from_str(raw).unwrap();
        *state = value.to_string();
        state.clone()
    }

    async fn get_spend_summary(&self) -> zbus::fdo::Result<String> {
        if self.hold_cost {
            self.cost_gate.acquire().await.unwrap().forget();
        }
        if self.cost_reply_error {
            return Err(zbus::fdo::Error::Failed("Cost history request failed".into()));
        }
        if let Some(cost) = self.codex_cost {
            return Ok(serde_json::json!({"by_provider":{"codex":{
                "today":cost,"yesterday":0,"last_window":cost,
                "models":[{"provider":"codex","model":"gpt-5","cost":cost}],
                "providers":["codex"]
            }}})
            .to_string());
        }
        Ok("{".into())
    }

    async fn get_review_summary(&self) -> zbus::fdo::Result<String> {
        if self.cost_reply_error {
            return Err(zbus::fdo::Error::Failed("Review summary request failed".into()));
        }
        Ok(self.code_reviews.unwrap_or_default().into())
    }

    fn release_cost(&self) {
        self.cost_gate.add_permits(1);
    }

    async fn get_rolled_history(&self, start: i64, end: i64, _rollup: &str, _utc: bool) -> zbus::fdo::Result<String> {
        self.history_calls.add_permits(1);
        let request = self.history_requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.hold_history {
            self.history_gate.notified().await;
        }
        if self.hold_history && request == 0 {
            return Err(zbus::fdo::Error::Failed("Superseded History request failed".into()));
        }
        if self.history_retry_sequence {
            if request == 1 {
                return Err(zbus::fdo::Error::Failed("History request failed".into()));
            }
            return Ok(serde_json::json!({"metric":"usage","start":start,"end":end,"series":[{
                "id":"claude:sonnet","provider":"claude","label":"sonnet","points":[[end-60,32+request*10,null]]
            }],
                "project_labels":{}
            })
            .to_string());
        }
        Ok("{".into())
    }

    async fn get_history_view(&self, action: &str, _time_zone: &str, _offset: i32) -> zbus::fdo::Result<String> {
        let action: serde_json::Value =
            serde_json::from_str(action).map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        let now = chrono::Utc::now().timestamp() as f64;
        let view = {
            let mut session = self.history_session.lock().await;
            match action["kind"].as_str() {
                Some("now") => session.select_period(None, now),
                Some("page") => session.page(now, action["forward"] == true),
                _ => {}
            }
            session.view(now)
        };
        match self
            .get_rolled_history(view.start, view.end, view.rollup, view.utc)
            .await
        {
            Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
                Ok(history) => Ok(serde_json::json!({"view":view,"history":history,"error":null}).to_string()),
                Err(_) => Ok(raw),
            },
            Err(error) => Ok(serde_json::json!({"view":view,"history":null,"error":error.to_string()}).to_string()),
        }
    }

    fn release_history(&self) {
        self.history_gate.notify_one();
    }

    async fn wait_for_history_call(&self) {
        self.history_calls.acquire().await.unwrap().forget();
    }

    async fn get_model_groups(&self, _query: &str, _revealed: &str) -> zbus::fdo::Result<String> {
        let first = self.activity_requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
        self.activity_calls.add_permits(1);
        if self.hold_first_activity && first {
            self.activity_gate.acquire().await.unwrap().forget();
            return Err(zbus::fdo::Error::Failed("Superseded model request failed".into()));
        }
        if self.fail_activity_until_enabled && !self.activity_enabled.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(zbus::fdo::Error::Failed("Model request failed".into()));
        }
        Ok(self.activity_reply.clone())
    }

    fn get_logs(&self, _full: bool) -> String {
        self.log_requests.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if self.malformed_logs { "{" } else { "[]" }.into()
    }

    fn get_log_request_count(&self) -> u64 {
        self.log_requests.load(std::sync::atomic::Ordering::SeqCst) as u64
    }

    fn start_sign_in(&self, _provider: &str) -> zbus::fdo::Result<bool> {
        Err(zbus::fdo::Error::Failed("Sign-in service failed".into()))
    }
}

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl NotificationSink {
    #[expect(
        clippy::too_many_arguments,
        reason = "D-Bus Notifications.Notify defines this signature"
    )]
    fn notify(
        &self,
        _application: &str,
        _replacement: u32,
        _icon: &str,
        title: &str,
        body: &str,
        actions: Vec<String>,
        hints: std::collections::HashMap<String, zbus::OwnedVariant>,
        _timeout: i32,
    ) -> u32 {
        self.0
            .send((
                title.into(),
                body.into(),
                actions,
                hints.contains_key("sound-name"),
                hints.contains_key("suppress-sound"),
            ))
            .unwrap();
        1
    }
}

#[cfg(feature = "fixture-api")]
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl GatedNotificationSink {
    #[expect(
        clippy::too_many_arguments,
        reason = "D-Bus Notifications.Notify defines this signature"
    )]
    async fn notify(
        &self,
        _application: &str,
        _replacement: u32,
        _icon: &str,
        title: &str,
        _body: &str,
        _actions: Vec<String>,
        _hints: std::collections::HashMap<String, zbus::OwnedVariant>,
        _timeout: i32,
    ) -> u32 {
        self.requested.send(()).unwrap();
        self.gate.acquire().await.unwrap().forget();
        self.delivered.send(title.into()).unwrap();
        1
    }
}
