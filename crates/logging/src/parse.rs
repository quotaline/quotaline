//! Parses persisted diagnostic text lines in one batch.

use jiff::civil::DateTime;
use jiff::tz::Offset;

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedLine {
    pub timestamp: f64,
    pub level: &'static str,
    pub category: &'static str,
    pub message: String,
    pub original_line: Option<String>,
}

pub fn parse_lines(lines: &[String], previous: f64, cutoff: f64, home: Option<&str>) -> Vec<ParsedLine> {
    let mut previous = previous;
    lines
        .iter()
        .filter_map(|line| {
            let record = parse_line(line, previous, home);
            previous = record.timestamp;
            (record.timestamp >= cutoff).then_some(record)
        })
        .collect()
}

fn parse_line(line: &str, previous: f64, home: Option<&str>) -> ParsedLine {
    let Some(timestamp) = stored_timestamp(line) else {
        return rendered(previous, "info", line, home);
    };
    let after_timestamp = line[25..].trim_start_matches(' ');
    let Some((level, after_level)) =
        bracketed(after_timestamp).and_then(|(level, rest)| log_level(level).map(|level| (level, rest)))
    else {
        return rendered(timestamp, "info", after_timestamp, home);
    };
    let after_level = after_level.trim_start_matches(' ');
    let (category, message) = bracketed(after_level)
        .and_then(|(category, rest)| log_category(category).map(|category| (category, rest.trim_start_matches(' '))))
        .unwrap_or(("app", after_level));
    let safe = crate::message(message, home);
    ParsedLine {
        timestamp,
        level,
        category,
        original_line: (safe == message).then(|| line.to_owned()),
        message: safe,
    }
}

fn rendered(timestamp: f64, level: &'static str, message: &str, home: Option<&str>) -> ParsedLine {
    ParsedLine {
        timestamp,
        level,
        category: "app",
        message: crate::message(message, home),
        original_line: None,
    }
}

fn bracketed(value: &str) -> Option<(&str, &str)> {
    value.strip_prefix('[')?.split_once(']')
}

fn log_level(value: &str) -> Option<&'static str> {
    match value {
        "debug" => Some("debug"),
        "info" => Some("info"),
        "warning" => Some("warning"),
        "error" => Some("error"),
        _ => None,
    }
}

fn log_category(value: &str) -> Option<&'static str> {
    match value {
        "app" => Some("app"),
        "geometry" => Some("geometry"),
        "network" => Some("network"),
        "persistence" => Some("persistence"),
        "refresh" => Some("refresh"),
        "status" => Some("status"),
        "tabs" => Some("tabs"),
        _ => None,
    }
}

fn stored_timestamp(line: &str) -> Option<f64> {
    let prefix = line.as_bytes().get(..25)?;
    if prefix[0] != b'[' || prefix[24] != b']' {
        return None;
    }
    let stamp = line.get(1..24)?;
    let date = DateTime::strptime("%Y-%m-%d %H:%M:%S%.3f", stamp).ok()?;
    (1..=9999)
        .contains(&date.year())
        .then_some(Offset::UTC.to_timestamp(date).ok()?.as_millisecond() as f64 / 1_000.0)
}

#[cfg(test)]
#[path = "../tests/unit/parse/tests.rs"]
mod tests;
