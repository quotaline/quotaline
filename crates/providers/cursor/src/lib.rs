//! Projects Cursor plan and team usage.

use quotaline_text::json::{number, text, timestamp};
use quotaline_text::text::humanize;

use jiff::Timestamp;
use quotaline_quota::{Detail, MoneyAmount, Notice, NoticeKind, Provider, SpendControl, Window};
use serde_json::{Value, json};

pub mod fetch;
pub mod identity;
pub mod request;
mod response;
pub use response::UsageResponse;

pub fn period_summary(period: &Value) -> Value {
    json!({
        "billingCycleStart": period["billingCycleStart"],
        "billingCycleEnd": period["billingCycleEnd"],
        "individualUsage": {"plan": period["planUsage"]},
    })
}

pub fn profile_email(profile: &Value) -> Option<&str> {
    text(&profile["email"])
}

pub fn project(
    summary: &Value,
    membership: Option<&str>,
    cached_email: Option<&str>,
    profile_email: Option<&str>,
    period_message: Option<&str>,
    source: String,
) -> Provider {
    let reset_at = timestamp(&summary["billingCycleEnd"]);
    let reset = reset_at.and_then(reset_iso);
    let duration_seconds = timestamp(&summary["billingCycleStart"])
        .zip(reset_at)
        .map(|(start, end)| end - start);
    let buckets = [
        (
            "plan",
            "Plan usage",
            "Included plan",
            &summary["individualUsage"]["plan"],
        ),
        (
            "on_demand",
            "On-demand",
            "Individual on-demand",
            &summary["individualUsage"]["onDemand"],
        ),
        ("overall", "Overall", "Overall", &summary["individualUsage"]["overall"]),
        ("team_pool", "Team pool", "Team pool", &summary["teamUsage"]["pooled"]),
        (
            "team_on_demand",
            "Team on-demand",
            "Team on-demand",
            &summary["teamUsage"]["onDemand"],
        ),
    ];
    let mut windows = Vec::new();
    let mut details = Vec::new();
    for (id, label, detail_title, bucket) in buckets {
        if !bucket.is_object() {
            continue;
        }
        let components = [
            ("auto", "Auto", number(&bucket["autoPercentUsed"])),
            ("api", "API", number(&bucket["apiPercentUsed"])),
        ];
        if bucket["enabled"].as_bool() != Some(false)
            && (id != "on_demand" || number(&bucket["limit"]).is_some_and(|limit| limit > 0.0))
        {
            if let Some(percent) = number(&bucket["totalPercentUsed"]).or_else(|| {
                Some(number(&bucket["used"])? / number(&bucket["limit"]).filter(|limit| *limit > 0.0)? * 100.0)
            }) {
                let detail = components
                    .iter()
                    .filter_map(|(_, title, percent)| {
                        percent.map(|percent| format!("{title} {:.0}%", percent.clamp(0.0, 100.0)))
                    })
                    .collect::<Vec<_>>()
                    .join(" · ");
                let mut row = window(id.into(), label.into(), percent, reset.clone(), duration_seconds);
                row.detail = detail;
                windows.push(row);
            } else {
                for (suffix, title, percent) in components {
                    if let Some(percent) = percent {
                        windows.push(window(
                            format!("{id}:{suffix}"),
                            format!("{label} ({title})"),
                            percent,
                            reset.clone(),
                            duration_seconds,
                        ));
                    }
                }
            }
        }
        if let Some(detail) = bucket_detail(id, detail_title, bucket) {
            details.push(detail);
        }
    }
    for (id, title, field) in [
        ("limit-type", "Limit type", "limitType"),
        ("auto-pool", "Auto model allowance", "autoModelSelectedDisplayMessage"),
        (
            "named-pool",
            "Named model allowance",
            "namedModelSelectedDisplayMessage",
        ),
    ] {
        if let Some(value) = text(&summary[field]) {
            details.push(Detail {
                id: id.into(),
                title: title.into(),
                value: value.into(),
                explanation: "The provider's description of this allowance. Model pools may have different limits."
                    .into(),
            });
        }
    }
    if let (Some(pool), Some(on_demand)) = (
        details.iter().position(|detail| detail.id == "team-pool"),
        details.iter().position(|detail| detail.id == "team-on-demand"),
    ) {
        details.swap(pool, on_demand);
    }
    let on_demand = &summary["individualUsage"]["onDemand"];
    let spend = on_demand.is_object().then(|| {
        let used = number(&on_demand["used"]);
        let limit = number(&on_demand["limit"]);
        let used_minor = used.map(|value| value.round() as i64);
        let limit_minor = limit.map(|value| value.round() as i64);
        SpendControl {
            enabled: on_demand["enabled"].as_bool(),
            can_toggle: false,
            disabled_reason: None,
            disabled_reason_code: None,
            used: used_minor.map(|value| value as f64 / 100.0),
            limit: limit_minor.map(|value| value as f64 / 100.0),
            used_money: used_minor.map(dollars),
            limit_money: limit_minor.map(dollars),
            percent: limit
                .zip(used)
                .map(|(limit, used)| if limit > 0.0 { used / limit * 100.0 } else { 0.0 }),
            currency: Some("USD".into()),
            reset: reset.clone(),
            limit_reached: number(&on_demand["remaining"]).unwrap_or(1.0) <= 0.0 && limit.unwrap_or(0.0) > 0.0,
            balance: None,
            balance_money: None,
            auto_reload: None,
            can_purchase_credits: false,
            credit_allowance: None,
        }
    });
    let mut provider = Provider::connected(
        "cursor",
        humanize(
            text(&summary["membershipType"])
                .or(membership.filter(|value| !value.is_empty()))
                .unwrap_or("Cursor"),
        ),
        source,
        profile_email.or(cached_email).unwrap_or_default().into(),
        windows,
    );
    if summary["isUnlimited"].as_bool() == Some(true) {
        provider
            .notices
            .push(Notice::new(NoticeKind::Info, "This plan has unlimited usage."));
    }
    if let Some(message) = period_message.filter(|message| !message.is_empty()) {
        provider.notices.push(Notice::new(NoticeKind::Info, message));
    }
    provider.details = details;
    provider.spend = spend;
    provider
}

