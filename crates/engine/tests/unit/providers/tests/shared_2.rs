//! Checks shared 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn token_refresh_keeps_credentials_when_the_cli_directory_becomes_read_only() {
    use std::os::unix::fs::PermissionsExt;

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
        let directory = path.parent().unwrap().to_path_buf();
        Mock::given(matchers::method("POST"))
            .and(matchers::path(endpoint))
            .respond_with(move |_: &wiremock::Request| {
                fs::set_permissions(&directory, fs::Permissions::from_mode(0o500)).unwrap();
                ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh"}))
            })
            .expect(1)
            .mount(&server)
            .await;
        let failure = collect(&fixture_api(&server), provider, home.path(), true)
            .await
            .unwrap_err();
        fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            failure.message,
            format!("Cannot save refreshed {} credentials", humanize(provider))
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            document
        );
    }
}
