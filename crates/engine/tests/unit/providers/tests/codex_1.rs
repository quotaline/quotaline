//! Checks codex 1 behavior through the public API.

use super::*;
use quotaline_analytics::Endpoint;

#[tokio::test]
async fn codex_confirms_an_early_zero_on_the_second_fetch() {
    let reset = Timestamp::now().as_second() + 3_600;
    assert_eq!(
        codex_reset_readings(&[45.0, 0.0, 0.0], Some(reset), &["one", "one", "one"]).await,
        [
            (45.0, String::new()),
            (45.0, "Confirming reset".into()),
            (0.0, String::new())
        ]
    );
}

#[tokio::test]
async fn codex_false_zero_does_not_change_the_accepted_window() {
    let reset = Timestamp::now().as_second() + 3_600;
    assert_eq!(
        codex_reset_readings(&[45.0, 0.0, 45.0, 0.0], Some(reset), &["one", "one", "one", "one"]).await,
        [
            (45.0, String::new()),
            (45.0, "Confirming reset".into()),
            (45.0, String::new()),
            (45.0, "Confirming reset".into())
        ]
    );
}

#[tokio::test]
async fn codex_account_change_drops_pending_reset_state() {
    let reset = Timestamp::now().as_second() + 3_600;
    assert_eq!(
        codex_reset_readings(&[45.0, 0.0], Some(reset), &["one", "two"]).await,
        [(45.0, String::new()), (0.0, String::new())]
    );
}

#[tokio::test]
async fn codex_accepts_zero_without_a_future_reset() {
    for reset in [None, Some(Timestamp::now().as_second() - 1)] {
        assert_eq!(
            codex_reset_readings(&[45.0, 0.0], reset, &["one", "one"]).await,
            [(45.0, String::new()), (0.0, String::new())]
        );
    }
}

#[tokio::test]
async fn codex_keeps_a_confirmed_zero_without_restarting_confirmation() {
    let reset = Timestamp::now().as_second() + 3_600;
    assert_eq!(
        codex_reset_readings(&[0.0, 0.0], Some(reset), &["one", "one"]).await,
        [(0.0, String::new()), (0.0, String::new())]
    );
}

#[tokio::test]
async fn codex_file_setting_uses_auth_file_with_keyring_available() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer file-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":29,"limit_window_seconds":18000}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"file-fixture"}}),
    );
    fs::write(
        home.path().join(".codex/config.toml"),
        "cli_auth_credentials_store = 'file'\n",
    )
    .unwrap();
    let mut api = fixture_api(&server);
    api.allow_keyring = true;
    let provider = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 29.0);
    server.verify().await;
}

#[tokio::test]
async fn codex_discovers_auth_in_the_config_home() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer alternate-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":41,"limit_window_seconds":18000}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".config/codex/auth.json",
        &json!({"tokens":{"access_token":"alternate-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 41.0);
    assert_eq!(
        provider.source,
        home.path().join(".config/codex/auth.json").display().to_string()
    );
    server.verify().await;
}

#[tokio::test]
async fn codex_prefers_the_configured_home_over_an_alternate() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer configured-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":42,"limit_window_seconds":18000}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"configured-fixture"}}),
    );
    credential_file(
        home.path(),
        ".config/codex/auth.json",
        &json!({"tokens":{"access_token":"alternate-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 42.0);
    server.verify().await;
}

#[tokio::test]
async fn codex_multiple_accounts_show_the_selected_home() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rate_limit":{}})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    for root in [".codex", ".config/codex"] {
        credential_file(
            home.path(),
            &format!("{root}/auth.json"),
            &json!({"tokens":{"access_token":root}}),
        );
    }
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(
        provider.account_selection,
        Some(quotaline_quota::AccountSelection {
            count: 2,
            label: "~/.codex".into(),
        })
    );
}

#[tokio::test]
async fn codex_multiple_accounts_show_the_selected_email() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rate_limit":{}})))
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"email":"chosen@example.com"}"#);
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"first","id_token":format!("header.{claims}.signature")}}),
    );
    credential_file(
        home.path(),
        ".config/codex/auth.json",
        &json!({"tokens":{"access_token":"second"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(
        provider.account_selection,
        Some(quotaline_quota::AccountSelection {
            count: 2,
            label: "chosen@example.com".into(),
        })
    );
}

#[tokio::test]
async fn codex_keeps_the_selected_home_when_its_credential_disappears() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rate_limit":{}})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    for root in [".codex", ".config/codex"] {
        credential_file(
            home.path(),
            &format!("{root}/auth.json"),
            &json!({"tokens":{"access_token":root}}),
        );
    }
    let api = fixture_api(&server);
    collect(&api, "codex", home.path(), false).await.unwrap();
    fs::remove_file(home.path().join(".codex/auth.json")).unwrap();
    let failure = collect(&api, "codex", home.path(), false).await.unwrap_err();
    assert_eq!(failure.status, "sign-in");
    server.verify().await;
}

#[tokio::test]
async fn codex_keeps_the_selected_home_when_an_earlier_account_appears() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer alternate-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rate_limit":{}})))
        .expect(2)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".config/codex/auth.json",
        &json!({"tokens":{"access_token":"alternate-fixture"}}),
    );
    let api = fixture_api(&server);
    collect(&api, "codex", home.path(), false).await.unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"new-fixture"}}),
    );
    collect(&api, "codex", home.path(), false).await.unwrap();
    server.verify().await;
}

