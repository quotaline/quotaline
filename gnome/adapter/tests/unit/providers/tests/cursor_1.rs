//! Checks cursor 1 behavior through the public API.

use super::*;

#[tokio::test]
async fn cursor_http_uses_local_session() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .and(matchers::header_exists("cookie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "individualUsage":{"plan":{"used":40,"limit":100}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 40.0);
    assert!(provider.credential_usable);
    assert_eq!(provider.credential_health, CredentialHealth::Valid);
}

#[tokio::test]
async fn cursor_expiring_sessions_require_sign_in_before_a_request() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    for expiry in [1, Timestamp::now().as_second() + 60] {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json!({"exp":expiry}).to_string());
        credential_file(
            home.path(),
            ".cursor/auth.json",
            &json!({"accessToken":format!("header.{payload}.signature")}),
        );
        let error = collect(&fixture_api(&server), "cursor", home.path(), false)
            .await
            .unwrap_err();
        assert_eq!(error.status, "sign-in");
        assert!(error.message.contains("expired"));
        assert!(!error.credential_usable);
        assert_eq!(error.credential_health, CredentialHealth::Expired);
    }
}

#[tokio::test]
async fn cursor_valid_jwt_session_reaches_usage() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "individualUsage":{"plan":{"used":35,"limit":100}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(json!({"exp":Timestamp::now().as_second() + 3600}).to_string());
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":format!("header.{payload}.signature")}),
    );
    let provider = collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 35.0);
}

#[tokio::test]
async fn cursor_cookie_preserves_the_complete_user_subject() {
    let server = MockServer::start().await;
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(json!({"sub":"auth0|user|region","exp":Timestamp::now().as_second() + 3600}).to_string());
    let token = format!("header.{payload}.signature");
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .and(matchers::header(
            "cookie",
            format!("WorkosCursorSessionToken=user|region%3A%3A{token}"),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"isUnlimited":true})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(home.path(), ".cursor/auth.json", &json!({"accessToken":token}));
    assert!(
        collect(&fixture_api(&server), "cursor", home.path(), false)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn cursor_http_reports_unlimited_plan_notice() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "isUnlimited":true,"displayMessage":"Dashboard text is not a fallback notice"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap();
    assert!(provider.windows.is_empty());
    assert_eq!(provider.notices.len(), 1);
    assert_eq!(provider.notices[0].text, "This plan has unlimited usage.");
}

#[tokio::test]
async fn cursor_http_falls_back_to_period_usage_and_reports_its_message() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/aiserver.v1.DashboardService/GetCurrentPeriodUsage"))
        .and(matchers::header("authorization", "Bearer cursor-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "planUsage":{"used":60,"limit":100},
            "displayMessage":"You have used 60% of your plan."
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 60.0);
    assert_eq!(provider.notices[0].text, "You have used 60% of your plan.");
    assert_eq!(provider.warnings[0], "Dashboard summary unavailable: HTTP 503");
}

#[tokio::test]
async fn cursor_http_omits_an_empty_period_message() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/aiserver.v1.DashboardService/GetCurrentPeriodUsage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "planUsage":{"used":60,"limit":100},"displayMessage":""
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let provider = collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap();
    assert!(provider.notices.is_empty());
    assert_eq!(provider.windows[0].used, 60.0);
}

#[tokio::test]
async fn cursor_http_reads_and_caches_account_email() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "individualUsage":{"plan":{"used":40,"limit":100}}
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/auth/me"))
        .and(matchers::header_exists("cookie"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"email":"cursor@example.com"})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let provider = collect(&api, "cursor", home.path(), false).await.unwrap();
        assert_eq!(provider.identity, "cursor@example.com");
    }
}

#[tokio::test]
async fn cursor_http_caches_a_failed_identity_lookup() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"isUnlimited":true})))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/auth/me"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let api = fixture_api(&server);
    for _ in 0..2 {
        let provider = collect(&api, "cursor", home.path(), false).await.unwrap();
        assert!(provider.identity.is_empty());
    }
}

#[tokio::test(start_paused = true)]
async fn cursor_retries_identity_after_failed_cache_expires() {
    let server = MockServer::start().await;
    let (release, parked) = std::sync::mpsc::channel();
    let clock_guard = tokio::task::spawn_blocking(move || parked.recv());
    tokio::task::yield_now().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"isUnlimited":true})))
        .mount(&server)
        .await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let identity_attempts = attempts.clone();
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/auth/me"))
        .respond_with(move |_: &wiremock::Request| {
            if identity_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503)
            } else {
                ResponseTemplate::new(200).set_body_json(json!({"email":"cursor@example.com"}))
            }
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".cursor/auth.json",
        &json!({"accessToken":"cursor-fixture"}),
    );
    let api = fixture_api(&server);
    assert!(
        collect(&api, "cursor", home.path(), false)
            .await
            .unwrap()
            .identity
            .is_empty()
    );
    tokio::time::advance(Duration::from_secs(301)).await;
    assert_eq!(
        collect(&api, "cursor", home.path(), false).await.unwrap().identity,
        "cursor@example.com"
    );
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    release.send(()).unwrap();
    clock_guard.await.unwrap().unwrap();
}

