//! Reads the native app's snapshot cache, preferences, history, and logs.

use std::{
    collections::{BTreeMap, BTreeSet},
    env, io,
    path::{Path, PathBuf},
    sync::Arc,
};

use jiff::{Timestamp, tz::TimeZone};
use quotaline_analytics::{
    self as analytics, HistoryAction, HistorySession, HistoryVisibility, VisibilityAction, WindowChartRequest, project,
    utc_day,
};
use quotaline_history::{
    SampleQuery, Store,
    chart::{AnalyticsQuery, HistoryChartSession},
};
use quotaline_macos_ffi::{
    logging::{
        DiagnosticIdentity, diagnostic_issue_url, log_load, log_queue_defaults, redact_diagnostic_report,
        repository_url,
    },
    provider_record::ProviderReadingRecord,
    reading_cache::read_reading_cache,
    time::time_compact_countdown,
    usage::usage_identity_presentation,
};
use quotaline_quota::pace::{PaceEstimate, PaceStatus, SAMPLE_HORIZON_SECONDS};
use quotaline_settings::{Settings, history_session_from_settings};
use quotaline_status_bar::default_window_ids;
use serde::Deserialize;

use super::{Command, History, Output, Series, State};
use crate::snapshot::{Provider, Window, provider_overview};

pub(crate) struct Backend {
    home: Option<PathBuf>,
    history: Option<HistorySession>,
    history_visibility: Option<HistoryVisibility>,
}

impl Default for Backend {
    fn default() -> Self {
        Self {
            home: env::var_os("HOME").map(PathBuf::from),
            history: None,
            history_visibility: None,
        }
    }
}

impl Backend {
    pub(crate) fn usage(&mut self) -> io::Result<State> {
        Desktop::load(self.home.as_deref())?.usage()
    }

    pub(crate) fn execute(&mut self, command: Command) -> io::Result<Output> {
        let desktop = Desktop::load(self.home.as_deref())?;
        match command {
            Command::History(action) => {
                let now = Timestamp::now();
                let time_zone = TimeZone::system();
                let name = time_zone.iana_name().unwrap_or_default().to_owned();
                let offset = time_zone.to_offset(now).seconds();
                let now = (now.as_millisecond() as f64 / 1000.0).ceil();
                let session = self
                    .history
                    .get_or_insert_with(|| history_session_from_settings(&desktop.settings, name.clone(), offset, now));
                session.update_zone(name, offset);
                serde_json::from_value::<HistoryAction>(action)?
                    .apply(session, now)
                    .map_err(io::Error::other)?;
                let mut history = desktop.history(session, now)?;
                if let Some(visibility) = &self.history_visibility {
                    history.hidden = visibility.hidden("global").into_iter().collect();
                }
                Ok(Output::History(history))
            }
            Command::Logs(full) => Ok(Output::Text(desktop.logs(full))),
            Command::Diagnostics => Ok(Output::Text(
                desktop.diagnostics()?.lines().map(str::to_owned).collect(),
            )),
            Command::Models(query) => Ok(Output::Text(desktop.models(&query)?)),
            Command::Spend => Ok(Output::Text(desktop.spend()?)),
            Command::Reviews => Ok(Output::Text(vec![desktop.reviews()?])),
            Command::ExportAll(path) => export(Path::new(&path), |path| {
                desktop
                    .store()?
                    .export_all(path, desktop.settings.hide_identity)
                    .map_err(io::Error::other)
            }),
            Command::ExportMetric(metric, start, end, path) => export(Path::new(&path), |path| {
                desktop.export_metric(&metric, start, end, path)
            }),
            Command::IssueUrl => {
                let url = diagnostic_issue_url(
                    repository_url(),
                    "Quotaline diagnostics".into(),
                    desktop.diagnostics()?,
                    desktop.home.to_string_lossy().into_owned(),
                )
                .expect("repository URL and title fit the issue URL limit");
                Ok(Output::Text(vec![url]))
            }
            Command::Refresh(provider) => Err(in_app(&format!("Refresh {provider}"))),
            Command::SignIn(provider) => Err(in_app(&format!("Set up {provider}"))),
            Command::Set(key, _value) => Err(in_app(&format!("Change {key}"))),
            Command::ToggleModel(key) => Err(in_app(&format!("Change selection for {key}"))),
            Command::ToggleProviderModels(provider) => Err(in_app(&format!("Change model selection for {provider}"))),
            Command::StepInterval(provider, _step) => {
                Err(in_app(&format!("Change the refresh interval for {provider}")))
            }
            Command::ToggleThreshold(threshold) => Err(in_app(&format!("Change the {threshold}% threshold"))),
            Command::Reorder(_kind, source, _target, _after) => Err(in_app(&format!("Move {source}"))),
            Command::Visibility(raw) => {
                let action = serde_json::from_str::<VisibilityAction>(&raw)?;
                let visibility = self.history_visibility.get_or_insert_with(|| {
                    let mut visibility = HistoryVisibility::default();
                    visibility.set_hidden("global", desktop.settings.history_hidden_keys);
                    visibility
                });
                action.apply(visibility, "global");
                self.execute(Command::History(serde_json::json!({"kind":"load"})))
            }
            Command::ClearLogs => Err(in_app("Clear logs")),
            Command::ClearHistory => Err(in_app("Clear history")),
            Command::ResetSettings => Err(in_app("Reset settings")),
        }
    }
}

