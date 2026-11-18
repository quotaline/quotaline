//! Projects Rust provider state into the GNOME Shell payload.

use std::collections::{BTreeMap, BTreeSet};

use quotaline_analytics as analytics;
use quotaline_catalog::IDS;
use quotaline_quota::Provider;
use quotaline_quota::notices::notice_visible;
use quotaline_quota::pace::PaceEstimate;
use quotaline_quota::reset_groups as quota;
use quotaline_quota::snapshot::snapshot_attention;
use quotaline_settings::Settings;
use quotaline_settings::context as settings_context;
use quotaline_setup::action;
use quotaline_status_bar::OrderDraft;
use quotaline_status_bar::default_selected_windows;
use quotaline_status_bar::selected_window_keys;
use quotaline_status_bar::settings_list_for_providers;
use quotaline_status_bar::short_label_maps;
use serde::Serialize;
use serde_json::json;

use crate::{claude_transcripts, status_projection, store};

pub struct StateInput<'a> {
    pub settings: &'a Settings,
    pub providers: &'a BTreeMap<String, Provider>,
    pub running: &'a BTreeSet<String>,
    pub last_refresh: Option<i64>,
    pub first_run_pending: bool,
    pub claude_local_usage: Option<&'a claude_transcripts::LocalUsage>,
    pub recent_samples: &'a store::RecentSamples,
    pub now: f64,
}

#[derive(Serialize)]
struct ProviderView {
    #[serde(flatten)]
    provider: Provider,
    action: action::ProviderAction,
    discovered: bool,
    show_in_settings: bool,
    show_in_usage: bool,
    show_organization: bool,
    credit_metrics: Vec<quotaline_quota::usage::CreditMetric>,
    spend_projection: Option<quotaline_quota::usage::SpendProjection>,
    cloud_projection: Option<quotaline_quota::usage::CloudCreditProjection>,
    reset_group_edges: Vec<quota::ResetGroupEdge>,
}

fn discovery(id: &str, provider: &Provider, has_claude_local_usage: bool) -> settings_context::Discovery {
    settings_context::Discovery {
        snapshot: provider.updated.is_some() || !provider.windows.is_empty(),
        analytics: id == "claude" && has_claude_local_usage,
        credential_usable: provider.credential_usable,
    }
}

