//! Checks claude 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn claude_malformed_spend_does_not_create_a_spend_control() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":25},"spend":false
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap();
    assert!(provider.spend.is_none());
}

#[tokio::test]
async fn claude_inactive_limit_remains_visible_in_usage_state() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "limits":[{"percent":100,"kind":"session","is_active":false}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({
            "claudeAiOauth":{"accessToken":"claude-fixture"}
        }),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap();
    assert!(!provider.windows[0].active);
}

#[tokio::test]
async fn claude_http_names_subscription_tiers_like_the_reference() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":42,"resets_at":"2026-10-10T12:00:00Z"}
        })))
        .expect(7)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    for (subscription, tier, expected) in [
        (None, None, "Claude"),
        (Some("max"), Some("default_claude_max_20x"), "Max 20x"),
        (Some("claude_pro"), None, "Pro"),
        (Some("team"), None, "Team"),
        (Some("enterprise"), None, "Enterprise"),
        (Some("custom_tier"), None, "Custom Tier"),
        (Some("max"), Some("invalid_tier"), "Max"),
    ] {
        credential_file(
            home.path(),
            ".claude/.credentials.json",
            &json!({"claudeAiOauth":{
                "accessToken":"claude-fixture",
                "subscriptionType":subscription,
                "rateLimitTier":tier
            }}),
        );
        let provider = collect(&fixture_api(&server), "claude", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.plan, expected);
    }
}

#[tokio::test]
async fn claude_profile_supplies_identity_and_subscription_details() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":42,"resets_at":"2026-10-10T12:00:00Z"}
        })))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .and(matchers::header("authorization", "Bearer claude-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "account":{"email":"profile@example.com"},
            "organization":{"name":"Example Team","organization_type":"claude_pro",
                "rate_limit_tier":"default_claude_max_5x","subscription_status":"active"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .and(matchers::header("authorization", "Bearer second-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "account":{"email":"second@example.com"},
            "organization":{"organization_type":"claude_team"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture","subscriptionType":"max",
            "emailAddress":"stored@example.com","scopes":["user:profile"]}}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let provider = collect(&api, "claude", home.path(), false).await.unwrap();
        assert_eq!(provider.plan, "Pro 5x");
        assert_eq!(provider.identity, "profile@example.com");
        assert_eq!(provider.organization, "Example Team");
        assert!(provider.warnings.is_empty());
        assert_eq!(
            provider.details[0].explanation,
            "Subscription status reported by the Claude OAuth profile."
        );
        assert!(
            provider
                .details
                .iter()
                .map(|detail| (detail.title.as_str(), detail.value.as_str()))
                .collect::<Vec<_>>()
                .contains(&("Subscription", "Active"))
        );
    }
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"second-fixture","subscriptionType":"max"}}),
    );
    let provider = collect(&api, "claude", home.path(), false).await.unwrap();
    assert_eq!(provider.plan, "Team");
    assert_eq!(provider.identity, "second@example.com");
    assert!(provider.organization.is_empty());
}

#[tokio::test]
async fn claude_profile_account_flags_supply_plan_without_subscription() {
    for (has_max, plan) in [(true, "Max"), (false, "Pro")] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "five_hour":{"utilization":42,"resets_at":"2026-10-10T12:00:00Z"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "account":{"has_claude_max":has_max,"has_claude_pro":!has_max}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        credential_file(
            home.path(),
            ".claude/.credentials.json",
            &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
        );
        let provider = collect(&fixture_api(&server), "claude", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.plan, plan);
    }
}

#[tokio::test]
async fn claude_reports_an_unreadable_credential_file() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".claude/.credentials.json")).unwrap();
    let error = collect(&Api::new().unwrap(), "claude", home.path(), false)
        .await
        .err()
        .unwrap();
    assert_eq!(error.status, "unreadable");
    assert_eq!(error.credential_health, CredentialHealth::Unreadable);
    assert!(error.message.contains("Cannot read"));
}

