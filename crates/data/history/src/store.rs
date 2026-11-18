//! Stores usage samples and provider analytics in one SQLite format for every desktop frontend.

mod demo;
mod export;
mod legacy;
mod provider;
mod query;
mod schema;
mod write;

use std::{path::Path, rc::Rc, sync::Mutex, time::Duration};

use rusqlite::{Connection, OpenFlags, types::Value, vtab::array::Array};
use thiserror::Error;

pub use demo::DEMO_DAYS;
pub use quotaline_analytics::{CoverageScope, Point as AnalyticsPoint, WindowSample as Sample};

pub const SCHEMA_VERSION: i32 = 2;
pub const DEFAULT_RETENTION_DAYS: u32 = quotaline_settings::DEFAULT_HISTORY_RETENTION_DAYS;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("unsupported history schema version {0}")]
    Schema(i32),
    #[error("{0}")]
    Database(#[from] rusqlite::Error),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("Export directory does not exist")]
    ExportDirectory,
    #[error("{0}")]
    Csv(#[from] csv::Error),
}

pub type Result<T> = std::result::Result<T, HistoryError>;

fn array(values: &[String]) -> Array {
    Rc::new(values.iter().cloned().map(Value::Text).collect())
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct SampleWrite {
    pub sample: Sample,
    pub label: String,
}

pub struct SampleQuery<'a> {
    pub keys: Option<&'a [String]>,
    pub start: f64,
    pub end: f64,
    pub rollup: Option<f64>,
    pub time_zone: &'a str,
    pub fallback_offset: i32,
    pub includes_end: bool,
}

pub struct SnapshotWrite<'a> {
    pub provider: &'a str,
    pub fingerprint: Option<&'a str>,
    pub now: f64,
    pub fetched_at: f64,
    pub network: bool,
    pub credit_balance: Option<f64>,
    pub credit_unlimited: bool,
    pub windows: &'a [SampleWrite],
}

// The macOS app already has a Swift type with this name.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
#[cfg_attr(feature = "uniffi", uniffi(name = "StoredHistoryWindowSummary"))]
pub struct WindowSummary {
    pub key: String,
    pub label: String,
    pub last_seen: f64,
    pub last_percent: f64,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct Stats {
    pub sample_count: u64,
    pub analytics_count: u64,
    pub oldest: Option<f64>,
    pub newest: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PruneResult {
    pub samples: u64,
    pub analytics: u64,
}

struct State {
    db: Connection,
    retention_days: u32,
    last_prune: Option<f64>,
}

pub struct Store {
    state: Mutex<State>,
    interrupt: rusqlite::InterruptHandle,
}

impl Store {
    pub fn open(path: Option<&Path>, retention_days: u32) -> Result<Self> {
        if let Some(parent) = path.and_then(Path::parent) {
            std::fs::create_dir_all(parent)?;
        }
        let mut db = match path {
            Some(path) => Connection::open(path)?,
            None => Connection::open_in_memory()?,
        };
        db.busy_timeout(Duration::from_secs(5))?;
        rusqlite::vtab::array::load_module(&db)?;
        db.execute_batch("PRAGMA auto_vacuum=INCREMENTAL; PRAGMA journal_mode=WAL")?;
        schema::migrate(&mut db)?;
        Ok(Self::from_connection(db, retention_days))
    }

    pub fn open_read_only(path: &Path, retention_days: u32) -> Result<Self> {
        let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        db.busy_timeout(Duration::from_secs(2))?;
        rusqlite::vtab::array::load_module(&db)?;
        let version = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if !(1..=SCHEMA_VERSION).contains(&version) {
            return Err(HistoryError::Schema(version));
        }
        Ok(Self::from_connection(db, retention_days))
    }

    fn from_connection(db: Connection, retention_days: u32) -> Self {
        let interrupt = db.get_interrupt_handle();
        Self {
            state: Mutex::new(State {
                db,
                retention_days: quotaline_settings::normalize_history_retention_days(retention_days),
                last_prune: None,
            }),
            interrupt,
        }
    }

    pub fn interrupt(&self) {
        self.interrupt.interrupt();
    }

    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn retention_days(&self) -> u32 {
        self.state().retention_days
    }

    pub fn set_retention_days(&self, days: u32) {
        let mut state = self.state();
        let days = quotaline_settings::normalize_history_retention_days(days);
        if state.retention_days != days {
            state.retention_days = days;
            state.last_prune = None;
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/store/tests.rs"]
mod tests;
