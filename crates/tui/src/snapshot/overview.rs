//! Projects provider-reported numbers for the Usage overview.

use quotaline_catalog::COPILOT;
use quotaline_quota::{
    CloudCredits, CreditExpiry, Detail, MoneyAmount, Notice, NoticeKind, Provider, ResetCredits, SpendControl,
    UsageCredits,
    usage::{
        CloudCreditFacts, CloudCreditStatus, CreditMetricKind, SpendAmount, SpendFacts, cloud_credit_projection,
        credit_metrics, spend_projection,
    },
};
use quotaline_text::{
    text::decimal,
    time::{countdown, relative_age},
};
use std::collections::BTreeSet;

pub(crate) fn provider_overview(provider: &Provider, selected: impl Fn(&str) -> bool, now: i64) -> Vec<String> {
    OverviewFacts {
        id: &provider.id,
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
    .project(selected, now)
}

pub(crate) struct OverviewFacts<'a> {
    pub(crate) id: &'a str,
    pub(crate) status: &'a str,
    pub(crate) credit_balance: Option<f64>,
    pub(crate) usage_credits: Option<&'a UsageCredits>,
    pub(crate) reset_credits: Option<&'a ResetCredits>,
    pub(crate) reset_credit_expiries: &'a [CreditExpiry],
    pub(crate) cloud_credits: Option<&'a CloudCredits>,
    pub(crate) spend: Option<&'a SpendControl>,
    pub(crate) details: &'a [Detail],
    pub(crate) notices: &'a [Notice],
    pub(crate) warnings: &'a [String],
}

impl OverviewFacts<'_> {
    pub(crate) fn project(&self, selected: impl Fn(&str) -> bool, now: i64) -> Vec<String> {
        let mut facts = self.credit_facts(now);
        facts.extend(self.cloud_credits.map(|credits| cloud_fact(credits, now)));
        facts.extend(self.spend.into_iter().flat_map(spend_facts));
        if self.id == COPILOT.id {
            facts.extend(
                self.details
                    .iter()
                    .filter_map(|detail| copilot_quota(detail, &selected)),
            );
        }
        facts.extend((self.status == "stale").then(|| "Status · Cached reading".into()));
        let messages: BTreeSet<_> = self
            .notices
            .iter()
            .filter_map(|notice| match (&notice.kind, notice.text.as_str()) {
                (NoticeKind::LimitReached, "Usage limit reached.") => Some("Usage limit reached."),
                (NoticeKind::LimitReached, _) => Some("Quota limit reached"),
                (NoticeKind::Promotion, _) => Some("Quota promotion · See details"),
                (NoticeKind::SpendControl, "Workspace spend limit reached.") if self.spend.is_none() => {
                    Some("Workspace spend limit reached.")
                }
                (NoticeKind::SpendControl, "Credit overage limit reached.") if self.usage_credits.is_none() => {
                    Some("Credit overage limit reached.")
                }
                _ => None,
            })
            .chain(self.warnings.iter().map(|warning| {
                if warning.starts_with("Plan details unavailable: ") {
                    "Plan details unavailable"
                } else {
                    "Provider warnings · See details"
                }
            }))
            .collect();
        facts.extend(messages.into_iter().map(str::to_owned));
        facts
    }

    fn credit_facts(&self, now: i64) -> Vec<String> {
        let balance = self.credit_balance.map(|amount| money(amount, "USD"));
        let expiries = self
            .reset_credit_expiries
            .iter()
            .map(|expiry| expiry.at)
            .collect::<Vec<_>>();
        let mut facts = Vec::new();
        for metric in credit_metrics(self.usage_credits, self.reset_credits, &expiries) {
            facts.extend(match metric.kind {
                CreditMetricKind::Credits => self.usage_credits.map(|usage| {
                    let value = if usage.unlimited {
                        "Unlimited"
                    } else {
                        balance.as_deref().unwrap_or("Balance not reported")
                    };
                    format!("Credits · {value}")
                }),
                CreditMetricKind::Overage => Some("Credit overage limit reached".into()),
                CreditMetricKind::LimitResets => self.reset_credits.map(|credits| {
                    format!(
                        "Limit resets · {} available ({} applicable now)",
                        credits.available, credits.applicable
                    )
                }),
                CreditMetricKind::NextExpiry => metric.epoch.map(|seconds| {
                    let until = seconds.saturating_sub(now) as f64;
                    if until <= 0.0 {
                        format!("Reset credit expired {}", relative_age(-until))
                    } else {
                        format!("Next reset credit expires in {}", countdown(until))
                    }
                }),
                _ => None,
            });
        }
        if self.usage_credits.is_none() && self.spend.and_then(spend_balance).is_none() {
            facts.extend(balance.map(|amount| format!("Credits · {amount} remaining")));
        }
        facts
    }
}

