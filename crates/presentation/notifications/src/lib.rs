//! Plans quota and account transition notifications.

use jiff::Timestamp;
use quotaline_quota::pace::{PaceEstimate, PaceStatus};
use quotaline_quota::{Provider, ResetPrecision, Window};
use quotaline_settings::{DEFAULT_THRESHOLDS, Settings};
use std::collections::{BTreeMap, BTreeSet};

mod copy;
mod delivery;
pub use copy::{Copy, Moments, Selection, copy, summary};
pub use delivery::{CredentialKey, DeliveryEvent, DeliveryLedger};

pub const THRESHOLD_CHOICES: [u8; 4] = [50, 75, 90, 100];
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Threshold,
    Reset,
    Authentication,
    Credits,
    ExpiringCredit,
    WillRunOut,
    AheadOfPace,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct ThresholdCrossing {
    pub label: String,
    pub used: f64,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Facts {
    Authentication {
        detail: Option<String>,
    },
    Reset {
        label: String,
        used: f64,
    },
    ThresholdGroup {
        crossings: Vec<ThresholdCrossing>,
        lowest: u8,
    },
    Threshold {
        label: String,
        used: f64,
        threshold: u8,
        reset_at: Option<i64>,
        precision: ResetPrecision,
    },
    Credits,
    ExpiringCredit {
        expires_at: i64,
    },
    WillRunOut {
        label: String,
        exhaustion_at: i64,
        reset_at: i64,
    },
    AheadOfPace {
        label: String,
        used: f64,
        reset_at: i64,
    },
}

#[derive(Default)]
pub struct NotificationLedger(BTreeSet<String>);

impl NotificationLedger {
    pub fn accept(&mut self, event: &Event) -> bool {
        self.0.insert(event.id.clone())
    }

    pub fn trim(&mut self) {
        if self.0.len() > 512 {
            self.0.clear();
        }
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[derive(Default)]
pub struct NotificationRoutes(BTreeMap<u32, String>);

impl NotificationRoutes {
    pub fn insert(&mut self, id: u32, provider: &str) {
        if self.0.len() >= 512 {
            self.0.clear();
        }
        self.0.insert(id, provider.into());
    }

    pub fn take(&mut self, id: u32) -> Option<String> {
        self.0.remove(&id)
    }
}

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct Event {
    pub id: String,
    pub kind: Kind,
    pub window_id: Option<String>,
    pub facts: Facts,
}

pub fn auth_problem(
    previous: Option<&Provider>,
    current: &Provider,
    settings: &Settings,
    credential_missing: bool,
) -> Option<Event> {
    (settings.notifications
        && settings.notify_sign_in
        && !credential_missing
        && current.status == "sign-in"
        && previous
            .is_some_and(|old| old.status != "sign-in" && !(old.status == "stale" && old.notice == current.notice)))
    .then(|| {
        event(
            Kind::Authentication,
            format!(
                "{}:auth:{}",
                current.id,
                previous.and_then(|old| old.updated).unwrap_or(0)
            ),
            Facts::Authentication {
                detail: Some(current.notice.clone()),
            },
        )
    })
}

pub fn auth_transition(
    provider: &str,
    previous_required: bool,
    current_required: bool,
    credential_missing: bool,
    settings: &Settings,
) -> Option<Event> {
    (settings.notifications && settings.notify_sign_in && !credential_missing && !previous_required && current_required)
        .then(|| {
            event(
                Kind::Authentication,
                format!("{provider}:auth"),
                Facts::Authentication { detail: None },
            )
        })
}

pub fn plan(
    previous: Option<&Provider>,
    current: &Provider,
    settings: &Settings,
    samples: &[(String, i64, f64)],
    now: i64,
) -> Vec<Event> {
    if !settings.notifications {
        return Vec::new();
    }
    let mut events = Vec::new();
    if let Some(previous) = previous {
        let mut crossings = Vec::new();
        let mut resets = Vec::new();
        let mut pace_events = Vec::new();
        for row in &current.windows {
            let Some(before) = previous.windows.iter().find(|item| item.id == row.id) else {
                continue;
            };
            let reset = quotaline_quota::reset_groups::has_reset(
                reset_at(before).map(|at| at as f64),
                reset_at(row).map(|at| at as f64),
                before.used,
                row.used,
            );
            if reset {
                let floor = settings
                    .notify_thresholds
                    .first()
                    .copied()
                    .unwrap_or(DEFAULT_THRESHOLDS[0]);
                if settings.notify_resets && before.used >= f64::from(floor) {
                    resets.push(
                        event(
                            Kind::Reset,
                            format!("{}:{}:reset:{}", current.id, row.id, reset_at(row).unwrap_or(0)),
                            Facts::Reset {
                                label: row.label.clone(),
                                used: row.used,
                            },
                        )
                        .for_window(&row.id),
                    );
                }
            } else if let Some(threshold) = settings
                .notify_thresholds
                .iter()
                .copied()
                .filter(|threshold| before.used < f64::from(*threshold) && row.used >= f64::from(*threshold))
                .max()
            {
                crossings.push((row, threshold));
            }
            if settings.notify_pace {
                let history: Vec<_> = samples
                    .iter()
                    .filter(|(id, _, _)| id == &row.id)
                    .map(|(_, at, used)| (*at, *used))
                    .collect();
                let pace = PaceEstimate::estimate(row, &history, settings.workdays, now);
                let prior_pace = if reset {
                    None
                } else {
                    Some(PaceEstimate::estimate(
                        before,
                        &history,
                        settings.workdays,
                        previous.updated.unwrap_or(now),
                    ))
                };
                if let Some(at) = pace.projected_exhaustion.filter(|_| {
                    pace.status != PaceStatus::Unknown
                        && pace.status != PaceStatus::Exhausted
                        && prior_pace
                            .as_ref()
                            .is_none_or(|old| old.projected_exhaustion.is_none() || old.status == PaceStatus::Unknown)
                }) {
                    pace_events.push(
                        event(
                            Kind::WillRunOut,
                            format!("{}:{}:pace-runout:{}", current.id, row.id, reset_at(row).unwrap_or(0)),
                            Facts::WillRunOut {
                                label: row.label.clone(),
                                exhaustion_at: at,
                                reset_at: reset_at(row).unwrap_or(now),
                            },
                        )
                        .for_window(&row.id),
                    );
                }
                if pace.status == PaceStatus::Ahead
                    && prior_pace.as_ref().is_none_or(|old| old.status != PaceStatus::Ahead)
                {
                    pace_events.push(
                        event(
                            Kind::AheadOfPace,
                            format!("{}:{}:pace-ahead:{}", current.id, row.id, reset_at(row).unwrap_or(0)),
                            Facts::AheadOfPace {
                                label: row.label.clone(),
                                used: row.used,
                                reset_at: reset_at(row).unwrap_or(now),
                            },
                        )
                        .for_window(&row.id),
                    );
                }
            }
        }
        if crossings.len() > 1 {
            let lowest = crossings
                .iter()
                .map(|(_, threshold)| *threshold)
                .min()
                .unwrap_or(DEFAULT_THRESHOLDS[0]);
            let key = crossings
                .iter()
                .map(|(row, _)| row.id.as_str())
                .collect::<Vec<_>>()
                .join("+");
            let epochs = crossings
                .iter()
                .map(|(row, _)| reset_at(row).unwrap_or(0).to_string())
                .collect::<Vec<_>>()
                .join("+");
            let crossings = crossings
                .iter()
                .map(|(row, _)| ThresholdCrossing {
                    label: row.label.clone(),
                    used: row.used,
                })
                .collect();
            events.push(event(
                Kind::Threshold,
                format!("{}:thresholds:{key}:{lowest}:{epochs}", current.id),
                Facts::ThresholdGroup { crossings, lowest },
            ));
        } else if let Some((row, threshold)) = crossings.pop() {
            events.push(
                event(
                    Kind::Threshold,
                    format!("{}:{}:{threshold}:{}", current.id, row.id, reset_at(row).unwrap_or(0)),
                    Facts::Threshold {
                        label: row.label.clone(),
                        used: row.used,
                        threshold,
                        reset_at: reset_at(row),
                        precision: row.reset_precision,
                    },
                )
                .for_window(&row.id),
            );
        }
        events.extend(resets);
        events.extend(pace_events);
        if previous.has_credits() == Some(true) && current.has_credits() == Some(false) {
            events.push(event(Kind::Credits, format!("{}:credits", current.id), Facts::Credits));
        }
    }
    if settings.notify_expiring_credits {
        for credit in &current.reset_credit_expiries {
            if credit.at <= now || credit.at - now > 86_400 {
                continue;
            }
            let warned = previous.is_some_and(|prior| {
                prior.reset_credit_expiries.iter().any(|item| {
                    item == credit
                        && prior
                            .updated
                            .is_some_and(|at| credit.at > at && credit.at - at <= 86_400)
                })
            });
            if !warned {
                events.push(event(
                    Kind::ExpiringCredit,
                    format!("{}:reset-credit:{}:{}", current.id, credit.id, credit.at),
                    Facts::ExpiringCredit { expires_at: credit.at },
                ));
            }
        }
    }
    events
}

fn event(kind: Kind, id: String, facts: Facts) -> Event {
    Event {
        id,
        kind,
        window_id: None,
        facts,
    }
}

impl Event {
    fn for_window(mut self, window_id: &str) -> Self {
        self.window_id = Some(window_id.into());
        self
    }
}

fn reset_at(window: &Window) -> Option<i64> {
    window
        .reset
        .as_deref()
        .and_then(|value| value.parse::<Timestamp>().ok())
        .map(Timestamp::as_second)
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
