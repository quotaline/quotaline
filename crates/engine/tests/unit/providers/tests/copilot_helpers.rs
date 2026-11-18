//! Builds copilot helpers fixtures for provider tests.

use super::*;

pub(super) async fn copilot_summary(data: Value) -> Provider {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(data))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({"loggedInUsers":[{"token":"copilot-fixture","host":"github.com","user":"octocat"}]}),
    );
    collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap()
}
