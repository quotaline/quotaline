//! Builds cursor helpers fixtures for provider tests.

use super::*;

pub(super) async fn cursor_summary(data: Value, credential: Value) -> Provider {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(data))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(home.path(), ".cursor/auth.json", &credential);
    collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap()
}
