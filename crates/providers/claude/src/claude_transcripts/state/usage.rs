//! Summarizes recent transcript events for the active quota window.

use super::*;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LocalTranscriptUsage {
    pub window_tokens: i64,
    pub window_cost: f64,
    pub cost_per_hour: f64,
    pub today_tokens: i64,
    pub today_cost: f64,
    pub today_messages: i64,
}

impl TranscriptState {
    pub fn local_usage_for_window(
        &self,
        reset_at: Option<f64>,
        duration: f64,
        today_start: f64,
        now: f64,
    ) -> Option<LocalTranscriptUsage> {
        self.local_usage(reset_at.unwrap_or(now) - duration, today_start, now)
    }

    pub fn local_usage(&self, window_start: f64, today_start: f64, now: f64) -> Option<LocalTranscriptUsage> {
        if self.message_count() == 0 {
            return None;
        }
        let mut usage = LocalTranscriptUsage {
            window_tokens: 0,
            window_cost: 0.0,
            cost_per_hour: 0.0,
            today_tokens: 0,
            today_cost: 0.0,
            today_messages: 0,
        };
        let mut window_first = None::<f64>;
        for aggregate in self.recent.values() {
            let first = aggregate.first_timestamp.unwrap_or(aggregate.timestamp).0;
            let last = aggregate.last_timestamp.unwrap_or(aggregate.timestamp).0;
            let fallback = RecentEvent {
                timestamp: aggregate.timestamp,
                tokens: aggregate.tokens,
                cost: aggregate.cost,
                messages: aggregate.messages,
            };
            let events = aggregate.events.as_deref().unwrap_or(std::slice::from_ref(&fallback));
            if first >= window_start && last <= now {
                usage.window_tokens += aggregate.tokens;
                usage.window_cost += aggregate.cost;
                window_first = Some(window_first.map_or(first, |current| current.min(first)));
            } else if last >= window_start && first <= now {
                for event in events
                    .iter()
                    .filter(|event| event.timestamp.0 >= window_start && event.timestamp.0 <= now)
                {
                    usage.window_tokens += event.tokens;
                    usage.window_cost += event.cost;
                    window_first =
                        Some(window_first.map_or(event.timestamp.0, |current| current.min(event.timestamp.0)));
                }
            }
            if first >= today_start && last <= now {
                usage.today_tokens += aggregate.tokens;
                usage.today_cost += aggregate.cost;
                usage.today_messages += aggregate.messages;
            } else if last >= today_start && first <= now {
                for event in events
                    .iter()
                    .filter(|event| event.timestamp.0 >= today_start && event.timestamp.0 <= now)
                {
                    usage.today_tokens += event.tokens;
                    usage.today_cost += event.cost;
                    usage.today_messages += event.messages;
                }
            }
        }
        if let Some(first) = window_first {
            usage.cost_per_hour = usage.window_cost / ((now - first) / 3_600.0).max(1.0 / 60.0);
        }
        Some(usage)
    }
}
