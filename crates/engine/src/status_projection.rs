//! Maps GNOME provider snapshots into shared quota projections.

use quotaline_quota::Provider;
use quotaline_settings::Settings;
use quotaline_status_bar::OrderDraft;
use quotaline_status_bar::StatusProjectedEntry;
use quotaline_status_bar::StatusProvider;
use quotaline_status_bar::StatusSelectedWindow;
use quotaline_status_bar::StatusSelectionEntry;
use quotaline_status_bar::StatusSelectionGroup;
use quotaline_status_bar::StatusSelectionMode;
use quotaline_status_bar::StatusWindow;
use quotaline_status_bar::select_status;
use quotaline_status_bar::selected_window_keys;
use quotaline_status_bar::status_entries;
use std::collections::BTreeMap;

pub struct StatusProjection {
    pub entries: Vec<StatusProjectedEntry>,
    pub provider_indices: Vec<usize>,
    pub mini_groups: Vec<StatusSelectionGroup>,
}

pub fn project(providers: &BTreeMap<String, Provider>, settings: &Settings, now: f64) -> StatusProjection {
    let source = providers
        .iter()
        .filter(|(id, provider)| {
            settings.providers.get(*id) != Some(&false) && matches!(provider.status.as_str(), "connected" | "stale")
        })
        .map(|(id, provider)| StatusProvider {
            id: id.clone(),
            windows: provider
                .windows
                .iter()
                .map(|window| StatusWindow {
                    id: window.id.clone(),
                    label: window.label.clone(),
                    group: window.group.clone(),
                    used: window.used,
                    active: window.active,
                    reset_at: window.reset.as_deref().and_then(quotaline_text::time::parse_iso_epoch),
                    scope: window.scope.clone(),
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    let selected = OrderDraft::from_state(settings, providers)
        .ordered_selection(&selected_window_keys(settings, providers))
        .into_iter()
        .filter_map(|key| {
            key.split_once(':').map(|(provider, window_id)| StatusSelectedWindow {
                provider: provider.into(),
                window_id: window_id.into(),
            })
        })
        .collect::<Vec<_>>();
    let projected = status_entries(&source, &selected, &settings.short_labels, now);
    let source = projected
        .iter()
        .map(|entry| StatusSelectionEntry {
            provider: &entry.provider,
            used: entry.used,
            restricted: entry.restricted,
        })
        .collect::<Vec<_>>();
    let selected = select_status(
        &source,
        StatusSelectionMode::All,
        settings.hide_zero,
        settings.order == "usage",
    );
    let mut projected = projected.into_iter().map(Some).collect::<Vec<_>>();
    let entries = selected
        .indices
        .into_iter()
        .map(|index| projected[index].take().expect("selected status entry exists"))
        .collect::<Vec<_>>();
    let source = entries
        .iter()
        .map(|entry| StatusSelectionEntry {
            provider: &entry.provider,
            used: entry.used,
            restricted: entry.restricted,
        })
        .collect::<Vec<_>>();
    StatusProjection {
        provider_indices: select_status(&source, StatusSelectionMode::WorstPerProvider, false, false).indices,
        mini_groups: select_status(&source, StatusSelectionMode::MiniBars, false, false).groups,
        entries,
    }
}

#[cfg(test)]
#[path = "../tests/unit/status_projection/tests.rs"]
mod tests;
