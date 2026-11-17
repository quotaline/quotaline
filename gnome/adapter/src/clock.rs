//! Formats notification times in the session's time zone and clock preference.

use jiff::Timestamp;
use jiff::tz::{Offset, TimeZone};
use quotaline_text::time::ClockFormat;

pub struct SessionClock {
    zone: TimeZone,
    time: &'static str,
}

impl SessionClock {
    pub fn new(zone: TimeZone, twelve_hour: bool) -> Self {
        Self {
            zone,
            time: if twelve_hour { "%-I:%M %p" } else { "%H:%M" },
        }
    }

    fn local(&self, at: i64, pattern: &str) -> String {
        self.zone.to_datetime(instant(at)).strftime(pattern).to_string()
    }
}

fn instant(at: i64) -> Timestamp {
    Timestamp::from_second(at).unwrap_or(Timestamp::UNIX_EPOCH)
}

impl ClockFormat for SessionClock {
    fn utc_day(&self, at: i64) -> String {
        Offset::UTC.to_datetime(instant(at)).strftime("%b %-d").to_string()
    }

    fn time(&self, at: i64) -> String {
        self.local(at, self.time)
    }

    fn weekday_time(&self, at: i64) -> String {
        self.local(at, &format!("%a {}", self.time))
    }

    fn date_time(&self, at: i64) -> String {
        self.local(at, &format!("%b %-d {}", self.time))
    }

    fn same_local_day(&self, at: i64, now: i64) -> bool {
        self.local(at, "%F") == self.local(now, "%F")
    }
}

#[cfg(test)]
#[path = "../tests/unit/clock/tests.rs"]
mod tests;
