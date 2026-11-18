//! Reads the current service snapshot for the GNOME adapter.

use super::*;
use quotaline_gnome_adapter::state::{StateInput, project};

impl App {
    pub(super) async fn state(&self) -> String {
        let inner = self.inner.lock().await;
        project(StateInput {
            settings: &inner.settings,
            providers: &inner.providers,
            running: &inner.running,
            last_refresh: inner.last_refresh,
            first_run_pending: inner.first_run_pending,
            claude_local_usage: inner.claude_local_usage.as_ref(),
            recent_samples: &inner.recent_samples,
            now: Timestamp::now().as_millisecond() as f64 / 1000.0,
        })
    }
}