fn in_app(action: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{action} in the Quotaline menu bar app"),
    )
}

fn export(path: &Path, write: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<Output> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Choose a file for the history export"))?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    write(temporary.path())?;
    temporary.persist_noclobber(path).map_err(|error| error.error)?;
    Ok(Output::Done("History exported".into()))
}

struct Desktop<'home> {
    home: &'home Path,
    support: PathBuf,
    settings: Settings,
}

impl<'home> Desktop<'home> {
    fn load(home: Option<&'home Path>) -> io::Result<Self> {
        let home = home.ok_or_else(|| io::Error::other("HOME is unset"))?;
        let mut library = home.join("Library");
        let support_path = "Application Support/Quotaline";
        if !library.join(support_path).try_exists()? {
            let sandbox = library.join("Containers/tech.bernat.quotaline/Data/Library");
            if sandbox.join(support_path).try_exists()? {
                library = sandbox;
            }
        }
        let preferences = library.join("Preferences/tech.bernat.quotaline.plist");
        let mut settings = if preferences.try_exists()? {
            let stored: plist::Dictionary = plist::from_file(&preferences).map_err(io::Error::other)?;
            match stored.get("settings") {
                Some(plist::Value::Data(document)) => serde_json::from_slice(document)?,
                Some(_) => return Err(io::Error::other("Invalid macOS settings document")),
                None => Self::legacy_settings(&stored),
            }
        } else {
            Settings {
                hide_identity: true,
                ..Settings::default()
            }
        };
        settings.normalize();
        settings.request_demo(env::var_os("QUOTALINE_DEMO").is_some());
        Ok(Self {
            home,
            support: library.join(support_path),
            settings,
        })
    }

