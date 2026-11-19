//! Sends shared provider pipeline requests through reqwest and reports their failures the GNOME way.

use super::*;
use quotaline_http::HttpError;
use quotaline_http::transport::Body;
use quotaline_http::transport::Request;
use quotaline_http::transport::Transport;
use std::future::Future;

impl Transport for Api {
    fn send(&self, request: Request) -> impl Future<Output = Result<Vec<u8>, HttpError>> + Send {
        let http = self.http.clone();
        async move {
            let method = if request.body == Body::Empty {
                Method::GET
            } else {
                Method::POST
            };
            let mut build = http.request(method, &request.url).header("Accept", "application/json");
            for (name, value) in &request.headers {
                build = build.header(name, value);
            }
            if let Some(timeout) = request.timeout_millis {
                build = build.timeout(Duration::from_millis(u64::from(timeout)));
            }
            build = match request.body {
                Body::Empty => build,
                Body::Json(body) => build.header("Content-Type", "application/json").body(body),
                Body::Form(fields) => build.form(&fields),
            };
            let network = |error: reqwest::Error| HttpError::Network(error.to_string());
            let response = build.send().await.map_err(network)?;
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let body = response.bytes().await.map_err(network)?;
            let Some(failure) = response_failure(
                status,
                headers.get("Content-Type").and_then(|value| value.to_str().ok()),
                headers.get("Retry-After").and_then(|value| value.to_str().ok()),
                &body,
                Timestamp::now().as_millisecond() as f64 / 1000.0,
            ) else {
                return Ok(body.to_vec());
            };
            Err(if failure.security_check {
                HttpError::VendorSecurityCheck(i32::from(status))
            } else {
                HttpError::Response {
                    status: i32::from(status),
                    body: String::from_utf8_lossy(&body).into_owned(),
                    retry_after: failure.retry_after,
                }
            })
        }
    }
}

/// Words a shared pipeline's failure as macOS does and maps its kind to a GNOME status.
pub(super) fn pipeline_failure(error: HttpError, id: &str) -> Failure {
    let failure = error.provider_failure(&format!("Run {}.", login(id)), crate::platform::home_text().as_deref());
    Failure {
        retry_after: failure.retry_after.map_or(0, |seconds| seconds.ceil() as u64),
        ..Failure::new(
            match failure.kind {
                ProviderFailureKind::NotAuthenticated => "sign-in",
                ProviderFailureKind::RateLimited => "rate-limited",
                ProviderFailureKind::NetworkUnavailable => "offline",
                ProviderFailureKind::Failed => "error",
            },
            failure.message,
        )
    }
}
