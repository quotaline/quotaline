//! Projects Antigravity quota responses into shared models.

use jiff::Timestamp;
use quotaline_quota::{Detail, Provider, Window};
use quotaline_text::json::{number, text};
use serde_json::Value;

pub mod fetch;
pub mod local;
pub mod request;
mod tier_cache;
pub use tier_cache::{TIER_CACHE_TTL, TierCache};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuotaError {
    Unauthenticated,
    RpcFailure,
    MissingGroups,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub plan_name: String,
    pub tier: Option<String>,
    pub email: Option<String>,
}

pub fn identity(data: &Value) -> Identity {
    let status = &data["userStatus"];
    let tier = if status.is_object() {
        &status["userTier"]
    } else {
        &data["currentTier"]
    };
    Identity {
        plan_name: text(&tier["name"])
            .or_else(|| text(&status["planStatus"]["planInfo"]["planName"]))
            .or_else(|| text(&data["planName"]))
            .unwrap_or("Antigravity")
            .into(),
        tier: text(&tier["id"]).map(str::to_owned),
        email: text(&status["email"])
            .or_else(|| text(&data["email"]))
            .map(str::to_owned),
    }
}

pub fn validate(data: &Value) -> Result<(), QuotaError> {
    let code = &data["code"];
    if code.as_i64() == Some(16)
        || text(code).is_some_and(|value| value == "16" || value.eq_ignore_ascii_case("unauthenticated"))
    {
        return Err(QuotaError::Unauthenticated);
    }
    if !code.is_null()
        && code.as_i64() != Some(0)
        && !text(code).is_some_and(|value| ["0", "ok", "success"].iter().any(|ok| value.eq_ignore_ascii_case(ok)))
    {
        return Err(QuotaError::RpcFailure);
    }
    groups(data).ok_or(QuotaError::MissingGroups)?;
    Ok(())
}

pub fn project(data: &Value, identity: Identity, source: String) -> Result<Provider, QuotaError> {
    project_with_id_policy(data, MissingIdPolicy::Fallback, identity, source)
}

pub fn project_identified(data: &Value, identity: Identity, source: String) -> Result<Provider, QuotaError> {
    project_with_id_policy(data, MissingIdPolicy::Skip, identity, source)
}

enum MissingIdPolicy {
    Fallback,
    Skip,
}

fn project_with_id_policy(
    data: &Value,
    missing_id: MissingIdPolicy,
    identity: Identity,
    source: String,
) -> Result<Provider, QuotaError> {
    validate(data)?;
    let mut windows = Vec::new();
    let mut details = Vec::new();
    for group in groups(data).into_iter().flatten() {
        for bucket in group["buckets"].as_array().into_iter().flatten() {
            if matches!(missing_id, MissingIdPolicy::Skip) && text(&bucket["bucketId"]).is_none() {
                continue;
            }
            let bucket_id = text(&bucket["bucketId"])
                .or_else(|| text(&bucket["displayName"]))
                .unwrap_or("quota");
            let remaining = remaining_share(bucket);
            let spec = window_spec(bucket_id, text(&bucket["displayName"]));
            if bucket["disabled"].as_bool() != Some(true)
                && let Some(remaining) = remaining
            {
                let reset = text(&bucket["resetTime"])
                    .filter(|value| value.parse::<Timestamp>().is_ok())
                    .map(str::to_owned);
                let mut window = Window::new(spec.0, spec.1, (1.0 - remaining) * 100.0, reset, spec.2);
                window.duration_seconds = spec.3;
                window.detail = text(&bucket["description"]).unwrap_or_default().into();
                windows.push(window);
                continue;
            }
            if text(&bucket["bucketId"]).is_none() && text(&bucket["displayName"]).is_none() {
                continue;
            }
            let amount = remaining_amount(bucket);
            let mut explanation = Vec::new();
            if let Some(description) = text(&bucket["description"]) {
                explanation.push(description.to_owned());
            }
            if let Some(reset) = text(&bucket["resetTime"]) {
                explanation.push(format!("Resets {reset}."));
            }
            explanation.push("The local source did not report a usable percentage for this quota.".into());
            details.push(Detail {
                id: bucket_id.into(),
                title: spec.1,
                value: match (bucket["disabled"].as_bool() == Some(true), amount) {
                    (true, Some(amount)) => format!("Disabled · {amount} remaining"),
                    (true, None) => "Disabled".into(),
                    (false, Some(amount)) => format!("{amount} remaining"),
                    (false, None) => "Usage percentage unavailable".into(),
                },
                explanation: explanation.join(" "),
            });
        }
    }
    let mut provider = Provider::connected(
        "antigravity",
        identity.plan_name,
        source,
        identity.email.unwrap_or_default(),
        windows,
    );
    provider.tier = identity.tier;
    provider.details = details;
    Ok(provider)
}

fn groups(data: &Value) -> Option<&Vec<Value>> {
    data["response"]["groups"]
        .as_array()
        .or_else(|| data["summary"]["groups"].as_array())
        .or_else(|| data["groups"].as_array())
}

fn remaining_share(bucket: &Value) -> Option<f64> {
    number(&bucket["remainingFraction"])
        .or_else(|| number(&bucket["remaining"]["remainingFraction"]))
        .or_else(|| {
            (text(&bucket["remaining"]["case"]) == Some("remainingFraction"))
                .then(|| number(&bucket["remaining"]["value"]))
                .flatten()
        })
}

fn remaining_amount(bucket: &Value) -> Option<String> {
    let remaining = &bucket["remaining"];
    let amount = if text(&remaining["case"]) == Some("remainingAmount") {
        &remaining["value"]
    } else {
        &remaining["remainingAmount"]
    };
    text(amount)
        .map(str::to_owned)
        .or_else(|| number(amount).map(compact_amount))
}

fn compact_amount(value: f64) -> String {
    let (scaled, suffix) = match value.abs() {
        amount if amount < 1_000.0 => (value, ""),
        amount if amount < 1_000_000.0 => (value / 1_000.0, "K"),
        amount if amount < 1_000_000_000.0 => (value / 1_000_000.0, "M"),
        _ => (value / 1_000_000_000.0, "B"),
    };
    if suffix.is_empty() || scaled.abs() >= 10.0 {
        format!("{scaled:.0}{suffix}")
    } else {
        format!("{scaled:.1}{suffix}")
    }
}

fn window_spec(id: &str, display_name: Option<&str>) -> (String, String, &'static str, Option<u64>) {
    match id {
        "gemini-5h" => ("gemini:session".into(), "Gemini 5-hour".into(), "session", Some(18_000)),
        "gemini-weekly" => ("gemini:weekly".into(), "Gemini weekly".into(), "weekly", Some(604_800)),
        "3p-5h" => (
            "3p:session".into(),
            "Claude and GPT 5-hour".into(),
            "session",
            Some(18_000),
        ),
        "3p-weekly" => (
            "3p:weekly".into(),
            "Claude and GPT weekly".into(),
            "weekly",
            Some(604_800),
        ),
        _ => (id.into(), display_name.unwrap_or(id).into(), "other", None),
    }
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
