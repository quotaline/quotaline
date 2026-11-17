//! Converts Codex usage into shared provider state.

use super::{LimitGroup, windows};
use crate::codex_usage::{self, Usage};
use jiff::Timestamp;
use jiff::tz::Offset;
use quotaline_quota::{
    CreditAllowance, CreditExpiry, Provider, ResetCreditDetail, ResetCredits, SpendControl, UsageCredits, Window,
    default_reset_credit_explanation,
};
use quotaline_text::text::humanize;

fn provider_windows(groups: &[LimitGroup], now: f64) -> Vec<Window> {
    windows(groups, now)
        .into_iter()
        .map(|window| {
            let mut row = Window::new(
                window.id,
                window.label,
                window.used_percent,
                window
                    .reset_at
                    .and_then(|value| Timestamp::from_second(value as i64).ok())
                    .map(|date| date.display_with_offset(Offset::UTC).to_string()),
                window.group,
            );
            row.duration_seconds = Some(window.duration_seconds.max(1.0) as u64);
            row.severity = Some(window.severity.into());
            row.scope = window.scope;
            row
        })
        .collect()
}

impl Usage {
    pub fn into_provider(self, reset_credits: Option<codex_usage::ResetCredits>, source: String, now: f64) -> Provider {
        let mut provider = Provider::connected(
            "codex",
            codex_usage::plan_name(self.plan_type.as_deref()),
            source,
            self.email.unwrap_or_default(),
            provider_windows(&self.groups, now),
        );
        provider.tier = self.plan_type;
        if self.dropped_additional_count > 0 {
            provider.warnings.push(format!(
                "Skipped {} unreadable Codex model limits. Other reported quotas remain visible.",
                self.dropped_additional_count
            ));
        }
        if let Some(credits) = self.credits {
            provider.credit_balance = amount(credits.balance.as_deref());
            provider.usage_credits = Some(UsageCredits {
                unlimited: credits.unlimited,
                has_credits: credits.has_credits,
                overage_limit_reached: credits.overage_limit_reached,
                local_messages: credits.local_messages,
                cloud_messages: credits.cloud_messages,
            });
        }
        if let Some(spend) = self.spend {
            provider.spend = Some(SpendControl {
                enabled: Some(true),
                can_toggle: false,
                disabled_reason: None,
                disabled_reason_code: None,
                used: None,
                limit: None,
                used_money: None,
                limit_money: None,
                percent: spend.percent,
                currency: None,
                reset: spend
                    .resets_at
                    .and_then(|value| Timestamp::from_second(value as i64).ok())
                    .map(|date| date.display_with_offset(Offset::UTC).to_string()),
                limit_reached: spend.limit_reached,
                balance: None,
                balance_money: None,
                auto_reload: None,
                can_purchase_credits: false,
                credit_allowance: Some(CreditAllowance {
                    used: amount(spend.used.as_deref()),
                    limit: amount(spend.limit.as_deref()),
                    remaining: amount(spend.remaining.as_deref()),
                    source: spend.source,
                }),
            });
        }
        if let Some(summary) = reset_credits.or(self.reset_credits) {
            provider.reset_credits = Some(ResetCredits {
                available: summary.available,
                applicable: summary.applicable,
                total_earned: summary.total_earned,
                purchase_eligible: summary.purchase_eligible == Some(true),
                details: summary
                    .credits
                    .as_deref()
                    .unwrap_or(&[])
                    .iter()
                    .map(|credit| ResetCreditDetail {
                        id: credit.id.clone(),
                        title: credit.title.clone(),
                        status: credit.status.as_deref().map(humanize),
                        credit_type: credit.credit_type.as_deref().map(humanize),
                        description: credit.description.clone(),
                        granted_at: credit.granted_display.clone(),
                        expires_at: credit.expires_display.clone(),
                        explanation: default_reset_credit_explanation(),
                    })
                    .collect(),
            });
            for credit in summary.credits.into_iter().flatten() {
                if let Some(at) = credit.usable_expiry {
                    provider.reset_credit_expiries.push(CreditExpiry {
                        id: credit.id,
                        at: at as i64,
                    });
                }
            }
        }
        provider.notices = self.notices;
        provider
    }
}

fn amount(value: Option<&str>) -> Option<f64> {
    value
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite())
}

#[cfg(test)]
#[path = "../tests/unit/presentation/tests.rs"]
mod tests;
