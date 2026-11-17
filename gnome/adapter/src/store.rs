//! Resolves GNOME data paths and adapts the shared SQLite history store.

use jiff::Timestamp;
use quotaline_analytics::{self as analytics, WindowRollup};
use quotaline_catalog::IDS;
use quotaline_history::{CoverageScope, Result, Sample, SampleQuery, Store};
use quotaline_quota::Provider;
use quotaline_settings::Settings;
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
};

use crate::history::{self, History};

pub type RecentSamples = BTreeMap<String, Vec<(String, i64, f64)>>;

type Identity = (PathBuf, u64, u64);

static HISTORY: Mutex<Option<(Identity, Arc<Store>)>> = Mutex::new(None);
static DEMO: Mutex<Option<Arc<Store>>> = Mutex::new(None);
static DEMO_ACTIVITY: Mutex<Option<(u32, BTreeMap<String, i64>)>> = Mutex::new(None);

const APP_DIRECTORY: &str = "quotaline";
// The project shipped as Token Menu Bar before the rename.
const LEGACY_APP_DIRECTORY: &str = "token-menu-bar-gnome";

fn home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
fn xdg_base(variable: &str, fallback: &str) -> PathBuf {
    env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(fallback))
}
pub fn xdg(variable: &str, fallback: &str) -> PathBuf {
    xdg_base(variable, fallback).join(APP_DIRECTORY)
}

/// Moves the pre-rename directory to the current name under `base`, unless the current one already exists.
pub fn adopt_legacy_directory(base: &Path) -> Option<(&'static str, String)> {
    let (legacy, current) = (base.join(LEGACY_APP_DIRECTORY), base.join(APP_DIRECTORY));
    if current.exists() || !legacy.exists() {
        return None;
    }
    let (legacy_shown, current_shown) = (legacy.display(), current.display());
    Some(match fs::rename(&legacy, &current) {
        Ok(()) => ("info", format!("Moved {legacy_shown} to {current_shown}")),
        Err(error) => (
            "warning",
            format!("Could not move {legacy_shown} to {current_shown}: {error}"),
        ),
    })
}

/// Adopts the pre-rename config, data, and cache directories and returns a log entry for each one acted on.
pub fn adopt_legacy_directories() -> Vec<(&'static str, String)> {
    [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_DATA_HOME", ".local/share"),
        ("XDG_CACHE_HOME", ".cache"),
    ]
    .into_iter()
    .filter_map(|(variable, fallback)| adopt_legacy_directory(&xdg_base(variable, fallback)))
    .collect()
}
pub fn config() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("settings.json")
}
pub fn database() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("history.sqlite3")
}
pub fn snapshot() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("snapshot.json")
}
pub fn usage() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("usage.json")
}
fn codex_watermarks() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("codex-analytics-watermarks.json")
}
pub fn load_codex_watermarks() -> Option<Vec<u8>> {
    fs::read(codex_watermarks()).ok()
}
pub fn save_codex_watermarks(encoded: &[u8]) -> std::io::Result<()> {
    quotaline_storage::write_private_file(&codex_watermarks(), encoded).map(drop)
}

pub fn read_settings() -> Settings {
    let mut settings: Settings = fs::read(config())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    if settings.demo && settings.demo_choice.is_none() {
        settings.demo_choice = Some(true);
    }
    settings.normalize();
    settings
}
pub fn read_snapshot() -> BTreeMap<String, Provider> {
    fs::read(snapshot())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub use quotaline_storage::atomic_json;

fn now() -> f64 {
    Timestamp::now().as_millisecond() as f64 / 1000.0
}

fn identity(path: &Path) -> Option<Identity> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(path)
        .ok()
        .map(|item| (path.to_path_buf(), item.dev(), item.ino()))
}

fn history_store() -> Result<Arc<Store>> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let path = database();
    fs::create_dir_all(path.parent().expect("history path has a directory"))?;
    if !path.exists() {
        // Create the file privately before SQLite opens it with default permissions.
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
    }
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let current = identity(&path);
    let mut cached = HISTORY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((stored, store)) = cached.as_ref()
        && Some(stored) == current.as_ref()
    {
        return Ok(store.clone());
    }
    let store = Arc::new(Store::open(Some(&path), read_settings().retention_days)?);
    *cached = identity(&path).map(|identity| (identity, store.clone()));
    Ok(store)
}

