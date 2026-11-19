//! Coordinates GNOME provider discovery and response collection.

pub(crate) mod antigravity_local;
pub mod claude_transcripts;
use quotaline_credential_sources::codex_rollouts;

use crate::credentials;
use jiff::Timestamp;
use quotaline_analytics::AnalyticsSnapshot;
use quotaline_analytics::watermark::Watermarks;
use quotaline_antigravity as core_antigravity;
use quotaline_antigravity::TierCache;
use quotaline_catalog::login;
use quotaline_claude::ProfileCache;
use quotaline_codex as codex;
use quotaline_codex::codex_reset::Guard as CodexResetGuard;
use quotaline_codex::codex_reset::ResetCreditCache;
use quotaline_codex::codex_usage::ResetCredits;
use quotaline_codex::fetch as codex_fetch;
use quotaline_copilot as copilot;
use quotaline_credentials::RefreshRejectionLedger;
use quotaline_cursor as cursor;
use quotaline_gemini as core_gemini;
use quotaline_gemini::AssistCache;
use quotaline_http::ProviderFailureKind;
use quotaline_http::response_failure;
use quotaline_quota::CredentialHealth;
use quotaline_quota::Provider;
use quotaline_quota::Window;
use quotaline_text::json::text;
use reqwest::{Client, Method, header::HeaderMap};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::time::Instant;

type ResetCache = tokio::sync::Mutex<ResetCreditCache<ResetCredits>>;
type ClaudeProfileCache = tokio::sync::Mutex<ProfileCache<Value>>;
type GeminiAssistCache = std::sync::Mutex<AssistCache<Value>>;
type AntigravityTierCache = tokio::sync::Mutex<TierCache<Option<Value>>>;
type CursorIdentityCache = tokio::sync::Mutex<cursor::identity::Cache>;
type CodexSelectedHome = tokio::sync::Mutex<Option<PathBuf>>;
type CodexResets = tokio::sync::Mutex<CodexResetState>;
type RefreshRejections = tokio::sync::Mutex<RefreshRejectionLedger>;

#[derive(Default)]
struct CodexResetState {
    account: Option<[u8; 32]>,
    guard: CodexResetGuard<Window>,
}

#[derive(Clone)]
pub struct Api {
    http: Client,
    allow_environment_credentials: bool,
    allow_keyring: bool,
    claude_usage: String,
    claude_profile: String,
    claude_token: String,
    claude_profile_cache: Arc<ClaudeProfileCache>,
    codex_base: String,
    codex_token: String,
    codex_selected_home: Arc<CodexSelectedHome>,
    codex_rollout_cache: Arc<codex_rollouts::RolloutCache>,
    codex_reset_cache: Arc<ResetCache>,
    codex_resets: Arc<CodexResets>,
    local_scan_throttled: Arc<AtomicBool>,
    gemini_base: String,
    gemini_token: String,
    gemini_assist_cache: Arc<GeminiAssistCache>,
    antigravity_bases: [String; 2],
    antigravity_local_enabled: bool,
    antigravity_auth_file: Option<PathBuf>,
    antigravity_refreshed: Arc<tokio::sync::Mutex<Option<(Value, Value)>>>,
    antigravity_tier_cache: Arc<AntigravityTierCache>,
    refresh_rejections: Arc<RefreshRejections>,
    cursor_usage: String,
    cursor_period: String,
    cursor_me: String,
    cursor_identity: Arc<CursorIdentityCache>,
    cache_epoch: Instant,
    copilot_usage: String,
    #[cfg(feature = "fixture-api")]
    fixture_base: Option<String>,
}

