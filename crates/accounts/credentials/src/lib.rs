//! Parses provider credentials without accessing platform key stores.

use base64::Engine;
use jiff::Timestamp;
use jiff::fmt::temporal::Pieces;
use serde_json::Value;
use sha2::{Digest, Sha256};

mod claude;
mod codex;
mod copilot;
mod gemini;
mod google;
mod refresh_completion;
mod refresh_rejection;
pub mod save_recovery;
mod selection;
pub mod source_chain;
pub use claude::{
    CLAUDE_CLIENT_ID, CLAUDE_USER_AGENT, ClaudeCredentialDocument, ClaudeRefresh, ClaudeRefreshError,
    claude_credential_document, claude_initial_document, claude_refresh_request, claude_refresh_response,
    claude_refreshed_document,
};
pub use codex::{
    CODEX_API_KEY_USAGE_UNAVAILABLE, CODEX_CLIENT_ID, CODEX_ORIGINATOR, CodexCredentialDocument, CodexRefresh,
    codex_credential_available, codex_credential_document, codex_initial_document, codex_refresh_request,
    codex_refresh_response, codex_refreshed_document, codex_request_headers,
};
pub use copilot::{
    CopilotAccount, CopilotCredential, CopilotSecretError, copilot_cli_accounts, copilot_environment_credential,
    copilot_keyring_credential, copilot_legacy_credential,
};
pub use gemini::{
    GeminiCredentialDocument, GeminiStorage, gemini_credential_document, gemini_initial_document,
    gemini_keychain_document, gemini_storage_mode,
};
pub use google::{
    GoogleRefresh, GoogleRefreshError, antigravity_refreshed_document, gemini_refreshed_document,
    google_refresh_request, google_refresh_response,
};
pub use refresh_completion::{RefreshCompletionError, RefreshProvider, complete_refresh};
pub use refresh_rejection::{
    RefreshRejectionLedger, refresh_failed_message, refresh_rejected_message, terminal_refresh_error_code,
};
pub use selection::{ClaudeCredentialCandidate, select_claude_credential};

