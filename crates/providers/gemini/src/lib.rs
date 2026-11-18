//! Projects Gemini CLI quota buckets.

use quotaline_text::json::{number, text};
use quotaline_text::text::{decimal, humanize};

use jiff::Timestamp;
use quotaline_quota::{Detail, Provider, Window};
use serde_json::Value;
use std::collections::BTreeMap;

mod assist_cache;
pub mod fetch;
pub mod request;
pub use assist_cache::{ASSIST_BALANCE_TTL, ASSIST_DEFAULT_TTL, AssistCache};

pub const UNSUPPORTED_CLIENT_MESSAGE: &str = "Google ended Login with Google for personal Gemini accounts on June 18, 2026. Gemini Code Assist Standard or Enterprise accounts still report quota.";

pub fn project_id(assist: &Value) -> Option<&str> {
    let project = &assist["cloudaicompanionProject"];
    text(project)
        .or_else(|| text(&project["id"]))
        .or_else(|| text(&project["projectId"]))
}

pub fn has_credit_balances(assist: &Value) -> bool {
    ["currentTier", "paidTier"].iter().any(|tier| {
        assist[tier]["availableCredits"]
            .as_array()
            .is_some_and(|credits| !credits.is_empty())
    })
}

pub fn unsupported_reason(assist: &Value) -> Option<String> {
    if !assist["currentTier"].is_null() || !assist["paidTier"].is_null() {
        return None;
    }
    let tiers = assist["ineligibleTiers"].as_array()?;
    let tier = tiers
        .iter()
        .find(|tier| text(&tier["reasonCode"]).is_some_and(|code| code.eq_ignore_ascii_case("UNSUPPORTED_CLIENT")))
        .or_else(|| tiers.first())?;
    Some(
        text(&tier["reasonMessage"])
            .unwrap_or(UNSUPPORTED_CLIENT_MESSAGE)
            .into(),
    )
}

pub fn subscription_required(body: &str) -> bool {
    let Ok(document) = serde_json::from_str::<Value>(body) else {
        return body.to_ascii_uppercase().contains("SUBSCRIPTION_REQUIRED");
    };
    let error = document.get("error").unwrap_or(&document);
    text(&error["status"]).is_some_and(|status| status.eq_ignore_ascii_case("SUBSCRIPTION_REQUIRED"))
        || error["details"].as_array().is_some_and(|details| {
            details.iter().any(|detail| {
                text(&detail["reason"]).is_some_and(|reason| reason.eq_ignore_ascii_case("SUBSCRIPTION_REQUIRED"))
            })
        })
}

pub fn project(quota: &Value, assist: &Value, hosted_domain: Option<&str>, email: String, source: String) -> Provider {
    let plan_name = text(&assist["paidTier"]["name"]).map(str::to_owned).unwrap_or_else(|| {
        match text(&assist["currentTier"]["id"]) {
            Some("standard-tier") => "Standard".into(),
            Some("legacy-tier") => "Legacy".into(),
            Some("free-tier") if hosted_domain.is_some() => "Workspace".into(),
            Some("free-tier") => "Free".into(),
            _ => text(&assist["currentTier"]["name"]).unwrap_or("Gemini").into(),
        }
    });
    let mut windows: Vec<Window> = Vec::new();
    let mut details = Vec::new();
    let mut credits = BTreeMap::new();
    for tier in [&assist["currentTier"], &assist["paidTier"]] {
        for credit in tier["availableCredits"].as_array().into_iter().flatten() {
            if let (Some(kind), Some(amount)) = (text(&credit["creditType"]), number(&credit["creditAmount"])) {
                credits.insert(kind.to_owned(), amount);
            }
        }
    }
    let credit_balance = credits
        .get("GOOGLE_ONE_AI")
        .or_else(|| (credits.len() == 1).then(|| credits.values().next()).flatten())
        .copied();
    for (kind, amount) in credits {
        details.push(Detail {
            id: format!("credit:{kind}"),
            title: humanize(&kind),
            value: format!("{} credits", decimal(amount)),
            explanation: "A separate provider credit balance. Different credit types are not interchangeable.".into(),
        });
    }
    for (index, bucket) in quota["buckets"].as_array().into_iter().flatten().enumerate() {
        if let (Some(model), Some(remaining)) = (text(&bucket["modelId"]), number(&bucket["remainingFraction"])) {
            let used_percent = (1.0 - remaining) * 100.0;
            let reset = text(&bucket["resetTime"])
                .filter(|value| value.parse::<Timestamp>().is_ok())
                .map(str::to_owned);
            let mut window = Window::new(format!("model:{model}"), humanize(model), used_percent, reset, "other");
            window.scope = Some(model.into());
            if let Some(existing) = windows.iter_mut().find(|existing| existing.id == window.id) {
                if existing.used < window.used {
                    *existing = window;
                }
            } else {
                windows.push(window);
            }
        }
        let mut values = Vec::new();
        if let Some(amount) = text(&bucket["remainingAmount"]) {
            values.push(format!(
                "{amount} {} remaining",
                text(&bucket["tokenType"]).unwrap_or("units")
            ));
        }
        if let Some(remaining) = number(&bucket["remainingFraction"]) {
            values.push(format!("{:.0}% used", (1.0 - remaining) * 100.0));
        }
        if let Some(reset) = text(&bucket["resetTime"]) {
            values.push(format!("Resets {reset}"));
        }
        if !values.is_empty() {
            details.push(Detail {
                id: format!("bucket:{index}"),
                title: text(&bucket["modelId"]).map(humanize).unwrap_or_else(|| "Quota".into()),
                value: values.join(" · "),
                explanation: "An individual quota bucket. The model bar uses its most consumed bucket; absolute counts and other buckets remain separate.".into(),
            });
        }
    }
    let mut provider = Provider::connected("gemini", plan_name, source, email, windows);
    provider.organization = hosted_domain.unwrap_or_default().into();
    provider.tier = text(&assist["currentTier"]["id"]).map(str::to_owned);
    provider.credit_balance = credit_balance;
    provider.details = details;
    provider
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
