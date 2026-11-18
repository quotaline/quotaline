//! Checks gemini 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn expiring_gemini_file_token_uses_installed_cli_oauth_client() {
    use std::os::unix::fs::PermissionsExt;

    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains("client_id=fixture-id"))
        .and(matchers::body_string_contains("client_secret=fixture-secret"))
        .and(matchers::body_string_contains("refresh_token=old-refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"new-access","expires_in":3600,
            "refresh_token":"new-refresh","id_token":"new-id"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .and(matchers::header("authorization", "Bearer new-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"cloudaicompanionProject":"project-fixture"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .and(matchers::header("authorization", "Bearer new-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "buckets":[{"modelId":"gemini-fixture","remainingFraction":0.4}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    gemini_oauth_fixture(home.path());
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({
            "token":{"accessToken":"old","refreshToken":"old-refresh","expiresAt":Timestamp::now().as_millisecond() + 60_000},"keep":true
        }),
    );
    let provider = collect(&fixture_api(&server), "gemini", home.path(), true)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 60.0);
    let path = home.path().join(".gemini/oauth_creds.json");
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["token"]["accessToken"], "new-access");
    assert_eq!(saved["token"]["idToken"], "new-id");
    assert_eq!(saved["token"]["refreshToken"], "new-refresh");
    assert_eq!(saved["keep"], true);
    assert!(saved["token"]["expiresAt"].as_i64().unwrap() > Timestamp::now().as_millisecond());
    assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
}

#[tokio::test]
async fn gemini_skips_an_unreadable_oauth_client_bundle() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains("client_id=working-id"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"fresh","expires_in":0
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".npm-global/lib/node_modules/@google");
    let broken = root.join("gemini-cli-core/dist/src/code_assist/oauth2.js");
    fs::create_dir_all(broken.parent().unwrap()).unwrap();
    fs::write(broken, "const OAUTH_CLIENT_ID = process.env.CLIENT_ID;").unwrap();
    let working = root.join("gemini-cli/dist/gemini.js");
    fs::create_dir_all(working.parent().unwrap()).unwrap();
    fs::write(
        working,
        "const OAUTH_CLIENT_ID = 'working-id'; const OAUTH_CLIENT_SECRET = 'working-secret';",
    )
    .unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"old","refresh_token":"shared","expiry_date":1}),
    );
    let failure = collect(&fixture_api(&server), "gemini", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Gemini refresh returned an invalid expiry");
    server.verify().await;
}

#[tokio::test]
async fn expiring_gemini_flat_token_persists_rotated_credentials() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"new-access","expires_in":3600,
            "refresh_token":"new-refresh","id_token":"new-id"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .and(matchers::header("authorization", "Bearer new-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "buckets":[{"modelId":"gemini-fixture","remainingFraction":0.4}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    gemini_oauth_fixture(home.path());
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"old","refresh_token":"old-refresh","expiry_date":1}),
    );
    let provider = collect(&fixture_api(&server), "gemini", home.path(), true)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 60.0);
    let saved: Value =
        serde_json::from_slice(&fs::read(home.path().join(".gemini/oauth_creds.json")).unwrap()).unwrap();
    assert_eq!(saved["access_token"], "new-access");
    assert_eq!(saved["refresh_token"], "new-refresh");
    assert_eq!(saved["id_token"], "new-id");
    assert!(saved["expiry_date"].as_i64().unwrap() > Timestamp::now().as_millisecond());
}

#[tokio::test]
async fn gemini_refresh_requires_a_refresh_token() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"old","expiry_date":1}),
    );
    let failure = collect(&fixture_api(&server), "gemini", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(
        failure.message,
        "Gemini refresh token is missing; sign in with Gemini CLI."
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    let saved: Value =
        serde_json::from_slice(&fs::read(home.path().join(".gemini/oauth_creds.json")).unwrap()).unwrap();
    assert_eq!(saved["access_token"], "old");
}

#[tokio::test]
async fn gemini_refresh_rejects_invalid_token_responses() {
    for (response, message) in [
        (json!({"expires_in":3600}), "Gemini refresh returned no access token"),
        (
            json!({"access_token":"fresh","expires_in":0}),
            "Gemini refresh returned an invalid expiry",
        ),
        (
            json!({"access_token":"fresh","expires_in":"NaN"}),
            "Gemini refresh returned an invalid expiry",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        gemini_oauth_fixture(home.path());
        let original = json!({"access_token":"old","refresh_token":"old-refresh","expiry_date":1});
        credential_file(home.path(), ".gemini/oauth_creds.json", &original);
        let failure = collect(&fixture_api(&server), "gemini", home.path(), true)
            .await
            .unwrap_err();
        assert_eq!(failure.message, message);
        let saved: Value =
            serde_json::from_slice(&fs::read(home.path().join(".gemini/oauth_creds.json")).unwrap()).unwrap();
        assert_eq!(saved, original);
    }
}

#[tokio::test]
async fn expired_gemini_file_token_stays_unchanged_without_opt_in() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let document = json!({"access_token":"old","refresh_token":"old-refresh","expiry_date":1});
    credential_file(home.path(), ".gemini/oauth_creds.json", &document);
    let error = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(server.received_requests().await.unwrap().is_empty());
    let saved: Value =
        serde_json::from_slice(&fs::read(home.path().join(".gemini/oauth_creds.json")).unwrap()).unwrap();
    assert_eq!(saved, document);
}

#[tokio::test]
async fn concurrent_gemini_cli_change_is_not_overwritten() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    gemini_oauth_fixture(home.path());
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({
            "access_token":"old","refresh_token":"old-refresh","expiry_date":1
        }),
    );
    let changed = json!({"access_token":"cli","refresh_token":"cli-refresh","expiry_date":9000000000000_i64});
    let replacement = changed.to_string();
    let path = home.path().join(".gemini/oauth_creds.json");
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .respond_with(move |_: &wiremock::Request| {
            fs::write(&path, &replacement).unwrap();
            ResponseTemplate::new(200).set_body_json(json!({"access_token":"service"}))
        })
        .expect(1)
        .mount(&server)
        .await;
    let error = collect(&fixture_api(&server), "gemini", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(error.message.contains("changed during refresh"));
    let saved: Value =
        serde_json::from_slice(&fs::read(home.path().join(".gemini/oauth_creds.json")).unwrap()).unwrap();
    assert_eq!(saved, changed);
}