pub fn jwt_claims(token: &str) -> Option<Value> {
    let mut segments = token.split('.');
    let (_, payload, signature) = (segments.next()?, segments.next()?, segments.next()?);
    if signature.is_empty() || segments.next().is_some() {
        return None;
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

pub fn jwt_expiry(token: &str) -> Option<f64> {
    jwt_claims(token)?["exp"].as_f64()
}

pub const CREDENTIAL_EXPIRY_BUFFER: f64 = 120.0;

pub fn credential_expired(expires_at: Option<f64>, now: f64) -> bool {
    expires_at.is_some_and(|expires_at| expires_at - now < CREDENTIAL_EXPIRY_BUFFER)
}

pub fn cursor_user_id(token: &str) -> Option<String> {
    let claims = jwt_claims(token)?;
    let subject = claims["sub"].as_str()?;
    let id = subject.split_once('|').map_or(
        subject,
        |(prefix, suffix)| {
            if suffix.is_empty() { prefix } else { suffix }
        },
    );
    (!id.is_empty()).then(|| id.to_string())
}

pub fn cursor_session_cookie(token: &str) -> String {
    format!(
        "WorkosCursorSessionToken={}%3A%3A{token}",
        cursor_user_id(token).unwrap_or_default()
    )
}

pub fn codex_subscription_until(id_token: &str) -> Option<String> {
    let claims = jwt_claims(id_token)?;
    let until = claims["https://api.openai.com/auth"]["chatgpt_subscription_active_until"].as_str()?;
    let offset = Pieces::parse(until).ok()?.to_numeric_offset()?;
    let date = until.parse::<Timestamp>().ok()?;
    Some(date.display_with_offset(offset).to_string())
}

pub fn codex_account_fingerprint(access_token: &str, account_id: Option<&str>, id_token: Option<&str>) -> String {
    let claims = id_token.and_then(jwt_claims);
    let identity = account_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            claims
                .as_ref()
                .and_then(|claims| claims["https://api.openai.com/auth"]["chatgpt_account_id"].as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
        .map(|value| format!("account:{value}"))
        .or_else(|| {
            claims
                .as_ref()
                .and_then(|claims| claims["email"].as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| format!("email:{}", value.to_lowercase()))
        })
        .unwrap_or_else(|| format!("token:{access_token}"));
    hex::encode(Sha256::digest(format!("codex-analytics:{identity}").as_bytes()))
}

pub fn copilot_host(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    let host = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(&value)
        .trim_matches('/');
    (!host.is_empty()
        && host
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '.' || character == '-'))
    .then(|| host.to_string())
}

pub fn codex_home_key(canonical_path: &str) -> String {
    let digest = Sha256::digest(canonical_path.as_bytes());
    format!("cli|{}", hex::encode(&digest[..8]))
}

pub fn claude_cache_fingerprint(access_token: &str, refresh_token: Option<&str>) -> [u8; 32] {
    cache_fingerprint(
        "claude-cache",
        jwt_claims(access_token).as_ref(),
        refresh_token,
        access_token,
    )
}

pub fn gemini_cache_fingerprint(access_token: &str, refresh_token: Option<&str>, id_token: Option<&str>) -> [u8; 32] {
    cache_fingerprint(
        "gemini-cache",
        id_token.and_then(jwt_claims).as_ref(),
        refresh_token,
        access_token,
    )
}

fn cache_fingerprint(
    prefix: &str,
    claims: Option<&Value>,
    refresh_token: Option<&str>,
    access_token: &str,
) -> [u8; 32] {
    let identity = claims
        .and_then(|claims| claims["sub"].as_str().or_else(|| claims["email"].as_str()))
        .or(refresh_token)
        .unwrap_or(access_token);
    Sha256::digest(format!("{prefix}:{identity}").as_bytes()).into()
}

pub fn claude_keychain_service(config_dir: Option<&str>) -> String {
    const SERVICE: &str = "Claude Code-credentials";
    match config_dir.filter(|value| !value.is_empty()) {
        Some(config_dir) => format!("{SERVICE}-{}", hex::encode(&Sha256::digest(config_dir.as_bytes())[..4])),
        None => SERVICE.into(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AntigravityCredential {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AntigravityDecodeError {
    InvalidBase64,
    InvalidJson,
}

pub fn antigravity_keyring_document(bytes: &[u8]) -> Result<Value, AntigravityDecodeError> {
    let decoded = if let Some(encoded) = bytes.strip_prefix(b"go-keyring-base64:") {
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| AntigravityDecodeError::InvalidBase64)?
    } else {
        bytes.to_vec()
    };
    serde_json::from_slice(&decoded).map_err(|_| AntigravityDecodeError::InvalidJson)
}

pub fn antigravity_credential(document: &Value) -> Option<AntigravityCredential> {
    let fields = document.get("token").unwrap_or(document);
    let access_token = antigravity_access_token(document)?;
    let refresh_token = antigravity_refresh_token(document).map(str::to_owned);
    let expiry = ["expiry", "expires_at", "expiresAt"]
        .into_iter()
        .find_map(|key| fields.get(key).filter(|value| !value.is_null()));
    let expires_at = expiry.and_then(|value| {
        value
            .as_f64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
            .or_else(|| {
                value
                    .as_str()
                    .and_then(|value| value.parse::<Timestamp>().ok())
                    .map(|value| value.as_millisecond() as f64 / 1000.0)
            })
    });
    Some(AntigravityCredential {
        access_token: access_token.into(),
        refresh_token,
        expires_at,
    })
}

pub fn antigravity_access_token(document: &Value) -> Option<&str> {
    let fields = document.get("token").unwrap_or(document);
    fields["access_token"]
        .as_str()
        .or_else(|| fields["accessToken"].as_str())
}

pub fn antigravity_refresh_token(document: &Value) -> Option<&str> {
    let fields = document.get("token").unwrap_or(document);
    fields["refresh_token"]
        .as_str()
        .or_else(|| fields["refreshToken"].as_str())
}

#[cfg(test)]
#[path = "../tests/unit/tests.rs"]
mod tests;
