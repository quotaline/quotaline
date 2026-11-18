//! Checks shared 1 behavior through the public API.

use super::*;
use quotaline_http::transport::Request;
use quotaline_http::transport::Transport;

#[tokio::test]
async fn rate_limited_providers_keep_locally_valid_credentials() {
    let server = MockServer::start().await;
    for method in ["GET", "POST"] {
        Mock::given(matchers::method(method))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
    }
    for (id, path, document) in [
        (
            "claude",
            ".claude/.credentials.json",
            json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
        ),
        (
            "codex",
            ".codex/auth.json",
            json!({"tokens":{"access_token":"codex-fixture"}}),
        ),
        (
            "gemini",
            ".gemini/oauth_creds.json",
            json!({"access_token":"gemini-fixture"}),
        ),
        (
            "antigravity",
            "antigravity.json",
            json!({"access_token":"antigravity-fixture"}),
        ),
        ("cursor", ".cursor/auth.json", json!({"accessToken":"cursor-fixture"})),
        (
            "copilot",
            ".copilot/config.json",
            json!({"loggedInUsers":[{"token":"copilot-fixture","host":"github.com"}]}),
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        credential_file(home.path(), path, &document);
        let mut api = fixture_api(&server);
        if id == "antigravity" {
            api.antigravity_auth_file = Some(home.path().join(path));
        }
        let error = collect(&api, id, home.path(), false).await.unwrap_err();
        assert_eq!(error.status, "rate-limited", "{id}");
        assert!(error.credential_usable, "{id}");
        assert_eq!(error.credential_health, CredentialHealth::Valid, "{id}");
    }
}

#[tokio::test]
async fn unknown_provider_is_rejected() {
    let home = tempfile::tempdir().unwrap();
    let failure = collect(&Api::new().unwrap(), "unknown", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Unknown provider");
}

async fn transport_failure(server: &MockServer, path: &str) -> Failure {
    let request = Request::get(format!("{}{path}", server.uri()), Vec::new(), "fixture");
    pipeline_failure(fixture_api(server).send(request).await.unwrap_err(), "codex")
}

#[tokio::test]
async fn transport_preserves_retry_after_and_rate_limit_status() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/rate-limited"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "42"))
        .expect(1)
        .mount(&server)
        .await;
    let failure = transport_failure(&server, "/rate-limited").await;
    assert_eq!(failure.status, "rate-limited");
    assert_eq!(failure.retry_after, 42);
}

#[tokio::test]
async fn transport_uses_http_date_retry_after() {
    let server = MockServer::start().await;
    let date = (Timestamp::now() + jiff::SignedDuration::from_secs(120))
        .strftime("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/rate-limited"))
        .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", date.as_str()))
        .expect(1)
        .mount(&server)
        .await;
    let failure = transport_failure(&server, "/rate-limited").await;
    assert_eq!(failure.status, "rate-limited");
    assert!((119..=120).contains(&failure.retry_after));
}

#[tokio::test]
async fn transport_uses_expired_http_date_before_body_retry_after() {
    let server = MockServer::start().await;
    let date = (Timestamp::now() - jiff::SignedDuration::from_secs(120))
        .strftime("%a, %d %b %Y %H:%M:%S GMT")
        .to_string();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/rate-limited"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("Retry-After", date.as_str())
                .set_body_json(json!({"retry_after": 30})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let failure = transport_failure(&server, "/rate-limited").await;
    assert_eq!(failure.retry_after, 0);
}

#[tokio::test]
async fn transport_uses_body_retry_after_when_header_is_invalid() {
    for (header, body, expected) in [
        ("invalid", json!({"retry_after": 30}), 30),
        ("-1", json!({"retry_after": 1.2}), 2),
        ("NaN", json!({"retry_after": 4}), 4),
        ("invalid", json!({"retry_after": -4}), 0),
        ("invalid", json!({"retry_after": "4"}), 4),
        ("3", json!({"retry_after": 40}), 3),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/rate-limited"))
            .respond_with(
                ResponseTemplate::new(429)
                    .insert_header("Retry-After", header)
                    .set_body_json(body),
            )
            .expect(1)
            .mount(&server)
            .await;
        let failure = transport_failure(&server, "/rate-limited").await;
        assert_eq!(failure.retry_after, expected, "header {header}");
    }
}

#[tokio::test]
async fn transport_classifies_http_failures() {
    for (status, expected) in [(401, "sign-in"), (403, "error"), (429, "rate-limited"), (500, "error")] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/failure"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        let failure = transport_failure(&server, "/failure").await;
        assert_eq!(failure.status, expected, "HTTP {status}");
        assert_eq!(failure.retry_after, 0, "HTTP {status}");
    }
}

#[tokio::test]
async fn transport_distinguishes_authentication_from_a_security_challenge() {
    for (path, body, content_type, status, message) in [
        (
            "/auth",
            r#"{"error":{"status":"UNAUTHENTICATED"}}"#,
            "application/json",
            "sign-in",
            "HTTP 403. Run codex login.",
        ),
        (
            "/challenge",
            "<!doctype html><title>Just a moment</title>",
            "text/html",
            "error",
            "Blocked by provider security check (HTTP 403)",
        ),
        (
            "/mislabelled-challenge",
            r#"{"error":"invalid_token","provider":"cloudflare"}"#,
            "text/html",
            "error",
            "Blocked by provider security check (HTTP 403)",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(path))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("Content-Type", content_type)
                    .set_body_string(body),
            )
            .expect(1)
            .mount(&server)
            .await;
        let failure = transport_failure(&server, path).await;
        assert_eq!((failure.status, failure.message.as_str()), (status, message));
    }
}

