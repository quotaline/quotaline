//! Projects Codex quota and credit responses.

use quotaline_quota::{Notice, NoticeKind};
use quotaline_text::json::{number, text, timestamp};
use quotaline_text::text::humanize;

use jiff::Timestamp;
use serde_json::{Value, json};

use crate::{LimitGroup, LimitKind, LimitWindow};

#[derive(Clone, Debug, PartialEq)]
pub struct Credits {
    pub balance: Option<String>,
    pub unlimited: bool,
    pub has_credits: Option<bool>,
    pub overage_limit_reached: bool,
    pub local_messages: Option<(i64, i64)>,
    pub cloud_messages: Option<(i64, i64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Spend {
    pub percent: Option<f64>,
    pub resets_at: Option<f64>,
    pub limit_reached: bool,
    pub used: Option<String>,
    pub limit: Option<String>,
    pub remaining: Option<String>,
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResetCredit {
    pub id: String,
    pub title: String,
    pub status: Option<String>,
    pub credit_type: Option<String>,
    pub description: Option<String>,
    pub granted_at: Option<f64>,
    pub expires_at: Option<f64>,
    pub granted_display: Option<String>,
    pub expires_display: Option<String>,
    pub usable_expiry: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResetCredits {
    pub available: u64,
    pub applicable: u64,
    pub total_earned: Option<u64>,
    pub purchase_eligible: Option<bool>,
    pub credits: Option<Vec<ResetCredit>>,
}

impl ResetCredits {
    pub fn complete(&self) -> bool {
        self.total_earned.is_some() && self.purchase_eligible.is_some()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Usage {
    pub plan_type: Option<String>,
    pub email: Option<String>,
    pub groups: Vec<LimitGroup>,
    pub dropped_additional_count: u32,
    pub credits: Option<Credits>,
    pub spend: Option<Spend>,
    pub notices: Vec<Notice>,
    pub reset_credits: Option<ResetCredits>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rollout {
    pub data: Value,
    pub observed_at: Option<f64>,
}

pub fn rollout(document: &Value) -> Option<Rollout> {
    let limits = find_rate_limits(document)?;
    let mut rate_limit = serde_json::Map::new();
    for (source, target) in [("primary", "primary_window"), ("secondary", "secondary_window")] {
        if let Some(window) = rollout_window(&limits[source]) {
            rate_limit.insert(target.into(), window);
        }
    }
    rate_limit.insert(
        "limit_reached".into(),
        Value::Bool(!limits["rate_limit_reached_type"].is_null()),
    );
    Some(Rollout {
        data: json!({
            "rate_limit": rate_limit,
            "rate_limit_reached_type": limits["rate_limit_reached_type"],
            "plan_type": limits["plan_type"],
            "credits": limits["credits"],
        }),
        observed_at: timestamp(&document["timestamp"]),
    })
}

fn find_rate_limits(value: &Value) -> Option<&Value> {
    match value {
        Value::Object(object) => object
            .get("rate_limits")
            .filter(|limits| limits.is_object())
            .or_else(|| object.values().find_map(find_rate_limits)),
        Value::Array(values) => values.iter().find_map(find_rate_limits),
        _ => None,
    }
}

fn rollout_window(value: &Value) -> Option<Value> {
    let used = number(&value["used_percent"])?;
    let seconds = number(&value["window_minutes"])
        .map(|minutes| minutes * 60.0)
        .or_else(|| number(&value["limit_window_seconds"]));
    Some(json!({
        "used_percent": used,
        "limit_window_seconds": seconds,
        "reset_after_seconds": number(&value["resets_in_seconds"]),
        "reset_at": number(&value["resets_at"]).or_else(|| number(&value["reset_at"])),
    }))
}

pub fn plan_name(plan: Option<&str>) -> String {
    match plan.unwrap_or("").to_ascii_lowercase().as_str() {
        "" => "ChatGPT".into(),
        "pro" => "Pro (More)".into(),
        "prolite" => "Pro".into(),
        "promax" => "Pro (Max)".into(),
        "plus" => "Plus".into(),
        "go" => "Go".into(),
        "free" => "Free".into(),
        "team" | "free_workspace" => "Team".into(),
        "business" | "self_serve_business_prolite" => "Business".into(),
        "enterprise" => "Enterprise".into(),
        "edu" | "education" => "Education".into(),
        "k12" => "K12".into(),
        "quorum_plus" => "Quorum Plus".into(),
        other => humanize(other),
    }
}

pub fn usage(data: &Value, now: f64) -> Usage {
    let mut groups = vec![group(&data["rate_limit"], LimitKind::Usage)];
    groups.push(group(&data["code_review_rate_limit"], LimitKind::CodeReview));
    let mut dropped_additional_count = 0;
    if let Some(additional) = data["additional_rate_limits"].as_array() {
        for entry in additional {
            if entry.is_object() {
                let name = text(&entry["limit_name"])
                    .or_else(|| text(&entry["name"]))
                    .unwrap_or("Additional");
                groups.push(group(&entry["rate_limit"], LimitKind::Additional(name.into())));
            } else {
                dropped_additional_count += 1;
            }
        }
    } else if !data["additional_rate_limits"].is_null() {
        dropped_additional_count = 1;
    }
    let credit_data = &data["credits"];
    let credits = credit_data.is_object().then(|| Credits {
        balance: amount(&credit_data["balance"]),
        unlimited: credit_data["unlimited"].as_bool() == Some(true),
        has_credits: credit_data["has_credits"].as_bool(),
        overage_limit_reached: credit_data["overage_limit_reached"].as_bool() == Some(true),
        local_messages: range(&credit_data["approx_local_messages"]),
        cloud_messages: range(&credit_data["approx_cloud_messages"]),
    });
    let limit = &data["spend_control"]["individual_limit"];
    let spend = limit.is_object().then(|| Spend {
        percent: number(&limit["used_percent"])
            .or_else(|| number(&limit["remaining_percent"]).map(|remaining| 100.0 - remaining)),
        resets_at: timestamp(&limit["reset_at"])
            .or_else(|| number(&limit["reset_after_seconds"]).map(|seconds| now + seconds)),
        limit_reached: data["spend_control"]["reached"].as_bool() == Some(true),
        used: amount(&limit["used"]),
        limit: amount(&limit["limit"]),
        remaining: amount(&limit["remaining"]),
        source: text(&limit["source"]).map(str::to_owned),
    });
    let mut notices = Vec::new();
    let reached = &data["rate_limit_reached_type"];
    if !reached.is_null() {
        let kind = text(&reached["type"])
            .or_else(|| text(reached))
            .map(str::to_owned)
            .unwrap_or_else(|| summary(reached));
        notices.push(Notice::new(
            NoticeKind::LimitReached,
            format!("Limit reached: {}.", humanize(&kind)),
        ));
    } else if data["rate_limit"]["limit_reached"].as_bool() == Some(true) {
        notices.push(Notice::new(NoticeKind::LimitReached, "Usage limit reached."));
    }
    if data["spend_control"]["reached"].as_bool() == Some(true) {
        notices.push(Notice::new(NoticeKind::SpendControl, "Workspace spend limit reached."));
    }
    let promo = &data["promo"];
    if !promo.is_null() {
        notices.push(Notice::new(
            NoticeKind::Promotion,
            text(&promo["text"])
                .or_else(|| text(&promo["title"]))
                .map(str::to_owned)
                .unwrap_or_else(|| summary(promo)),
        ));
    }
    let upsell = &data["rate_limit_upsell"];
    if let Some(message) = text(&upsell["text"]).or_else(|| text(&upsell["title"])) {
        notices.push(Notice::new(NoticeKind::Info, message));
    }
    if credits.as_ref().is_some_and(|credits| credits.overage_limit_reached) {
        notices.push(Notice::new(NoticeKind::SpendControl, "Credit overage limit reached."));
    }
    Usage {
        plan_type: data["plan_type"].as_str().map(str::to_owned),
        email: data["email"].as_str().map(str::to_owned),
        groups,
        dropped_additional_count,
        credits,
        spend,
        notices,
        reset_credits: reset_credits(&data["rate_limit_reset_credits"]),
    }
}

pub fn reset_credits(data: &Value) -> Option<ResetCredits> {
    data.as_object().map(|_| {
        let available = data["available_count"].as_u64().unwrap_or(0);
        ResetCredits {
            available,
            applicable: data["applicable_available_count"].as_u64().unwrap_or(available),
            total_earned: data["total_earned_count"].as_u64(),
            purchase_eligible: data["immediate_reset_purchase_eligible"].as_bool(),
            credits: data["credits"].as_array().map(|entries| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(index, entry)| ResetCredit {
                        id: text(&entry["id"])
                            .or_else(|| text(&entry["credit_id"]))
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("credit-{index}")),
                        title: text(&entry["title"]).unwrap_or("Reset credit").into(),
                        status: text(&entry["status"]).map(str::to_owned),
                        credit_type: text(&entry["reset_type"]).map(str::to_owned),
                        description: text(&entry["description"]).map(str::to_owned),
                        granted_at: timestamp(&entry["granted_at"]),
                        expires_at: timestamp(&entry["expires_at"]),
                        granted_display: display_timestamp(&entry["granted_at"]),
                        expires_display: display_timestamp(&entry["expires_at"]),
                        usable_expiry: text(&entry["id"])
                            .filter(|_| text(&entry["status"]).is_none_or(|status| status == "available"))
                            .and_then(|_| timestamp(&entry["expires_at"])),
                    })
                    .collect()
            }),
        }
    })
}

fn group(data: &Value, kind: LimitKind) -> LimitGroup {
    LimitGroup {
        kind,
        limit_reached: data["limit_reached"].as_bool() == Some(true),
        primary: window(&data["primary_window"]),
        secondary: window(&data["secondary_window"]),
    }
}

fn window(data: &Value) -> Option<LimitWindow> {
    data.as_object().and_then(|_| {
        Some(LimitWindow {
            used_percent: number(&data["used_percent"])?,
            duration_seconds: number(&data["limit_window_seconds"]),
            reset_at: number(&data["reset_at"]),
            reset_after_seconds: number(&data["reset_after_seconds"]),
        })
    })
}

fn amount(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_number().map(ToString::to_string))
}

fn range(value: &Value) -> Option<(i64, i64)> {
    let values = value.as_array()?;
    let low = values.first()?.as_i64()?;
    let high = values.last()?.as_i64()?;
    (low <= high).then_some((low, high))
}

fn display_timestamp(value: &Value) -> Option<String> {
    if let Some(seconds) = number(value) {
        return Timestamp::from_second(seconds as i64)
            .ok()
            .map(|date| date.strftime("%Y-%m-%dT%H:%M:%SZ").to_string());
    }
    text(value).map(str::to_owned)
}

fn summary(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values.iter().map(summary).collect::<Vec<_>>().join(", "),
        Value::Object(values) => values
            .iter()
            .map(|(key, value)| format!("{key}: {}", summary(value)))
            .collect::<Vec<_>>()
            .join(", "),
    }
}

#[cfg(test)]
#[path = "../tests/unit/codex_usage/tests.rs"]
mod tests;
