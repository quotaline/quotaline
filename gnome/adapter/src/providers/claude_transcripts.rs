//! Connects the shared Claude reader to GNOME history and data paths.

use crate::store;
use jiff::tz::{Offset, TimeZone};
use jiff::{Timestamp, ToSpan};
use quotaline_analytics::Point;
use quotaline_claude::claude_transcripts::LocalTranscriptUsage;
use quotaline_history::claude::{Config, Reader};
use std::{fs, io, path::PathBuf};

pub type LocalUsage = LocalTranscriptUsage;

pub struct Scan {
    pub pending: bool,
    pub usage: Option<LocalUsage>,
    first: String,
    last: String,
    points: Vec<Point>,
}

pub struct Scanner {
    root: PathBuf,
    checkpoint: PathBuf,
    reader: Reader,
}

fn config() -> Config {
    Config {
        maximum_line_bytes: 1_048_576,
        ..Config::default()
    }
}

impl Scanner {
    pub fn new(root: PathBuf, checkpoint: PathBuf) -> Self {
        let reader = Reader::new(root.clone(), Some(checkpoint.clone()), config());
        Self {
            root,
            checkpoint,
            reader,
        }
    }

    pub fn scan(&mut self, retention_days: u32, window_start: i64, discover: bool) -> Scan {
        let now = Timestamp::now();
        if discover {
            self.reader.request_discovery();
        }
        let snapshot = self.reader.refresh(now.as_second() as f64, retention_days, !discover);
        let today = now.to_zoned_datetime(TimeZone::system()).start_of_day().unwrap();
        let today = today.timestamp().as_second() as f64;
        let usage = snapshot
            .state
            .local_usage(window_start as f64, today, now.as_second() as f64);
        let utc_today = Offset::UTC.to_datetime(now).date();
        let first = (utc_today - i64::from(retention_days.clamp(7, 365)).days()).to_string();
        let last = utc_today.to_string();
        let points = snapshot
            .state
            .analytics_without_unpriced_costs()
            .map_or_else(Vec::new, |analytics| {
                analytics.points.into_iter().filter(|point| point.day <= last).collect()
            });
        Scan {
            pending: !snapshot.projects_complete,
            usage,
            first,
            last,
            points,
        }
    }

    pub fn clear(&mut self) -> io::Result<()> {
        match fs::remove_file(&self.checkpoint) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        self.reader = Reader::new(self.root.clone(), Some(self.checkpoint.clone()), config());
        Ok(())
    }
}

pub fn persist(scan: &Scan) -> quotaline_history::Result<()> {
    store::replace_analytics(
        "claude",
        None,
        &quotaline_claude::claude_transcripts::ANALYTICS_METRICS,
        &scan.first,
        &scan.last,
        &scan.points,
    )
}

#[cfg(test)]
#[path = "../../tests/unit/providers/claude_transcripts/tests.rs"]
mod tests;