fn cloud_fact(credits: &CloudCredits, now: i64) -> String {
    let status = cloud_credit_projection(CloudCreditFacts::from_credits(credits), now).status;
    let suffix = match status {
        CloudCreditStatus::Active => "",
        CloudCreditStatus::Locked => " · Unavailable",
        CloudCreditStatus::Expired => " · Expired",
    };
    format!(
        "Cloud session credits · {} of {} remaining{suffix}",
        money(credits.remaining, "USD"),
        money(credits.limit, "USD"),
    )
}

fn spend_facts(spend: &SpendControl) -> Vec<String> {
    let currency = spend.currency.as_deref();
    let used_cash = spend_money(spend.used_money.as_ref(), spend.used, currency);
    let limit_cash = spend_money(spend.limit_money.as_ref(), spend.limit, currency);
    let projection = spend_projection(SpendFacts {
        used_money: used_cash.is_some(),
        limit_money: limit_cash.is_some(),
        ..SpendFacts::from_spend(spend)
    });
    let allowance = spend.credit_allowance.as_ref();
    let format_amount = |kind, cash, credits: Option<f64>| match kind {
        Some(SpendAmount::Credits) => credits.map(|amount| format!("{} credits", decimal(amount))),
        Some(SpendAmount::Money) => cash,
        _ => None,
    };
    let used = format_amount(
        projection.used,
        used_cash,
        allowance.and_then(|allowance| allowance.used),
    );
    let limit = format_amount(
        projection.limit,
        limit_cash,
        allowance.and_then(|allowance| allowance.limit),
    );
    let summary = match (used, limit) {
        (Some(used), Some(limit)) => Some(format!("Spend control · {used} of {limit}")),
        (Some(used), None) => Some(format!("Spend used · {used}")),
        (None, Some(limit)) => Some(format!("Spend limit · {limit}")),
        (None, None) => projection
            .percent
            .map(|value| format!("Spend control · {value:.1}% used")),
    };
    let facts = [
        summary,
        allowance
            .and_then(|allowance| allowance.remaining)
            .map(|remaining| format!("Spend allowance · {} credits remaining", decimal(remaining))),
        spend_balance(spend).map(|balance| format!("Spend credit balance · {balance}")),
        spend.limit_reached.then(|| "Spend limit reached".into()),
        (spend.enabled == Some(false)).then(|| "Spend control · Off".into()),
    ];
    facts.into_iter().flatten().collect()
}

fn spend_balance(spend: &SpendControl) -> Option<String> {
    spend_money(spend.balance_money.as_ref(), spend.balance, spend.currency.as_deref())
}

fn spend_money(exact: Option<&MoneyAmount>, amount: Option<f64>, currency: Option<&str>) -> Option<String> {
    exact.map_or_else(
        || amount.zip(currency).map(|(amount, currency)| money(amount, currency)),
        |exact| {
            Some(format_money(
                exact.value(),
                &exact.currency,
                exact.exponent.clamp(0, 6) as usize,
            ))
        },
    )
}

fn money(amount: f64, currency: &str) -> String {
    format_money(amount, currency, 2)
}

fn format_money(amount: f64, currency: &str, precision: usize) -> String {
    if currency == "USD" {
        format!("${amount:.precision$}")
    } else {
        format!("{amount:.precision$} {currency}")
    }
}

fn copilot_quota(detail: &Detail, selected: &impl Fn(&str) -> bool) -> Option<String> {
    detail.id.strip_prefix("quota:").filter(|id| selected(id))?;
    let segments = detail.value.split(" · ").collect::<Vec<_>>();
    let values = ["Remaining: ", "Included: ", "Credits used: "]
        .iter()
        .filter_map(|prefix| segments.iter().copied().find(|segment| segment.starts_with(prefix)))
        .collect::<Vec<_>>();
    if segments.contains(&"Unlimited") {
        return Some(format!("{} · Unlimited", detail.title));
    }
    (!values.is_empty()).then(|| format!("{} · {}", detail.title, values.join(" · ")))
}

#[cfg(test)]
#[path = "../../tests/unit/overview.rs"]
mod tests;
