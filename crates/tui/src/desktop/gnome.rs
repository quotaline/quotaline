//! Uses the GNOME Shell extension's service on the current session bus.

use jiff::{Timestamp, tz::TimeZone};
use quotaline_quota::{CloudCredits, CreditExpiry, Detail, Notice, ResetCredits, SpendControl, UsageCredits};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::Path,
    time::Duration,
};
use tokio::runtime::Runtime;
use zbus::{Connection, DynamicType, Proxy, connection::Builder, proxy::MethodFlags, wire::DynamicDeserialize};

use crate::{
    desktop::{Command, History, Output, State},
    snapshot::{OverviewFacts, Provider, Window},
};

#[derive(Default)]
pub(crate) struct Backend {
    session: Option<Session>,
    hidden: BTreeSet<String>,
    stacked: bool,
}

impl Backend {
    pub(crate) fn usage(&mut self) -> io::Result<State> {
        let state = serde_json::from_str::<ServiceState>(&self.request::<String, _>("GetState", &())?)?;
        let mut providers = state.providers;
        let now = Timestamp::now().as_second();
        self.stacked = state.settings["history_stacked"] == true;
        self.hidden = serde_json::from_value(state.settings["history_hidden_keys"].clone()).unwrap_or_default();
        Ok(State {
            providers: state
                .ordering
                .providers
                .into_iter()
                .filter_map(|id| {
                    let provider = providers.remove(&id)?;
                    provider.show_in_usage.then(|| {
                        let selected = |window: &str| state.selected_window_keys.contains(&format!("{id}:{window}"));
                        let overview = OverviewFacts {
                            id: &id,
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
                        .project(selected, now);
                        Provider {
                            plan: (!provider.plan.is_empty()).then_some(provider.plan),
                            fetched_at: provider.updated.and_then(|at| Timestamp::from_second(at).ok()),
                            status: provider.status,
                            overview,
                            details: provider
                                .details
                                .into_iter()
                                .map(|detail| format!("{}  {}", detail.title, detail.value))
                                .chain(provider.notices.into_iter().map(|notice| notice.text))
                                .chain(provider.warnings)
                                .chain(provider.organization.filter(|_| provider.show_organization))
                                .chain((!provider.identity.is_empty()).then_some(provider.identity))
                                .collect(),
                            windows: provider
                                .windows
                                .into_iter()
                                .filter(|window| selected(&window.id))
                                .map(|window| Window {
                                    label: window.label,
                                    used_percent: window.used,
                                    resets_at: window.reset.and_then(|reset| reset.parse().ok()),
                                    reset_by_day: window.reset_precision == "day",
                                    pace: window.pace.and_then(|pace| {
                                        pace.expected_percent.map(|percent| {
                                            format!("Expected {percent:.0}% · {}", pace.status.replace('_', " "))
                                        })
                                    }),
                                })
                                .collect(),
                            id,
                        }
                    })
                })
                .collect(),
            settings: state.settings,
        })
    }

    pub(crate) fn execute(&mut self, command: Command) -> io::Result<Output> {
        match command {
            Command::History(action) => self.history(action),
            Command::Logs(full) => self.logs(full),
            Command::Diagnostics => self.json_text("GetDiagnostics", &()),
            Command::Models(query) => self.json_text("GetModelGroups", &(query, "")),
            Command::Refresh(provider) => self.confirm("Refresh", &(provider,), "Refresh requested"),
            Command::SignIn(provider) => self.confirm("StartSignIn", &(provider,), "Sign-in opened"),
            Command::Set(key, value) => self.change("SetSetting", &(key, value)),
            Command::ToggleModel(key) => self.change("ToggleModel", &(key,)),
            Command::ToggleProviderModels(provider) => self.change("ToggleProviderModels", &(provider,)),
            Command::StepInterval(provider, delta) => self.change("StepProviderInterval", &(provider, delta)),
            Command::ToggleThreshold(threshold) => self.change("ToggleNotificationThreshold", &(threshold,)),
            Command::Reorder(kind, source, target, after) => self.change("Reorder", &(kind, source, target, after)),
            Command::Visibility(action) => {
                let raw = self.request::<String, _>("UpdateHistoryVisibility", &(action,))?;
                self.hidden = serde_json::from_str::<Visibility>(&raw)?.hidden;
                self.history(serde_json::json!({"kind":"load"}))
            }
            Command::Spend => self.json_text("GetSpendSummary", &()),
            Command::Reviews => self.json_text("GetReviewSummary", &()),
            Command::ExportAll(path) => self.export(&path, |backend, staging| {
                backend.confirm("ExportAllHistory", &(staging,), "History exported")
            }),
            Command::ExportMetric(metric, start, end, path) => self.export(&path, |backend, staging| {
                backend.confirm(
                    "ExportMetricHistory",
                    &(metric, start, end, staging),
                    "History exported",
                )
            }),
            Command::IssueUrl => Ok(Output::Text(vec![self.request("GetIssueUrl", &())?])),
            Command::ClearLogs => self.confirm("ClearLogs", &(), "Logs cleared"),
            Command::ClearHistory => self.confirm("ClearHistory", &(), "History cleared"),
            Command::ResetSettings => self.change("ResetSettings", &()),
        }
    }

    fn history(&mut self, action: Value) -> io::Result<Output> {
        let zone = TimeZone::system();
        let name = zone.iana_name().unwrap_or("UTC");
        let offset = zone.to_offset(Timestamp::now()).seconds();
        let raw = self.request::<String, _>("GetHistoryView", &(action.to_string(), name, offset))?;
        let response = serde_json::from_str::<HistoryResponse>(&raw)?;
        let Some(mut result) = response.history else {
            return Err(io::Error::other(
                response.error.unwrap_or_else(|| "History unavailable".into()),
            ));
        };
        for series in &mut result.history.series {
            if matches!(result.history.metric.as_str(), "projectCost" | "projectMessages") {
                series.label = result
                    .project_labels
                    .get(&series.label)
                    .cloned()
                    .unwrap_or_else(|| "Project".into());
            }
        }
        result.history.hidden.clone_from(&self.hidden);
        result.history.utc = response.view.utc;
        result.history.stacked = self.stacked;
        Ok(Output::History(result.history))
    }

    fn logs(&mut self, full: bool) -> io::Result<Output> {
        let raw = self.request::<String, _>("GetLogs", &(full,))?;
        let entries = serde_json::from_str::<Vec<Log>>(&raw)?;
        Ok(Output::Text(entries.into_iter().map(|entry| entry.line).collect()))
    }

    fn json_text(&mut self, method: &str, body: &(impl serde::Serialize + DynamicType)) -> io::Result<Output> {
        let raw = self.request::<String, _>(method, body)?;
        let text = if raw.is_empty() {
            "No readings reported".into()
        } else {
            serde_json::to_string_pretty(&serde_json::from_str::<Value>(&raw)?)?
        };
        Ok(Output::Text(text.lines().map(str::to_owned).collect()))
    }

    fn confirm(
        &mut self,
        method: &str,
        body: &(impl serde::Serialize + DynamicType),
        success: &str,
    ) -> io::Result<Output> {
        if !self.request::<bool, _>(method, body)? {
            return Err(io::Error::other(format!("{method} was declined by Quotaline")));
        }
        Ok(Output::Done(success.into()))
    }

    fn change(&mut self, method: &str, body: &(impl serde::Serialize + DynamicType)) -> io::Result<Output> {
        self.request::<String, _>(method, body)?;
        Ok(Output::Done("Settings saved".into()))
    }

    fn export(
        &mut self,
        path: &str,
        request: impl FnOnce(&mut Self, &str) -> io::Result<Output>,
    ) -> io::Result<Output> {
        let destination = Path::new(path);
        let directory = destination
            .parent()
            .ok_or_else(|| io::Error::other("Choose a filename for the export"))?;
        let staging = tempfile::NamedTempFile::new_in(directory)?;
        let result = request(self, &staging.path().to_string_lossy())?;
        staging.persist_noclobber(destination).map_err(io::Error::other)?;
        Ok(result)
    }

    fn request<Response, Body>(&mut self, method: &str, body: &Body) -> io::Result<Response>
    where
        Response: for<'data> DynamicDeserialize<'data>,
        Body: serde::Serialize + DynamicType,
    {
        let session = match &mut self.session {
            Some(session) => session,
            next @ None => next.insert(Session::connect()?),
        };
        let response = session.runtime.block_on(async {
            const SERVICE: &str = "tech.bernat.Quotaline.Gnome";
            Proxy::new(&session.connection, SERVICE, "/tech/bernat/Quotaline/Gnome", SERVICE)
                .await?
                .call_with_flags(method, MethodFlags::NO_AUTO_START, body)
                .await
                .map(|response| response.expect("the request expects a reply"))
        });
        if response.is_err() {
            self.session = None;
        }
        response.map_err(io::Error::other)
    }
}

struct Session {
    runtime: Runtime,
    connection: Connection,
}

impl Session {
    fn connect() -> io::Result<Self> {
        let timeout = Duration::from_secs(2);
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let connection = runtime
            .block_on(async {
                tokio::time::timeout(
                    timeout,
                    Builder::session().method_timeout(Duration::from_secs(30)).build(),
                )
                .await
            })
            .map_err(io::Error::other)?
            .map_err(io::Error::other)?;
        Ok(Self { runtime, connection })
    }
}

#[derive(Deserialize)]
struct ServiceState {
    providers: BTreeMap<String, ServiceProvider>,
    ordering: Order,
    selected_window_keys: BTreeSet<String>,
    settings: Value,
}

#[derive(Deserialize)]
struct Order {
    providers: Vec<String>,
}

#[derive(Deserialize)]
struct ServiceProvider {
    plan: String,
    updated: Option<i64>,
    status: String,
    windows: Vec<ServiceWindow>,
    show_in_usage: bool,
    details: Vec<Detail>,
    notices: Vec<Notice>,
    warnings: Vec<String>,
    organization: Option<String>,
    show_organization: bool,
    identity: String,
    credit_balance: Option<f64>,
    usage_credits: Option<UsageCredits>,
    reset_credits: Option<ResetCredits>,
    #[serde(default)]
    reset_credit_expiries: Vec<CreditExpiry>,
    cloud_credits: Option<CloudCredits>,
    spend: Option<SpendControl>,
}

#[derive(Deserialize)]
struct ServiceWindow {
    id: String,
    label: String,
    used: Option<f64>,
    reset: Option<String>,
    #[serde(default)]
    reset_precision: String,
    pace: Option<Pace>,
}

#[derive(Deserialize)]
struct Pace {
    status: String,
    expected_percent: Option<f64>,
}

#[derive(Deserialize)]
struct Visibility {
    hidden: BTreeSet<String>,
}

#[derive(Deserialize)]
struct HistoryResponse {
    history: Option<ServiceHistory>,
    error: Option<String>,
    #[serde(default)]
    view: HistoryView,
}

#[derive(Default, Deserialize)]
struct HistoryView {
    utc: bool,
}

#[derive(Deserialize)]
struct ServiceHistory {
    #[serde(flatten)]
    history: History,
    project_labels: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Log {
    line: String,
}