    fn legacy_settings(stored: &plist::Dictionary) -> Settings {
        let known = legacy_value::<Vec<String>>(stored, "knownProviders");
        let enabled = legacy_value::<Vec<String>>(stored, "enabledProviders");
        let configured = legacy_value::<Vec<String>>(stored, "configuredProviders");
        let order = legacy_value::<Vec<String>>(stored, "providerOrder");
        let preferences = quotaline_settings::migrate_provider_preferences(
            known.as_deref(),
            enabled.as_deref(),
            configured.as_deref(),
            order.as_deref(),
        );
        let windows = legacy_value::<Vec<LegacyWindow>>(stored, "selectedWindows").unwrap_or_default();
        Settings {
            providers: quotaline_catalog::IDS
                .into_iter()
                .map(|id| (id.into(), preferences.enabled.iter().any(|enabled| enabled == id)))
                .collect(),
            configured_providers: preferences.configured.into_iter().collect(),
            provider_order: preferences.order,
            selected_windows: windows
                .into_iter()
                .map(|window| format!("{}:{}", window.provider, window.window_id))
                .collect(),
            has_custom_selection: stored.get("hasCustomSelection").and_then(plist::Value::as_boolean),
            hide_identity: stored
                .get("hidePersonalInformation")
                .and_then(plist::Value::as_boolean)
                .unwrap_or(true),
            demo_choice: stored.get("demoMode").and_then(plist::Value::as_boolean),
            ..Settings::default()
        }
    }

    fn usage(&self) -> io::Result<State> {
        let readings = self.readings()?;
        let selected = self.selected(&readings);
        let now = Timestamp::now().as_second();
        let samples = self.recent_samples(&selected, now);
        let mut providers = readings
            .into_iter()
            .map(|reading| {
                let provider: quotaline_quota::Provider = reading.provider.into();
                let identity = usage_identity_presentation(
                    nonempty(&provider.identity),
                    nonempty(&provider.organization),
                    self.settings.hide_identity,
                );
                let redact = |mut detail: String| {
                    if self.settings.hide_identity {
                        for replacement in &identity.replacements {
                            detail = detail.replace(&replacement.value, &replacement.replacement);
                        }
                    }
                    self.redact(detail, Vec::new())
                };
                let overview =
                    provider_overview(&provider, |id| selected.contains(&format!("{}:{id}", provider.id)), now)
                        .into_iter()
                        .map(&redact)
                        .collect();
                let provider_details = provider
                    .details
                    .iter()
                    .filter(|detail| detail.id != "oauth-availability" && !detail.value.trim().is_empty())
                    .map(|detail| format!("{} · {}", detail.title, detail.value))
                    .chain(provider.notices.into_iter().map(|notice| notice.text))
                    .chain(provider.warnings)
                    .chain(provider.credit_balance.map(|balance| format!("Credits · {balance:.2}")))
                    .chain(provider.cloud_credits.map(|credits| {
                        format!(
                            "Cloud session credits · ${:.2} of ${:.2} left",
                            credits.remaining, credits.limit
                        )
                    }))
                    .map(redact);
                let details = identity
                    .email
                    .into_iter()
                    .chain(identity.organization.filter(|_| identity.show_organization))
                    .chain(provider_details)
                    .collect();
                Provider {
                    plan: (!provider.plan.is_empty()).then_some(provider.plan),
                    fetched_at: Timestamp::from_second(reading.fetched_at as i64).ok(),
                    status: "Cached".into(),
                    overview,
                    details,
                    windows: provider
                        .windows
                        .into_iter()
                        .filter_map(|window| {
                            let key = format!("{}:{}", provider.id, window.id);
                            selected.contains(&key).then(|| {
                                let reset_by_day = window.reset_precision == quotaline_quota::ResetPrecision::Day;
                                let pace = PaceEstimate::estimate(
                                    &window,
                                    samples.get(&key).map_or(&[], Vec::as_slice),
                                    self.settings.workdays,
                                    now,
                                );
                                Window {
                                    label: window.label,
                                    used_percent: Some(window.used),
                                    resets_at: window.reset.and_then(|reset| reset.parse().ok()),
                                    reset_by_day,
                                    pace: pace_summary(pace, now),
                                }
                            })
                        })
                        .collect(),
                    id: provider.id,
                }
            })
            .collect::<Vec<_>>();
        providers.sort_by_key(|provider| self.settings.provider_order.iter().position(|id| id == &provider.id));
        Ok(State {
            providers,
            settings: serde_json::to_value(&self.settings)?,
        })
    }