#[tokio::test]
async fn codex_analytics_keeps_the_selected_home() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rate_limit":{}})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    for root in [".codex", ".config/codex"] {
        credential_file(
            home.path(),
            &format!("{root}/auth.json"),
            &json!({"tokens":{"access_token":root}}),
        );
    }
    let api = fixture_api(&server);
    collect(&api, "codex", home.path(), false).await.unwrap();
    fs::remove_file(home.path().join(".codex/auth.json")).unwrap();
    assert!(codex_analytics(&api, home.path(), 7, None).await.is_none());
    server.verify().await;
}

#[tokio::test]
async fn codex_rollout_fallback_keeps_the_selected_home() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"rate_limit":{}})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    for root in [".codex", ".config/codex"] {
        credential_file(
            home.path(),
            &format!("{root}/auth.json"),
            &json!({"tokens":{"access_token":root}}),
        );
    }
    let rollout = home.path().join(".codex/sessions/2026/10/10/rollout-selected.jsonl");
    fs::create_dir_all(rollout.parent().unwrap()).unwrap();
    fs::write(
        &rollout,
        json!({"timestamp":"2026-10-10T10:00:00Z","rate_limits":{"primary":{"used_percent":61}}}).to_string(),
    )
    .unwrap();
    let api = fixture_api(&server);
    collect(&api, "codex", home.path(), false).await.unwrap();
    fs::remove_file(home.path().join(".codex/auth.json")).unwrap();
    let provider = collect(&api, "codex", home.path(), false).await.unwrap();
    assert_eq!(provider.windows[0].used, 61.0);
    assert_eq!(provider.source, rollout.display().to_string());
    server.verify().await;
}

#[tokio::test]
async fn codex_uses_an_alternate_when_configured_auth_is_unusable() {
    for document in ["{}", "{"] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .and(matchers::header("authorization", "Bearer alternate-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "rate_limit":{"primary_window":{"used_percent":43,"limit_window_seconds":18000}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        let configured = home.path().join(".codex/auth.json");
        fs::create_dir_all(configured.parent().unwrap()).unwrap();
        fs::write(configured, document).unwrap();
        credential_file(
            home.path(),
            ".config/codex/auth.json",
            &json!({"tokens":{"access_token":"alternate-fixture"}}),
        );
        let provider = collect(&fixture_api(&server), "codex", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.windows[0].used, 43.0, "{document}");
        server.verify().await;
    }
}

#[tokio::test]
async fn codex_reads_rollouts_from_the_selected_alternate_home() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("authorization", "Bearer alternate-fixture"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(
        json!({
            "https://api.openai.com/auth":{"chatgpt_subscription_active_until":"2027-01-02T03:04:05Z"}
        })
        .to_string(),
    );
    credential_file(
        home.path(),
        ".config/codex/auth.json",
        &json!({"tokens":{"access_token":"alternate-fixture","id_token":format!("header.{claims}.signature")}}),
    );
    let path = home
        .path()
        .join(".config/codex/sessions/2026/10/10/rollout-alternate.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        json!({"timestamp":"2026-10-10T10:00:00Z","rate_limits":{"primary":{"used_percent":63}}}).to_string(),
    )
    .unwrap();
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.status, "stale");
    assert_eq!(provider.windows[0].used, 63.0);
    assert_eq!(provider.source, path.display().to_string());
    assert_eq!(
        provider.subscription_active_until.as_deref(),
        Some("2027-01-02T03:04:05+00:00")
    );
    server.verify().await;
}

#[tokio::test]
async fn codex_analytics_uses_the_selected_alternate_home() {
    let server = MockServer::start().await;
    for endpoint in Endpoint::ALL {
        Mock::given(matchers::method("GET"))
            .and(matchers::path(format!("/backend-api/{}", endpoint.path())))
            .and(matchers::header("authorization", "Bearer alternate-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":[]})))
            .expect(1)
            .mount(&server)
            .await;
    }
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".config/codex/auth.json",
        &json!({"tokens":{"access_token":"alternate-fixture"}}),
    );
    assert_eq!(
        covered_endpoints(codex_analytics(&fixture_api(&server), home.path(), 7, None).await),
        Endpoint::ALL.len()
    );
    server.verify().await;
}

#[tokio::test]
async fn codex_http_uses_account_and_reset_credit_endpoints() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .and(matchers::header("chatgpt-account-id", "account-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/rate-limit-reset-credits"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"available_count":2})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"codex-fixture","account_id":"account-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 25.0);
    assert_eq!(provider.reset_credits.unwrap().available, 2);
}

#[tokio::test]
async fn codex_reports_a_failed_usage_request_instead_of_local_readings() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(503))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"fixture"}}),
    );
    let path = home.path().join(".codex/sessions/2026/10/10/rollout-current.jsonl");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        json!({"timestamp":"2026-10-10T10:00:00Z","rate_limits":{"primary":{"used_percent":63}}}).to_string(),
    )
    .unwrap();
    let error = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!((error.status, error.message.as_str()), ("error", "HTTP 503"));
}

#[tokio::test]
async fn codex_api_key_sign_in_skips_the_usage_endpoint() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"OPENAI_API_KEY":"api-key-fixture"}),
    );
    let error = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(
        (error.status, error.message.as_str()),
        ("sign-in", quotaline_credentials::CODEX_API_KEY_USAGE_UNAVAILABLE)
    );
    server.verify().await;
}
