//! Checks antigravity 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn antigravity_refresh_rejects_incomplete_installed_oauth_client() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let binary = home.path().join(".local/bin/agy");
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::write(
        &binary,
        b"\x0012345-fixture.apps.googleusercontent.com\0GOCSPX-incomplete\0",
    )
    .unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"old","refresh_token":"shared","expiry":1}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let failure = collect(&api, "antigravity", home.path(), true).await.unwrap_err();
    assert_eq!(failure.message, "Cannot find the installed Antigravity OAuth client");
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn antigravity_refresh_rejects_invalid_token_responses() {
    for (response, message) in [
        (
            json!({"expires_in":3600}),
            "Antigravity refresh returned no access token",
        ),
        (
            json!({"access_token":"fresh","expires_in":0}),
            "Antigravity refresh returned an invalid expiry",
        ),
        (
            json!({"access_token":"fresh","expires_in":"NaN"}),
            "Antigravity refresh returned an invalid expiry",
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
        antigravity_oauth_fixture(home.path());
        let path = home.path().join("antigravity.json");
        let original = json!({"token":{"access_token":"old","refresh_token":"shared","expiry":1}});
        fs::write(&path, original.to_string()).unwrap();
        let mut api = fixture_api(&server);
        api.antigravity_auth_file = Some(path.clone());
        let failure = collect(&api, "antigravity", home.path(), true).await.unwrap_err();
        assert_eq!(failure.message, message);
        let saved: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert_eq!(saved, original);
    }
}

#[tokio::test]
async fn antigravity_retries_after_quota_rejects_a_token() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer old"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"code":"UNAUTHENTICATED"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"new"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.5}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    antigravity_oauth_fixture(home.path());
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"old","refresh_token":"shared"}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let provider = collect(&api, "antigravity", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 50.0);
}

#[tokio::test]
async fn antigravity_drops_the_cached_token_when_the_cli_credential_changes() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"access_token":"refreshed"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer refreshed"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer cli-new"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.75}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    antigravity_oauth_fixture(home.path());
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"old","refresh_token":"shared","expiry":1}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path.clone());
    let first = collect(&api, "antigravity", home.path(), true).await.unwrap();
    fs::write(&path, json!({"token":{"access_token":"cli-new"}}).to_string()).unwrap();
    let second = collect(&api, "antigravity", home.path(), true).await.unwrap();
    assert_eq!(first.windows[0].used, 75.0);
    assert_eq!(second.windows[0].used, 25.0);
}

#[tokio::test]
async fn rejected_antigravity_refresh_waits_for_a_new_cli_credential() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/token"))
        .and(matchers::body_string_contains("refresh_token=old-refresh"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error":"invalid_grant"})))
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
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer fresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.5}]}]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    antigravity_oauth_fixture(home.path());
    let path = home.path().join("antigravity.json");
    fs::write(
        &path,
        json!({"token":{"access_token":"old","refresh_token":"old-refresh","expiry":1}}).to_string(),
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path.clone());
    for _ in 0..2 {
        let error = collect(&api, "antigravity", home.path(), true).await.unwrap_err();
        assert_eq!(error.status, "sign-in");
        assert!(error.message.contains("invalid_grant"));
    }
    fs::write(
        path,
        json!({"token":{"access_token":"old","refresh_token":"cli-refresh","expiry":1}}).to_string(),
    )
    .unwrap();
    let provider = collect(&api, "antigravity", home.path(), true).await.unwrap();
    assert_eq!(provider.windows[0].used, 50.0);
}

#[test]
fn antigravity_oneof_fraction_and_local_identity() {
    let data = json!({"response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remaining":{"case":"remainingFraction","value":0.25},"resetTime":"2026-10-10T00:00:00Z"}]}]}});
    let status = json!({"userStatus":{"userTier":{"name":"Ultra"},"email":"example@test.invalid"}});
    let provider = antigravity_provider(&data, &status, "local".into()).unwrap();
    assert_eq!(provider.plan, "Ultra");
    assert_eq!(provider.windows[0].used, 75.0);
    assert_eq!(provider.windows[0].group, "session");
}