    fn recent_samples(&self, selected: &BTreeSet<String>, now: i64) -> BTreeMap<String, Vec<(i64, f64)>> {
        let keys = selected.iter().cloned().collect::<Vec<_>>();
        let Ok(rows) = self.store().and_then(|store| {
            store
                .samples(SampleQuery {
                    keys: Some(&keys),
                    start: (now - SAMPLE_HORIZON_SECONDS) as f64,
                    end: now as f64,
                    rollup: None,
                    time_zone: "UTC",
                    fallback_offset: 0,
                    includes_end: true,
                })
                .map_err(io::Error::other)
        }) else {
            return BTreeMap::new();
        };
        let mut samples = BTreeMap::<String, Vec<(i64, f64)>>::new();
        for row in rows {
            samples.entry(row.key).or_default().push((row.at as i64, row.used));
        }
        samples
    }

    fn readings(&self) -> io::Result<Vec<ProviderReadingRecord>> {
        Ok(self
            .cached_readings()?
            .into_iter()
            .filter(|reading| self.enabled(&reading.provider.id))
            .collect())
    }

    fn cached_readings(&self) -> io::Result<Vec<ProviderReadingRecord>> {
        let cache = self.support.join(if self.settings.demo {
            "snapshots-demo.json"
        } else {
            "snapshots.json"
        });
        read_reading_cache(cache.to_string_lossy().into_owned()).map_err(io::Error::other)
    }

    fn enabled(&self, id: &str) -> bool {
        !self.settings.configured_providers.contains(id) || self.settings.providers.get(id) == Some(&true)
    }

    fn selected(&self, readings: &[ProviderReadingRecord]) -> BTreeSet<String> {
        if self.settings.has_custom_selection == Some(true) {
            return self
                .settings
                .selected_windows
                .iter()
                .filter(|key| key.split_once(':').is_some_and(|(id, _)| self.enabled(id)))
                .cloned()
                .collect();
        }
        readings
            .iter()
            .flat_map(|reading| {
                let provider = &reading.provider;
                default_window_ids(
                    provider
                        .windows
                        .iter()
                        .map(|window| (window.id.as_str(), window.active)),
                )
                .into_iter()
                .map(move |id| format!("{}:{id}", provider.id))
            })
            .collect()
    }

    fn history(&self, session: &HistorySession, now: f64) -> io::Result<History> {
        let view = session.view(now);
        let store = self.store()?;
        let chart = HistoryChartSession::new(Arc::clone(&store), 0);
        let (series, canonical_total) = if view.metric == "usage" {
            let selected = self.selected(&self.readings()?);
            let labels = store
                .summaries()
                .map_err(io::Error::other)?
                .into_iter()
                .filter(|summary| selected.contains(&summary.key))
                .map(|summary| (summary.key, summary.label))
                .collect::<BTreeMap<_, _>>();
            let keys = labels.keys().cloned().collect::<Vec<_>>();
            let zone = if view.utc { TimeZone::UTC } else { TimeZone::system() };
            let data = chart
                .window(WindowChartRequest {
                    visible: keys.clone(),
                    keys,
                    start: view.start as f64,
                    end: view.end as f64,
                    now,
                    rollup: session.rollup(),
                    stacked: false,
                    time_zone: zone.iana_name().unwrap_or_default().into(),
                    fallback_offset: zone.to_offset(Timestamp::now()).seconds(),
                    includes_end: true,
                })
                .map_err(io::Error::other)?;
            (
                data.series
                    .into_iter()
                    .filter_map(|line| {
                        let (provider, _) = line.key.split_once(':')?;
                        let provider = provider.to_owned();
                        (!line.points.is_empty()).then(|| Series {
                            provider,
                            label: labels[&line.key].clone(),
                            points: line
                                .points
                                .into_iter()
                                .map(|point| (point.at as i64, point.value, point.reset_at, point.carried))
                                .collect(),
                            id: line.key,
                        })
                    })
                    .collect(),
                None,
            )
        } else {
            self.analytics(&chart, &view.metric, view.start, view.end)?
        };
        Ok(History {
            metric: view.metric,
            start: view.start,
            end: view.end,
            series,
            canonical_total,
            stacked: self.settings.history_stacked,
            utc: view.utc,
            hidden: self.settings.history_hidden_keys.iter().cloned().collect(),
        })
    }

