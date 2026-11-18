//! Tracks analytics progress across incremental refreshes.

use crate::Endpoint;
use crate::utc_day::retained_utc_days;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Coverage {
    start: String,
    through: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Account {
    #[serde(rename = "lastAccess")]
    last_access: f64,
    coverage: BTreeMap<String, Coverage>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct State {
    version: u8,
    accounts: BTreeMap<String, Account>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: BTreeMap::new(),
        }
    }
}

#[derive(Deserialize)]
struct LegacyState {
    accounts: BTreeMap<String, BTreeMap<String, String>>,
}

pub struct Watermarks {
    state: State,
    account: String,
    current: BTreeMap<String, Coverage>,
    planned: BTreeMap<String, String>,
    dirty: bool,
}

impl Watermarks {
    pub fn load(raw: Option<&[u8]>, account: String, now: f64, retention_days: u32) -> Self {
        let (mut state, mut dirty) = match raw {
            None => (State::default(), false),
            Some(raw) => match serde_json::from_slice::<State>(raw) {
                Ok(state) if state.version == 1 => (state, false),
                _ => match serde_json::from_slice::<LegacyState>(raw) {
                    Ok(legacy) => (
                        State {
                            version: 1,
                            accounts: legacy
                                .accounts
                                .into_iter()
                                .map(|(key, dates)| {
                                    (
                                        key,
                                        Account {
                                            last_access: now - 978_307_200.0,
                                            coverage: dates
                                                .into_iter()
                                                .map(|(endpoint, day)| {
                                                    (
                                                        endpoint,
                                                        Coverage {
                                                            start: day.clone(),
                                                            through: day,
                                                        },
                                                    )
                                                })
                                                .collect(),
                                        },
                                    )
                                })
                                .collect(),
                        },
                        true,
                    ),
                    Err(_) => (State::default(), true),
                },
            },
        };
        dirty |= prune(&mut state, now, retention_days);
        let current = state
            .accounts
            .get(&account)
            .map(|stored| stored.coverage.clone())
            .unwrap_or_default();
        Self {
            state,
            account,
            current,
            planned: BTreeMap::new(),
            dirty,
        }
    }

    pub fn plan(&mut self, fallback: &str, end: &str) -> Vec<(Endpoint, String)> {
        self.current.retain(|_, coverage| {
            coverage.start <= coverage.through
                && coverage.through.as_str() >= fallback
                && coverage.through.as_str() <= end
        });
        self.planned = Endpoint::ALL
            .into_iter()
            .map(|endpoint| {
                let start = self
                    .current
                    .get(endpoint.path())
                    .filter(|coverage| coverage.start.as_str() <= fallback)
                    .and_then(|coverage| previous_day(&coverage.through))
                    .map_or_else(|| fallback.to_owned(), |through| through.max(fallback.to_owned()));
                (endpoint.path().to_owned(), start)
            })
            .collect();
        Endpoint::ALL
            .into_iter()
            .map(|endpoint| (endpoint, self.planned[endpoint.path()].clone()))
            .collect()
    }

    pub fn complete(&mut self, successful: &[Endpoint], end: &str, now: f64, retention_days: u32) {
        if successful.is_empty() {
            return;
        }
        for endpoint in successful {
            let Some(start) = self.planned.get(endpoint.path()) else {
                continue;
            };
            let start = self.current.get(endpoint.path()).map_or_else(
                || start.clone(),
                |coverage| coverage.start.as_str().min(start.as_str()).to_owned(),
            );
            self.current.insert(
                endpoint.path().to_owned(),
                Coverage {
                    start,
                    through: end.to_owned(),
                },
            );
        }
        self.state.accounts.insert(
            self.account.clone(),
            Account {
                last_access: now - 978_307_200.0,
                coverage: self.current.clone(),
            },
        );
        prune(&mut self.state, now, retention_days);
        self.dirty = true;
    }

    pub fn encoded_if_changed(&mut self) -> Option<Vec<u8>> {
        if !self.dirty {
            return None;
        }
        self.dirty = false;
        Some(serde_json::to_vec(&self.state).expect("watermark state is serializable"))
    }
}

fn previous_day(day: &str) -> Option<String> {
    Date::strptime("%Y-%m-%d", day)
        .ok()?
        .yesterday()
        .ok()
        .map(|day| day.to_string())
}

fn prune(state: &mut State, now: f64, retention_days: u32) -> bool {
    let previous = state.clone();
    let (cutoff, end) = retained_utc_days(now, retention_days);
    state.accounts.retain(|_, account| {
        account.coverage.retain(|key, coverage| {
            Endpoint::ALL.iter().any(|endpoint| endpoint.path() == key)
                && Date::strptime("%Y-%m-%d", &coverage.start).is_ok()
                && Date::strptime("%Y-%m-%d", &coverage.through).is_ok()
                && coverage.start <= coverage.through
                && coverage.through >= cutoff
                && coverage.through <= end
        });
        for coverage in account.coverage.values_mut() {
            if coverage.start < cutoff {
                coverage.start = cutoff.clone();
            }
        }
        !account.coverage.is_empty()
    });
    if state.accounts.len() > 4 {
        let mut accounts: Vec<_> = state.accounts.iter().collect();
        accounts.sort_by(|(first_key, first), (second_key, second)| {
            second
                .last_access
                .total_cmp(&first.last_access)
                .then_with(|| second_key.cmp(first_key))
        });
        let keep: Vec<_> = accounts.into_iter().take(4).map(|(key, _)| key.clone()).collect();
        state.accounts.retain(|key, _| keep.contains(key));
    }
    *state != previous
}

#[cfg(test)]
#[path = "../tests/unit/watermark/tests.rs"]
mod tests;
