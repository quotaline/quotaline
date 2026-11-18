//! Reads the redacted export and preserves the last usable snapshot.

use jiff::Timestamp;
use serde::Deserialize;
use std::{
    fmt,
    fs::File,
    io::{self, BufReader},
};

use crate::Source;

pub(crate) struct Snapshot {
    pub(crate) providers: Vec<Provider>,
    pub(crate) warning: Option<String>,
}

impl Snapshot {
    pub(crate) fn refresh(&mut self, source: &Source) {
        match source.read() {
            Ok(export) => {
                self.providers = export.providers;
                self.warning = None;
            }
            Err(error) => self.warning = Some(error.to_string()),
        }
    }
}

impl Source {
    fn read(&self) -> io::Result<Export> {
        let mut export: Export = match self {
            Self::File(path) => {
                let file = File::from(rustix::fs::open(
                    path,
                    rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
                    rustix::fs::Mode::empty(),
                )?);
                if !file.metadata()?.is_file() {
                    return Err(io::Error::other("expected a regular snapshot file"));
                }
                serde_json::from_reader(BufReader::new(file))?
            }
            Self::Demo => serde_json::from_slice(&quotaline_export::UsageExport::from(&quotaline_demo::demo()).json())?,
        };
        for provider in &mut export.providers {
            provider.id.retain(|character| !character.is_control());
            if let Some(plan) = &mut provider.plan {
                plan.retain(|character| !character.is_control());
            }
            for window in &mut provider.windows {
                window.label.retain(|character| !character.is_control());
            }
        }
        Ok(export)
    }
}

impl fmt::Display for Source {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => formatter.write_str(&path.to_string_lossy().replace(char::is_control, "")),
            Self::Demo => formatter.write_str("Demo data"),
        }
    }
}

#[derive(Deserialize)]
struct Export {
    providers: Vec<Provider>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Provider {
    pub(crate) id: String,
    pub(crate) plan: Option<String>,
    pub(crate) fetched_at: Timestamp,
    pub(crate) windows: Vec<Window>,
}

impl Provider {
    pub(crate) fn name(&self) -> &str {
        quotaline_catalog::definition(&self.id).map_or(&self.id, |provider| provider.name)
    }

    pub(crate) fn most_used(&self) -> Option<(&Window, f64)> {
        self.windows
            .iter()
            .filter_map(|window| window.used_percent.map(|percent| (window, percent)))
            .max_by(|(_, left), (_, right)| left.total_cmp(right))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Window {
    pub(crate) label: String,
    pub(crate) used_percent: Option<f64>,
    pub(crate) resets_at: Option<Timestamp>,
}
