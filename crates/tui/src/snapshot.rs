//! Reads the redacted export and preserves the last usable snapshot.

use jiff::Timestamp;
use quotaline_catalog::COPILOT;
use serde::Deserialize;
use std::{
    fs::File,
    io::{self, BufReader},
    path::PathBuf,
};

use crate::desktop::State;

mod overview;
#[cfg(all(not(test), target_os = "linux"))]
pub(crate) use overview::OverviewFacts;
pub(crate) use overview::provider_overview;

pub(crate) enum LocalSnapshot {
    File(PathBuf),
    Demo,
}

impl LocalSnapshot {
    pub(crate) fn read(&self) -> io::Result<State> {
        match self {
            Self::File(path) => {
                let file = File::from(rustix::fs::open(
                    path,
                    rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )?);
                if !file.metadata()?.is_file() {
                    return Err(io::Error::other("expected a regular snapshot file"));
                }
                Ok(serde_json::from_reader(BufReader::new(file))?)
            }
            Self::Demo => {
                let now = Timestamp::now();
                let providers = quotaline_demo::demo_at(now);
                let mut state: State = serde_json::from_slice(&quotaline_export::UsageExport::from(&providers).json())?;
                for provider in &mut state.providers {
                    provider.overview = provider_overview(&providers[&provider.id], |_| true, now.as_second());
                }
                Ok(state)
            }
        }
    }
}

impl State {
    pub(crate) fn sanitize(&mut self) {
        for provider in &mut self.providers {
            provider.status.retain(|character| !character.is_control());
            provider.id.retain(|character| !character.is_control());
            for detail in provider.details.iter_mut().chain(&mut provider.overview) {
                detail.retain(|character| !character.is_control());
            }
            if let Some(plan) = &mut provider.plan {
                plan.retain(|character| !character.is_control());
            }
            for window in &mut provider.windows {
                window.label.retain(|character| !character.is_control());
                // Older Copilot readings can contain the epoch in place of a reset date.
                if provider.id == COPILOT.id && window.resets_at == Some(Timestamp::UNIX_EPOCH) {
                    window.resets_at = None;
                }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Provider {
    pub(crate) id: String,
    pub(crate) plan: Option<String>,
    pub(crate) fetched_at: Option<Timestamp>,
    #[serde(skip)]
    pub(crate) status: String,
    #[serde(skip)]
    pub(crate) details: Vec<String>,
    #[serde(skip)]
    pub(crate) overview: Vec<String>,
    pub(crate) windows: Vec<Window>,
}

impl Provider {
    pub(crate) fn name(&self) -> &str {
        quotaline_catalog::definition(&self.id).map_or(&self.id, |provider| provider.name)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Window {
    pub(crate) label: String,
    pub(crate) used_percent: Option<f64>,
    pub(crate) resets_at: Option<Timestamp>,
    #[serde(skip)]
    pub(crate) reset_by_day: bool,
    #[serde(skip)]
    pub(crate) pace: Option<String>,
}
