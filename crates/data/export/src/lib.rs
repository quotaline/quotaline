//! Builds redacted usage and history exports.

use jiff::Timestamp;
use quotaline_quota::Provider;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowExport<'a> {
    id: &'a str,
    label: &'a str,
    used_percent: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    resets_at: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderExport<'a> {
    id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    plan: Option<&'a str>,
    fetched_at: String,
    windows: Vec<WindowExport<'a>>,
}

#[derive(Serialize)]
pub struct UsageExport<'a> {
    providers: Vec<ProviderExport<'a>>,
}

impl<'a> UsageExport<'a> {
    pub fn from(providers: &'a BTreeMap<String, Provider>) -> Self {
        Self {
            providers: providers
                .iter()
                .filter_map(|(id, provider)| {
                    let stamp = provider.updated.and_then(|at| Timestamp::from_second(at).ok())?;
                    Some(ProviderExport {
                        id,
                        plan: (!provider.plan.is_empty()).then_some(provider.plan.as_str()),
                        fetched_at: stamp.strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
                        windows: provider
                            .windows
                            .iter()
                            .map(|row| WindowExport {
                                id: &row.id,
                                label: &row.label,
                                used_percent: row.used,
                                resets_at: row
                                    .reset
                                    .as_deref()
                                    .and_then(|reset| reset.parse::<Timestamp>().ok())
                                    .map(|reset| reset.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()),
                            })
                            .collect(),
                    })
                })
                .collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    pub fn json(&self) -> Vec<u8> {
        let value = serde_json::to_value(self).expect("usage export records contain only JSON values");
        serde_json::to_vec_pretty(&value).expect("usage export records contain only JSON values")
    }
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
