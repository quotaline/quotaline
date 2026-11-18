//! Checks antigravity 1 behavior through the public API.

use super::*;

#[tokio::test(start_paused = true)]
async fn antigravity_reloads_plan_after_tier_cache_expires() {
    let server = MockServer::start().await;
    let (release, parked) = std::sync::mpsc::channel();
    let clock_guard = tokio::task::spawn_blocking(move || parked.recv());
    tokio::task::yield_now().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
        })))
        .mount(&server)
        .await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tier_attempts = attempts.clone();
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(move |_: &wiremock::Request| {
            let name = if tier_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                "Google AI Pro"
            } else {
                "Google AI Ultra"
            };
            ResponseTemplate::new(200).set_body_json(json!({"currentTier":{"name":name}}))
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    assert_eq!(
        collect(&api, "antigravity", home.path(), false).await.unwrap().plan,
        "Google AI Pro"
    );
    tokio::time::advance(Duration::from_secs(3601)).await;
    assert_eq!(
        collect(&api, "antigravity", home.path(), false).await.unwrap().plan,
        "Google AI Ultra"
    );
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    release.send(()).unwrap();
    clock_guard.await.unwrap().unwrap();
}

#[tokio::test]
async fn antigravity_reloads_plan_when_the_account_token_changes() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let tier_attempts = attempts.clone();
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(move |_: &wiremock::Request| {
            let name = if tier_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                "Google AI Pro"
            } else {
                "Google AI Ultra"
            };
            ResponseTemplate::new(200).set_body_json(json!({"currentTier":{"name":name}}))
        })
        .expect(2)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"first-token"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path.clone());
    let first = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(first.plan, "Google AI Pro");
    fs::write(path, json!({"access_token":"second-token"}).to_string()).unwrap();
    let second = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(second.plan, "Google AI Ultra");
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
    server.verify().await;
}

#[tokio::test]
async fn antigravity_tier_without_a_name_preserves_the_local_plan() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"currentTier":{"id":"pro"}})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"access_token":"antigravity-fixture","planName":"Local plan"}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(provider.plan, "Local plan");
    server.verify().await;
}

#[tokio::test]
async fn antigravity_http_tries_secondary_quota_service() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/first/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(500))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/second/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer antigravity-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
        })))
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/second/v1internal:loadCodeAssist"))
        .and(matchers::header("authorization", "Bearer antigravity-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "currentTier":{"id":"ultra-tier","name":"Google AI Ultra"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_bases = [format!("{}/first", server.uri()), format!("{}/second", server.uri())];
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 75.0);
    assert_eq!(provider.windows[0].group, "session");
    assert_eq!(provider.plan, "Google AI Ultra");
    let cached = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(cached.plan, "Google AI Ultra");
}

#[tokio::test]
async fn antigravity_rpc_failure_tries_secondary_quota_service() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/first/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"code":13})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/second/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.4}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_bases = [format!("{}/first", server.uri()), format!("{}/second", server.uri())];
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 60.0);
}

#[tokio::test]
async fn antigravity_tier_failure_keeps_quota_with_a_warning() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path(
            "/antigravity-primary/v1internal:retrieveUserQuotaSummary",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.4}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/antigravity-primary/v1internal:loadCodeAssist"))
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_bases = [
        format!("{}/antigravity-primary", server.uri()),
        format!("{}/antigravity-secondary", server.uri()),
    ];
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 60.0);
    assert_eq!(provider.warnings[0], "Plan details unavailable: HTTP 500");
}

#[tokio::test]
async fn antigravity_refreshes_from_installed_cli_and_keeps_file_unchanged() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains(
            "client_id=12345-fixture.apps.googleusercontent.com",
        ))
        .and(matchers::body_string_contains("refresh_token=shared"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"new-access","expires_in":3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer new-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
        })))
        .expect(2)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    antigravity_oauth_fixture(home.path());
    let path = home.path().join("antigravity.json");
    let document = json!({
        "auth_method":"consumer",
        "token":{"access_token":"expired","refresh_token":"shared","expiry":"2020-01-01T00:00:00Z"}
    });
    fs::write(&path, document.to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path.clone());
    let first = collect(&api, "antigravity", home.path(), true).await.unwrap();
    let second = collect(&api, "antigravity", home.path(), true).await.unwrap();
    assert_eq!(first.windows[0].used, 75.0);
    assert_eq!(second.windows[0].used, 75.0);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
        document
    );
}

#[tokio::test]
async fn antigravity_refreshes_after_a_live_session_is_rejected() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer old-access"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains("refresh_token=shared"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"fresh-access","expires_in":3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer fresh-access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    antigravity_oauth_fixture(home.path());
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"old-access","refresh_token":"shared","expiry":4102444800_i64}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 75.0);
    server.verify().await;
}

#[tokio::test]
async fn antigravity_does_not_refresh_twice_after_a_rejected_new_token() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token":"fresh-access","expires_in":3600
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer fresh-access"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    antigravity_oauth_fixture(home.path());
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"expired","refresh_token":"shared","expiry":1}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let error = collect(&api, "antigravity", home.path(), true).await.unwrap_err();
    assert_eq!(error.status, "sign-in");
    server.verify().await;
}

#[tokio::test]
async fn antigravity_explicit_credential_file_takes_precedence_over_local_discovery() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer file-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"file-token"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_local_enabled = true;
    api.antigravity_auth_file = Some(path.clone());
    let provider = collect(&api, "antigravity", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 75.0);
    assert_eq!(provider.source, path.to_string_lossy());
    server.verify().await;
}

#[tokio::test]
async fn antigravity_refresh_accepts_flat_and_camel_case_tokens() {
    for (case, document) in [
        (
            "flat token",
            json!({"access_token":"expired","refresh_token":"shared","expiry":1}),
        ),
        (
            "camel token",
            json!({"token":{"accessToken":"expired","refreshToken":"shared","expiry":1}}),
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .and(matchers::body_string_contains("refresh_token=shared"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token":"refreshed","expires_in":3600
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
            .and(matchers::header("authorization", "Bearer refreshed"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        antigravity_oauth_fixture(home.path());
        let path = home.path().join("antigravity.json");
        fs::write(&path, document.to_string()).unwrap();
        let mut api = fixture_api(&server);
        api.antigravity_auth_file = Some(path.clone());
        let provider = collect(&api, "antigravity", home.path(), true).await.unwrap();
        assert_eq!(provider.windows[0].used, 75.0, "{case}");
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap(),
            document
        );
        server.verify().await;
    }
}

#[tokio::test]
async fn antigravity_expired_token_does_not_refresh_without_opt_in() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"token":{"access_token":"old","expiry":1}}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let error = collect(&api, "antigravity", home.path(), false).await.unwrap_err();
    assert_eq!(error.status, "sign-in");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn antigravity_refresh_requires_a_refresh_token() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"token":{"access_token":"old","expiry":1}}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let failure = collect(&api, "antigravity", home.path(), true).await.unwrap_err();
    assert_eq!(failure.message, "Antigravity refresh token is missing; sign in again.");
    assert!(server.received_requests().await.unwrap().is_empty());
}
