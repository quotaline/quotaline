//! Classifies provider HTTP failures and retry delays for both frontends.

use jiff::fmt::rfc2822;
use serde_json::Value;

use quotaline_logging as logging;

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

mod request;
pub mod transport;
pub use request::{Origin, redacted_endpoint, same_origin};

pub const MAX_ERROR_BODY_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum HttpError {
    Response {
        status: i32,
        body: String,
        retry_after: Option<f64>,
    },
    VendorSecurityCheck(i32),
    Network(String),
    Decoding(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum ProviderFailureKind {
    NotAuthenticated,
    NetworkUnavailable,
    RateLimited,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProviderFailure {
    pub kind: ProviderFailureKind,
    pub message: String,
    pub retry_after: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ResponseFailure {
    pub status: u16,
    pub body: String,
    pub retry_after: Option<f64>,
    pub security_check: bool,
    pub kind: ProviderFailureKind,
}

pub fn response_failure(
    status: u16,
    content_type: Option<&str>,
    retry_header: Option<&str>,
    bytes: &[u8],
    now: f64,
) -> Option<ResponseFailure> {
    if (200..300).contains(&status) {
        return None;
    }
    let security_check = vendor_security_check(status, content_type, bytes);
    let body = &bytes[..bytes.len().min(MAX_ERROR_BODY_BYTES)];
    let delay = retry_after(retry_header, None, now).or_else(|| {
        serde_json::from_slice::<Value>(bytes)
            .ok()
            .and_then(|document| retry_after(None, Some(&document), now))
    });
    Some(ResponseFailure {
        status,
        body: String::from_utf8_lossy(body).into_owned(),
        retry_after: delay,
        security_check,
        kind: response_failure_kind(status, body, security_check),
    })
}

impl HttpError {
    pub fn message(&self, home: Option<&str>) -> String {
        match self {
            Self::Response {
                status: status @ 300..=399,
                ..
            } => {
                format!("Unexpected redirect (HTTP {status})")
            }
            Self::Response { status, .. } => format!("HTTP {status}"),
            Self::VendorSecurityCheck(status) => {
                format!("Blocked by provider security check (HTTP {status})")
            }
            Self::Network(detail) => format!("Network error: {}", logging::message(detail, home)),
            Self::Decoding(detail) => format!("Unexpected response: {}", logging::message(detail, home)),
        }
    }

    pub fn provider_failure(&self, hint: &str, home: Option<&str>) -> ProviderFailure {
        match self {
            Self::Network(detail) => ProviderFailure {
                kind: ProviderFailureKind::NetworkUnavailable,
                message: detail.clone(),
                retry_after: None,
            },
            Self::Response {
                status,
                body,
                retry_after,
            } => {
                let kind = response_failure_kind((*status).clamp(0, u16::MAX as i32) as u16, body.as_bytes(), false);
                ProviderFailure {
                    kind,
                    message: if kind == ProviderFailureKind::NotAuthenticated {
                        format!("{}. {hint}", self.message(home))
                    } else {
                        self.message(home)
                    },
                    retry_after: (kind == ProviderFailureKind::RateLimited)
                        .then_some(*retry_after)
                        .flatten(),
                }
            }
            _ => ProviderFailure {
                kind: ProviderFailureKind::Failed,
                message: self.message(home),
                retry_after: None,
            },
        }
    }
}

pub fn response_failure_kind(status: u16, body: &[u8], security_check: bool) -> ProviderFailureKind {
    if security_check {
        ProviderFailureKind::Failed
    } else if authentication_failure(status, body) {
        ProviderFailureKind::NotAuthenticated
    } else if status == 429 {
        ProviderFailureKind::RateLimited
    } else {
        ProviderFailureKind::Failed
    }
}

pub fn retry_after(header: Option<&str>, body: Option<&Value>, now: f64) -> Option<f64> {
    header
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|seconds| seconds.is_finite() && *seconds >= 0.0)
        .or_else(|| {
            header
                .and_then(|value| rfc2822::parse(value).ok())
                .map(|date| (date.timestamp().as_millisecond() as f64 / 1000.0 - now).max(0.0))
        })
        .or_else(|| {
            body.and_then(|body| body.get("retry_after"))
                .and_then(|value| {
                    value
                        .as_f64()
                        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
                })
                .filter(|seconds: &f64| seconds.is_finite())
                .map(|seconds| seconds.max(0.0))
        })
}

pub fn authentication_failure(status: u16, body: &[u8]) -> bool {
    if status == 401 {
        return true;
    }
    if status != 403 || serde_json::from_slice::<Value>(body).is_err() {
        return false;
    }
    let normalized = String::from_utf8_lossy(body).to_ascii_lowercase();
    [
        "unauthenticated",
        "unauthorized",
        "authentication_required",
        "invalid_token",
        "token_expired",
        "expired token",
        "invalid session",
        "session expired",
        "login_required",
        "sign in",
        "sign-in",
    ]
    .into_iter()
    .any(|marker| normalized.contains(marker))
}

pub fn vendor_security_check(status: u16, content_type: Option<&str>, body: &[u8]) -> bool {
    if status != 403 {
        return false;
    }
    if content_type.is_some_and(|value| value.to_ascii_lowercase().contains("text/html")) {
        return true;
    }
    let normalized = String::from_utf8_lossy(&body[..body.len().min(MAX_ERROR_BODY_BYTES)]).to_ascii_lowercase();
    ["<!doctype html", "<html", "cf-ray", "cloudflare", "challenge-platform"]
        .into_iter()
        .any(|marker| normalized.contains(marker))
}

#[cfg(feature = "test-support")]
pub mod test_transport;

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
