//! Fills a store with the shared demo data so previews read history through the same queries as real data.

use jiff::Timestamp;

use super::{Result, Sample, SampleWrite, Store};

pub const DEMO_DAYS: u32 = 30;
const DEMO_INTERVAL: f64 = 1800.0;

fn date(at: f64) -> Timestamp {
    Timestamp::from_millisecond((at * 1000.0) as i64).unwrap_or(Timestamp::UNIX_EPOCH)
}

impl Store {
    pub fn seed_demo(&self, providers: &[String], now: f64, long_text: bool) -> Result<()> {
        // Half-hour boundaries keep the points stable when a preview reseeds a few minutes later.
        let start = (now / DEMO_INTERVAL).floor() * DEMO_INTERVAL - f64::from(DEMO_DAYS) * 86_400.0;
        let mut rows = Vec::new();
        for provider in providers {
            let mut at = start;
            while at <= now {
                for window in quotaline_demo::demo_provider_at(provider, date(at), long_text)
                    .map(|snapshot| snapshot.windows)
                    .unwrap_or_default()
                {
                    rows.push(SampleWrite {
                        sample: Sample {
                            at,
                            key: format!("{provider}:{}", window.id),
                            used: window.used,
                            reset_at: window
                                .reset
                                .and_then(|reset| reset.parse::<Timestamp>().ok())
                                .map(|reset| reset.as_millisecond() as f64 / 1000.0),
                        },
                        label: window.label,
                    });
                }
                at += DEMO_INTERVAL;
            }
            if let Some(points) = quotaline_demo::demo_analytics_at(provider, date(now), DEMO_DAYS, long_text) {
                self.record_analytics(provider, None, now, &points, &[])?;
            }
        }
        self.seed(&rows)
    }
}
