//! Checks claude 3 behavior through the public API.

use super::*;

#[tokio::test]
async fn expired_claude_token_stays_unchanged_without_opt_in() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let document = json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}});
    credential_file(home.path(), ".claude/.credentials.json", &document);
    let error = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(error.message.contains("expired"));
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(home.path().join(".claude/.credentials.json")).unwrap()).unwrap(),
        document
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn expiring_claude_token_refreshes_and_preserves_credential_fields() {
    use std::os::unix::fs::PermissionsExt;

    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/oauth/token"))
        .and(matchers::header("user-agent", "claude-code/2.1.251"))
        .and(matchers::body_json(json!({
            "grant_type":"refresh_token", "refresh_token":"shared",
            "client_id":"9d1c250a-e61b-44d9-88ed-5944d1962f5e"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"new", "refresh_token":"rotated", "expires_in":3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .and(matchers::header("authorization", "Bearer new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":42,"resets_at":"2026-10-10T12:00:00Z"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({
            "claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":Timestamp::now().as_millisecond() + 60_000,"subscriptionType":"Pro"},
            "unrelated":{"retained":true}
        }),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), true)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 42.0);
    let path = home.path().join(".claude/.credentials.json");
    let saved: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(saved["claudeAiOauth"]["accessToken"], "new");
    assert_eq!(saved["claudeAiOauth"]["refreshToken"], "rotated");
    assert_eq!(saved["claudeAiOauth"]["subscriptionType"], "Pro");
    assert_eq!(saved["unrelated"]["retained"], true);
    assert!(saved["claudeAiOauth"]["expiresAt"].as_i64().unwrap() > Timestamp::now().as_millisecond());
    assert_eq!(fs::metadata(path).unwrap().permissions().mode() & 0o777, 0o600);
}

#[tokio::test]
async fn concurrent_claude_cli_change_is_not_overwritten() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".claude/.credentials.json");
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({
            "claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}
        }),
    );
    let changed =
        json!({"claudeAiOauth":{"accessToken":"cli","refreshToken":"cli-rotated","expiresAt":9000000000000_i64}});
    let replacement = changed.to_string();
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/oauth/token"))
        .respond_with(move |_: &wiremock::Request| {
            fs::write(&path, &replacement).unwrap();
            ResponseTemplate::new(200)
                .set_body_json(json!({"access_token":"service","refresh_token":"service-rotated"}))
        })
        .expect(1)
        .mount(&server)
        .await;
    let error = collect(&fixture_api(&server), "claude", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(error.message.contains("changed during refresh"));
    let saved: Value =
        serde_json::from_slice(&fs::read(home.path().join(".claude/.credentials.json")).unwrap()).unwrap();
    assert_eq!(saved, changed);
}

#[tokio::test]
async fn claude_refresh_requires_a_refresh_token() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"old","expiresAt":1}}),
    );
    let error = collect(&fixture_api(&server), "claude", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(error.message.contains("refresh token is missing"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn failed_claude_refresh_keeps_the_old_credentials() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/oauth/token"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let document = json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"shared","expiresAt":1}});
    credential_file(home.path(), ".claude/.credentials.json", &document);
    let error = collect(&fixture_api(&server), "claude", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(error.status, "error");
    let saved: Value =
        serde_json::from_slice(&fs::read(home.path().join(".claude/.credentials.json")).unwrap()).unwrap();
    assert_eq!(saved, document);
}

#[tokio::test]
async fn claude_refresh_http_auth_errors_require_sign_in() {
    for status in [401, 403] {
        let failure = claude_refresh_failure(ResponseTemplate::new(status)).await;
        assert_eq!(failure.status, "sign-in", "HTTP {status}");
    }
}

#[tokio::test]
async fn claude_refresh_http_rate_limit_preserves_retry_delay() {
    let failure = claude_refresh_failure(ResponseTemplate::new(429).insert_header("Retry-After", "17")).await;
    assert_eq!((failure.status, failure.retry_after), ("rate-limited", 17));
}

#[tokio::test]
async fn claude_refresh_http_rate_limit_reads_body_delay() {
    let failure = claude_refresh_failure(ResponseTemplate::new(429).set_body_json(json!({"retry_after": 2.5}))).await;
    assert_eq!((failure.status, failure.retry_after), ("rate-limited", 3));
}

#[tokio::test]
async fn claude_refresh_rejects_invalid_responses() {
    for (response, expected) in [
        (ResponseTemplate::new(200).set_body_string("not json"), "invalid JSON"),
        (ResponseTemplate::new(200).set_body_json(json!({})), "no access token"),
        (
            ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh","expires_in":0})),
            "invalid expiry",
        ),
        (
            ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh","expires_in":"NaN"})),
            "invalid expiry",
        ),
    ] {
        let failure = claude_refresh_failure(response).await;
        assert!(failure.message.contains(expected), "{}", failure.message);
    }
}

#[tokio::test]
async fn rejected_claude_refresh_waits_for_a_new_cli_credential() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/oauth/token"))
        .and(matchers::body_string_contains("\"refresh_token\":\"old-refresh\""))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error":"invalid_grant"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1/oauth/token"))
        .and(matchers::body_string_contains("\"refresh_token\":\"cli-refresh\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .and(matchers::header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"five_hour":{"utilization":42}})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"old-refresh","expiresAt":1}}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let error = collect(&api, "claude", home.path(), true).await.unwrap_err();
        assert_eq!(error.status, "sign-in");
        assert!(error.message.contains("invalid_grant"));
    }
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"old","refreshToken":"cli-refresh","expiresAt":1}}),
    );
    let provider = collect(&api, "claude", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 42.0);
}

