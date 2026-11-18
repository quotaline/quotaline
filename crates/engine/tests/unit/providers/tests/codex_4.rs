//! Checks codex 4 behavior through the public API.

use super::*;

#[tokio::test]
async fn codex_textual_reset_expiry_stays_visible_without_an_alert() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "credits":[
                {"status":"available","title":"Promo reset","expires_at":"soon"},
                {"status":"available","title":"No date"}
            ]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"reset-text-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert!(provider.reset_credit_expiries.is_empty());
    let details = provider.reset_credits.unwrap().details;
    assert_eq!(details[0].expires_at.as_deref(), Some("soon"));
    assert_eq!(details[1].title, "No date");
    server.verify().await;
}

#[tokio::test]
async fn codex_reset_credit_cache_evicts_old_accounts() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}}
        })))
        .expect(21)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"available_count":7})))
        .expect(21)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let api = fixture_api(&server);
    for index in 0..20 {
        credential_file(
            home.path(),
            ".codex/auth.json",
            &json!({"tokens":{"access_token":format!("bounded-cache-{index}")}}),
        );
        let provider = collect(&api, "codex", home.path(), false).await.unwrap();
        assert_eq!(provider.reset_credits.unwrap().available, 7);
    }
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"bounded-cache-0"}}),
    );
    let provider = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(provider.reset_credits.unwrap().available, 7);
    server.verify().await;
}

#[tokio::test(start_paused = true)]
async fn codex_retries_reset_credits_after_failed_cache_expires() {
    let server = MockServer::start().await;
    let (release, parked) = std::sync::mpsc::channel();
    let clock_guard = tokio::task::spawn_blocking(move || parked.recv());
    tokio::task::yield_now().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}}
        })))
        .mount(&server)
        .await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let reset_attempts = attempts.clone();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(move |_: &wiremock::Request| {
            if reset_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"available_count":7}))
            }
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"reset-expiry-fixture"}}),
    );
    let api = fixture_api(&server);
    let first = collect(&api, "codex", home.path(), false).await.unwrap();
    assert!(first.reset_credits.is_none());
    assert_eq!(first.warnings, ["Reset credits unavailable: HTTP 503"]);
    tokio::time::advance(Duration::from_secs(301)).await;
    let second = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(second.reset_credits.unwrap().available, 7);
    assert!(second.warnings.is_empty());
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    release.send(()).unwrap();
    clock_guard.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn codex_keeps_cached_reset_credits_when_the_lookup_fails() {
    let server = MockServer::start().await;
    let (release, parked) = std::sync::mpsc::channel();
    let clock_guard = tokio::task::spawn_blocking(move || parked.recv());
    tokio::task::yield_now().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}}
        })))
        .mount(&server)
        .await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let reset_attempts = attempts.clone();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(move |_: &wiremock::Request| {
            if reset_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(200).set_body_json(json!({"available_count":4}))
            } else {
                ResponseTemplate::new(503)
            }
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"reset-stale-fixture"}}),
    );
    let api = fixture_api(&server);
    let first = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(first.reset_credits.unwrap().available, 4);
    tokio::time::advance(Duration::from_secs(901)).await;
    let second = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(second.reset_credits.unwrap().available, 4);
    assert_eq!(second.warnings, ["Reset credits unavailable: HTTP 503"]);
    let third = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(third.reset_credits.unwrap().available, 4);
    assert_eq!(third.warnings, second.warnings);
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    release.send(()).unwrap();
    clock_guard.await.unwrap().unwrap();
}

#[tokio::test]
async fn codex_account_change_invalidates_reset_credit_cache() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25}}
        })))
        .expect(2)
        .mount(&server)
        .await;
    for (account, count) in [("account-a", 2), ("account-b", 3)] {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
            .and(matchers::header("chatgpt-account-id", account))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"available_count":count})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    let api = fixture_api(&server);
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"switch-fixture","account_id":"account-a"}}),
    );
    let first = collect(&api, "codex", home.path(), false).await.unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"switch-fixture","account_id":"account-b"}}),
    );
    let second = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(first.reset_credits.unwrap().available, 2);
    assert_eq!(second.reset_credits.unwrap().available, 3);
}