// Seeded once per process, as the macOS app seeds its demo history at launch.
fn demo_store() -> Result<Arc<Store>> {
    let mut cached = DEMO.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(store) = cached.as_ref() {
        return Ok(store.clone());
    }
    let store = Arc::new(Store::open(None, quotaline_history::DEFAULT_RETENTION_DAYS)?);
    store.seed_demo(&IDS.map(String::from), now(), false)?;
    *cached = Some(store.clone());
    Ok(store)
}

/// Seeds the demo history ahead of the first chart request.
pub fn warm_demo() -> Result<()> {
    demo_store().map(drop)
}

fn store_for(settings: &Settings) -> Result<Arc<Store>> {
    if settings.demo { demo_store() } else { history_store() }
}

fn enabled(providers: &BTreeMap<String, bool>) -> Vec<String> {
    IDS.iter()
        .filter(|id| providers.get(**id) != Some(&false))
        .map(|id| (*id).to_string())
        .collect()
}

pub fn record(provider: &Provider) -> Result<()> {
    record_into(&*history_store()?, provider, now())
}

pub fn record_into(store: &Store, provider: &Provider, now: f64) -> Result<()> {
    store
        .record_provider(provider, now, provider.status != "stale")
        .map(drop)
}

fn recent(now: i64) -> Result<Vec<Sample>> {
    history_store()?.samples(SampleQuery {
        keys: None,
        start: (now - 3600) as f64,
        end: now as f64,
        rollup: None,
        time_zone: "UTC",
        fallback_offset: 0,
        includes_end: true,
    })
}

pub fn recent_samples(provider: &str, now: i64) -> Result<Vec<(String, i64, f64)>> {
    Ok(recent_samples_all(now)?.remove(provider).unwrap_or_default())
}

pub fn recent_samples_all(now: i64) -> Result<RecentSamples> {
    let mut samples = RecentSamples::new();
    for row in recent(now)? {
        let (provider, window) = row.key.split_once(':').unwrap_or((&row.key, ""));
        samples
            .entry(provider.into())
            .or_default()
            .push((window.into(), row.at as i64, row.used));
    }
    Ok(samples)
}

fn activity(store: &Store, retention_days: u32, now: i64) -> Result<BTreeMap<String, i64>> {
    let start = now - i64::from(retention_days) * 86_400;
    Ok(store
        .last_usage_dates(None, start as f64, now as f64)?
        .into_iter()
        .map(|(key, at)| (key, at as i64))
        .collect())
}

pub fn model_activity(settings: &Settings, now: i64) -> Result<BTreeMap<String, i64>> {
    if !settings.demo {
        return activity(&*history_store()?, settings.retention_days, now);
    }
    // The Shell asks on every Settings render, and the seeded demo history never changes.
    let mut cached = DEMO_ACTIVITY.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some((days, activity)) = cached.as_ref()
        && *days == settings.retention_days
    {
        return Ok(activity.clone());
    }
    let computed = activity(&*demo_store()?, settings.retention_days, now)?;
    *cached = Some((settings.retention_days, computed.clone()));
    Ok(computed)
}

pub fn replace_analytics(
    provider: &str,
    fingerprint: Option<&str>,
    metrics: &[&str],
    start: &str,
    end: &str,
    points: &[analytics::Point],
) -> Result<()> {
    let coverage = Coverage { metrics, start, end };
    replace_analytics_into(&*history_store()?, provider, fingerprint, &coverage, points, now())
}

pub struct Coverage<'a> {
    pub metrics: &'a [&'a str],
    pub start: &'a str,
    pub end: &'a str,
}