impl Api {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let api = Self {
            http: Client::builder().timeout(Duration::from_secs(12)).build()?,
            allow_environment_credentials: true,
            allow_keyring: true,
            claude_usage: quotaline_claude::request::USAGE_URL.into(),
            claude_profile: quotaline_claude::request::PROFILE_URL.into(),
            claude_token: quotaline_claude::request::TOKEN_URL.into(),
            claude_profile_cache: Arc::new(tokio::sync::Mutex::new(ProfileCache::default())),
            codex_base: quotaline_codex::request::BASE_URL.into(),
            codex_token: quotaline_codex::request::TOKEN_URL.into(),
            codex_selected_home: Arc::new(tokio::sync::Mutex::new(None)),
            codex_rollout_cache: Arc::new(codex_rollouts::RolloutCache::default()),
            codex_reset_cache: Arc::new(tokio::sync::Mutex::new(ResetCreditCache::default())),
            codex_resets: Arc::new(tokio::sync::Mutex::new(CodexResetState::default())),
            local_scan_throttled: Arc::new(AtomicBool::new(false)),
            gemini_base: quotaline_gemini::request::BASE_URL.into(),
            gemini_token: quotaline_gemini::request::TOKEN_URL.into(),
            gemini_assist_cache: Arc::new(std::sync::Mutex::new(AssistCache::default())),
            antigravity_bases: quotaline_antigravity::request::CLOUD_BASES.map(str::to_owned),
            antigravity_local_enabled: true,
            antigravity_auth_file: env::var_os("ANTIGRAVITY_AUTH_FILE").map(PathBuf::from),
            antigravity_refreshed: Arc::new(tokio::sync::Mutex::new(None)),
            antigravity_tier_cache: Arc::new(tokio::sync::Mutex::new(TierCache::default())),
            refresh_rejections: Arc::new(tokio::sync::Mutex::new(RefreshRejectionLedger::default())),
            cursor_usage: quotaline_cursor::request::SUMMARY_URL.into(),
            cursor_period: quotaline_cursor::request::PERIOD_URL.into(),
            cursor_me: quotaline_cursor::request::PROFILE_URL.into(),
            cursor_identity: Arc::new(tokio::sync::Mutex::new(cursor::identity::Cache::default())),
            cache_epoch: Instant::now(),
            copilot_usage: quotaline_copilot::request::user_url("github.com"),
            #[cfg(feature = "fixture-api")]
            fixture_base: None,
        };
        #[cfg(feature = "fixture-api")]
        {
            let mut api = api;
            if let Ok(base) = env::var("TMB_FIXTURE_API_URL") {
                let url = reqwest::Url::parse(&base)?;
                if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
                    return Err("Fixture API must use loopback HTTP".into());
                }
                let base = base.trim_end_matches('/');
                api.claude_usage = format!("{base}/api/oauth/usage");
                api.claude_profile = format!("{base}/api/oauth/profile");
                api.claude_token = format!("{base}/v1/oauth/token");
                api.codex_base = format!("{base}/backend-api");
                api.codex_token = format!("{base}/oauth/token");
                api.gemini_base = base.into();
                api.gemini_token = format!("{base}/token");
                api.antigravity_bases = [
                    format!("{base}/antigravity-primary"),
                    format!("{base}/antigravity-secondary"),
                ];
                api.antigravity_local_enabled = false;
                api.cursor_usage = format!("{base}/api/usage-summary");
                api.cursor_period = format!("{base}/aiserver.v1.DashboardService/GetCurrentPeriodUsage");
                api.cursor_me = format!("{base}/api/auth/me");
                api.copilot_usage = format!("{base}/copilot_internal/user");
                api.fixture_base = Some(base.into());
            }
            Ok(api)
        }
        #[cfg(not(feature = "fixture-api"))]
        {
            Ok(api)
        }
    }

    pub fn set_local_scan_throttled(&self, throttled: bool) {
        self.local_scan_throttled.store(throttled, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub struct Failure {
    pub status: &'static str,
    pub recovery_status: Option<&'static str>,
    pub message: String,
    pub retry_after: u64,
    pub credential_missing: bool,
    pub credential_usable: bool,
    pub credential_health: CredentialHealth,
}

impl Failure {
    fn new(status: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            recovery_status: None,
            message: message.into(),
            retry_after: 0,
            credential_missing: false,
            credential_usable: false,
            credential_health: if status == "unreadable" {
                CredentialHealth::Unreadable
            } else {
                CredentialHealth::Unchecked
            },
        }
    }

    pub fn kind(&self) -> ProviderFailureKind {
        match self.status {
            "rate-limited" => ProviderFailureKind::RateLimited,
            "sign-in" | "unreadable" => ProviderFailureKind::NotAuthenticated,
            "offline" => ProviderFailureKind::NetworkUnavailable,
            _ => ProviderFailureKind::Failed,
        }
    }

    fn missing_sign_in(message: impl Into<String>) -> Self {
        Self {
            credential_missing: true,
            credential_health: CredentialHealth::Missing,
            ..Self::new("sign-in", message)
        }
    }

    fn with_credential(mut self) -> Self {
        self.credential_usable = true;
        self.credential_health = CredentialHealth::Valid;
        self
    }

    fn with_expired(mut self) -> Self {
        self.credential_health = CredentialHealth::Expired;
        self
    }

    fn unsupported(message: impl Into<String>) -> Self {
        Self {
            recovery_status: Some("unsupported"),
            ..Self::new("sign-in", message)
        }
    }
}

fn credential_file_failure(provider: &str, error: quotaline_storage::CredentialFileError) -> Failure {
    use quotaline_storage::CredentialFileError;
    match error {
        CredentialFileError::Read(_) => Failure::new("unreadable", format!("Cannot reread {provider} credentials")),
        CredentialFileError::InvalidJson(_) => {
            Failure::new("unreadable", format!("{provider} credentials changed during refresh"))
        }
        CredentialFileError::Changed => Failure::new(
            "sign-in",
            format!("{provider} credentials changed during refresh; check again."),
        ),
        CredentialFileError::Write(_) => {
            Failure::new("unreadable", format!("Cannot save refreshed {provider} credentials"))
        }
    }
}

fn source(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
fn json_file(path: &Path) -> Result<Value, Failure> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| Failure::new("unreadable", format!("{} is not valid JSON", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(_) => Err(Failure::new("unreadable", format!("Cannot read {}", path.display()))),
    }
}
fn credential<'a>(value: Option<&'a str>, id: &str) -> Result<&'a str, Failure> {
    value.ok_or_else(|| Failure::missing_sign_in(format!("No sign-in found. Run {}.", login(id))))
}
use crate::platform::{app_data, config_home};

