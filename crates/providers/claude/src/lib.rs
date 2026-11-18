//! Projects Claude usage and profile responses into shared models.

use quotaline_text::json::{number, text};
use quotaline_text::text::{humanize, slug};

use jiff::Timestamp;
use jiff::tz::Offset;
use quotaline_quota::{CloudCredits, Detail, MoneyAmount, Notice, NoticeKind, Provider, SpendControl, Window};
use serde_json::Value;
use std::collections::HashSet;

pub mod claude_transcripts;
pub mod fetch;
mod profile_cache;
pub mod request;
pub use profile_cache::{PROFILE_FAILURE_TTL, PROFILE_SUCCESS_TTL, ProfileCache, ProfileCacheRead};

pub fn profile_valid(value: &Value) -> bool {
    value["account"].is_object() || value["organization"].is_object()
}

mod validation;

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;

pub struct Input<'a> {
    pub data: &'a Value,
    pub auth: &'a Value,
    pub profile: Option<&'a Value>,
    pub source: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectionError {
    NoReadableQuotas,
}

fn claude_plan_name(auth: &Value, profile: Option<&Value>) -> String {
    let profile = profile.unwrap_or(&Value::Null);
    let base = match text(&profile["organization"]["organization_type"])
        .or_else(|| text(&auth["subscriptionType"]))
        .map(|value| value.trim_start_matches("claude_"))
    {
        Some("max") => "Max".into(),
        Some("pro") => "Pro".into(),
        Some("team") => "Team".into(),
        Some("enterprise") => "Enterprise".into(),
        Some(other) => humanize(other),
        None if profile["account"]["has_claude_max"] == true => "Max".into(),
        None if profile["account"]["has_claude_pro"] == true => "Pro".into(),
        None => "Claude".into(),
    };
    let multiplier = text(&profile["organization"]["rate_limit_tier"])
        .or_else(|| text(&auth["rateLimitTier"]))
        .and_then(|tier| tier.strip_suffix('x'))
        .map(|tier| {
            tier.chars()
                .rev()
                .take_while(|character| character.is_ascii_digit())
                .collect::<String>()
        })
        .filter(|digits| !digits.is_empty())
        .map(|digits| digits.chars().rev().collect::<String>());
    if let Some(digits) = multiplier {
        format!("{base} {digits}x")
    } else {
        base
    }
}
fn iso(value: &Value) -> Option<String> {
    if let Some(number) = number(value) {
        return Timestamp::from_second(number as i64)
            .ok()
            .map(|date| date.display_with_offset(Offset::UTC).to_string());
    }
    text(value).map(str::to_string)
}
fn money(value: &Value, fallback_exponent: i32, fallback_currency: &str) -> Option<MoneyAmount> {
    let amount_minor = value["amount_minor"].as_i64().or_else(|| value.as_i64()).or_else(|| {
        number(&value["amount_minor"])
            .or_else(|| number(value))
            .and_then(|amount| {
                (amount.fract() == 0.0 && amount >= i64::MIN as f64 && amount < i64::MAX as f64)
                    .then_some(amount as i64)
            })
    })?;
    let exponent = value["exponent"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(fallback_exponent);
    (0..=9).contains(&exponent).then(|| MoneyAmount {
        amount_minor,
        currency: text(&value["currency"]).unwrap_or(fallback_currency).into(),
        exponent,
    })
}

pub fn project(
    Input {
        data,
        auth,
        profile,
        source,
    }: Input<'_>,
) -> Result<Provider, ProjectionError> {
    let fields = data.as_object().ok_or(ProjectionError::NoReadableQuotas)?;
    if fields.contains_key("error") {
        return Err(ProjectionError::NoReadableQuotas);
    }
    let (malformed_fields, dropped_limits) = validation::inspect(fields);
    let cloud = &data["iguana_necktie"];
    let cloud_amount = |key| number(&cloud[key]);
    let cloud_credits = cloud_amount("limit_dollars")
        .filter(|limit| *limit > 0.0)
        .map(|limit| CloudCredits {
            limit,
            remaining: cloud_amount("remaining_dollars")
                .or_else(|| cloud_amount("used_dollars").map(|used| limit - used))
                .or_else(|| cloud_amount("utilization").map(|used| limit * (1.0 - used / 100.0)))
                .unwrap_or(limit)
                .clamp(0.0, limit),
            expires_at: iso(&cloud["resets_at"]),
            locked_reason: text(&cloud["locked_reason"]).map(str::to_owned),
        });
    let known = [
        ("five_hour", "session", "Current session", "session"),
        ("seven_day", "weekly", "All models", "weekly"),
        ("seven_day_opus", "weekly:opus", "Opus", "weekly"),
        ("seven_day_sonnet", "weekly:sonnet", "Sonnet", "weekly"),
        ("seven_day_oauth_apps", "weekly:oauth-apps", "OAuth apps", "weekly"),
        ("seven_day_cowork", "weekly:cowork", "Cowork", "weekly"),
    ];
    let mut windows = Vec::new();
    if let Some(limits) = data["limits"].as_array() {
        for limit in limits {
            let (Some(used), Some(kind)) = (number(&limit["percent"]), text(&limit["kind"])) else {
                continue;
            };
            let scope = text(&limit["scope"]["model"]["display_name"]).or_else(|| text(&limit["scope"]["surface"]));
            let (id, label, group) = match kind {
                "session" => ("session".into(), "Current session".into(), "session"),
                "weekly_all" => ("weekly".into(), "All models".into(), "weekly"),
                "weekly_scoped" => (
                    format!("weekly:{}", slug(scope.unwrap_or("scoped"))),
                    scope.unwrap_or("Scoped weekly").into(),
                    "weekly",
                ),
                _ => (
                    scope.map_or_else(|| kind.to_string(), |value| format!("{kind}:{}", slug(value))),
                    scope.map_or_else(|| humanize(kind), |value| format!("{} {value}", humanize(kind))),
                    match text(&limit["group"]) {
                        Some("session") => "session",
                        Some("weekly") => "weekly",
                        Some("monthly") => "monthly",
                        _ => "other",
                    },
                ),
            };
            let mut row = Window::new(id, label, used, iso(&limit["resets_at"]), group);
            row.scope = scope.map(str::to_string);
            row.severity = text(&limit["severity"]).map(str::to_owned);
            row.active = limit["is_active"].as_bool().unwrap_or(true);
            row.duration_seconds = match group {
                "session" => Some(18_000),
                "weekly" => Some(604_800),
                _ => None,
            };
            windows.push(row);
        }
    }
    let structured: HashSet<_> = windows.iter().map(|row| row.id.clone()).collect();
    windows.extend(known.iter().copied().filter_map(|(field, key, label, group)| {
        if structured.contains(key) {
            None
        } else {
            number(&data[field]["utilization"]).map(|used| {
                let mut row = Window::new(key, label, used, iso(&data[field]["resets_at"]), group);
                row.duration_seconds = Some(if group == "session" { 18_000 } else { 604_800 });
                row
            })
        }
    }));
    for (field, value) in fields {
        if known.iter().any(|(key, _, _, _)| key == field)
            || [
                "limits",
                "spend",
                "extra_usage",
                "member_dashboard_available",
                "iguana_necktie",
            ]
            .contains(&field.as_str())
        {
            continue;
        }
        let Some(used) = number(&value["utilization"]) else {
            continue;
        };
        let identity = field
            .strip_prefix("seven_day_")
            .map(|suffix| format!("weekly:{}", slug(suffix)))
            .unwrap_or_else(|| field.clone());
        if structured.contains(&identity) {
            continue;
        }
        let group = if field.starts_with("seven_day") {
            "weekly"
        } else {
            "other"
        };
        let mut row = Window::new(
            field,
            humanize(field.strip_prefix("seven_day_").unwrap_or(field)),
            used,
            iso(&value["resets_at"]),
            group,
        );
        if group == "weekly" {
            row.duration_seconds = Some(604_800);
        }
        windows.push(row);
    }
    let empty_schema = {
        let recognized_null = fields.iter().any(|(field, value)| {
            value.is_null()
                && (field == "limits"
                    || field == "spend"
                    || field == "extra_usage"
                    || known.iter().any(|(key, _, _, _)| key == field))
        });
        let empty_limits = fields
            .get("limits")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty);
        (recognized_null || empty_limits) && malformed_fields.is_empty() && dropped_limits == 0
    };
    if windows.is_empty()
        && cloud_credits.is_none()
        && !data["spend"].is_object()
        && !data["extra_usage"].is_object()
        && !empty_schema
    {
        return Err(ProjectionError::NoReadableQuotas);
    }
    let mut provider = Provider::connected(
        "claude",
        claude_plan_name(auth, profile),
        source,
        profile
            .and_then(|value| text(&value["account"]["email"]))
            .or_else(|| text(&auth["emailAddress"]))
            .unwrap_or("")
            .into(),
        windows,
    );
    provider.tier = profile
        .and_then(|value| text(&value["organization"]["rate_limit_tier"]))
        .or_else(|| text(&auth["rateLimitTier"]))
        .map(str::to_owned);
    if !malformed_fields.is_empty() {
        provider.warnings.push(format!(
            "Claude returned unreadable fields: {}. Other reported values are shown.",
            malformed_fields.join(", ")
        ));
    }
    if dropped_limits > 0 {
        let noun = if dropped_limits == 1 { "limit" } else { "limits" };
        provider
            .warnings
            .push(format!("Skipped {dropped_limits} malformed Claude usage {noun}."));
    }
    provider.cloud_credits = cloud_credits;
    provider.organization = profile
        .and_then(|value| text(&value["organization"]["name"]))
        .unwrap_or("")
        .into();
    if let Some(status) = profile.and_then(|value| text(&value["organization"]["subscription_status"])) {
        provider.details.push(Detail {
            id: "subscription".into(),
            title: "Subscription".into(),
            value: humanize(status),
            explanation: "Subscription status reported by the Claude OAuth profile.".into(),
        });
    }
    let spend = &data["spend"];
    let extra = &data["extra_usage"];
    let currency = text(&spend["used"]["currency"])
        .or_else(|| text(&extra["currency"]))
        .unwrap_or("USD");
    let exponent = extra["decimal_places"]
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .unwrap_or(2);
    let used_money = money(&spend["used"], 2, currency).or_else(|| money(&extra["used_credits"], exponent, currency));
    let limit_money =
        money(&spend["limit"], 2, currency).or_else(|| money(&extra["monthly_limit"], exponent, currency));
    let used = used_money.as_ref().map(MoneyAmount::value);
    let limit = limit_money.as_ref().map(MoneyAmount::value);
    let enabled = spend["enabled"].as_bool().or_else(|| extra["is_enabled"].as_bool());
    let can_purchase_credits = spend["can_purchase_credits"].as_bool() == Some(true);
    let balance_money = money(&spend["balance"], 2, currency);
    let balance = balance_money.as_ref().map(MoneyAmount::value);
    provider.credit_balance = balance;
    let disabled_reason = (enabled != Some(true))
        .then(|| text(&spend["disabled_reason"]).or_else(|| text(&extra["disabled_reason"])))
        .flatten();
    if spend.is_object() || extra.is_object() {
        provider.spend = Some(SpendControl {
            enabled,
            can_toggle: spend["can_toggle"].as_bool() == Some(true),
            disabled_reason: disabled_reason.map(humanize),
            disabled_reason_code: disabled_reason.map(str::to_owned),
            used,
            limit,
            used_money,
            limit_money,
            percent: number(&spend["percent"])
                .or_else(|| number(&extra["utilization"]))
                .or_else(|| {
                    limit.and_then(|limit| used.map(|used| if limit > 0.0 { used / limit * 100.0 } else { 0.0 }))
                }),
            currency: Some(currency.into()),
            reset: iso(&data["credit_reset_at"])
                .or_else(|| iso(&spend["resets_at"]))
                .or_else(|| iso(&extra["resets_at"]))
                .filter(|value| value.parse::<Timestamp>().is_ok()),
            limit_reached: extra["spend_limit_reached"].as_bool() == Some(true),
            balance,
            balance_money,
            auto_reload: spend["auto_reload"]
                .as_bool()
                .or_else(|| spend["auto_reload"]["enabled"].as_bool()),
            can_purchase_credits,
            credit_allowance: None,
        });
    }
    if extra["spend_limit_reached"].as_bool() == Some(true) {
        provider.notices.push(Notice::new(
            NoticeKind::SpendControl,
            "Monthly usage-credit spend limit reached.",
        ));
    }
    for row in provider.windows.iter().filter(|row| row.used >= 100.0) {
        let mut notice = Notice::new(NoticeKind::LimitReached, format!("{} limit reached.", row.label));
        notice.window_id = Some(row.id.clone());
        notice.resets_at = row.reset.clone();
        provider.notices.push(notice);
    }
    Ok(provider)
}
