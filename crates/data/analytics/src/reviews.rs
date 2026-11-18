//! Counts daily code reviews for provider cards.

use jiff::civil::Date;
use jiff::tz::Offset;
use jiff::{Timestamp, ToSpan};

#[derive(Clone, Debug, PartialEq)]
pub struct ReviewRow {
    pub day: String,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct ReviewCounts {
    pub today: i64,
    pub week: i64,
}

pub fn review_counts(rows: &[ReviewRow], now: Timestamp) -> Option<ReviewCounts> {
    if rows.is_empty() {
        return None;
    }
    let today = Offset::UTC.to_datetime(now).date();
    let week = today.checked_sub(6.days()).expect("valid UTC day");
    let (today_count, week_count) = rows.iter().fold((0.0, 0.0), |(today_count, week_count), row| {
        let Ok(day) = Date::strptime("%Y-%m-%d", &row.day) else {
            return (today_count, week_count);
        };
        (
            today_count + if day == today { row.value } else { 0.0 },
            week_count + if day >= week && day <= today { row.value } else { 0.0 },
        )
    });
    Some(ReviewCounts {
        today: today_count as i64,
        week: week_count as i64,
    })
}

#[cfg(test)]
#[path = "../tests/unit/reviews/tests.rs"]
mod tests;
