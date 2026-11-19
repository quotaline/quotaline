//! Shares one engine's live readings with the other engines on the machine through a local SQLite file.
//!
//! The engine holding the lease polls providers and publishes what it learns; the others read the published state,
//! queue refresh requests for the holder, and claim the notices it plans.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{fmt, path::Path, sync::Mutex, time::Duration};

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS lease (role TEXT PRIMARY KEY, holder TEXT NOT NULL, expires REAL NOT NULL);
    CREATE TABLE IF NOT EXISTS published (id INTEGER PRIMARY KEY CHECK (id = 1), state TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS requests (id INTEGER PRIMARY KEY AUTOINCREMENT, request TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS presence (holder TEXT PRIMARY KEY, seen REAL NOT NULL);
    CREATE TABLE IF NOT EXISTS outbox (id INTEGER PRIMARY KEY AUTOINCREMENT, created REAL NOT NULL, notice TEXT NOT NULL);";

#[derive(Debug)]
pub enum LiveError {
    Sql(rusqlite::Error),
    Json(serde_json::Error),
}

impl fmt::Display for LiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sql(error) => error.fmt(formatter),
            Self::Json(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LiveError {}

impl From<rusqlite::Error> for LiveError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}

impl From<serde_json::Error> for LiveError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

pub type Result<T> = std::result::Result<T, LiveError>;

struct Connected {
    db: Connection,
    data_version: i64,
}

pub struct Live {
    connected: Mutex<Connected>,
    holder: String,
}

impl Live {
    pub fn open(path: &Path, holder: String) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL;")?;
        db.execute_batch(SCHEMA)?;
        let data_version = db.pragma_query_value(None, "data_version", |row| row.get(0))?;
        Ok(Self {
            connected: Mutex::new(Connected { db, data_version }),
            holder,
        })
    }

    fn with<T>(&self, work: impl FnOnce(&mut Connected) -> Result<T>) -> Result<T> {
        work(&mut self.connected.lock().unwrap_or_else(std::sync::PoisonError::into_inner))
    }

    /// Takes the polling lease when it is free or expired, or renews it; true while this engine holds it.
    pub fn lead(&self, now: f64, ttl: f64) -> Result<bool> {
        self.with(|connected| {
            let changed = connected.db.execute(
                "INSERT INTO lease (role, holder, expires) VALUES ('poller', ?1, ?2)
                 ON CONFLICT (role) DO UPDATE SET holder = excluded.holder, expires = excluded.expires
                 WHERE lease.holder = excluded.holder OR lease.expires <= ?3",
                params![self.holder, now + ttl, now],
            )?;
            Ok(changed == 1)
        })
    }

    /// Gives the lease up, so another engine can poll without waiting for it to expire.
    pub fn resign(&self) -> Result<()> {
        self.with(|connected| {
            connected.db.execute(
                "DELETE FROM lease WHERE role = 'poller' AND holder = ?1",
                params![self.holder],
            )?;
            connected
                .db
                .execute("DELETE FROM presence WHERE holder = ?1", params![self.holder])?;
            Ok(())
        })
    }

    pub fn publish(&self, state: &impl Serialize) -> Result<()> {
        let state = serde_json::to_string(state)?;
        self.with(|connected| {
            connected.db.execute(
                "INSERT INTO published (id, state) VALUES (1, ?1) ON CONFLICT (id) DO UPDATE SET state = excluded.state",
                params![state],
            )?;
            Ok(())
        })
    }

    pub fn published<T: DeserializeOwned>(&self) -> Result<Option<T>> {
        let state = self.with(|connected| {
            Ok(connected
                .db
                .query_row("SELECT state FROM published WHERE id = 1", [], |row| {
                    row.get::<_, String>(0)
                })
                .optional()?)
        })?;
        Ok(state.map(|state| serde_json::from_str(&state)).transpose()?)
    }

    /// Whether another engine committed anything since the last call; this engine's own writes do not count.
    pub fn changed(&self) -> Result<bool> {
        self.with(|connected| {
            let version = connected
                .db
                .pragma_query_value(None, "data_version", |row| row.get(0))?;
            let changed = version != connected.data_version;
            connected.data_version = version;
            Ok(changed)
        })
    }

    pub fn request(&self, request: &impl Serialize) -> Result<()> {
        let request = serde_json::to_string(request)?;
        self.with(|connected| {
            connected
                .db
                .execute("INSERT INTO requests (request) VALUES (?1)", params![request])?;
            Ok(())
        })
    }

    pub fn take_requests<T: DeserializeOwned>(&self) -> Result<Vec<T>> {
        let requests = self.with(|connected| {
            let mut requests = connected
                .db
                .prepare("DELETE FROM requests RETURNING id, request")?
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            requests.sort_unstable_by_key(|(id, _)| *id);
            Ok(requests)
        })?;
        Ok(requests
            .into_iter()
            .map(|(_, request)| serde_json::from_str(&request))
            .collect::<serde_json::Result<_>>()?)
    }

    /// Records whether this engine's front end shows its panel, so the polling engine refreshes at the visible pace.
    pub fn present(&self, visible: bool, now: f64) -> Result<()> {
        self.with(|connected| {
            if visible {
                connected.db.execute(
                    "INSERT INTO presence (holder, seen) VALUES (?1, ?2)
                     ON CONFLICT (holder) DO UPDATE SET seen = excluded.seen",
                    params![self.holder, now],
                )?;
            } else {
                connected
                    .db
                    .execute("DELETE FROM presence WHERE holder = ?1", params![self.holder])?;
            }
            Ok(())
        })
    }

    /// Whether another engine reported a visible panel within the last `fresh` seconds.
    pub fn visible(&self, now: f64, fresh: f64) -> Result<bool> {
        self.with(|connected| {
            Ok(connected.db.query_row(
                "SELECT EXISTS (SELECT 1 FROM presence WHERE seen > ?1 AND holder != ?2)",
                params![now - fresh, self.holder],
                |row| row.get(0),
            )?)
        })
    }

    pub fn post<T: Serialize>(&self, notices: &[T], now: f64) -> Result<()> {
        let notices = notices
            .iter()
            .map(serde_json::to_string)
            .collect::<serde_json::Result<Vec<_>>>()?;
        self.with(|connected| {
            let transaction = connected.db.transaction()?;
            for notice in &notices {
                transaction.execute(
                    "INSERT INTO outbox (created, notice) VALUES (?1, ?2)",
                    params![now, notice],
                )?;
            }
            transaction.commit()?;
            Ok(())
        })
    }

    /// How many notices planned within the last `max_age` seconds wait for an engine to claim them.
    pub fn pending(&self, now: f64, max_age: f64) -> Result<u32> {
        self.with(|connected| {
            Ok(connected.db.query_row(
                "SELECT COUNT(*) FROM outbox WHERE created >= ?1",
                params![now - max_age],
                |row| row.get(0),
            )?)
        })
    }

    /// Claims every notice planned within the last `max_age` seconds; older ones are dropped unseen.
    pub fn claim<T: DeserializeOwned>(&self, now: f64, max_age: f64) -> Result<Vec<T>> {
        let notices = self.with(|connected| {
            let transaction = connected.db.transaction()?;
            transaction.execute("DELETE FROM outbox WHERE created < ?1", params![now - max_age])?;
            let mut notices = transaction
                .prepare("DELETE FROM outbox RETURNING id, notice")?
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            transaction.commit()?;
            // RETURNING yields rows in no promised order, and notices should show in the order they were planned.
            notices.sort_unstable_by_key(|(id, _)| *id);
            Ok(notices)
        })?;
        Ok(notices
            .into_iter()
            .map(|(_, notice)| serde_json::from_str(&notice))
            .collect::<serde_json::Result<_>>()?)
    }
}

#[cfg(test)]
#[path = "../tests/unit/live/tests.rs"]
mod tests;
