//! Projects GitHub Copilot quota responses.

use quotaline_text::json::{number, text};
use quotaline_text::text::{decimal, humanize};

use jiff::Timestamp;
use jiff::civil::Date;
use jiff::tz::Offset;
use serde_json::Value;

use quotaline_quota::{Detail, Notice, NoticeKind, Provider, ResetPrecision, Window};

pub mod fetch;
pub mod request;

pub fn project(data: &Value, source: String, identity: String) -> Provider {
    let credits = data["token_based_billing"].as_bool() == Some(true);
    let cycle_reset = [
        &data["quota_reset_date_utc"],
        &data["quota_reset_date"],
        &data["limited_user_reset_date"],
    ]
    .into_iter()
    .find_map(reset);
    let mut windows = Vec::new();
    let mut details = Vec::new();
    let mut notices = Vec::new();
    if let Some(snapshots) = data["quota_snapshots"].as_object() {
        for (key, snapshot) in ["premium_interactions", "chat", "completions"]
            .into_iter()
            .filter_map(|key| snapshots.get_key_value(key))
            .chain(
                snapshots
                    .iter()
                    .filter(|(key, _)| !["premium_interactions", "chat", "completions"].contains(&key.as_str())),
            )
        {
            let title = label(key, credits);
            if let Some(detail) = detail(key, &title, snapshot, credits) {
                details.push(detail);
            }
            if let Some(percent) = percent_used(snapshot) {
                if percent > 100.0 && ["premium_interactions", "chat", "completions"].contains(&key.as_str()) {
                    notices.push(Notice::new(
                        if snapshot["overage_permitted"].as_bool() == Some(true) {
                            NoticeKind::Info
                        } else {
                            NoticeKind::LimitReached
                        },
                        format!(
                            "{title}: quota exceeded, {} overage {}.",
                            decimal(number(&snapshot["overage_count"]).unwrap_or(0.0)),
                            if credits { "credits" } else { "requests" }
                        ),
                    ));
                }
                windows.push(window(
                    key.clone(),
                    title,
                    percent,
                    reset(&snapshot["quota_reset_at"]).or(cycle_reset),
                ));
            }
        }
    }
    if let (Some(limited), Some(monthly)) = (
        data["limited_user_quotas"].as_object(),
        data["monthly_quotas"].as_object(),
    ) {
        for (key, remaining) in limited {
            if windows.iter().any(|window| window.id == *key) {
                continue;
            }
            if let (Some(remaining), Some(limit)) = (number(remaining), monthly.get(key).and_then(number))
                && limit > 0.0
            {
                windows.push(window(
                    format!("free:{key}"),
                    label(key, false),
                    (1.0 - remaining / limit) * 100.0,
                    reset(&data["limited_user_reset_date"]).or(cycle_reset),
                ));
            }
        }
    }
    details.sort_by(|left, right| left.title.cmp(&right.title));
    let mut provider = Provider::connected("copilot", plan_name(data), source, identity, windows);
    provider.tier = text(&data["access_type_sku"]).map(str::to_owned);
    provider.details = details;
    provider.notices = notices;
    provider
}

fn window(id: String, label: String, used: f64, reset: Option<Reset>) -> Window {
    let mut row = Window::new(
        id,
        label,
        used,
        reset.and_then(|reset| {
            Timestamp::from_millisecond((reset.at * 1000.0) as i64)
                .ok()
                .map(|date| {
                    if reset.precision == ResetPrecision::Day {
                        date.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
                    } else {
                        date.display_with_offset(Offset::UTC).to_string()
                    }
                })
        }),
        "monthly",
    );
    row.reset_precision = reset.map_or(ResetPrecision::Instant, |reset| reset.precision);
    row
}

fn plan_name(data: &Value) -> String {
    let sku = text(&data["copilot_plan"])
        .or_else(|| text(&data["access_type_sku"]))
        .unwrap_or("Copilot");
    let plan = match sku {
        "individual" => "Pro".into(),
        "individual_pro" => "Pro+".into(),
        "individual_max" => "Max".into(),
        "individual_edu" => "Student".into(),
        _ => humanize(sku),
    };
    let organizations = data["organization_login_list"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| text(value))
        .collect::<Vec<_>>();
    if organizations.is_empty() {
        plan
    } else {
        format!("{plan} via {}", organizations.join(", "))
    }
}

fn detail(key: &str, title: &str, snapshot: &Value, credits: bool) -> Option<Detail> {
    let mut values = Vec::new();
    if snapshot["unlimited"].as_bool() == Some(true) {
        values.push("Unlimited".into());
    }
    for (field, title) in [
        ("entitlement", "Included"),
        ("quota_remaining", "Remaining"),
        ("credits_used", "Credits used"),
        ("overage_count", "Overage used"),
        ("overage_entitlement", "Overage allowance"),
    ] {
        if let Some(value) = number(&snapshot[field]) {
            values.push(format!(
                "{title}: {} {}",
                decimal(value),
                if field == "credits_used" || credits {
                    "credits"
                } else {
                    "requests"
                }
            ));
        }
    }
    if let Some(allowed) = snapshot["overage_permitted"].as_bool() {
        values.push(format!("Overage {}", if allowed { "allowed" } else { "off" }));
    }
    (!values.is_empty()).then(|| Detail {
        id: format!("quota:{key}"),
        title: title.into(),
        value: values.join(" · "),
        explanation: format!(
            "Provider-reported category totals. Categories may overlap and are not added together. {}",
            if credits {
                "Token-based credit billing."
            } else {
                "Request-based billing."
            }
        ),
    })
}

fn percent_used(snapshot: &Value) -> Option<f64> {
    if snapshot["unlimited"].as_bool() == Some(true) {
        return None;
    }
    let entitlement = number(&snapshot["entitlement"]);
    if entitlement.is_some_and(|value| value <= 0.0) {
        return None;
    }
    let remaining = number(&snapshot["quota_remaining"]).or_else(|| number(&snapshot["remaining"]));
    entitlement
        .zip(remaining)
        .map(|(limit, remaining)| (limit - remaining) * 100.0 / limit)
        .or_else(|| number(&snapshot["percent_remaining"]).map(|remaining| 100.0 - remaining))
}

fn label(key: &str, credits: bool) -> String {
    if key == "premium_interactions" {
        if credits { "Premium credits" } else { "Premium requests" }.into()
    } else {
        humanize(key)
    }
}

#[derive(Clone, Copy)]
struct Reset {
    at: f64,
    precision: ResetPrecision,
}

fn reset(value: &Value) -> Option<Reset> {
    if let Some(at) = number(value) {
        return Some(Reset {
            at,
            precision: ResetPrecision::Instant,
        });
    }
    let stamp = text(value)?;
    if let Ok(date) = stamp.parse::<Timestamp>() {
        return Some(Reset {
            at: date.as_millisecond() as f64 / 1000.0,
            precision: ResetPrecision::Instant,
        });
    }
    let date = Date::strptime("%Y-%m-%d", stamp).ok()?;
    Some(Reset {
        at: Offset::UTC.to_timestamp(date.into()).ok()?.as_second() as f64,
        precision: ResetPrecision::Day,
    })
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
