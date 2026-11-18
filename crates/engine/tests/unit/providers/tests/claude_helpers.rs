//! Builds claude helpers fixtures for provider tests.

use super::*;

pub(super) async fn claude_summary(data: Value) -> Provider {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(data))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap()
}

pub(super) async fn claude_refresh_failure(response: ResponseTemplate) -> Failure {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/oauth/token"))
        .respond_with(response)
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}}),
    );
    collect(&fixture_api(&server), "claude", home.path(), true)
        .await
        .unwrap_err()
}