pub fn replace_analytics_into(
    store: &Store,
    provider: &str,
    fingerprint: Option<&str>,
    coverage: &Coverage<'_>,
    points: &[analytics::Point],
    now: f64,
) -> Result<()> {
    let Coverage { metrics, start, end } = *coverage;
    let points = points
        .iter()
        .filter(|point| {
            point.provider == provider
                && point.day.as_str() >= start
                && point.day.as_str() <= end
                && metrics.contains(&point.metric.as_str())
        })
        .cloned()
        .collect::<Vec<_>>();
    let scope = CoverageScope {
        start_day: start.into(),
        end_day: end.into(),
        metrics: metrics.iter().map(|metric| (*metric).into()).collect(),
    };
    store.record_current_analytics(provider, fingerprint, now, &points, &[scope])?;
    Ok(())
}

pub fn record_snapshot(snapshot: &analytics::AnalyticsSnapshot) -> Result<bool> {
    record_snapshot_into(&*history_store()?, snapshot)
}

pub fn record_snapshot_into(store: &Store, snapshot: &analytics::AnalyticsSnapshot) -> Result<bool> {
    Ok(store
        .record_current_analytics(
            &snapshot.provider,
            snapshot.account_fingerprint.as_deref(),
            snapshot.fetched_at,
            &snapshot.points,
            &snapshot.covered_scopes,
        )?
        .is_some())
}

pub fn clear() -> Result<()> {
    history_store()?.clear()?;
    Ok(())
}

pub fn prune(days: u32) -> Result<()> {
    history_store()?.apply_retention_days(days, now())?;
    Ok(())
}

pub fn rolled_history(settings: &Settings, start: i64, end: i64, rollup: WindowRollup, utc: bool) -> Result<History> {
    history::history(store_for(settings)?, start, end, &settings.providers, rollup, utc)
}

pub fn history_analytics(settings: &Settings, metric: &str, start: i64, end: i64) -> Result<History> {
    history::history_analytics(&*store_for(settings)?, metric, start, end, &settings.providers)
}

pub fn review_counts(settings: &Settings, now: Timestamp) -> Result<Option<analytics::ReviewCounts>> {
    history::review_counts(
        &*store_for(settings)?,
        &settings.providers,
        settings.retention_days,
        now,
    )
}

pub fn spend_report(settings: &Settings, now: Timestamp) -> Result<analytics::SpendReport> {
    history::spend_report(&*store_for(settings)?, &settings.providers, now)
}

pub fn export(
    settings: &Settings,
    path: &Path,
    start: i64,
    end: i64,
    selected: Option<&BTreeSet<String>>,
) -> Result<()> {
    export_from(&*store_for(settings)?, settings, path, (start, end), selected)
}

pub fn export_from(
    store: &Store,
    settings: &Settings,
    path: &Path,
    (start, end): (i64, i64),
    selected: Option<&BTreeSet<String>>,
) -> Result<()> {
    let enabled = enabled(&settings.providers);
    let visible = |key: &String| {
        key.split_once(':')
            .is_some_and(|(provider, _)| enabled.iter().any(|id| id == provider))
    };
    let keys = match selected {
        Some(selected) => selected.iter().filter(|key| visible(key)).cloned().collect::<Vec<_>>(),
        None => store
            .summaries()?
            .into_iter()
            .map(|summary| summary.key)
            .filter(visible)
            .collect(),
    };
    store.export_samples(path, start as f64, end as f64, Some(&keys), true)
}

pub fn export_analytics(settings: &Settings, path: &Path, metric: &str, start: i64, end: i64) -> Result<()> {
    export_analytics_from(&*store_for(settings)?, settings, path, metric, (start, end))
}

pub fn export_analytics_from(
    store: &Store,
    settings: &Settings,
    path: &Path,
    metric: &str,
    (start, end): (i64, i64),
) -> Result<()> {
    store.export_analytics(
        path,
        metric,
        &enabled(&settings.providers),
        start as f64,
        end as f64,
        settings.hide_identity,
    )
}

pub fn export_all(settings: &Settings, path: &Path) -> Result<()> {
    store_for(settings)?.export_all(path, settings.hide_identity)
}

#[cfg(test)]
#[path = "../tests/unit/store/tests.rs"]
mod tests;
