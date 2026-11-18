//! Checks antigravity 3 behavior through the public API.

use super::*;

#[tokio::test]
async fn antigravity_missing_cloud_tier_preserves_local_plan() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.5}]}]
        })))
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"access_token":"antigravity-fixture","planName":"Ultra"}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(provider.plan, "Ultra");
}

#[tokio::test]
async fn antigravity_refresh_uses_valid_secondary_installed_client() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains(
            "client_id=67890-secondary.apps.googleusercontent.com",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.5}]}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let first = home.path().join(".local/bin/agy");
    fs::create_dir_all(first.parent().unwrap()).unwrap();
    fs::write(first, b"\0bad.apps.googleusercontent.com\0-bad.apps.googleusercontent.com\x00123-.apps.googleusercontent.com\0letters-name.apps.googleusercontent.com\0").unwrap();
    let second = home
        .path()
        .join(".local/share/antigravity/resources/app/extensions/antigravity/bin/language_server_linux_x64");
    fs::create_dir_all(second.parent().unwrap()).unwrap();
    fs::write(
        second,
        b"\x0067890-secondary.apps.googleusercontent.com\0GOCSPX-AAAAAAAAAAAAAAAAAAAAAAAAAAAA\0",
    )
    .unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"expired","refresh_token":"shared","expiry":1}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 50.0);
}
