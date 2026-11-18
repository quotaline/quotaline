//! Checks copilot 1 behavior through the public API.

use super::*;

#[tokio::test]
async fn copilot_http_uses_cli_session() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token copilot-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quota_snapshots":{"premium_interactions":{"entitlement":100,"quota_remaining":75}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({"loggedInUsers":[{"token":"copilot-fixture","host":"github.com"}]}),
    );
    let api = fixture_api(&server);
    #[cfg(feature = "fixture-api")]
    let api = Api {
        fixture_base: None,
        ..api
    };
    let provider = collect(&api, "copilot", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 25.0);
    assert_eq!(
        provider.details[0].value,
        "Included: 100 requests · Remaining: 75 requests"
    );
}

#[tokio::test]
async fn copilot_cli_session_accepts_legacy_token_and_username_fields() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token legacy-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({"loggedInUsers":[{
            "oauth_token":"legacy-fixture","url":"https://github.com","username":"octavia"
        }]}),
    );
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.identity, "octavia");
    server.verify().await;
}

#[tokio::test]
async fn copilot_object_session_preserves_account_identity() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token object-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({"loggedInUsers":{"https://github.com":{"login":"ada","oauthToken":"object-fixture"}}}),
    );
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.identity, "ada");
    server.verify().await;
}

#[tokio::test]
async fn copilot_object_session_rejects_invalid_host_key() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({"loggedInUsers":{"corp.example/path":{"login":"ada","token":"object-fixture"}}}),
    );
    let failure = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Invalid GitHub host");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn copilot_legacy_session_defaults_an_empty_host_to_github() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token legacy-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".config/github-copilot/hosts.json",
        &json!({"":{"oauth_token":"legacy-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.status, "connected");
    server.verify().await;
}

#[tokio::test]
async fn copilot_file_session_uses_api_identity_without_a_user() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token account-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"login":"octocat"})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({"loggedInUsers":[{"keychainAccount":"https://github.com:grace","token":"account-fixture"}]}),
    );
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.identity, "octocat");
    server.verify().await;
}

#[tokio::test]
async fn copilot_legacy_hosts_prefers_github_and_keeps_its_user() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token gho_test_token_123"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join(".config/github-copilot/hosts.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, include_str!("../../../fixtures/copilot_hosts.json")).unwrap();
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.identity, "octocat");
    assert_eq!(provider.source, path.to_string_lossy());
}

#[tokio::test]
async fn copilot_legacy_hosts_skips_entries_without_tokens() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token valid-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".config/github-copilot/hosts.json",
        &json!({
            "api.github.com":{"user":"missing-token"},
            "github.com":{"oauth_token":"valid-fixture","user":"valid-user"}
        }),
    );
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.identity, "valid-user");
    server.verify().await;
}

#[tokio::test]
async fn copilot_uses_legacy_apps_when_hosts_file_is_damaged() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .and(matchers::header("authorization", "token apps-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(home.path(), ".copilot/config.json", &json!({"loggedInUsers":[]}));
    let root = home.path().join(".config/github-copilot");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("hosts.json"), "{").unwrap();
    let apps = root.join("apps.json");
    fs::write(
        &apps,
        json!({"github.com":{"oauth_token":"apps-fixture","user":"ada"}}).to_string(),
    )
    .unwrap();
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.identity, "ada");
    assert_eq!(provider.source, apps.to_string_lossy());
}

#[tokio::test]
async fn copilot_reports_the_damaged_cli_file_before_missing_fallbacks() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".copilot");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("config.json"), "{").unwrap();
    let failure = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.status, "unreadable");
    assert!(failure.message.contains("config.json is not valid JSON"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn copilot_reports_a_damaged_legacy_file_without_other_credentials() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".config/github-copilot");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("hosts.json"), "{").unwrap();
    let failure = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.status, "unreadable");
    assert!(failure.message.contains("hosts.json is not valid JSON"));
}

#[tokio::test]
async fn copilot_keeps_the_cli_error_when_legacy_is_also_damaged() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let cli = home.path().join(".copilot");
    let legacy = home.path().join(".config/github-copilot");
    fs::create_dir_all(&cli).unwrap();
    fs::create_dir_all(&legacy).unwrap();
    fs::write(cli.join("config.json"), "{").unwrap();
    fs::write(legacy.join("hosts.json"), "{").unwrap();
    let failure = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.status, "unreadable");
    assert!(failure.message.contains("config.json is not valid JSON"));
}

#[tokio::test]
async fn copilot_reports_an_unreadable_legacy_file() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(".config/github-copilot");
    fs::create_dir_all(root.join("hosts.json")).unwrap();
    let failure = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.status, "unreadable");
    assert!(failure.message.contains("Cannot read"));
    assert!(failure.message.contains("hosts.json"));
}

#[tokio::test]
async fn copilot_ignores_incomplete_free_quota_pairs() {
    for (remaining, limit) in [
        (json!("invalid"), json!(100)),
        (json!(50), Value::Null),
        (json!(50), json!(0)),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "limited_user_quotas":{"premium_interactions":remaining},
                "monthly_quotas":{"premium_interactions":limit}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".copilot/config.json",
            &json!({"loggedInUsers":[{"token":"free-quota-fixture","host":"github.com"}]}),
        );
        let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
            .await
            .unwrap();
        assert!(provider.windows.iter().all(|window| !window.id.starts_with("free:")));
        server.verify().await;
    }
}

