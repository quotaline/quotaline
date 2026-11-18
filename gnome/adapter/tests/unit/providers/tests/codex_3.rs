//! Checks codex 3 behavior through the public API.

use super::*;
use quotaline_analytics::Endpoint;

#[tokio::test]
async fn expiring_codex_file_token_refreshes_before_usage() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/oauth/token"))
        .and(matchers::body_json(json!({
            "client_id":"app_EMoamEEZ73f0CkXaXp7hrann","grant_type":"refresh_token","refresh_token":"old-refresh"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"new-access","refresh_token":"new-refresh","id_token":"new-id"
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer new-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":37,"limit_window_seconds":18000}},
            "rate_limit_reset_credits":[]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let expired = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(json!({"exp":Timestamp::now().as_second() + 60}).to_string())
    );
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({
            "tokens":{"access_token":expired,"refresh_token":"old-refresh","account_id":"account-1"},
            "auth_mode":"chatgpt"
        }),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), true)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 37.0);
    let saved: Value = serde_json::from_slice(&fs::read(home.path().join(".codex/auth.json")).unwrap()).unwrap();
    assert_eq!(saved["tokens"]["access_token"], "new-access");
    assert_eq!(saved["tokens"]["refresh_token"], "new-refresh");
    assert_eq!(saved["tokens"]["id_token"], "new-id");
    assert_eq!(saved["tokens"]["account_id"], "account-1");
    assert!(saved["last_refresh"].as_str().is_some());
}

#[tokio::test]
async fn expired_codex_file_token_does_not_refresh_without_opt_in() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let expired = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    let document = json!({"tokens":{"access_token":expired,"refresh_token":"old-refresh"}});
    credential_file(home.path(), ".codex/auth.json", &document);
    let error = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(server.received_requests().await.unwrap().is_empty());
    let saved: Value = serde_json::from_slice(&fs::read(home.path().join(".codex/auth.json")).unwrap()).unwrap();
    assert_eq!(saved, document);
}

#[tokio::test]
async fn codex_refresh_requires_a_refresh_token() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let expired = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    let document = json!({"tokens":{"access_token":expired}});
    credential_file(home.path(), ".codex/auth.json", &document);
    let failure = collect(&fixture_api(&server), "codex", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(
        failure.message,
        "Codex refresh token is missing; sign in with Codex CLI."
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    let saved: Value = serde_json::from_slice(&fs::read(home.path().join(".codex/auth.json")).unwrap()).unwrap();
    assert_eq!(saved, document);
}

#[tokio::test]
async fn codex_refresh_rejects_a_response_without_an_access_token() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/oauth/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"refresh_token":"new-refresh"})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let expired = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    let document = json!({"tokens":{"access_token":expired,"refresh_token":"old-refresh"}});
    credential_file(home.path(), ".codex/auth.json", &document);
    let failure = collect(&fixture_api(&server), "codex", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Codex refresh returned no access token");
    let saved: Value = serde_json::from_slice(&fs::read(home.path().join(".codex/auth.json")).unwrap()).unwrap();
    assert_eq!(saved, document);
}

#[tokio::test]
async fn concurrent_codex_cli_change_is_not_overwritten() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let expired = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":expired,"refresh_token":"old-refresh"}}),
    );
    let path = home.path().join(".codex/auth.json");
    let changed = json!({"tokens":{"access_token":"cli","refresh_token":"cli-refresh"}});
    let replacement = changed.to_string();
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/oauth/token"))
        .respond_with(move |_: &wiremock::Request| {
            fs::write(&path, &replacement).unwrap();
            ResponseTemplate::new(200).set_body_json(json!({"access_token":"service"}))
        })
        .expect(1)
        .mount(&server)
        .await;
    let error = collect(&fixture_api(&server), "codex", home.path(), true)
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(error.message.contains("changed during refresh"));
    let saved: Value = serde_json::from_slice(&fs::read(home.path().join(".codex/auth.json")).unwrap()).unwrap();
    assert_eq!(saved, changed);
}

#[tokio::test]
async fn rejected_codex_refresh_waits_for_a_new_cli_credential() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/oauth/token"))
        .and(matchers::body_string_contains("\"refresh_token\":\"old-refresh\""))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"error":"invalid_client"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/oauth/token"))
        .and(matchers::body_string_contains("\"refresh_token\":\"cli-refresh\""))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"fresh"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":37,"limit_window_seconds":18000}},
            "rate_limit_reset_credits":[]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let expired = format!(
        "h.{}.s",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"exp":1}"#)
    );
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":expired,"refresh_token":"old-refresh"}}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let error = collect(&api, "codex", home.path(), true).await.unwrap_err();
        assert_eq!(error.status, "sign-in");
        assert!(error.message.contains("invalid_client"));
    }
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":expired,"refresh_token":"cli-refresh"}}),
    );
    let provider = collect(&api, "codex", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 37.0);
}

#[tokio::test]
async fn codex_analytics_requests_every_daily_endpoint_and_the_credit_events() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage/credit-usage-events"))
        .and(matchers::header("chatgpt-account-id", "account-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .expect(1)
        .mount(&server)
        .await;
    for endpoint in Endpoint::ALL {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/backend-api/{}", endpoint.path())))
            .and(matchers::header("chatgpt-account-id", "account-fixture"))
            .and(matchers::query_param("group_by", "day"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"analytics-fixture","account_id":"account-fixture"}}),
    );
    assert_eq!(
        covered_endpoints(codex_analytics(&fixture_api(&server), home.path(), 7, None).await),
        Endpoint::ALL.len()
    );
}