#[test]
fn claude_keeps_zero_cap_and_balance() {
    let usage = json!({
        "five_hour":{"utilization":42,"resets_at":"2026-10-10T00:00:00Z"},
        "spend":{"used":{"amount_minor":0,"currency":"USD"},"limit":{"amount_minor":0},"balance":{"amount_minor":0},"auto_reload":false}
    });
    let provider = claude_provider(&usage, &json!({"subscriptionType":"Pro"}), None, "fixture".into()).unwrap();
    assert_eq!(provider.credit_balance, Some(0.0));
    assert_eq!(provider.spend.as_ref().unwrap().used, Some(0.0));
    assert_eq!(provider.spend.as_ref().unwrap().limit, Some(0.0));
    assert_eq!(provider.spend.as_ref().unwrap().percent, Some(0.0));
    assert_eq!(provider.spend.as_ref().unwrap().balance, Some(0.0));
    assert_eq!(provider.spend.as_ref().unwrap().auto_reload, Some(false));
}

#[test]
fn claude_spend_explains_why_credits_are_off() {
    let provider = claude_provider(
        &json!({"spend":{"enabled":false,"disabled_reason":"organization_disabled"}}),
        &Value::Null,
        None,
        "fixture".into(),
    )
    .unwrap();
    assert_eq!(
        provider.spend.unwrap().disabled_reason.as_deref(),
        Some("Organization Disabled")
    );
}

#[test]
fn claude_legacy_spend_explains_why_credits_are_off() {
    let provider = claude_provider(
        &json!({"extra_usage":{"is_enabled":false,"disabled_reason":"billing_inactive"}}),
        &Value::Null,
        None,
        "fixture".into(),
    )
    .unwrap();
    assert_eq!(
        provider.spend.unwrap().disabled_reason.as_deref(),
        Some("Billing Inactive")
    );
}

#[test]
fn claude_spend_ignores_a_disabled_reason_when_enabled() {
    let provider = claude_provider(
        &json!({"spend":{"enabled":true,"disabled_reason":"old_reason"}}),
        &Value::Null,
        None,
        "fixture".into(),
    )
    .unwrap();
    assert!(provider.spend.unwrap().disabled_reason.is_none());
}

#[test]
fn claude_spend_reports_purchase_availability() {
    let provider = claude_provider(
        &json!({"spend":{"can_purchase_credits":true}}),
        &Value::Null,
        None,
        "fixture".into(),
    )
    .unwrap();
    assert!(provider.spend.unwrap().can_purchase_credits);
}

#[test]
fn claude_credit_reset_matches_provider_response() {
    let provider = claude_provider(
        &json!({"credit_reset_at":"2026-11-01T00:00:00Z","spend":{}}),
        &Value::Null,
        None,
        "fixture".into(),
    )
    .unwrap();
    assert_eq!(
        provider.spend.as_ref().unwrap().reset.as_deref(),
        Some("2026-11-01T00:00:00Z")
    );
}

#[test]
fn claude_credit_toggle_matches_provider_response() {
    let provider = claude_provider(
        &json!({"spend":{"can_toggle":true}}),
        &Value::Null,
        None,
        "fixture".into(),
    )
    .unwrap();
    assert!(provider.spend.as_ref().unwrap().can_toggle);
}

#[tokio::test]
async fn claude_scoped_limit_normalizes_repeated_punctuation() {
    let provider = claude_summary(json!({
        "limits":[{"percent":35,"kind":"weekly_scoped",
            "scope":{"model":{"display_name":"? A--B/"}}}]
    }))
    .await;
    assert_eq!(provider.windows[0].id, "weekly:a-b");
}

#[tokio::test]
async fn claude_spend_calculates_percent_from_minor_currency_values() {
    let provider = claude_summary(json!({
        "spend":{"used":{"amount_minor":2500},"limit":{"amount_minor":10000}}
    }))
    .await;
    assert_eq!(provider.spend.unwrap().percent, Some(25.0));
}

#[tokio::test]
async fn claude_rejects_a_non_object_quota_response() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    let failure = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Unexpected response: claude.usage: invalid payload");
}
