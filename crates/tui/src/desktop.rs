//! Defines the readings the dashboard displays and the commands it can send.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::snapshot::Provider;

#[cfg(not(test))]
mod engine;
mod host;
mod state;

#[cfg(not(test))]
pub(crate) use engine::Backend;

#[derive(Deserialize)]
pub(crate) struct State {
    pub(crate) providers: Vec<Provider>,
    #[serde(default)]
    pub(crate) settings: Value,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum Command {
    History(Value),
    Logs(bool),
    Diagnostics,
    Models(String),
    Refresh(String),
    SignIn(String),
    Set(String, String),
    ToggleModel(String),
    ToggleProviderModels(String),
    StepInterval(String, i32),
    ToggleThreshold(i32),
    Reorder(String, String, String, bool),
    Visibility(String),
    Spend,
    Reviews,
    ExportAll(String),
    ExportMetric(String, i64, i64, String),
    IssueUrl,
    ClearLogs,
    ClearHistory,
    ResetSettings,
}

#[cfg_attr(test, derive(Serialize))]
pub(crate) enum Output {
    History(History),
    Text(Vec<String>),
    Done(String),
}

#[derive(Deserialize, Serialize)]
pub(crate) struct History {
    pub(crate) metric: String,
    pub(crate) start: i64,
    pub(crate) end: i64,
    pub(crate) series: Vec<Series>,
    #[serde(rename = "canonicalTotal", default)]
    pub(crate) canonical_total: Option<f64>,
    #[serde(default)]
    pub(crate) hidden: BTreeSet<String>,
    #[serde(default)]
    pub(crate) stacked: bool,
    #[serde(default)]
    pub(crate) utc: bool,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Series {
    pub(crate) id: String,
    pub(crate) provider: String,
    pub(crate) label: String,
    pub(crate) points: Vec<(i64, f64, Option<f64>, bool)>,
}