#[tokio::test]
async fn rejected_gemini_refresh_waits_for_a_new_cli_credential() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains("refresh_token=old-refresh"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error":"refresh_token_reused"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains("refresh_token=cli-refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .and(matchers::header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"cloudaicompanionProject":"project"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .and(matchers::header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "buckets":[{"modelId":"gemini-fixture","remainingFraction":0.5}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    gemini_oauth_fixture(home.path());
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"old","refresh_token":"old-refresh","expiry_date":1}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let error = collect(&api, "gemini", home.path(), true).await.unwrap_err();
        assert_eq!(error.status, "sign-in");
        assert!(error.message.contains("refresh_token_reused"));
    }
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"old","refresh_token":"cli-refresh","expiry_date":1}),
    );
    let provider = collect(&api, "gemini", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 50.0);
}

#[tokio::test]
async fn gemini_preserves_zero_credits() {
    let quota = json!({"buckets":[
        {"modelId":"gemini-pro","remainingFraction":0.8,"remainingAmount":"200","tokenType":"tokens"},
        {"modelId":"gemini-pro","remainingFraction":0.2,"remainingAmount":"50","tokenType":"tokens"}
    ]});
    let assist = json!({"currentTier":{"name":"Standard","availableCredits":[{"creditType":"GOOGLE_ONE_AI","creditAmount":"0"}]}});
    let provider = gemini_summary(quota, assist).await;
    assert_eq!(provider.credit_balance, Some(0.0));
}

#[tokio::test]
async fn gemini_uses_most_consumed_bucket_for_model() {
    let provider = gemini_summary(
        json!({"buckets":[
            {"modelId":"gemini-pro","remainingFraction":0.8,"remainingAmount":"200","tokenType":"tokens"},
            {"modelId":"gemini-pro","remainingFraction":0.2,"remainingAmount":"50","tokenType":"tokens"}
        ]}),
        json!({}),
    )
    .await;
    assert_eq!(provider.windows.len(), 1);
    assert_eq!(provider.windows[0].used, 80.0);
}

#[tokio::test]
async fn gemini_reference_quota_keeps_model_order() {
    let provider = gemini_reference().await;
    assert_eq!(provider.windows[0].id, "model:gemini-2.5-pro");
    assert_eq!(provider.windows[0].scope.as_deref(), Some("gemini-2.5-pro"));
    assert_eq!(provider.windows[0].label, "Gemini 2.5 Pro");
    assert_eq!(provider.windows[0].used, 60.0);
    assert_eq!(provider.windows[0].reset.as_deref(), Some("2026-08-30T07:00:00Z"));
    assert_eq!(provider.windows[1].id, "model:gemini-2.5-flash");
    assert!((provider.windows[1].used - 10.0).abs() < 1e-9);
}

#[tokio::test]
async fn gemini_reference_credit_uses_paid_tier() {
    let provider = gemini_reference().await;
    assert_eq!(provider.plan, "Google AI Pro");
    assert_eq!(provider.credit_balance, Some(300.0));
}

#[tokio::test]
async fn gemini_reference_quota_keeps_all_bucket_details() {
    let provider = gemini_reference().await;
    assert_eq!(provider.details.len(), 5);
    assert_eq!(provider.details[0].title, "GOOGLE ONE AI");
    assert_eq!(provider.details[0].value, "300 credits");
    assert_eq!(
        provider.details[0].explanation,
        "A separate provider credit balance. Different credit types are not interchangeable."
    );
    assert_eq!(provider.details[2].value, "60% used · Resets 2026-08-30T07:00:00Z");
    assert!(provider.details[2].explanation.contains("most consumed bucket"));
    assert_eq!(provider.details[4].title, "Quota");
    assert_eq!(provider.details[4].value, "90% used");
}

#[tokio::test]
async fn gemini_empty_quota_still_shows_connected_tier() {
    let provider = gemini_summary(json!({"buckets":[]}), json!({"currentTier":{"id":"free-tier"}})).await;
    assert_eq!(provider.plan, "Free");
    assert!(provider.windows.is_empty());
}

#[tokio::test]
async fn gemini_uses_single_nonstandard_credit_balance() {
    let provider = gemini_malformed().await;
    assert_eq!(provider.credit_balance, Some(2.5));
}

#[tokio::test]
async fn gemini_skips_invalid_model_buckets_and_reset() {
    let provider = gemini_malformed().await;
    assert_eq!(provider.windows.len(), 1);
    assert_eq!(provider.windows[0].used, 80.0);
    assert_eq!(provider.windows[0].reset, None);
}

#[tokio::test]
async fn gemini_keeps_amount_and_anonymous_bucket_details() {
    let provider = gemini_malformed().await;
    assert!(
        provider
            .details
            .iter()
            .any(|detail| detail.value == "50 units remaining")
    );
    assert!(provider.details.iter().any(|detail| detail.title == "Quota"));
}