#[tokio::test]
async fn copilot_paid_fixture_orders_windows_and_preserves_day_resets() {
    let data = serde_json::from_str(include_str!("../../../fixtures/copilot_user.json")).unwrap();
    let provider = copilot_summary(data).await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["premium_interactions", "completions"]
    );
    assert_eq!(provider.windows[1].used, 75.0);
    assert!(
        provider
            .windows
            .iter()
            .all(|window| window.reset_precision == ResetPrecision::Day)
    );
    assert_eq!(provider.windows[0].reset.as_deref(), Some("2026-09-01T00:00:00Z"));
}

#[tokio::test]
async fn copilot_paid_fixture_keeps_account_identity() {
    let data = serde_json::from_str(include_str!("../../../fixtures/copilot_user.json")).unwrap();
    let provider = copilot_summary(data).await;
    assert_eq!(provider.plan, "Pro Plus");
    assert_eq!(provider.identity, "octocat");
}

#[tokio::test]
async fn copilot_paid_fixture_keeps_only_supported_overage_notices() {
    let data = serde_json::from_str(include_str!("../../../fixtures/copilot_user.json")).unwrap();
    let provider = copilot_summary(data).await;
    assert_eq!(provider.notices.len(), 1);
    assert_eq!(provider.notices[0].kind, NoticeKind::Info);
    assert_eq!(
        provider.notices[0].text,
        "Premium credits: quota exceeded, 15 overage credits."
    );
}

#[tokio::test]
async fn copilot_denied_overage_is_a_limit_warning() {
    let provider = copilot_summary(json!({
        "quota_snapshots":{"premium_interactions":{
            "entitlement":100,"quota_remaining":-2,"overage_count":2,"overage_permitted":false
        }}
    }))
    .await;
    assert_eq!(provider.notices.len(), 1);
    assert_eq!(provider.notices[0].kind, NoticeKind::LimitReached);
    assert_eq!(
        provider.notices[0].text,
        "Premium requests: quota exceeded, 2 overage requests."
    );
}

#[tokio::test]
async fn copilot_ignores_custom_category_overage_notices() {
    let provider = copilot_summary(json!({
        "quota_snapshots":{"custom":{"percent_remaining":-10,"overage_count":2}}
    }))
    .await;
    assert_eq!(provider.windows.len(), 1);
    assert!(provider.notices.is_empty());
}

#[tokio::test]
async fn copilot_paid_fixture_sorts_budget_details() {
    let data = serde_json::from_str(include_str!("../../../fixtures/copilot_user.json")).unwrap();
    let provider = copilot_summary(data).await;
    assert_eq!(
        provider
            .details
            .iter()
            .map(|detail| detail.title.as_str())
            .collect::<Vec<_>>(),
        ["Chat", "Completions", "Premium credits", "Zz Custom"]
    );
    assert!(provider.details[2].explanation.ends_with("Token-based credit billing."));
}

#[tokio::test]
async fn copilot_request_details_round_provider_quantities() {
    let provider = copilot_summary(json!({
        "quota_snapshots":{"chat":{"entitlement":"10.1234567","quota_remaining":"4.5"}}
    }))
    .await;
    assert_eq!(
        provider.details[0].value,
        "Included: 10.123457 requests · Remaining: 4.5 requests"
    );
    assert!(provider.details[0].explanation.ends_with("Request-based billing."));
}

#[tokio::test]
async fn copilot_free_fixture_places_legacy_windows_after_snapshots() {
    let data = serde_json::from_str(include_str!("../../../fixtures/copilot_user_free.json")).unwrap();
    let provider = copilot_summary(data).await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["premium_interactions", "free:chat", "free:completions"]
    );
    assert!((provider.windows[1].used - 18.0).abs() < 0.001);
    assert_eq!(provider.windows[0].reset_precision, ResetPrecision::Day);
}

#[tokio::test]
async fn copilot_category_reset_overrides_the_billing_cycle_day() {
    let provider = copilot_summary(json!({
        "quota_reset_date":"2026-10-01",
        "quota_snapshots":{"chat":{"entitlement":100,"remaining":50,"quota_reset_at":1788558705}}
    }))
    .await;
    assert_eq!(provider.windows[0].reset.as_deref(), Some("2026-09-04T21:51:45+00:00"));
    assert_eq!(provider.windows[0].reset_precision, ResetPrecision::Instant);
}

#[tokio::test]
async fn copilot_invalid_reset_has_no_countdown() {
    let provider = copilot_summary(json!({
        "quota_reset_date":"whenever",
        "quota_snapshots":{"chat":{"percent_remaining":40}}
    }))
    .await;
    assert_eq!(provider.windows[0].reset, None);
    assert_eq!(provider.windows[0].used, 60.0);
}

#[tokio::test]
async fn copilot_http_reports_credit_overage_notice() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "token_based_billing":true,
            "quota_snapshots":{"premium_interactions":{
                "percent_remaining":-10,"overage_count":3,"overage_entitlement":10,"overage_permitted":true
            }}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({
            "loggedInUsers":[{"token":"copilot-fixture","host":"github.com"}]
        }),
    );
    let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.notices[0].kind, NoticeKind::Info);
    assert_eq!(
        provider.notices[0].text,
        "Premium credits: quota exceeded, 3 overage credits."
    );
    assert_eq!(
        provider.details[0].value,
        "Overage used: 3 credits · Overage allowance: 10 credits · Overage allowed"
    );
}