#[tokio::test]
async fn claude_quota_survives_missing_profile_without_repeated_requests() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":42,"resets_at":"2026-10-10T12:00:00Z"}
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .respond_with(ResponseTemplate::new(403))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture","subscriptionType":"pro",
            "emailAddress":"stored@example.com"}}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let provider = collect(&api, "claude", home.path(), false).await.unwrap();
        assert_eq!(provider.windows[0].used, 42.0);
        assert_eq!(provider.plan, "Pro");
        assert_eq!(provider.identity, "stored@example.com");
        assert_eq!(
            provider.warnings[0],
            "Token lacks the user:profile scope; sign in with `claude` rather than `claude setup-token`."
        );
        assert_eq!(provider.warnings[1], "Profile unavailable: HTTP 403");
    }
}

#[tokio::test]
async fn claude_invalid_profile_keeps_quota_and_explains_the_warning() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":42}
        })))
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"status":"ok"})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 42.0);
    assert_eq!(
        provider.warnings.last().map(String::as_str),
        Some("Profile unavailable: Provider returned an invalid profile")
    );
}

#[tokio::test]
async fn claude_organization_only_profile_supplies_the_plan() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"five_hour":{"utilization":42}})))
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "organization":{"organization_type":"claude_team"}
        })))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.plan, "Team");
    assert_eq!(provider.windows[0].used, 42.0);
    assert!(
        provider
            .warnings
            .iter()
            .all(|warning| !warning.starts_with("Profile unavailable:"))
    );
}

#[tokio::test(start_paused = true)]
async fn claude_keeps_cached_identity_after_profile_failure() {
    let server = MockServer::start().await;
    let (release, parked) = std::sync::mpsc::channel();
    let clock_guard = tokio::task::spawn_blocking(move || parked.recv());
    tokio::task::yield_now().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"five_hour":{"utilization":42}})))
        .mount(&server)
        .await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let profile_attempts = attempts.clone();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .respond_with(move |_: &wiremock::Request| {
            if profile_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_json(json!({
                    "account":{"email":"profile@example.com"},
                    "organization":{"name":"Example Team"}
                }))
            } else {
                ResponseTemplate::new(503)
            }
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    let api = fixture_api(&server);
    assert_eq!(
        collect(&api, "claude", home.path(), false).await.unwrap().identity,
        "profile@example.com"
    );
    tokio::time::advance(Duration::from_secs(6 * 3600 + 1)).await;
    let provider = collect(&api, "claude", home.path(), false).await.unwrap();
    assert_eq!(provider.identity, "profile@example.com");
    assert_eq!(provider.organization, "Example Team");
    assert_eq!(
        provider.warnings.last().map(String::as_str),
        Some("Profile unavailable: HTTP 503")
    );
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"other-token","emailAddress":"other@example.com"}}),
    );
    let provider = collect(&api, "claude", home.path(), false).await.unwrap();
    assert_eq!(provider.identity, "other@example.com");
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
    release.send(()).unwrap();
    clock_guard.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn claude_retries_an_empty_profile_cache_after_its_short_lifetime() {
    let server = MockServer::start().await;
    let (release, parked) = std::sync::mpsc::channel();
    let clock_guard = tokio::task::spawn_blocking(move || parked.recv());
    tokio::task::yield_now().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"five_hour":{"utilization":42}})))
        .expect(3)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/profile"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"cache-fixture","emailAddress":"stored@example.com"}}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let provider = collect(&api, "claude", home.path(), false).await.unwrap();
        assert_eq!(provider.identity, "stored@example.com");
    }
    tokio::time::advance(Duration::from_secs(301)).await;
    let provider = collect(&api, "claude", home.path(), false).await.unwrap();
    assert_eq!(provider.identity, "stored@example.com");
    server.verify().await;
    release.send(()).unwrap();
    clock_guard.await.unwrap().unwrap();
}

#[tokio::test]
async fn claude_http_reports_spend_and_exhausted_window_notices() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":100,"resets_at":"2026-10-10T12:00:00Z"},
            "extra_usage":{"spend_limit_reached":true}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({
            "claudeAiOauth":{"accessToken":"claude-fixture"}
        }),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.notices[0].kind, NoticeKind::SpendControl);
    assert_eq!(provider.notices[1].kind, NoticeKind::LimitReached);
    assert_eq!(provider.notices[1].window_id.as_deref(), Some("session"));
    assert_eq!(provider.notices[1].resets_at.as_deref(), Some("2026-10-10T12:00:00Z"));
}
