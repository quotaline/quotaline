//! Applies settings changes and schedules their effects.

use super::*;
use quotaline_settings::SettingsUpdateEffects;

impl Engine {
    pub(super) async fn apply_setting(&self, key: &str, raw: &str) -> Result<String, Error> {
        let mut value: Value = serde_json::from_str(raw).map_err(invalid)?;
        let effects;
        {
            let mut inner = self.inner.lock().await;
            let mut effective_key = key;
            if key == "toggle_notification_threshold" {
                let threshold = value
                    .as_i64()
                    .filter(|value| {
                        quotaline_notifications::THRESHOLD_CHOICES
                            .iter()
                            .any(|choice| i64::from(*choice) == *value)
                    })
                    .ok_or_else(|| Error::InvalidArgs("Unknown notification threshold".into()))?;
                let thresholds = &inner.settings.notify_thresholds;
                value = json!(quotaline_settings::set_threshold(
                    thresholds.iter().copied(),
                    threshold as i32,
                    !thresholds.contains(&(threshold as u8)),
                ));
                effective_key = "notify_thresholds";
            }
            if key == "step_provider_interval" {
                let (provider, delta): (String, i32) = serde_json::from_value(value).map_err(invalid)?;
                if ![-1, 1].contains(&delta) {
                    return Err(Error::InvalidArgs("Interval step must be one minute".into()));
                }
                value = json!(
                    quotaline_settings::step_provider_interval(&inner.settings.interval_minutes, &provider, delta,)
                        .ok_or_else(|| Error::InvalidArgs("Unknown provider".into()))?
                );
                effective_key = "interval_minutes";
            }
            if let Some(model) = key.strip_prefix("short_label:") {
                let draft = value
                    .as_str()
                    .ok_or_else(|| Error::InvalidArgs("Short label must be text".into()))?;
                let preview = preview_short_label(&inner.providers, &inner.settings.short_labels, model, draft)
                    .ok_or_else(|| Error::InvalidArgs("Unknown model".into()))?;
                if preview.conflict_key.is_some() {
                    drop(inner);
                    return Ok(self.state().await);
                }
                let mut labels = inner.settings.short_labels.clone();
                if let Some(label) = preview.override_value {
                    labels.insert(model.into(), label);
                } else {
                    labels.remove(model);
                }
                value = json!(labels);
                effective_key = "short_labels";
            }
            if effective_key == "short_labels" {
                let overrides: BTreeMap<String, String> = serde_json::from_value(value.clone()).map_err(invalid)?;
                if short_label_conflicts(&inner.providers, &inner.settings.short_labels, &overrides) {
                    return Err(Error::InvalidArgs("Short label conflicts with another model".into()));
                }
            }
            let mut changed = inner.settings.clone();
            changed.set(effective_key, value).map_err(Error::InvalidArgs)?;
            effects = SettingsUpdateEffects::between(&inner.settings, &changed, key);
            store::atomic_json(&store::config(), &changed.stored()).map_err(failed)?;
            if effects.reset_demo {
                inner.providers = if changed.demo { demo() } else { BTreeMap::new() };
                inner.statuses.clear();
                inner.last_refresh = None;
                if changed.demo {
                    self.warm_demo();
                }
            }
            inner.first_run_pending &= !changed.first_run_card_dismissed;
            inner.settings = changed;
            if effects.sync_history_visibility {
                let hidden = inner.settings.history_hidden_keys.clone();
                inner.history_visibility.set_hidden("global", hidden);
            }
            if effects.reset_history_session {
                inner.history_session = quotaline_settings::history_session_from_settings(
                    &inner.settings,
                    "UTC".into(),
                    0,
                    Timestamp::now().as_second() as f64,
                );
            }
        }
        if let Some(days) = effects.prune_days {
            blocking(|| store::prune(days)).map_err(failed)?;
        }
        self.schedule_changed.notify_one();
        if effects.refresh_all {
            // Fetches in flight read the previous settings, so their results must not land.
            self.inner.lock().await.session.cancel();
            self.refresh("all").await;
        } else {
            for id in effects.newly_enabled {
                self.refresh(id).await;
            }
            self.emit().await;
        }
        Ok(self.state().await)
    }
}
