//! Checks claude 1 behavior through the public API.

use super::*;

#[tokio::test]
async fn claude_cloud_session_credit_is_separate_from_quota() {
    let provider = claude_summary(json!({
        "seven_day":{"utilization":91},
        "iguana_necktie":{"utilization":66.8,"limit_dollars":250,"used_dollars":167,
            "remaining_dollars":83,"resets_at":"2026-11-05T07:59:00Z"}
    }))
    .await;
    assert_eq!(
        provider.windows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["weekly"]
    );
    let credits = provider.cloud_credits.unwrap();
    assert_eq!((credits.limit, credits.remaining), (250.0, 83.0));
    assert_eq!(credits.expires_at.as_deref(), Some("2026-11-05T07:59:00Z"));
}

#[tokio::test]
async fn claude_cloud_session_credit_uses_reported_remaining() {
    let provider = claude_summary(json!({
        "iguana_necktie":{"limit_dollars":250,"used_dollars":167,"remaining_dollars":70}
    }))
    .await;
    assert_eq!(provider.cloud_credits.unwrap().remaining, 70.0);
}

#[tokio::test]
async fn claude_cloud_session_credit_derives_remaining() {
    for (value, expected) in [
        (json!({"limit_dollars":250,"used_dollars":167}), 83.0),
        (json!({"limit_dollars":250,"utilization":66.8}), 83.0),
        (json!({"limit_dollars":250}), 250.0),
        (json!({"limit_dollars":250,"remaining_dollars":-1}), 0.0),
        (json!({"limit_dollars":250,"remaining_dollars":300}), 250.0),
    ] {
        let provider = claude_summary(json!({"iguana_necktie":value})).await;
        assert!((provider.cloud_credits.unwrap().remaining - expected).abs() < 0.000_001);
    }
}

#[tokio::test]
async fn claude_cloud_session_credit_keeps_unavailability_reason() {
    let provider = claude_summary(json!({
        "iguana_necktie":{"limit_dollars":250,"remaining_dollars":83,"locked_reason":"provider_locked"}
    }))
    .await;
    assert_eq!(
        provider.cloud_credits.unwrap().locked_reason.as_deref(),
        Some("provider_locked")
    );
}

#[tokio::test]
async fn claude_missing_cloud_credit_does_not_become_a_quota() {
    let provider = claude_summary(json!({
        "five_hour":{"utilization":50},
        "iguana_necktie":{"utilization":66.8,"limit_dollars":null}
    }))
    .await;
    assert_eq!(
        provider.windows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["session"]
    );
    assert!(provider.cloud_credits.is_none());
}

#[tokio::test]
async fn claude_http_uses_local_oauth_credential() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .and(matchers::header("authorization", "Bearer claude-fixture"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":{"utilization":42,"resets_at":"2026-10-10T12:00:00Z"},
            "spend":{"balance":{"amount_minor":250}}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture","subscriptionType":"Pro"}}),
    );
    let provider = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].used, 42.0);
    assert_eq!(provider.credit_balance, Some(2.5));
}

#[tokio::test]
async fn claude_scoped_limit_keeps_its_model_name_for_panel_templates() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "limits":[{"percent":42,"kind":"weekly","scope":{"model":{"display_name":"Model Alpha"}}}]
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
    assert_eq!(provider.windows[0].scope.as_deref(), Some("Model Alpha"));
}

#[tokio::test]
async fn claude_structured_limits_use_stable_ids() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "limits":[
                {"percent":20,"kind":"session"},
                {"percent":30,"kind":"weekly_all"},
                {"percent":40,"kind":"weekly_scoped","scope":{"model":{"display_name":"Opus 4"}}},
                {"percent":50,"kind":"surface_limit","scope":{"surface":"Code Review"},"group":"monthly"}
            ]
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
    assert_eq!(
        provider.windows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>(),
        ["session", "weekly", "weekly:opus-4", "surface_limit:code-review"]
    );
}

#[tokio::test]
async fn claude_unknown_limits_keep_their_groups() {
    let provider = claude_summary(json!({
        "limits":[
            {"kind":"broken","percent":"unknown"},
            {"kind":"daily","percent":10,"group":"session"},
            {"kind":"quota","percent":20,"group":"weekly"}
        ],
        "seven_day_custom":{"utilization":30},
        "unknown_bucket":{"utilization":40}
    }))
    .await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|row| (row.id.as_str(), row.group.as_str(), row.duration_seconds))
            .collect::<Vec<_>>(),
        [
            ("daily", "session", Some(18_000)),
            ("quota", "weekly", Some(604_800)),
            ("seven_day_custom", "weekly", Some(604_800)),
            ("unknown_bucket", "other", None),
        ]
    );
}