#[tokio::test]
async fn codex_analytics_resume_from_the_stored_watermarks() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"watermark-fixture"}}),
    );
    let api = fixture_api(&server);
    let first = codex_analytics(&api, home.path(), 30, None).await.unwrap();
    let stored = first.watermarks.unwrap();
    server.reset().await;
    Mock::given(matchers::method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
        .mount(&server)
        .await;
    codex_analytics(&api, home.path(), 30, Some(&stored)).await.unwrap();
    let yesterday = (Timestamp::now() - jiff::SignedDuration::from_hours(24))
        .strftime("%Y-%m-%d")
        .to_string();
    let requests = server.received_requests().await.unwrap();
    let starts = requests
        .iter()
        .filter_map(|request| {
            request
                .url
                .query_pairs()
                .find(|(name, _)| name == "start_date")
                .map(|(_, value)| value.into_owned())
        })
        .collect::<Vec<_>>();
    assert_eq!(starts, vec![yesterday; Endpoint::ALL.len()]);
}

#[tokio::test]
async fn codex_analytics_without_credentials_is_empty() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    assert!(
        codex_analytics(&fixture_api(&server), home.path(), 7, None)
            .await
            .is_none()
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn codex_analytics_accepts_api_key_credentials() {
    let server = MockServer::start().await;
    for endpoint in Endpoint::ALL {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/backend-api/{}", endpoint.path())))
            .and(matchers::header("authorization", "Bearer api-key-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"OPENAI_API_KEY":"api-key-fixture"}),
    );
    assert_eq!(
        covered_endpoints(codex_analytics(&fixture_api(&server), home.path(), 7, None).await),
        Endpoint::ALL.len()
    );
}

#[tokio::test]
async fn codex_analytics_recovers_after_malformed_credentials_are_repaired() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".codex/auth.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{").unwrap();
    assert!(
        codex_analytics(&fixture_api(&server), home.path(), 7, None)
            .await
            .is_none()
    );
    for endpoint in Endpoint::ALL {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/backend-api/{}", endpoint.path())))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
            .expect(1)
            .mount(&server)
            .await;
    }
    fs::write(path, json!({"tokens":{"access_token":"repaired-fixture"}}).to_string()).unwrap();
    assert_eq!(
        covered_endpoints(codex_analytics(&fixture_api(&server), home.path(), 7, None).await),
        Endpoint::ALL.len()
    );
}

#[tokio::test]
async fn codex_analytics_continues_after_server_error() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage/daily-token-usage-breakdown"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    for endpoint in Endpoint::ALL.into_iter().skip(1) {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/backend-api/{}", endpoint.path())))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"error-fixture"}}),
    );
    assert_eq!(
        covered_endpoints(codex_analytics(&fixture_api(&server), home.path(), 7, None).await),
        4
    );
}

#[tokio::test]
async fn codex_analytics_ignores_success_responses_without_daily_data() {
    for payload in [json!({}), json!({"data":null})] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/backend-api/{}", Endpoint::TokenUsage.path())))
            .respond_with(ResponseTemplate::new(200).set_body_json(payload))
            .expect(1)
            .mount(&server)
            .await;
        for endpoint in Endpoint::ALL.into_iter().skip(1) {
            Mock::given(matchers::method("GET"))
                .and(matchers::path(format!("/backend-api/{}", endpoint.path())))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
                .expect(1)
                .mount(&server)
                .await;
        }
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".codex/auth.json",
            &json!({"tokens":{"access_token":"invalid-data-fixture"}}),
        );
        let snapshot = codex_analytics(&fixture_api(&server), home.path(), 7, None)
            .await
            .and_then(|analytics| analytics.snapshot)
            .unwrap();
        assert_eq!(snapshot.covered_scopes.len(), 4);
        assert!(
            snapshot
                .covered_scopes
                .iter()
                .all(|scope| scope.metrics != Endpoint::TokenUsage.metrics())
        );
    }
}

#[tokio::test]
async fn codex_reset_credits_reuses_cached_response() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}}
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"available_count":5})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"cache-fixture"}}),
    );
    let api = fixture_api(&server);
    let first = collect(&api, "codex", home.path(), false).await.unwrap();
    let second = collect(&api, "codex", home.path(), false).await.unwrap();
    for provider in [first, second] {
        assert_eq!(provider.reset_credits.unwrap().available, 5);
        assert!(provider.warnings.is_empty());
    }
}

#[tokio::test]
async fn codex_complete_inline_reset_credits_seed_the_cache() {
    let server = MockServer::start().await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let usage_attempts = attempts.clone();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(move |_: &wiremock::Request| {
            let resets = if usage_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                json!({"available_count":3,"total_earned_count":5,
                    "immediate_reset_purchase_eligible":true})
            } else {
                Value::Null
            };
            ResponseTemplate::new(200).set_body_json(json!({
                "rate_limit":{"primary_window":{"used_percent":25}},
                "rate_limit_reset_credits":resets
            }))
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"reset-inline-fixture"}}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let provider = collect(&api, "codex", home.path(), false).await.unwrap();
        assert_eq!(provider.reset_credits.unwrap().available, 3);
        assert!(provider.warnings.is_empty());
    }
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn codex_partial_inline_reset_credits_survive_a_lookup_failure() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}},
            "rate_limit_reset_credits":{"available_count":2}
        })))
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"reset-partial-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 25.0);
    assert_eq!(provider.reset_credits.unwrap().available, 2);
    assert_eq!(provider.warnings, ["Reset credits unavailable: HTTP 503"]);
}
