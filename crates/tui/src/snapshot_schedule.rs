//! Keeps only one snapshot request in flight.

use std::time::{Duration, Instant};

pub(crate) struct SnapshotSchedule {
    due_at: Instant,
    pending: bool,
}

impl SnapshotSchedule {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            due_at: now,
            pending: false,
        }
    }

    pub(crate) fn start(&mut self, now: Instant) -> bool {
        if self.pending || now < self.due_at {
            return false;
        }
        self.pending = true;
        self.due_at = now + Duration::from_secs(2);
        true
    }

    pub(crate) fn finish(&mut self) {
        self.pending = false;
    }

    pub(crate) fn reload(&mut self, now: Instant) {
        self.due_at = now;
    }
}

#[cfg(test)]
#[path = "../tests/unit/snapshot_schedule.rs"]
mod tests;
