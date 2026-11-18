//! Stores bounded diagnostics for the GNOME service in the text log format both desktops share.

use jiff::Timestamp;
use quotaline_logging::buffer;
use quotaline_logging::buffer::Buffer;
use quotaline_logging::buffer::FILE_BYTE_LIMIT;
use quotaline_logging::buffer::FILE_COUNT;
use quotaline_logging::buffer::RETAINED_ENTRY_LIMIT;
use quotaline_logging::buffer::Record;
use quotaline_logging::entry;
use quotaline_storage::logs;
use serde::Serialize;
use std::{
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Entry {
    pub timestamp: i64,
    pub level: String,
    pub category: String,
    pub message: String,
    pub line: String,
}

impl From<Record> for Entry {
    fn from(record: Record) -> Self {
        Self {
            timestamp: (record.timestamp * 1_000.0).round() as i64,
            level: record.level,
            category: record.category,
            message: record.message,
            line: record.line,
        }
    }
}

pub struct LogStore {
    path: PathBuf,
    entries: Mutex<Buffer>,
}

impl LogStore {
    pub fn new(path: PathBuf) -> Self {
        let now = now();
        let entries = load(&path, now);
        Self {
            path,
            entries: Mutex::new(Buffer::new(entries, now, false)),
        }
    }

    pub fn record(&self, level: &str, category: &str, message: &str) -> io::Result<()> {
        let now = now();
        let record = entry::create(now, level, category, message, home().as_deref(), 0);
        let line = record.line.clone();
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .append(record, now);
        logs::append(&self.path, &[line], FILE_BYTE_LIMIT, FILE_COUNT)
    }

    pub fn tail(&self) -> Vec<Entry> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot()
            .into_iter()
            .map(Entry::from)
            .collect()
    }

    pub fn retained(&self) -> Vec<Entry> {
        load(&self.path, now()).into_iter().map(Entry::from).collect()
    }

    pub fn clear(&self) -> io::Result<()> {
        let mut entries = self.entries.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        logs::clear(&self.path, FILE_COUNT)?;
        entries.clear();
        Ok(())
    }
}

fn now() -> f64 {
    Timestamp::now().as_millisecond() as f64 / 1_000.0
}

fn home() -> Option<String> {
    std::env::var("HOME").ok()
}

fn load(path: &Path, now: f64) -> Vec<Record> {
    logs::load(
        path,
        FILE_COUNT,
        FILE_BYTE_LIMIT,
        RETAINED_ENTRY_LIMIT,
        now - buffer::RETENTION_SECONDS,
        home().as_deref(),
    )
    .entries
    .into_iter()
    .map(|line| entry::retained(line, 0))
    .collect()
}

#[cfg(test)]
#[path = "../tests/unit/logging/tests.rs"]
mod tests;
