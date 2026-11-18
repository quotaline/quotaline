//! Formats diagnostic records and preserves identity across retained and live entries.

use jiff::Timestamp;

use super::{buffer::Record, message, parse::ParsedLine};

pub fn create(
    timestamp: f64,
    level: &str,
    category: &str,
    value: &str,
    home: Option<&str>,
    sequence_id: u64,
) -> Record {
    let safe = message(value, home);
    record(timestamp, level, category, safe, None, sequence_id)
}

pub fn record(
    timestamp: f64,
    level: &str,
    category: &str,
    message: String,
    original_line: Option<String>,
    sequence_id: u64,
) -> Record {
    let line = original_line.unwrap_or_else(|| format_line(timestamp, level, category, &message));
    Record {
        timestamp,
        level: level.into(),
        category: category.into(),
        message,
        line,
        sequence_id,
    }
}

pub fn retained(line: ParsedLine, sequence_id: u64) -> Record {
    record(
        line.timestamp,
        line.level,
        line.category,
        line.message,
        line.original_line,
        sequence_id,
    )
}

pub fn align(retained: &mut [Record], live: &[Record]) {
    for (stored, current) in retained.iter_mut().rev().zip(live.iter().rev()) {
        if stored.line != current.line {
            break;
        }
        stored.sequence_id = current.sequence_id;
    }
}

fn format_line(timestamp: f64, level: &str, category: &str, message: &str) -> String {
    let milliseconds = (timestamp * 1_000.0).round() as i64;
    let stamp = Timestamp::from_millisecond(milliseconds)
        .expect("diagnostic timestamp is valid")
        .strftime("%Y-%m-%d %H:%M:%S%.3f");
    let prefix = format!("[{stamp}] [{level}]");
    if category == "app" {
        format!("{prefix} {message}")
    } else {
        format!("{prefix} [{category}] {message}")
    }
}

#[cfg(test)]
#[path = "../tests/unit/entry/tests.rs"]
mod tests;
