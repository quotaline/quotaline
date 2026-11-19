//! Reads the engine's state into the providers and settings the dashboard shows.

use jiff::Timestamp;
use quotaline_quota::{CloudCredits, CreditExpiry, Detail, Notice, ResetCredits, SpendControl, UsageCredits};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};

use crate::{
    desktop::State,
    snapshot::{OverviewFacts, Provider, Window},
};

/// Keeps the providers the usage page lists, in the engine's order, with only their selected windows.
pub(crate) fn project(raw: &str, now: i64) -> io::Result<State> {
    let state = serde_json::from_str::<ServiceState>(raw)?;
    let mut providers = state.providers;
    Ok(State {
        providers: state
            .ordering
            .providers
            .into_iter()
            .filter_map(|id| {
                let provider = providers.remove(&id)?;
                provider.show_in_usage.then(|| {
                    let selected = |window: &str| state.selected_window_keys.contains(&format!("{id}:{window}"));
                    let overview = OverviewFacts {
                        id: &id,
                        status: &provider.status,
                        credit_balance: provider.credit_balance,
                        usage_credits: provider.usage_credits.as_ref(),
                        reset_credits: provider.reset_credits.as_ref(),
                        reset_credit_expiries: &provider.reset_credit_expiries,
                        cloud_credits: provider.cloud_credits.as_ref(),
                        spend: provider.spend.as_ref(),
                        details: &provider.details,
                        notices: &provider.notices,
                        warnings: &provider.warnings,
                    }
                    .project(selected, now);
                    Provider {
                        plan: (!provider.plan.is_empty()).then_some(provider.plan),
                        fetched_at: provider.updated.and_then(|at| Timestamp::from_second(at).ok()),
                        status: provider.status,
                        overview,
                        details: provider
                            .details
                            .into_iter()
                            .map(|detail| format!("{}  {}", detail.title, detail.value))
                            .chain(provider.notices.into_iter().map(|notice| notice.text))
                            .chain(provider.warnings)
                            .chain(provider.organization.filter(|_| provider.show_organization))
                            .chain((!provider.identity.is_empty()).then_some(provider.identity))
                            .collect(),
                        windows: provider
                            .windows
                            .into_iter()
                            .filter(|window| selected(&window.id))
                            .map(|window| Window {
                                label: window.label,
                                used_percent: window.used,
                                resets_at: window.reset.and_then(|reset| reset.parse().ok()),
                                reset_by_day: window.reset_precision == "day",
                                pace: window.pace.and_then(|pace| {
                                    pace.expected_percent.map(|percent| {
                                        format!("Expected {percent:.0}% · {}", pace.status.replace('_', " "))
                                    })
                                }),
                            })
                            .collect(),
                        id,
                    }
                })
            })
            .collect(),
        settings: state.settings,
    })
}

#[derive(Deserialize)]
struct ServiceState {
    providers: BTreeMap<String, ServiceProvider>,
    ordering: Order,
    selected_window_keys: BTreeSet<String>,
    settings: Value,
}

#[derive(Deserialize)]
struct Order {
    providers: Vec<String>,
}

#[derive(Deserialize)]
struct ServiceProvider {
    plan: String,
    updated: Option<i64>,
    status: String,
    windows: Vec<ServiceWindow>,
    show_in_usage: bool,
    details: Vec<Detail>,
    notices: Vec<Notice>,
    warnings: Vec<String>,
    organization: Option<String>,
    show_organization: bool,
    identity: String,
    credit_balance: Option<f64>,
    usage_credits: Option<UsageCredits>,
    reset_credits: Option<ResetCredits>,
    #[serde(default)]
    reset_credit_expiries: Vec<CreditExpiry>,
    cloud_credits: Option<CloudCredits>,
    spend: Option<SpendControl>,
}

#[derive(Deserialize)]
struct ServiceWindow {
    id: String,
    label: String,
    used: Option<f64>,
    reset: Option<String>,
    #[serde(default)]
    reset_precision: String,
    pace: Option<Pace>,
}

#[derive(Deserialize)]
struct Pace {
    status: String,
    expected_percent: Option<f64>,
}

#[cfg(test)]
#[path = "../../tests/unit/desktop/state.rs"]
mod tests;
