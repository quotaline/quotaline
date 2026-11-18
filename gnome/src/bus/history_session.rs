//! Combines Rust-owned history navigation with chart retrieval for the Shell.

use super::*;
use quotaline_analytics::HistoryAction;
use quotaline_settings::sync_history_settings;

impl Bus {
    pub(super) async fn history_view(&self, raw_action: &str, time_zone: &str, offset: i32) -> fdo::Result<String> {
        if !(-86_399..=86_399).contains(&offset) {
            return Err(fdo::Error::InvalidArgs("Invalid time zone offset".into()));
        }
        let action: HistoryAction =
            serde_json::from_str(raw_action).map_err(|error| fdo::Error::InvalidArgs(error.to_string()))?;
        let now = (Timestamp::now().as_millisecond() as f64 / 1_000.0).ceil();
        let (view, rollup, settings, selected, changed) = {
            let mut inner = self.0.inner.lock().await;
            let mut session = inner.history_session.clone();
            session.update_zone(time_zone.into(), offset);
            action
                .apply(&mut session, now)
                .map_err(|error| fdo::Error::InvalidArgs(error.into()))?;
            let mut settings = inner.settings.clone();
            let changed = sync_history_settings(&session, &mut settings);
            if changed {
                store::atomic_json(&store::config(), &settings.stored()).map_err(failed)?;
            }
            let selected = quotaline_status_bar::history_window_selection(&settings, &inner.providers);
            let (view, rollup) = (session.view(now), session.rollup());
            inner.settings = settings.clone();
            inner.history_session = session;
            (view, rollup, settings, selected, changed)
        };
        if changed {
            self.0.emit().await;
        }
        let history = blocking(|| {
            if view.metric == "usage" {
                store::rolled_history(&settings, view.start, view.end, rollup, view.utc).map(|mut data| {
                    data.retain_selected(selected.as_ref());
                    data
                })
            } else {
                store::history_analytics(&settings, &view.metric, view.start, view.end)
            }
        });
        Ok(match history {
            Ok(data) => {
                let project_labels = if matches!(view.metric.as_str(), "projectCost" | "projectMessages") {
                    let names = data
                        .series
                        .iter()
                        .map(|series| series.label.clone())
                        .collect::<Vec<_>>();
                    project::labels(&names, settings.hide_identity)
                } else {
                    BTreeMap::new()
                };
                json!({"view": view, "history": history_view(&data, &project_labels), "error": null})
            }
            Err(error) => json!({"view": view, "history": null, "error": failed(error).to_string()}),
        }
        .to_string())
    }
}
