//! Covers GNOME credential paths and keyring decoding.

use super::*;
use base64::Engine;

#[test]
fn codex_key_matches_canonical_home() {
    let home = tempfile::tempdir().unwrap();
    let key = codex_key(home.path());
    assert!(key.starts_with("cli|"));
    assert_eq!(key.len(), 20);
    assert_eq!(codex_key(&home.path().join(".")), key);
}

#[test]
fn codex_key_accepts_missing_home() {
    assert_eq!(codex_key(Path::new("/missing-codex-home")), "cli|827d2572d655e306");
}

#[test]
fn antigravity_accepts_nested_base64_credentials() {
    let encoded = base64::engine::general_purpose::STANDARD
        .encode(br#"{"token":{"access_token":"sample","refresh_token":"refresh"}}"#);
    let document = antigravity_document(format!("go-keyring-base64:{encoded}").as_bytes()).unwrap();
    assert_eq!(antigravity_token(&document), Some("sample"));
}

#[test]
fn antigravity_accepts_root_camel_case_credentials() {
    let document = antigravity_document(br#"{"accessToken":"sample"}"#).unwrap();
    assert_eq!(antigravity_token(&document), Some("sample"));
}

#[test]
fn antigravity_rejects_invalid_base64() {
    assert!(antigravity_document(b"go-keyring-base64:!").is_err());
}

#[test]
fn antigravity_rejects_invalid_json() {
    assert_eq!(
        antigravity_document(b"bad"),
        Err("Antigravity keyring item is not JSON")
    );
}

#[test]
fn antigravity_rejects_missing_token() {
    assert!(antigravity_document(br#"{"token":{}}"#).is_err());
}
