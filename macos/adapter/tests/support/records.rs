//! Builds provider records for adapter tests, with resets given as Unix seconds.

use quotaline_quota::Provider;

use crate::provider_record::{ProviderReadingRecord, ProviderRecord, ProviderWindowRecord, ReadingSource};

pub(crate) fn window(id: &str, used: f64, reset_at: Option<f64>) -> ProviderWindowRecord {
    ProviderWindowRecord {
        id: id.into(),
        label: id.into(),
        used,
        active: true,
        reset: reset_at
            .and_then(|at| jiff::Timestamp::from_second(at as i64).ok())
            .map(|at| at.display_with_offset(jiff::tz::Offset::UTC).to_string()),
        reset_by_day: false,
        group: id.into(),
        scope: None,
        severity: None,
        detail: String::new(),
        duration_seconds: None,
    }
}

pub(crate) fn provider(id: &str, windows: Vec<ProviderWindowRecord>) -> ProviderRecord {
    ProviderRecord {
        windows,
        ..ProviderRecord::from(Provider::empty(id, "connected"))
    }
}

pub(crate) fn reading(id: &str, windows: Vec<ProviderWindowRecord>, fetched_at: f64) -> ProviderReadingRecord {
    ProviderReadingRecord {
        provider: provider(id, windows),
        fetched_at,
        source: ReadingSource::Network,
    }
}