#[test]
fn antigravity_reports_disabled_pool_as_detail() {
    let data = json!({"groups":[{"buckets":[{"bucketId":"disabled","disabled":true}]}]});
    let provider = antigravity_provider(&data, &Value::Null, "fixture".into()).unwrap();
    assert!(provider.windows.is_empty());
    assert_eq!(provider.details[0].value, "Disabled");
    assert_eq!(
        provider.details[0].explanation,
        "The local source did not report a usable percentage for this quota."
    );
}

#[test]
fn antigravity_rejects_rpc_errors_before_mapping_windows() {
    for (data, status) in [
        (json!({"code":16}), "sign-in"),
        (json!({"code":7}), "error"),
        (json!({}), "error"),
    ] {
        assert_eq!(
            antigravity_provider(&data, &Value::Null, "fixture".into())
                .unwrap_err()
                .status,
            status
        );
    }
}

#[tokio::test]
async fn antigravity_http_maps_weekly_and_unclassified_pools() {
    let provider = antigravity_summary(json!({"response":{"groups":[{"buckets":[
        {"bucketId":"gemini-weekly","remainingFraction":0.25},
        {"bucketId":"3p-5h","remainingFraction":0.5},
        {"bucketId":"3p-weekly","remainingFraction":0.75},
        {"bucketId":"custom","displayName":"Custom pool","remainingFraction":0.1},
        {"bucketId":"unreported","remaining":{"remainingAmount":"42"},
            "description":"Limited pool.","resetTime":"2026-10-10T00:00:00Z"}
    ]}]}}))
    .await
    .unwrap();
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|row| (row.id.as_str(), row.group.as_str(), row.duration_seconds))
            .collect::<Vec<_>>(),
        [
            ("gemini:weekly", "weekly", Some(604_800)),
            ("3p:session", "session", Some(18_000)),
            ("3p:weekly", "weekly", Some(604_800)),
            ("custom", "other", None),
        ]
    );
    assert_eq!(provider.details[0].value, "42 remaining");
    assert_eq!(
        provider.details[0].explanation,
        "Limited pool. Resets 2026-10-10T00:00:00Z. The local source did not report a usable percentage for this quota."
    );
}

#[tokio::test]
async fn antigravity_formats_numeric_amounts_and_disabled_pools() {
    let provider = antigravity_summary(json!({"groups":[{"buckets":[
        {"bucketId":"small","remaining":{"case":"remainingAmount","value":250}},
        {"bucketId":"thousands","remaining":{"remainingAmount":1200}},
        {"bucketId":"large-thousands","remaining":{"remainingAmount":12000}},
        {"bucketId":"millions","remaining":{"remainingAmount":1500000}},
        {"bucketId":"large-millions","remaining":{"remainingAmount":12000000}},
        {"bucketId":"billions","remaining":{"remainingAmount":1500000000}},
        {"bucketId":"large-billions","remaining":{"remainingAmount":12000000000_i64}},
        {"bucketId":"disabled","disabled":true,"remaining":{"remainingAmount":1500}}
    ]}]}))
    .await
    .unwrap();
    assert_eq!(
        provider
            .details
            .iter()
            .map(|detail| detail.value.as_str())
            .collect::<Vec<_>>(),
        [
            "250 remaining",
            "1.2K remaining",
            "12K remaining",
            "1.5M remaining",
            "12M remaining",
            "1.5B remaining",
            "12B remaining",
            "Disabled · 1.5K remaining"
        ]
    );
}