#[tokio::test]
async fn cursor_http_refreshes_identity_after_account_change() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"isUnlimited":true})))
        .expect(2)
        .mount(&server)
        .await;
    for (token, email) in [
        ("cursor-first", "first@example.com"),
        ("cursor-second", "second@example.com"),
    ] {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/auth/me"))
            .and(matchers::header(
                "cookie",
                format!("WorkosCursorSessionToken=%3A%3A{token}"),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"email":email})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    let api = fixture_api(&server);
    for (token, email) in [
        ("cursor-first", "first@example.com"),
        ("cursor-second", "second@example.com"),
    ] {
        credential_file(
            home.path(),
            ".cursor/auth.json",
            &json!({
                "accessToken":token,"email":"old@example.com"
            }),
        );
        let provider = collect(&api, "cursor", home.path(), false).await.unwrap();
        assert_eq!(provider.identity, email);
    }
}

#[tokio::test]
async fn cursor_reports_damaged_app_state_when_the_agent_file_is_unreadable() {
    for (kind, expected) in [
        ("directory", "Cannot read Cursor state"),
        ("invalid", "Cannot query Cursor state"),
        ("wrong-column", "Cannot read Cursor state"),
    ] {
        let home = tempfile::tempdir().unwrap();
        let state = home.path().join(".config/Cursor/User/globalStorage/state.vscdb");
        fs::create_dir_all(state.parent().unwrap()).unwrap();
        match kind {
            "directory" => fs::create_dir(&state).unwrap(),
            "invalid" => fs::write(&state, b"invalid database").unwrap(),
            _ => {
                let db = rusqlite::Connection::open(&state).unwrap();
                db.execute_batch(
                    "CREATE TABLE ItemTable (key TEXT, value INTEGER); \
                     INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', 123)",
                )
                .unwrap();
            }
        }
        let agent = home.path().join(".cursor/auth.json");
        fs::create_dir_all(agent.parent().unwrap()).unwrap();
        fs::write(&agent, "{").unwrap();
        let failure = collect(&Api::new().unwrap(), "cursor", home.path(), false)
            .await
            .unwrap_err();
        assert_eq!(failure.message, expected, "{kind}");
    }
}

#[tokio::test]
async fn cursor_reports_unreadable_agent_directory() {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join(".cursor/auth.json")).unwrap();
    let failure = collect(&Api::new().unwrap(), "cursor", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.status, "unreadable");
    assert!(failure.message.contains("Cannot read"));
    assert!(failure.message.contains("auth.json"));
}

#[tokio::test]
async fn cursor_prefers_app_state_over_invalid_agent_file() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/usage-summary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"isUnlimited":true})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let state = home.path().join(".config/Cursor/User/globalStorage/state.vscdb");
    fs::create_dir_all(state.parent().unwrap()).unwrap();
    let db = rusqlite::Connection::open(&state).unwrap();
    db.execute_batch("CREATE TABLE ItemTable (key TEXT, value TEXT); INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', 'app-token')").unwrap();
    let agent = home.path().join(".cursor/auth.json");
    fs::create_dir_all(agent.parent().unwrap()).unwrap();
    fs::write(&agent, b"{").unwrap();
    let provider = collect(&fixture_api(&server), "cursor", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.source, source(&state));
}

#[tokio::test]
async fn cursor_reports_invalid_agent_file_without_app_state() {
    let home = tempfile::tempdir().unwrap();
    let agent = home.path().join(".cursor/auth.json");
    fs::create_dir_all(agent.parent().unwrap()).unwrap();
    fs::write(&agent, b"{").unwrap();
    let failure = collect(&Api::new().unwrap(), "cursor", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, format!("{} is not valid JSON", agent.display()));
}

#[tokio::test]
async fn cursor_keeps_team_and_individual_budgets_separate() {
    let provider = cursor_summary(
        json!({
            "billingCycleEnd":"2026-11-01T00:00:00Z",
            "individualUsage":{"plan":{"used":2500,"limit":10000}},
            "teamUsage":{"onDemand":{"used":500,"limit":1000}}
        }),
        json!({"accessToken":"cursor-fixture"}),
    )
    .await;
    assert_eq!(provider.windows.len(), 2);
    assert_eq!(provider.windows[0].used, 25.0);
    assert_eq!(provider.windows[1].id, "team_on_demand");
    assert!(
        provider
            .details
            .iter()
            .any(|detail| detail.value.contains("Used: $25.00"))
    );
}

#[tokio::test]
async fn cursor_matches_macos_window_order_and_components() {
    let data = serde_json::from_str(include_str!("../../../../../tests/fixtures/cursor_usage_summary.json")).unwrap();
    let provider = cursor_summary(data, json!({"accessToken":"cursor-fixture"})).await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["plan", "on_demand", "team_pool", "team_on_demand"]
    );
    assert_eq!(provider.windows[0].detail, "Auto 12% · API 48%");
    assert_eq!(provider.windows[0].duration_seconds, Some(31 * 86400));
    assert_eq!(provider.windows[1].used, 5.0);
}

#[tokio::test]
async fn cursor_summary_humanizes_the_plan_and_uses_cached_identity() {
    let data = serde_json::from_str(include_str!("../../../../../tests/fixtures/cursor_usage_summary.json")).unwrap();
    let provider = cursor_summary(
        data,
        json!({"accessToken":"cursor-fixture","email":"cached@example.com","membershipType":"pro"}),
    )
    .await;
    assert_eq!(provider.plan, "Pro Plus");
    assert_eq!(provider.identity, "cached@example.com");
}