#[tokio::test]
async fn claude_invalid_credit_exponent_uses_cents() {
    let provider = claude_summary(json!({
        "five_hour":{"utilization":10},
        "extra_usage":{"used_credits":"50","decimal_places":3_000_000_000_i64}
    }))
    .await;
    assert_eq!(provider.spend.unwrap().used, Some(0.5));
}

#[tokio::test]
async fn claude_out_of_range_money_exponent_uses_cents() {
    let provider = claude_summary(json!({
        "spend":{"used":{"amount_minor":50,"exponent":3_000_000_000_i64}}
    }))
    .await;
    assert_eq!(provider.spend.unwrap().used, Some(0.5));
}

#[tokio::test]
async fn claude_zero_spend_limit_has_zero_percent() {
    let provider = claude_summary(json!({
        "five_hour":{"utilization":10},
        "extra_usage":{"used_credits":"50","monthly_limit":"0"}
    }))
    .await;
    assert_eq!(provider.spend.unwrap().percent, Some(0.0));
}

#[tokio::test]
async fn claude_structured_limit_replaces_matching_legacy_window() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "limits":[{"percent":42,"kind":"weekly_all"}],
            "seven_day":{"utilization":90}
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
    assert_eq!(provider.windows.len(), 1);
    assert_eq!(provider.windows[0].used, 42.0);
}

#[tokio::test]
async fn claude_keeps_new_legacy_weekly_windows() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "seven_day_new_model":{"utilization":67,"resets_at":"2026-10-12T12:00:00Z"}
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
    assert_eq!(
        (
            provider.windows[0].id.as_str(),
            provider.windows[0].label.as_str(),
            provider.windows[0].group.as_str(),
            provider.windows[0].duration_seconds
        ),
        ("seven_day_new_model", "New Model", "weekly", Some(604_800))
    );
}

#[tokio::test]
async fn claude_structured_limit_replaces_new_legacy_window() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "limits":[{"percent":42,"kind":"weekly_scoped","scope":{"model":{"display_name":"New Model"}}}],
            "seven_day_new_model":{"utilization":90}
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
    assert_eq!(provider.windows.iter().map(|row| row.used).collect::<Vec<_>>(), [42.0]);
}

#[tokio::test]
async fn claude_spend_only_response_keeps_account_connected() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "spend":{"balance":{"amount_minor":250}}
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
    assert_eq!(
        (provider.status.as_str(), provider.credit_balance),
        ("connected", Some(2.5))
    );
}

#[tokio::test]
async fn claude_empty_quota_schema_keeps_account_connected() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"five_hour":null,"limits":[]})))
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
    assert_eq!((provider.status.as_str(), provider.windows.len()), ("connected", 0));
}

#[tokio::test]
async fn claude_known_empty_sections_keep_the_account_connected() {
    for usage in [
        json!({"limits":null}),
        json!({"spend":null}),
        json!({"extra_usage":null}),
        json!({"five_hour":null}),
        json!({"limits":[]}),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(usage))
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
        assert_eq!((provider.status.as_str(), provider.windows.len()), ("connected", 0));
        server.verify().await;
    }
}

#[tokio::test]
async fn claude_empty_schema_does_not_hide_malformed_window() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "five_hour":null,"seven_day":{"utilization":"invalid"}
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
    let error = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .err()
        .unwrap();
    assert_eq!(error.message, "Unexpected response: claude.usage: invalid payload");
}

#[tokio::test]
async fn claude_empty_schema_does_not_hide_malformed_unknown_window() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "limits":null,"custom_window":{"utilization":"invalid"}
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
    let failure = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .unwrap_err();
    assert_eq!(failure.message, "Unexpected response: claude.usage: invalid payload");
    server.verify().await;
}

#[tokio::test]
async fn claude_malformed_spend_is_not_treated_as_credit_data() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/api/oauth/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"spend":false})))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".claude/.credentials.json",
        &json!({"claudeAiOauth":{"accessToken":"claude-fixture"}}),
    );
    let error = collect(&fixture_api(&server), "claude", home.path(), false)
        .await
        .err()
        .unwrap();
    assert_eq!(error.message, "Unexpected response: claude.usage: invalid payload");
}