    fn store(&self) -> io::Result<Arc<Store>> {
        let filename = if self.settings.demo {
            "usage-demo.sqlite"
        } else {
            "usage.sqlite"
        };
        Store::open_read_only(&self.support.join(filename), self.settings.retention_days)
            .map(Arc::new)
            .map_err(io::Error::other)
    }

    fn analytics(
        &self,
        chart: &HistoryChartSession,
        metric: &str,
        start: i64,
        end: i64,
    ) -> io::Result<(Vec<Series>, Option<f64>)> {
        let data = chart
            .analytics(
                AnalyticsQuery {
                    metric: metric.into(),
                    providers: self.enabled_ids(),
                    start_day: utc_day::day(start as f64),
                    before_day: utc_day::day(end as f64),
                },
                &[],
                false,
            )
            .map_err(io::Error::other)?;
        let Some(data) = data else {
            return Ok((Vec::new(), None));
        };
        let labels = if matches!(metric, "projectCost" | "projectMessages") {
            let names = data
                .series
                .iter()
                .map(|series| series.series.clone())
                .collect::<Vec<_>>();
            project::labels(&names, self.settings.hide_identity)
        } else {
            BTreeMap::new()
        };
        Ok((
            data.series
                .into_iter()
                .filter(|series| series.visible)
                .map(|series| {
                    let label = labels.get(&series.series).cloned().unwrap_or(series.series);
                    Series {
                        id: format!("{}:{label}", series.provider),
                        provider: series.provider,
                        label,
                        points: series
                            .points
                            .into_iter()
                            .map(|point| (point.at as i64, point.value, None, false))
                            .collect(),
                    }
                })
                .collect(),
            Some(data.summary_total),
        ))
    }

    fn enabled_ids(&self) -> Vec<String> {
        quotaline_catalog::IDS
            .into_iter()
            .filter(|id| self.enabled(id))
            .map(str::to_owned)
            .collect()
    }

    fn logs(&self, full: bool) -> Vec<String> {
        let config = log_queue_defaults();
        let now = Timestamp::now().as_second() as f64;
        let logs = log_load(
            self.support.join("log.txt").to_string_lossy().into_owned(),
            config.file_count,
            config.file_byte_limit,
            if full {
                u64::from(config.retained_entry_limit)
            } else {
                25
            },
            now - config.retention_seconds,
            self.home.to_string_lossy().into_owned(),
            Vec::new(),
        );
        let identities = if self.settings.hide_identity {
            self.cached_readings()
                .unwrap_or_default()
                .into_iter()
                .map(|reading| DiagnosticIdentity {
                    email: nonempty(&reading.provider.identity),
                    organization: nonempty(&reading.provider.organization),
                })
                .collect()
        } else {
            Vec::new()
        };
        let lines = logs
            .entries
            .into_iter()
            .map(|entry| entry.line)
            .collect::<Vec<_>>()
            .join("\n");
        self.redact(lines, identities).lines().map(str::to_owned).collect()
    }

    fn diagnostics(&self) -> io::Result<String> {
        let mut rows = vec!["Quotaline macOS diagnostics".into()];
        rows.extend(
            self.usage()?
                .providers
                .into_iter()
                .map(|provider| format!("{} · {}", provider.name(), provider.status)),
        );
        rows.extend(self.logs(false));
        Ok(rows.join("\n"))
    }

