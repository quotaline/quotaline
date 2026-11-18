//! Builds antigravity helpers fixtures for provider tests.

use super::*;

pub(super) async fn antigravity_summary(data: Value) -> Result<Provider, Failure> {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(data))
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"currentTier":{"name":"Google AI Pro"}})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    collect(&api, "antigravity", home.path(), false).await
}

pub(super) fn antigravity_oauth_fixture(home: &Path) {
    let path = home.join(".local/bin/agy");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut binary = b"\0-fixture.apps.googleusercontent.com\0abc-fixture.apps.googleusercontent.com\x0012345-.apps.googleusercontent.com\0GOCSPX-AAAAAAAAAAAAAAAAAAAAAAAAAAA!\0".to_vec();
    binary.resize(65_770, b'x');
    binary.extend(b"\x0012345-fixture.apps.googleusercontent.com\0GOCSPX-AAAAAAAAAA-BBBBBBBBBB_CCCCCC\0");
    fs::write(path, binary).unwrap();
}