fn refresh_failure(status: &'static str, provider: &str, detail: &str) -> Failure {
    let name = quotaline_catalog::definition(provider)
        .expect("OAuth refresh runs for known providers")
        .name;
    Failure::new(status, quotaline_credentials::refresh_failed_message(name, detail))
}

fn retry_after(headers: &HeaderMap, body: Option<&Value>) -> u64 {
    quotaline_http::retry_after(
        headers.get("Retry-After").and_then(|value| value.to_str().ok()),
        body,
        Timestamp::now().as_millisecond() as f64 / 1000.0,
    )
    .map_or(0, |seconds| seconds.ceil() as u64)
}

async fn check_oauth_rejection(client: &Api, provider: &'static str, stored: &Value) -> Result<(), Failure> {
    let mut rejections = client.refresh_rejections.lock().await;
    if let Some(code) = rejections.rejected_code(provider, stored) {
        return Err(refresh_failure(
            "sign-in",
            provider,
            &quotaline_credentials::refresh_rejected_message(code),
        ));
    }
    Ok(())
}

async fn oauth_request(
    client: &Api,
    provider: &'static str,
    stored: &Value,
    request: reqwest::RequestBuilder,
) -> Result<Value, Failure> {
    check_oauth_rejection(client, provider, stored).await?;
    let response = request.send().await.map_err(|error| {
        refresh_failure(
            "offline",
            provider,
            &quotaline_http::HttpError::Network(error.to_string()).message(None),
        )
    })?;
    if response.status().is_success() {
        let value = response
            .json()
            .await
            .map_err(|_| refresh_failure("error", provider, "Provider returned invalid JSON"))?;
        client.refresh_rejections.lock().await.clear(provider);
        return Ok(value);
    }
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let body = response.json::<Value>().await.ok();
    let retry_after = retry_after(&headers, body.as_ref());
    let terminal_code = if let Some(body) = body.as_ref() {
        client
            .refresh_rejections
            .lock()
            .await
            .record_failure(provider, stored, status, body)
    } else {
        None
    };
    if let Some(code) = terminal_code {
        return Err(refresh_failure(
            "sign-in",
            provider,
            &quotaline_credentials::refresh_rejected_message(&code),
        ));
    }
    let detail = quotaline_http::HttpError::Response {
        status: i32::from(status),
        body: String::new(),
        retry_after: None,
    }
    .message(None);
    Err(Failure {
        retry_after,
        ..refresh_failure(
            match status {
                401 | 403 => "sign-in",
                429 => "rate-limited",
                _ => "error",
            },
            provider,
            &detail,
        )
    })
}

pub async fn collect(client: &Api, id: &str, home: &Path, allow_token_refresh: bool) -> Result<Provider, Failure> {
    match id {
        "claude" => claude(client, home, allow_token_refresh).await,
        "codex" => codex(client, home, allow_token_refresh).await,
        "gemini" => gemini(client, home, allow_token_refresh).await,
        "antigravity" => antigravity(client, home, allow_token_refresh).await,
        "cursor" => cursor(client, home).await,
        "copilot" => copilot(client, home).await,
        _ => Err(Failure::new("error", "Unknown provider")),
    }
}

mod claude_io;
use claude_io::*;
mod codex_io;
use codex_io::*;
mod codex_extra;
use codex_extra::*;
pub use codex_extra::{CodexAnalytics, codex_analytics};
mod gemini_io;
use gemini_io::*;
mod cursor_io;
use cursor_io::*;
mod copilot_io;
use copilot_io::*;
mod antigravity_io;
use antigravity_io::*;
mod transport;
use transport::*;
#[cfg(target_os = "linux")]
mod keyring;
#[cfg(target_os = "macos")]
#[path = "keychain.rs"]
mod keyring;
#[cfg(windows)]
#[path = "credential_manager.rs"]
mod keyring;

#[cfg(test)]
#[path = "../../tests/unit/providers/tests/mod.rs"]
mod tests;