    fn models(&self, query: &str) -> io::Result<Vec<String>> {
        let readings = self.readings()?;
        let selected = self.selected(&readings);
        let query = query.to_lowercase();
        Ok(readings
            .into_iter()
            .flat_map(|reading| {
                let id = reading.provider.id;
                reading
                    .provider
                    .windows
                    .into_iter()
                    .map(move |window| (format!("{id}:{}", window.id), window.label))
            })
            .filter(|(key, label)| key.to_lowercase().contains(&query) || label.to_lowercase().contains(&query))
            .map(|(key, label)| {
                let status = if selected.contains(&key) { "Selected" } else { "Hidden" };
                format!("{status} · {key} · {label}")
            })
            .collect())
    }

    fn spend(&self) -> io::Result<Vec<String>> {
        let now = Timestamp::now();
        let days = analytics::spend_days(now);
        let points = self
            .store()?
            .analytics_rows("costUSD", &self.enabled_ids(), &days.start, &days.today, true)
            .map_err(io::Error::other)?;
        let summary = analytics::spend_summary(
            &points
                .into_iter()
                .map(|point| analytics::SpendRow {
                    day: point.day,
                    provider: point.provider,
                    model: point.series,
                    value: point.value,
                })
                .collect::<Vec<_>>(),
            now,
        );
        let mut rows = vec![format!(
            "Today ${:.2} · Yesterday ${:.2} · Last 30 days ${:.2}",
            summary.today, summary.yesterday, summary.last_window
        )];
        rows.extend(
            summary
                .models
                .into_iter()
                .map(|model| format!("{} · {} · ${:.2}", model.provider, model.model, model.cost)),
        );
        Ok(rows)
    }

    fn reviews(&self) -> io::Result<String> {
        let now = Timestamp::now();
        let (first, today) = utc_day::retained_utc_days(now.as_second() as f64, 7);
        let rows = self
            .store()?
            .analytics_rows("codeReviews", &self.enabled_ids(), &first, &today, true)
            .map_err(io::Error::other)?
            .into_iter()
            .map(|point| analytics::ReviewRow {
                day: point.day,
                value: point.value,
            })
            .collect::<Vec<_>>();
        Ok(analytics::review_counts(&rows, now).map_or_else(
            || "No review history yet".into(),
            |counts| format!("{} today · {} this week", counts.today, counts.week),
        ))
    }

    fn export_metric(&self, metric: &str, start: i64, end: i64, path: &Path) -> io::Result<()> {
        let store = self.store()?;
        if metric == "usage" {
            let keys = self.selected(&self.readings()?).into_iter().collect::<Vec<_>>();
            store.export_samples(path, start as f64, end as f64, Some(&keys), true)
        } else {
            store.export_analytics(
                path,
                metric,
                &self.enabled_ids(),
                start as f64,
                end as f64,
                self.settings.hide_identity,
            )
        }
        .map_err(io::Error::other)
    }

    fn redact(&self, message: String, identities: Vec<DiagnosticIdentity>) -> String {
        redact_diagnostic_report(
            message,
            self.home.to_string_lossy().into_owned(),
            identities,
            self.settings.hide_identity,
        )
    }
}

fn legacy_value<Value: serde::de::DeserializeOwned>(stored: &plist::Dictionary, key: &str) -> Option<Value> {
    serde_json::from_slice(stored.get(key)?.as_data()?).ok()
}

#[derive(Deserialize)]
struct LegacyWindow {
    provider: String,
    #[serde(rename = "windowID")]
    window_id: String,
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_owned())
}

fn pace_summary(pace: PaceEstimate, now: i64) -> Option<String> {
    let status = match pace.status {
        PaceStatus::Unknown => return None,
        PaceStatus::OnTrack => "On track",
        PaceStatus::Ahead => "Ahead of pace",
        PaceStatus::Behind => "Behind pace",
        PaceStatus::Exhausted => return Some("Exhausted".into()),
    };
    if let Some(at) = pace.projected_exhaustion {
        Some(format!(
            "{status} · hits 100% in {}",
            time_compact_countdown((at - now) as f64)
        ))
    } else {
        Some(status.into())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/desktop/macos.rs"]
mod tests;
