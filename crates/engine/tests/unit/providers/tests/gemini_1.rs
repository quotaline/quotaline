//! Checks gemini 1 behavior through the public API.

use super::*;

#[tokio::test]
async fn gemini_http_sends_project_from_code_assist() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"cloudaicompanionProject":"project-fixture"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .and(matchers::body_json(json!({"project":"project-fixture"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "buckets":[{"modelId":"gemini-fixture","remainingFraction":0.4}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 60.0);
}

#[tokio::test]
async fn gemini_reuses_plan_details_without_credit_balances() {
    assert_eq!(
        gemini_cached_plans(&["one", "one"], false, false).await,
        ["Tier 1", "Tier 1"]
    );
}

#[tokio::test]
async fn gemini_refreshes_credit_balances_after_a_minute() {
    assert_eq!(
        gemini_cached_plans(&["one", "one"], true, true).await,
        ["Tier 1", "Tier 2"]
    );
}

#[tokio::test]
async fn gemini_does_not_share_plan_details_between_accounts() {
    assert_eq!(
        gemini_cached_plans(&["one", "two"], false, false).await,
        ["Tier 1", "Tier 2"]
    );
}

#[tokio::test]
async fn gemini_http_sends_nested_project_from_code_assist() {
    for project in [json!({"id":"project-fixture"}), json!({"projectId":"project-fixture"})] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"cloudaicompanionProject":project})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .and(matchers::body_json(json!({"project":"project-fixture"})))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "buckets":[{"modelId":"gemini-fixture","remainingFraction":0.4}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".gemini/oauth_creds.json",
            &json!({"access_token":"gemini-fixture"}),
        );
        let provider = collect(&fixture_api(&server), "gemini", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.windows[0].used, 60.0);
    }
}

#[tokio::test]
async fn gemini_http_uses_id_token_for_workspace_identity() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentTier":{"id":"free-tier","name":"Free Tier"}
        })))
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
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(br#"{"email":"gemini@example.com","hd":"example.com"}"#);
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture","id_token":format!("header.{claims}.signature")}),
    );
    let provider = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.plan, "Workspace");
    assert_eq!(provider.identity, "gemini@example.com");
    assert_eq!(provider.organization, "example.com");
}

#[tokio::test]
async fn gemini_http_names_each_reference_tier() {
    for (tier, paid, expected) in [
        ("standard-tier", None, "Standard"),
        ("standard-tier", Some(""), "Standard"),
        ("legacy-tier", None, "Legacy"),
        ("free-tier", None, "Free"),
        ("other-tier", Some("Google AI Ultra"), "Google AI Ultra"),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "currentTier":{"id":tier,"name":"Other"},"paidTier":{"name":paid}
            })))
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
        credential_file(
            home.path(),
            ".gemini/oauth_creds.json",
            &json!({"access_token":"gemini-fixture"}),
        );
        let provider = collect(&fixture_api(&server), "gemini", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.plan, expected);
    }
}

#[tokio::test]
async fn gemini_paid_tier_without_current_tier_is_supported() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "paidTier":{"name":"Google AI Pro"}
        })))
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
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.plan, "Google AI Pro");
    assert_eq!(provider.windows[0].used, 60.0);
}

#[tokio::test]
async fn gemini_plan_error_keeps_quota_available() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .and(matchers::body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "buckets":[{"modelId":"gemini-fixture","remainingFraction":0.4}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.plan, "Gemini");
    assert_eq!(provider.windows[0].used, 60.0);
    assert_eq!(provider.warnings[0], "Plan details unavailable: HTTP 500");
}

#[tokio::test]
async fn gemini_plan_auth_and_rate_limit_errors_stop_quota_request() {
    for (status, expected, retry_after) in [(401, "sign-in", 0), (429, "rate-limited", 12)] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(status).insert_header("Retry-After", "12"))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"buckets":[]})))
            .expect(0)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".gemini/oauth_creds.json",
            &json!({"access_token":"gemini-fixture"}),
        );
        let error = collect(&fixture_api(&server), "gemini", home.path(), false)
            .await
            .unwrap_err();
        assert_eq!(error.status, expected);
        assert_eq!(error.retry_after, retry_after);
        assert_eq!(error.credential_health, CredentialHealth::Valid);
    }
}

#[tokio::test]
async fn gemini_quota_subscription_errors_explain_unsupported_accounts() {
    for body in [
        r#"{"error":{"status":"subscription_required"}}"#,
        r#"{"status":"SUBSCRIPTION_REQUIRED"}"#,
        r#"{"error":{"status":"PERMISSION_DENIED","details":[{"reason":"OTHER"},{"reason":"subscription_required"}]}}"#,
        "prefix SUBSCRIPTION_REQUIRED suffix",
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"currentTier":{"id":"standard-tier"}})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .respond_with(ResponseTemplate::new(403).set_body_string(body))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".gemini/oauth_creds.json",
            &json!({"access_token":"gemini-fixture"}),
        );
        let error = collect(&fixture_api(&server), "gemini", home.path(), false)
            .await
            .unwrap_err();
        assert_eq!(error.status, "sign-in", "body {body}");
        assert_eq!(error.recovery_status, Some("unsupported"), "body {body}");
        assert_eq!(error.message, core_gemini::UNSUPPORTED_CLIENT_MESSAGE, "body {body}");
        assert_eq!(error.credential_health, CredentialHealth::Valid, "body {body}");
    }
}

#[tokio::test]
async fn gemini_quota_other_forbidden_errors_are_failures_without_retry_delay() {
    for body in [
        r#"{"error":{"status":"PERMISSION_DENIED","details":[{"reason":"OTHER"}]}}"#,
        r#"{"error":{"message":"Permission denied"}}"#,
        "Permission denied",
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"currentTier":{"id":"standard-tier"}})))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("Retry-After", "17")
                    .set_body_string(body),
            )
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".gemini/oauth_creds.json",
            &json!({"access_token":"gemini-fixture"}),
        );
        let error = collect(&fixture_api(&server), "gemini", home.path(), false)
            .await
            .unwrap_err();
        assert_eq!(error.status, "error", "body {body}");
        assert_eq!(error.recovery_status, None, "body {body}");
        assert_eq!(error.message, "HTTP 403", "body {body}");
        assert_eq!(error.retry_after, 0, "body {body}");
        assert_eq!(error.credential_health, CredentialHealth::Valid, "body {body}");
    }
}

#[tokio::test]
async fn gemini_unsupported_account_uses_reported_reason() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"ineligibleTiers":[
            {"reasonCode":"OTHER","reasonMessage":"Other issue"},
            {"reasonCode":"unsupported_client","reasonMessage":"Personal account unsupported"}
        ]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuota"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"buckets":[]})))
        .expect(0)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture"}),
    );
    let error = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert_eq!(error.recovery_status, Some("unsupported"));
    assert_eq!(error.message, "Personal account unsupported");
    assert_eq!(error.credential_health, CredentialHealth::Valid);
}

#[tokio::test]
async fn gemini_unsupported_account_explains_missing_reason() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ineligibleTiers":[{"reasonCode":"UNSUPPORTED_CLIENT"}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".gemini/oauth_creds.json",
        &json!({"access_token":"gemini-fixture"}),
    );
    let error = collect(&fixture_api(&server), "gemini", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(error.recovery_status, Some("unsupported"));
    assert!(error.message.contains("June 18, 2026"));
}