#[tokio::test]
async fn antigravity_uses_display_name_when_a_bucket_has_no_id() {
    let provider = antigravity_summary(json!({"summary":{"groups":[{"buckets":[
        {"displayName":"Shared pool","remainingFraction":0.25},
        {"remainingFraction":0.5},
        {"displayName":"Unreported"},
        {"remaining":{"remainingAmount":"9"}}
    ]}]}}))
    .await
    .unwrap();
    assert_eq!(
        provider.windows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["Shared pool", "quota"]
    );
    assert_eq!(provider.details[0].value, "Usage percentage unavailable");
}

#[tokio::test]
async fn antigravity_uses_top_level_groups_when_other_wrappers_are_absent() {
    let provider = antigravity_summary(json!({"groups":[{"buckets":[
        {"bucketId":"gemini-5h","remainingFraction":0.8}
    ]}]}))
    .await
    .unwrap();
    assert!((provider.windows[0].used - 20.0).abs() < 0.000_001);
}

#[tokio::test]
async fn antigravity_http_shows_an_empty_quota_as_connected() {
    let provider = antigravity_summary(json!({"response":{"groups":[]}})).await.unwrap();
    assert_eq!((provider.status.as_str(), provider.windows.len()), ("connected", 0));
}

#[tokio::test]
async fn antigravity_http_rejects_expired_session_code() {
    let error = antigravity_summary(json!({"code":"unauthenticated"}))
        .await
        .unwrap_err();
    assert_eq!(error.status, "sign-in");
}

#[tokio::test]
async fn antigravity_unauthorized_response_stops_before_the_secondary_service() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/first/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/second/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_bases = [format!("{}/first", server.uri()), format!("{}/second", server.uri())];
    api.antigravity_auth_file = Some(path);
    let error = collect(&api, "antigravity", home.path(), false).await.unwrap_err();
    assert_eq!(error.status, "sign-in");
    server.verify().await;
}

#[tokio::test]
async fn antigravity_missing_local_sign_in_reports_its_source() {
    let server = MockServer::start().await;
    let home = tempfile::tempdir().unwrap();
    let failure = collect(&fixture_api(&server), "antigravity", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "No Antigravity sign-in found");
    assert_eq!(failure.credential_health, CredentialHealth::Missing);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn antigravity_uses_second_local_credential_after_invalid_primary() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .and(matchers::header("authorization", "Bearer alternate-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.2}]}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let primary = home.path().join(".gemini/antigravity-cli/antigravity-oauth-token");
    fs::create_dir_all(primary.parent().unwrap()).unwrap();
    fs::write(primary, b"invalid JSON").unwrap();
    credential_file(
        home.path(),
        ".config/Antigravity/auth.json",
        &json!({"access_token":"alternate-token","planName":"Ultra"}),
    );
    let provider = collect(&fixture_api(&server), "antigravity", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 80.0);
    assert_eq!(provider.plan, "Ultra");
    assert!(provider.source.ends_with(".config/Antigravity/auth.json"));
}

#[tokio::test]
async fn antigravity_numeric_authentication_code_requires_sign_in() {
    let failure = antigravity_summary(json!({"code":16})).await.unwrap_err();
    assert_eq!(failure.status, "sign-in");
}

#[tokio::test]
async fn antigravity_unrecognized_rpc_code_reports_a_quota_error() {
    let failure = antigravity_summary(json!({"code":"UNAVAILABLE"})).await.unwrap_err();
    assert_eq!(
        failure.message,
        "Unexpected response: Antigravity quota request failed at the RPC layer"
    );
}

#[tokio::test]
async fn antigravity_unavailable_endpoints_preserve_http_failure() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("POST"))
        .and(matchers::path("/v1internal:retrieveUserQuotaSummary"))
        .respond_with(ResponseTemplate::new(503))
        .expect(2)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("antigravity.json");
    fs::write(&path, json!({"access_token":"antigravity-fixture"}).to_string()).unwrap();
    let mut api = fixture_api(&server);
    api.antigravity_auth_file = Some(path);
    let failure = collect(&api, "antigravity", home.path(), false).await.unwrap_err();
    assert_eq!(failure.message, "HTTP 503");
}
