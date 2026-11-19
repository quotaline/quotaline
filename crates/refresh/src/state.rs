//! Keeps the refresh status both desktops show beside a provider's last reading.

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProviderStatus {
    pub availability: String,
    pub last_error: Option<String>,
    pub warnings: Vec<String>,
    pub retry_not_before: Option<f64>,
    pub last_attempt: Option<f64>,
    pub last_analytics_attempt: Option<f64>,
    pub last_success: Option<f64>,
    pub refreshing: bool,
}

impl Default for ProviderStatus {
    fn default() -> Self {
        Self {
            availability: "loading".into(),
            last_error: None,
            warnings: Vec::new(),
            retry_not_before: None,
            last_attempt: None,
            last_analytics_attempt: None,
            last_success: None,
            refreshing: false,
        }
    }
}

/// The parts of an applied result the status shows.
pub struct SettleInput<'a> {
    pub availability: &'a str,
    pub last_error: Option<&'a str>,
    pub retry_not_before: Option<f64>,
    pub succeeded: bool,
    pub warnings: Vec<String>,
    pub included_analytics: bool,
    /// False once the provider was switched off or lost its credential while the fetch ran.
    pub active: bool,
    pub now: f64,
}

/// The status after one fetch result.
pub fn settle(previous: &ProviderStatus, input: SettleInput<'_>) -> ProviderStatus {
    let settled = ProviderStatus {
        availability: input.availability.into(),
        last_error: input.last_error.map(str::to_owned),
        warnings: input.warnings,
        retry_not_before: input.retry_not_before,
        last_attempt: Some(input.now),
        last_analytics_attempt: if input.included_analytics {
            Some(input.now)
        } else {
            previous.last_analytics_attempt
        },
        last_success: if input.succeeded {
            Some(input.now)
        } else {
            previous.last_success
        },
        refreshing: false,
    };
    if input.active {
        settled
    } else {
        keep_shown_when_disabled(
            ProviderStatus {
                last_error: None,
                warnings: Vec::new(),
                ..settled
            },
            previous,
        )
    }
}

/// A switched-off provider keeps showing its last success, error, and warnings, but nothing it was waiting on.
pub fn keep_shown_when_disabled(next: ProviderStatus, previous: &ProviderStatus) -> ProviderStatus {
    ProviderStatus {
        availability: "disabled".into(),
        last_error: next.last_error.or_else(|| previous.last_error.clone()),
        warnings: if next.warnings.is_empty() {
            previous.warnings.clone()
        } else {
            next.warnings
        },
        last_success: next.last_success.or(previous.last_success),
        ..next
    }
}

#[cfg(test)]
#[path = "../tests/unit/state/tests.rs"]
mod tests;
