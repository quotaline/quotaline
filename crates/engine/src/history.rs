//! Shapes stored history into the series the Shell charts draw.

mod analytics;
mod chart;
mod reviews;
mod spend;

pub use analytics::{history_analytics, history_points};
pub use chart::{History, Point, Series, history};
pub use reviews::review_counts;
pub use spend::spend_report;
