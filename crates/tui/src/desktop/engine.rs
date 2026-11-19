//! Runs Quotaline's engine inside the terminal, sharing the machine's lease and history with other front ends.

use jiff::{Timestamp, tz::TimeZone};
use quotaline_engine::service::{self, Engine};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    io,
    path::Path,
    sync::Arc,
};
use tokio::{runtime::Runtime, task::JoinHandle};

use crate::desktop::{Command, History, Output, State, host::TerminalHost, state};

pub(crate) struct Backend {
    runtime: Runtime,
    engine: Engine,
    running: JoinHandle<()>,
    hidden: BTreeSet<String>,
    stacked: bool,
}

fn failed(error: service::Error) -> io::Error {
    io::Error::other(error.to_string())
}

impl Backend {
    pub(crate) fn start() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let engine = runtime
            .block_on(async { Engine::new(false, Arc::new(TerminalHost)) })
            .map_err(|error| io::Error::other(error.to_string()))?;
        let running = runtime.spawn({
            let engine = engine.clone();
            async move {
                // An open dashboard keeps providers polled at the pace of a visible panel.
                engine.set_panel_open(true).await;
                engine.run().await;
            }
        });
        Ok(Self {
            runtime,
            engine,
            running,
            hidden: BTreeSet::new(),
            stacked: false,
        })
    }

    fn call<T>(&self, request: impl Future<Output = T>) -> T {
        self.runtime.block_on(request)
    }

    pub(crate) fn usage(&mut self) -> io::Result<State> {
        let state = state::project(&self.call(self.engine.state()), Timestamp::now().as_second())?;
        self.stacked = state.settings["history_stacked"] == true;
        self.hidden = serde_json::from_value(state.settings["history_hidden_keys"].clone()).unwrap_or_default();
        Ok(state)
    }

    pub(crate) fn execute(&mut self, command: Command) -> io::Result<Output> {
        let engine = self.engine.clone();
        match command {
            Command::History(action) => self.history(action),
            Command::Logs(full) => self.logs(full),
            Command::Diagnostics => text(Ok(self.call(engine.diagnostics()))),
            Command::Models(query) => text(self.call(engine.model_groups(&query, ""))),
            // The terminal runs a sign-in itself, so the engine only refreshes the provider afterwards.
            Command::Refresh(provider) | Command::SignIn(provider) => {
                confirm(self.call(engine.refresh(&provider)), "Refresh", "Refresh requested")
            }
            Command::Set(key, value) => saved(self.call(engine.set_setting(&key, &value))),
            Command::ToggleModel(key) => saved(self.call(engine.toggle_model(&key))),
            Command::ToggleProviderModels(provider) => saved(self.call(engine.toggle_provider_models(&provider))),
            Command::StepInterval(provider, delta) => saved(self.call(engine.step_provider_interval(&provider, delta))),
            Command::ToggleThreshold(threshold) => saved(self.call(engine.toggle_notification_threshold(threshold))),
            Command::Reorder(kind, source, target, after) => {
                saved(self.call(engine.reorder(&kind, &source, &target, after)))
            }
            Command::Visibility(action) => {
                let raw = self.call(engine.history_visibility(&action)).map_err(failed)?;
                self.hidden = serde_json::from_str::<Visibility>(&raw)?.hidden;
                self.history(serde_json::json!({"kind":"load"}))
            }
            Command::Spend => text(self.call(engine.spend_summary())),
            Command::Reviews => text(self.call(engine.review_summary())),
            Command::ExportAll(path) => self.export(&path, |backend, staging| {
                let done = backend
                    .call(backend.engine.export_all_history(staging))
                    .map_err(failed)?;
                confirm(done, "ExportAllHistory", "History exported")
            }),
            Command::ExportMetric(metric, start, end, path) => self.export(&path, |backend, staging| {
                let done = backend
                    .call(backend.engine.export_metric_history(&metric, start, end, staging))
                    .map_err(failed)?;
                confirm(done, "ExportMetricHistory", "History exported")
            }),
            Command::IssueUrl => Ok(Output::Text(vec![self.call(engine.issue_url()).map_err(failed)?])),
            Command::ClearLogs => confirm(
                self.call(engine.clear_logs()).map_err(failed)?,
                "ClearLogs",
                "Logs cleared",
            ),
            Command::ClearHistory => confirm(
                self.call(engine.clear_history()).map_err(failed)?,
                "ClearHistory",
                "History cleared",
            ),
            Command::ResetSettings => saved(self.call(engine.reset_settings())),
        }
    }

    fn history(&mut self, action: Value) -> io::Result<Output> {
        let zone = TimeZone::system();
        let name = zone.iana_name().unwrap_or("UTC");
        let offset = zone.to_offset(Timestamp::now()).seconds();
        let raw = self
            .call(self.engine.history_view(&action.to_string(), name, offset))
            .map_err(failed)?;
        let response = serde_json::from_str::<HistoryResponse>(&raw)?;
        let Some(mut result) = response.history else {
            // The engine names the error whenever it sends no history.
            return Err(io::Error::other(response.error.unwrap_or_default()));
        };
        // The engine labels project series only, naming each project without its full path.
        for series in &mut result.history.series {
            if let Some(label) = result.project_labels.remove(&series.label) {
                series.label = label;
            }
        }
        result.history.hidden.clone_from(&self.hidden);
        result.history.utc = response.view.utc;
        result.history.stacked = self.stacked;
        Ok(Output::History(result.history))
    }

    fn logs(&mut self, full: bool) -> io::Result<Output> {
        let raw = self.engine.logs(full).map_err(failed)?;
        let entries = serde_json::from_str::<Vec<Log>>(&raw)?;
        Ok(Output::Text(entries.into_iter().map(|entry| entry.line).collect()))
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
}

impl Drop for Backend {
    fn drop(&mut self) {
        // Hand the lease to another front end now instead of letting it expire.
        self.engine.quit();
        let _ = self.runtime.block_on(&mut self.running);
        self.runtime.block_on(self.engine.release());
    }
}

fn text(raw: Result<String, service::Error>) -> io::Result<Output> {
    let raw = raw.map_err(failed)?;
    let text = if raw.is_empty() {
        "No readings reported".into()
    } else {
        serde_json::to_string_pretty(&serde_json::from_str::<Value>(&raw)?)?
    };
    Ok(Output::Text(text.lines().map(str::to_owned).collect()))
}

fn confirm(done: bool, action: &str, success: &str) -> io::Result<Output> {
    if !done {
        return Err(io::Error::other(format!("{action} was declined by Quotaline")));
    }
    Ok(Output::Done(success.into()))
}

fn saved(result: Result<String, service::Error>) -> io::Result<Output> {
    result.map_err(failed)?;
    Ok(Output::Done("Settings saved".into()))
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
