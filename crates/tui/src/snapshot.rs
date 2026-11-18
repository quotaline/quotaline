//! Reads the redacted export and preserves the last usable snapshot.

use jiff::Timestamp;
use serde::Deserialize;
use std::{
    fs::File,
    io::{self, BufReader},
    path::PathBuf,
};

use crate::desktop::State;

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
            Self::Demo => serde_json::from_slice(&quotaline_export::UsageExport::from(&quotaline_demo::demo()).json())
                .map_err(io::Error::from),
        }
    }
}

impl State {
    pub(crate) fn sanitize(&mut self) {
        for provider in &mut self.providers {
            provider.status.retain(|character| !character.is_control());
            provider.id.retain(|character| !character.is_control());
            for detail in &mut provider.details {
                detail.retain(|character| !character.is_control());
            }
            if let Some(plan) = &mut provider.plan {
                plan.retain(|character| !character.is_control());
            }
            for window in &mut provider.windows {
                window.label.retain(|character| !character.is_control());
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