pub fn project(input: StateInput<'_>) -> String {
    let mut providers = BTreeMap::new();
    let mut organization_visibility = BTreeMap::new();
    for id in IDS {
        let mut provider = input.providers.get(id).cloned().unwrap_or_else(|| {
            Provider::empty(
                id,
                if input.running.contains(id) {
                    "checking"
                } else {
                    "sign-in"
                },
            )
        });
        let identity = quotaline_quota::usage::identity_presentation(
            (!provider.identity.is_empty()).then_some(provider.identity.as_str()),
            (!provider.organization.is_empty()).then_some(provider.organization.as_str()),
            input.settings.hide_identity,
        );
        organization_visibility.insert(id, identity.show_organization);
        if input.settings.hide_identity {
            provider.identity = identity.email.unwrap_or_default();
            provider.organization = if identity.show_organization {
                identity.organization.unwrap_or_default()
            } else {
                String::new()
            };
            if provider.source.starts_with('/') || provider.account_selection.is_some() {
                provider.source = "Local credential".into();
            }
            provider.account_selection = None;
        }
        for row in &mut provider.windows {
            let samples: Vec<_> = input
                .recent_samples
                .get(id)
                .into_iter()
                .flatten()
                .filter(|(window, _, _)| window == &row.id)
                .map(|(_, at, used)| (*at, *used))
                .collect();
            let now = input.now as i64;
            let pace = PaceEstimate::estimate(row, &samples, input.settings.workdays, now);
            row.pace = Some(pace);
        }
        providers.insert(id.to_string(), provider);
    }
    let first_run_pending =
        input.first_run_pending && input.running.is_empty() && !input.settings.first_run_card_dismissed;
    let demo_local = input.settings.demo.then(|| {
        let used = providers["claude"].windows[0].used;
        claude_transcripts::LocalUsage {
            window_tokens: (1_840_000.0 * used / 60.0) as i64,
            window_cost: 22.4 * used / 60.0,
            cost_per_hour: 8.1,
            today_tokens: 6_200_000,
            today_cost: 71.3,
            today_messages: 214,
        }
    });
    let default_selected = default_selected_windows(&providers);
    let selected_windows: BTreeSet<_> = selected_window_keys(input.settings, &providers).into_iter().collect();
    let view_providers: BTreeMap<_, _> = providers
        .iter()
        .map(|(id, provider)| {
            let facts = discovery(id, provider, demo_local.as_ref().or(input.claude_local_usage).is_some());
            let full_windows = provider
                .windows
                .iter()
                .filter(|window| window.used >= 100.0 && selected_windows.contains(&format!("{id}:{}", window.id)))
                .map(|window| window.id.clone())
                .collect();
            let mut visible = provider.clone();
            visible.notices.retain(|notice| {
                notice_visible(
                    notice.kind == quotaline_quota::NoticeKind::LimitReached,
                    notice.window_id.as_deref(),
                    &full_windows,
                )
            });
            let reset_group_edges = quota::reset_group_edges(
                &provider
                    .windows
                    .iter()
                    .filter(|window| selected_windows.contains(&format!("{id}:{}", window.id)))
                    .map(|window| quota::ResetGroupInput {
                        reset_at: window.reset.as_deref().and_then(quotaline_text::time::parse_iso_epoch),
                        precision: window.reset_precision,
                    })
                    .collect::<Vec<_>>(),
            );
            let show_in_usage = quotaline_quota::usage::provider_card_visible(
                &visible,
                input.settings.providers.get(id.as_str()) == Some(&true),
                id.as_str() == "claude" && demo_local.as_ref().or(input.claude_local_usage).is_some(),
            );
            (
                id,
                ProviderView {
                    provider: visible,
                    action: action::from_service_status(&provider.status, provider.recovery_status.as_deref()),
                    discovered: settings_context::discovered(facts),
                    show_in_settings: settings_context::visible(facts, input.settings.show_all_providers, false),
                    show_in_usage,
                    show_organization: organization_visibility[id.as_str()],
                    credit_metrics: quotaline_quota::usage::credit_metrics(
                        provider.usage_credits.as_ref(),
                        provider.reset_credits.as_ref(),
                        &provider
                            .reset_credit_expiries
                            .iter()
                            .map(|entry| entry.at)
                            .collect::<Vec<_>>(),
                    ),
                    spend_projection: provider.spend.as_ref().map(|spend| {
                        quotaline_quota::usage::spend_projection(quotaline_quota::usage::SpendFacts::from_spend(spend))
                    }),
                    cloud_projection: provider.cloud_credits.as_ref().map(|credits| {
                        quotaline_quota::usage::cloud_credit_projection(
                            quotaline_quota::usage::CloudCreditFacts::from_credits(credits),
                            input.now as i64,
                        )
                    }),
                    reset_group_edges,
                },
            )
        })
        .collect();
    let (default_short_labels, resolved_short_labels) = short_label_maps(&providers, &input.settings.short_labels);
    let order = OrderDraft::from_state(input.settings, &providers);
    let settings_providers = providers
        .iter()
        .filter(|(id, provider)| {
            settings_context::active(
                discovery(id, provider, demo_local.as_ref().or(input.claude_local_usage).is_some()),
                Some(input.settings.providers.get(*id).copied().unwrap_or(false)),
            )
        })
        .map(|(id, provider)| settings_context::Provider {
            id: id.clone(),
            long_window: provider
                .windows
                .iter()
                .any(|window| window.duration_seconds.is_some_and(|seconds| seconds > 86_400)),
            reset_credits: provider.reset_credits.is_some(),
            local_auth: id == "antigravity" && provider.source.starts_with("Antigravity "),
        })
        .collect::<Vec<_>>();
    let visibility = settings_context::visibility(
        &settings_providers,
        settings_context::Options {
            analytics_interval_changed: input.settings.analytics_interval_minutes
                != quotaline_settings::DEFAULT_ANALYTICS_INTERVAL_MINUTES,
            workdays_changed: input.settings.workdays != quotaline_quota::pace::MAX_WORKDAYS,
            token_refresh_enabled: input.settings.allow_token_refresh,
            expiring_credits_enabled: input.settings.notify_expiring_credits,
            sandboxed: false,
        },
    );
    let all_model_groups = settings_list_for_providers(input.settings, &providers, &BTreeMap::new(), "", None, false);
    let selected_model_count: usize = all_model_groups.iter().map(|group| group.selected_count).sum();
    let status = status_projection::project(&providers, input.settings, input.now);
    let status = json!({
        "version": 1,
        "app_version": env!("CARGO_PKG_VERSION"),
        "provider_definitions": quotaline_catalog::ALL,
        "notification_threshold_choices": quotaline_notifications::THRESHOLD_CHOICES,
        "notification_summary": quotaline_notifications::summary(&quotaline_notifications::Selection {
            enabled: input.settings.notifications,
            thresholds: &input.settings.notify_thresholds,
            resets: input.settings.notify_resets,
            sign_in: input.settings.notify_sign_in,
            pace: input.settings.notify_pace,
            expiring_credits: input.settings.notify_expiring_credits,
        }),
        "maximum_refresh_minutes": quotaline_catalog::MAXIMUM_REFRESH_MINUTES,
        "providers": view_providers,
        "settings": input.settings,
        "settings_context": {"visibility": visibility},
        "ordering": {"providers": order.providers, "models": order.models},
        "model_groups": all_model_groups,
        "selected_model_count": selected_model_count,
        "first_run_pending": first_run_pending,
        "status_entries": status.entries,
        "status_provider_indices": status.provider_indices,
        "status_mini_groups": status.mini_groups,
        "template_tokens": quotaline_status_bar::parse_template(&input.settings.template),
        "snapshot_attention": snapshot_attention(&providers, &input.settings.providers),
        "default_short_labels": default_short_labels,
        "resolved_short_labels": resolved_short_labels,
        "short_label_limit": quotaline_status_bar::SHORT_LABEL_LIMIT,
        "default_selected_windows": default_selected,
        "selected_window_keys": selected_windows,
        "history_path": store::database(),
        "metric_definitions": analytics::metric_definitions(),
        "metric_groups": analytics::METRIC_GROUPS,
        "claude_local_usage": demo_local.as_ref().or(input.claude_local_usage),
        "local_metrics": demo_local.as_ref().or(input.claude_local_usage).map(|local| {
            quotaline_quota::usage::local_metrics(local.window_cost, local.cost_per_hour, local.today_cost)
        }),
        "last_refresh": input.last_refresh,
        "is_refreshing": !input.running.is_empty(),
        "refreshing_providers": input.running,
        "now": input.now as i64,
    });
    status.to_string()
}

#[cfg(test)]
#[path = "../tests/unit/state/tests.rs"]
mod tests;