#[tokio::test]
async fn token_refresh_does_not_recreate_a_removed_cli_credential() {
    let expired_codex = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    for (provider, relative, endpoint, document) in [
        (
            "claude",
            ".claude/.credentials.json",
            "/v1/oauth/token",
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}}),
        ),
        (
            "codex",
            ".codex/auth.json",
            "/oauth/token",
            json!({"tokens":{"access_token":expired_codex,"refresh_token":"shared"}}),
        ),
        (
            "gemini",
            ".gemini/oauth_creds.json",
            "/token",
            json!({"access_token":"old","refresh_token":"shared","expiry_date":1}),
        ),
    ] {
        let server = MockServer::start().await;
        let home = tempfile::tempdir().unwrap();
        if provider == "gemini" {
            gemini_oauth_fixture(home.path());
        }
        credential_file(home.path(), relative, &document);
        let path = home.path().join(relative);
        Mock::given(matchers::method("POST"))
            .and(matchers::path(endpoint))
            .respond_with(move |_: &wiremock::Request| {
                fs::remove_file(&path).unwrap();
                ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh"}))
            })
            .expect(1)
            .mount(&server)
            .await;
        let failure = collect(&fixture_api(&server), provider, home.path(), true)
            .await
            .unwrap_err();
        assert_eq!(
            failure.message,
            format!("Cannot reread {} credentials", humanize(provider))
        );
        assert!(!home.path().join(relative).exists());
    }
}

#[tokio::test]
async fn token_refresh_does_not_overwrite_a_changed_cli_credential() {
    let expired_codex = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    for (provider, relative, endpoint, document) in [
        (
            "claude",
            ".claude/.credentials.json",
            "/v1/oauth/token",
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}}),
        ),
        (
            "codex",
            ".codex/auth.json",
            "/oauth/token",
            json!({"tokens":{"access_token":expired_codex,"refresh_token":"shared"}}),
        ),
        (
            "gemini",
            ".gemini/oauth_creds.json",
            "/token",
            json!({"access_token":"old","refresh_token":"shared","expiry_date":1}),
        ),
    ] {
        let server = MockServer::start().await;
        let home = tempfile::tempdir().unwrap();
        if provider == "gemini" {
            gemini_oauth_fixture(home.path());
        }
        credential_file(home.path(), relative, &document);
        let path = home.path().join(relative);
        let changed = json!({"access_token":"new-session"});
        let changed_bytes = changed.to_string();
        Mock::given(matchers::method("POST"))
            .and(matchers::path(endpoint))
            .respond_with(move |_: &wiremock::Request| {
                fs::write(&path, &changed_bytes).unwrap();
                ResponseTemplate::new(200).set_body_json(json!({"access_token":"refreshed"}))
            })
            .expect(1)
            .mount(&server)
            .await;
        let failure = collect(&fixture_api(&server), provider, home.path(), true)
            .await
            .unwrap_err();
        assert_eq!(failure.status, "sign-in");
        assert_eq!(
            failure.message,
            format!(
                "{} credentials changed during refresh; check again.",
                humanize(provider)
            )
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(home.path().join(relative)).unwrap()).unwrap(),
            changed
        );
        server.verify().await;
    }
}

#[tokio::test]
async fn token_refresh_rechecks_a_cli_credential_before_contacting_oauth() {
    use std::{ffi::CString, io::Write, os::unix::ffi::OsStrExt};

    let expired_codex = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    for (provider, relative, document) in [
        (
            "claude",
            ".claude/.credentials.json",
            json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}}),
        ),
        (
            "codex",
            ".codex/auth.json",
            json!({"tokens":{"access_token":expired_codex,"refresh_token":"shared"}}),
        ),
        (
            "gemini",
            ".gemini/oauth_creds.json",
            json!({"access_token":"old","refresh_token":"shared","expiry_date":1}),
        ),
    ] {
        for (replacement, status, message) in [
            (
                br#"{"replacement":true}"#.as_slice(),
                "sign-in",
                format!(
                    "{} credentials changed during refresh; check again.",
                    humanize(provider)
                ),
            ),
            (
                b"{".as_slice(),
                "unreadable",
                format!("{} credentials changed during refresh", humanize(provider)),
            ),
        ] {
            let server = MockServer::start().await;
            let home = tempfile::tempdir().unwrap();
            let path = home.path().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let name = CString::new(path.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
            let first = document.to_string();
            let writer = std::thread::spawn(move || {
                // The first read holds the pipe open until the writer closes it, so swapping the file in before the
                // close guarantees the re-check reads the replacement.
                let mut pipe = fs::OpenOptions::new().write(true).open(&path).unwrap();
                pipe.write_all(first.as_bytes()).unwrap();
                let staged = path.with_extension("replacement");
                fs::write(&staged, replacement).unwrap();
                fs::rename(&staged, &path).unwrap();
                drop(pipe);
            });
            let failure = collect(&fixture_api(&server), provider, home.path(), true)
                .await
                .unwrap_err();
            assert_eq!(failure.status, status);
            assert_eq!(failure.message, message);
            writer.join().unwrap();
            assert!(server.received_requests().await.unwrap().is_empty());
        }
    }
}