#[test]
fn codex_primary_and_secondary() {
    let response = json!({"rate_limit": {
        "primary_window": {"used_percent": 20, "limit_window_seconds": 18000, "reset_at": 1750000000},
        "secondary_window": {"used_percent": 70, "limit_window_seconds": 604800}
    }});
    let windows = codex_provider(&response, "fixture".into()).windows;
    assert_eq!(windows.len(), 2);
    assert_eq!(windows[0].group, "session");
    assert_eq!(windows[1].group, "weekly");
}

#[test]
fn codex_keeps_usage_balance_and_reset_credits_separate() {
    let usage = json!({
        "rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000}},
        "credits":{"balance":"0"},
        "spend_control":{"individual_limit":{"used":"2.5","limit":"10"}},
        "rate_limit_reset_credits":{"available_count":2,"credits":[
            {"title":"Promo reset","status":"available","expires_at":"2026-10-31T00:00:00Z"},
            {"title":"Used reset","status":"used","expires_at":"2026-10-31T00:00:00Z"}
        ]}
    });
    let provider = codex_provider(&usage, "fixture".into());
    assert_eq!(provider.credit_balance, Some(0.0));
    assert_eq!(
        provider.spend.as_ref().unwrap().credit_allowance.as_ref().unwrap().used,
        Some(2.5)
    );
    assert_eq!(
        provider
            .spend
            .as_ref()
            .unwrap()
            .credit_allowance
            .as_ref()
            .unwrap()
            .limit,
        Some(10.0)
    );
    assert_eq!(provider.spend.as_ref().unwrap().currency, None);
    assert_eq!(provider.reset_credits.as_ref().unwrap().available, 2);
    assert_eq!(
        provider.reset_credits.as_ref().unwrap().details[1].status.as_deref(),
        Some("Used")
    );
}

#[test]
fn codex_credit_allowance_keeps_remaining_and_source() {
    let provider = codex_provider(
        &json!({"spend_control":{"individual_limit":{
            "used":"2.125","limit":"10","remaining":"7.875","source":"workspace_owner"
        }}}),
        "fixture".into(),
    );
    let allowance = provider.spend.as_ref().unwrap().credit_allowance.as_ref().unwrap();
    assert_eq!(allowance.remaining, Some(7.875));
    assert_eq!(allowance.source.as_deref(), Some("workspace_owner"));
}

#[tokio::test]
async fn codex_additional_limit_accepts_legacy_name_field() {
    let provider = codex_summary(json!({
        "additional_rate_limits":[{"name":"Review quota","rate_limit":{
            "primary_window":{"used_percent":25}
        }}]
    }))
    .await;
    assert_eq!(provider.windows[0].scope.as_deref(), Some("Review quota"));
}

#[tokio::test]
async fn codex_unnamed_additional_limit_stays_visible() {
    let provider = codex_summary(json!({
        "additional_rate_limits":[{"rate_limit":{"primary_window":{"used_percent":25}}}]
    }))
    .await;
    assert_eq!(provider.windows[0].scope.as_deref(), Some("Additional"));
}

#[tokio::test]
async fn codex_reports_an_unreadable_local_rollout_directory() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".codex")).unwrap();
    fs::write(home.path().join(".codex/sessions"), b"not a directory").unwrap();
    let failure = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Cannot read Codex rollouts");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn codex_reset_credit_cache_is_owned_by_the_api() {
    let first = MockServer::start().await;
    let second = MockServer::start().await;
    for (server, available) in [(&first, 4), (&second, 9)] {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "rate_limit":{"primary_window":{"used_percent":25}}
            })))
            .mount(server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"available_count":available})))
            .expect(1)
            .mount(server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"same-account"}}),
    );
    let seeded = collect(&fixture_api(&first), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(seeded.reset_credits.unwrap().available, 4);
    let isolated = collect(&fixture_api(&second), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(isolated.reset_credits.unwrap().available, 9);
    second.verify().await;
}
