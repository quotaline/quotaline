//! Reads local GNOME credentials from files and Secret Service.

use serde_json::Value;
use std::path::Path;

pub use quotaline_credentials::jwt_claims;

pub fn codex_key(root: &Path) -> String {
    let canonical = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    quotaline_credentials::codex_home_key(&canonical.to_string_lossy())
}

pub fn antigravity_document(bytes: &[u8]) -> Result<Value, &'static str> {
    let document = quotaline_credentials::antigravity_keyring_document(bytes).map_err(|error| match error {
        quotaline_credentials::AntigravityDecodeError::InvalidBase64 => "Antigravity keyring item is not base64",
        quotaline_credentials::AntigravityDecodeError::InvalidJson => "Antigravity keyring item is not JSON",
    })?;
    if antigravity_token(&document).is_none() {
        return Err("Antigravity keyring item has no access token");
    }
    Ok(document)
}

pub fn antigravity_token(document: &Value) -> Option<&str> {
    quotaline_credentials::antigravity_access_token(document).filter(|token| !token.is_empty())
}

#[cfg(test)]
#[path = "../tests/unit/credentials/tests.rs"]
mod tests;
