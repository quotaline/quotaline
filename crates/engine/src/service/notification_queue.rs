//! Holds planned notices until the front end is ready to show them.

use quotaline_notifications::Event;
use std::collections::VecDeque;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Planned {
    pub provider: String,
    pub event: Event,
    pub sign_in: Option<String>,
    pub fetched_at: i64,
}

pub(super) struct NotificationQueue<const CAPACITY: usize>(VecDeque<Planned>);

impl<const CAPACITY: usize> Default for NotificationQueue<CAPACITY> {
    fn default() -> Self {
        Self(VecDeque::new())
    }
}

impl<const CAPACITY: usize> NotificationQueue<CAPACITY> {
    pub(super) fn extend(&mut self, events: impl IntoIterator<Item = Planned>) {
        self.0.extend(events);
        if self.0.len() > CAPACITY {
            self.0.drain(..self.0.len() - CAPACITY);
        }
    }

    pub(super) fn take(&mut self) -> Vec<Planned> {
        std::mem::take(&mut self.0).into_iter().collect()
    }

    #[cfg(feature = "fixture-api")]
    pub(super) fn len(&self) -> usize {
        self.0.len()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/service/notification_queue/tests.rs"]
mod tests;