fn window(id: String, label: String, used: f64, reset: Option<String>, duration_seconds: Option<f64>) -> Window {
    let mut row = Window::new(id, label, used, reset, "monthly");
    row.duration_seconds = duration_seconds.filter(|value| *value >= 0.0).map(|value| value as u64);
    row
}

fn dollars(amount_minor: i64) -> MoneyAmount {
    MoneyAmount {
        amount_minor,
        currency: "USD".into(),
        exponent: 2,
    }
}

fn reset_iso(value: f64) -> Option<String> {
    Timestamp::from_millisecond((value * 1000.0) as i64)
        .ok()
        .map(|date| date.to_string())
}

fn bucket_detail(id: &str, title: &str, bucket: &Value) -> Option<Detail> {
    let mut values = Vec::new();
    if let Some(enabled) = bucket["enabled"].as_bool() {
        values.push(if enabled { "On".into() } else { "Off".into() });
    }
    for (title, field) in [("Used", "used"), ("Limit", "limit"), ("Remaining", "remaining")] {
        if let Some(value) = number(&bucket[field]) {
            values.push(format!("{title}: {}", usd(value)));
        }
    }
    for (title, field) in [("Included", "included"), ("Bonus", "bonus"), ("Total", "total")] {
        if let Some(value) = number(&bucket["breakdown"][field]) {
            values.push(format!("{title}: {}", usd(value)));
        }
    }
    (!values.is_empty()).then(|| Detail {
        id: match id {
            "on_demand" => "on-demand",
            "team_on_demand" => "team-on-demand",
            "team_pool" => "team-pool",
            other => other,
        }
        .into(),
        title: title.into(),
        value: values.join(" · "),
        explanation: "Provider-reported budget in USD. Individual and team allowances remain separate.".into(),
    })
}

fn usd(minor: f64) -> String {
    format!("${:.2}", minor.round() / 100.0)
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
