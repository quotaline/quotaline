//! Generates provider analytics for shared demo previews.

use jiff::{SignedDuration, Timestamp};
use quotaline_analytics::Point;

pub fn demo_analytics_at(id: &str, now: Timestamp, days: u32, long_text: bool) -> Option<Vec<Point>> {
    if id != "claude" && id != "codex" {
        return None;
    }
    let mut points = Vec::new();
    for offset in (0..days).rev() {
        let day = demo_day(now, offset);
        let activity = demo_activity(offset);
        let mut push = |metric: &str, series: &str, value: f64| {
            points.push(Point {
                day: day.clone(),
                provider: id.into(),
                metric: metric.into(),
                series: if long_text {
                    format!("{series}-verification-series-with-a-deliberately-long-identifier")
                } else {
                    series.into()
                },
                value,
            });
        };
        if id == "claude" {
            for (model, share) in [("fable", 0.6), ("sonnet", 0.3), ("haiku", 0.1)] {
                push("inputTokens", model, 900_000.0 * share * activity);
                push("cachedInputTokens", model, 4_000_000.0 * share * activity);
                push("outputTokens", model, 120_000.0 * share * activity);
                push("cacheWriteTokens", model, 300_000.0 * share * activity);
                push("costUSD", model, 38.0 * share * activity);
                push("messages", model, (140.0 * share * activity).round());
                push("toolCalls", model, (260.0 * share * activity).round());
            }
            push("sessions", "sessions", (6.0 * activity).round());
            push("projectCost", "/work/demo", 38.0 * activity);
            push("projectMessages", "/work/demo", (140.0 * activity).round());
        } else {
            for (surface, share) in [("cli", 0.55), ("vscode", 0.3), ("web", 0.15)] {
                push("surfaceUsagePercent", surface, 40.0 * share * activity);
                push("turns", surface, (90.0 * share * activity).round());
            }
            for (model, share) in [("gpt-5.3-codex", 0.7), ("gpt-5.3-codex-spark", 0.3)] {
                push("modelCredits", model, 12.0 * share * activity);
                push("credits", model, 12.0 * share * activity);
                push("threads", model, (14.0 * share * activity).round());
            }
            push("skillInvocations", "review", (5.0 * activity).round());
            push("pluginInvocations", "github", (3.0 * activity).round());
            push("codeReviews", "reviews", (2.0 * activity).round());
            push("creditBalance", "Available credits", 28.0 * activity);
        }
    }
    Some(points)
}

fn demo_day(now: Timestamp, offset: u32) -> String {
    (now - SignedDuration::from_hours(i64::from(offset) * 24))
        .strftime("%Y-%m-%d")
        .to_string()
}

fn demo_activity(offset: u32) -> f64 {
    let day = f64::from(offset);
    0.55 + 0.45 * (day * 1.7 + 0.4).sin() * (day * 0.6).sin()
}

#[cfg(test)]
#[path = "../tests/unit/analytics/tests.rs"]
mod tests;
