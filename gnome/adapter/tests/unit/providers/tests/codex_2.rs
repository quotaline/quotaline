//! Checks codex 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn codex_subscription_renewal_comes_from_id_token() {
    for (claims, expected) in [
        (
            Some(json!({"https://api.openai.com/auth":{
                "chatgpt_subscription_active_until":"2027-01-02T03:04:05Z"
            }})),
            Some("2027-01-02T03:04:05+00:00"),
        ),
        (
            Some(json!({"https://api.openai.com/auth":{
                "chatgpt_subscription_active_until":"invalid"
            }})),
            None,
        ),
        (Some(json!({"email":"account@example.com"})), None),
        (None, None),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "rate_limit_reset_credits":{
                    "total_earned_count":0,"immediate_reset_purchase_eligible":false
                }
            })))
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        let token = claims.map_or_else(
            || "opaque".to_string(),
            |value| {
                format!(
                    "header.{}.signature",
                    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string())
                )
            },
        );
        credential_file(
            home.path(),
            ".codex/auth.json",
            &json!({"tokens":{"access_token":"codex-fixture","id_token":token}}),
        );
        let provider = collect(&fixture_api(&server), "codex", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.subscription_active_until.as_deref(), expected);
    }
}

#[tokio::test]
async fn codex_http_two_week_window_keeps_its_duration() {
    let provider = codex_summary(json!({
        "rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":1_209_600}}
    }))
    .await;
    assert_eq!(provider.windows[0].id, "window-1209600");
    assert_eq!(provider.windows[0].label, "14d");
    assert_eq!(provider.windows[0].group, "other");
    assert_eq!(provider.windows[0].duration_seconds, Some(1_209_600));
}

#[tokio::test]
async fn codex_reference_window_ids_and_labels() {
    let provider = codex_summary(json!({
        "rate_limit": {"primary_window":{"used_percent":25,"limit_window_seconds":18_000},
            "secondary_window":{"used_percent":70,"limit_window_seconds":604_800}},
        "code_review_rate_limit":{"primary_window":{"used_percent":5,"limit_window_seconds":86_400}},
        "additional_rate_limits":[{"limit_name":"Spark Beta","rate_limit":{
            "primary_window":{"used_percent":10,"limit_window_seconds":2_592_000},
            "secondary_window":{"used_percent":20,"limit_window_seconds":2_678_400}}}]
    }))
    .await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|row| (row.id.as_str(), row.label.as_str(), row.group.as_str()))
            .collect::<Vec<_>>(),
        [
            ("session", "5-hour", "session"),
            ("weekly", "Weekly", "weekly"),
            ("code_review:window-86400", "Code review Daily", "other"),
            ("additional:spark-beta:monthly", "Spark Beta Monthly", "monthly"),
            ("additional:spark-beta:monthly", "Spark Beta Monthly", "monthly"),
        ]
    );
}

#[tokio::test]
async fn codex_missing_duration_defaults_to_five_hours() {
    let provider = codex_summary(json!({"rate_limit":{"secondary_window":{"used_percent":15}}})).await;
    assert_eq!(provider.windows[0].id, "session");
    assert_eq!(provider.windows[0].label, "5-hour");
}

#[tokio::test]
async fn codex_other_durations_keep_distinct_ids() {
    let provider = codex_summary(json!({
        "rate_limit":{"primary_window":{"used_percent":10,"limit_window_seconds":1_800},
            "secondary_window":{"used_percent":20,"limit_window_seconds":5_400}},
        "code_review_rate_limit":{"primary_window":{"used_percent":30,"limit_window_seconds":90_000}}
    }))
    .await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|row| (row.id.as_str(), row.label.as_str()))
            .collect::<Vec<_>>(),
        [
            ("window-1800", "30m"),
            ("window-5400", "1h"),
            ("code_review:window-90000", "Code review 25h"),
        ]
    );
}

#[tokio::test]
async fn codex_http_reports_unlimited_usage_credits() {
    let provider = codex_summary(json!({"credits":{"unlimited":true}})).await;
    assert!(provider.usage_credits.unwrap().unlimited);
}

#[tokio::test]
async fn codex_http_preserves_credit_estimates_and_reset_metadata() {
    let provider = codex_summary(json!({
        "credits":{
            "balance":"42.5","has_credits":true,"overage_limit_reached":true,
            "approx_local_messages":[8,12],"approx_cloud_messages":[3,5]
        },
        "rate_limit_reset_credits":{
            "available_count":2,"applicable_available_count":1,"total_earned_count":4,
            "immediate_reset_purchase_eligible":true,
            "credits":[
                {"id":"bonus","title":"Bonus reset","status":"available","reset_type":"session_reset",
                 "description":"One extra session reset","granted_at":"2026-10-01T00:00:00Z",
                 "expires_at":"2026-11-01T00:00:00Z"},
                {"title":"Used reset","status":"used","expires_at":"2026-11-02T00:00:00Z"}
            ]
        }
    }))
    .await;
    let credits = provider.usage_credits.unwrap();
    assert_eq!(provider.credit_balance, Some(42.5));
    assert_eq!(credits.has_credits, Some(true));
    assert!(credits.overage_limit_reached);
    assert_eq!(credits.local_messages, Some((8, 12)));
    assert_eq!(credits.cloud_messages, Some((3, 5)));
    let resets = provider.reset_credits.unwrap();
    assert_eq!(
        (resets.available, resets.applicable, resets.total_earned),
        (2, 1, Some(4))
    );
    assert!(resets.purchase_eligible);
    assert_eq!(resets.details[0].credit_type.as_deref(), Some("Session Reset"));
    assert!(resets.details[0].explanation.contains("never reserves or consumes"));
    assert_eq!(
        resets.details[0].description.as_deref(),
        Some("One extra session reset")
    );
    assert_eq!(resets.details[0].granted_at.as_deref(), Some("2026-10-01T00:00:00Z"));
    assert_eq!(resets.details[1].status.as_deref(), Some("Used"));
    assert_eq!(provider.reset_credit_expiries.len(), 1);
    assert_eq!(provider.reset_credit_expiries[0].id, "bonus");
}

#[tokio::test]
async fn codex_http_ignores_unordered_estimates_and_nonfinite_balance() {
    let provider = codex_summary(json!({
        "credits":{"balance":"NaN","approx_local_messages":[12,8],"approx_cloud_messages":[3]},
        "rate_limit_reset_credits":{"available_count":0,"credits":[{"status":"available","expires_at":"bad"}]}
    }))
    .await;
    assert_eq!(provider.credit_balance, None);
    assert_eq!(provider.usage_credits.unwrap().local_messages, None);
    assert_eq!(provider.reset_credits.unwrap().available, 0);
    assert!(provider.reset_credit_expiries.is_empty());
}

#[tokio::test]
async fn codex_spend_uses_remaining_percent_when_used_is_absent() {
    let provider = codex_summary(json!({
        "spend_control":{"individual_limit":{"remaining_percent":40}}
    }))
    .await;
    assert_eq!(provider.spend.unwrap().percent, Some(60.0));
}

#[tokio::test]
async fn codex_spend_reset_uses_relative_seconds() {
    let before = Timestamp::now().as_second();
    let provider = codex_summary(json!({
        "spend_control":{"individual_limit":{"reset_after_seconds":3600}}
    }))
    .await;
    let reset = provider.spend.unwrap().reset.unwrap();
    let at = reset.parse::<Timestamp>().unwrap().as_second();
    assert!((before + 3600..=Timestamp::now().as_second() + 3600).contains(&at));
}

#[tokio::test]
async fn codex_unknown_limit_reason_is_shown() {
    let provider = codex_summary(json!({
        "rate_limit_reached_type":{"unexpected":"quota"}
    }))
    .await;
    assert!(
        provider
            .notices
            .iter()
            .any(|notice| notice.text == "Limit reached: Unexpected: quota.")
    );
}

#[tokio::test]
async fn codex_invalid_reset_credit_expiry_is_not_scheduled() {
    let provider = codex_summary(json!({
        "rate_limit_reset_credits":{"credits":[{"title":"Extra reset","expires_at":"unknown"}]}
    }))
    .await;
    assert_eq!(provider.reset_credits.unwrap().details[0].title, "Extra reset");
    assert!(provider.reset_credit_expiries.is_empty());
}

#[tokio::test]
async fn codex_http_shows_rate_limit_upsell() {
    let provider = codex_summary(json!({"rate_limit_upsell":{"title":"Upgrade for more usage"}})).await;
    assert!(
        provider
            .notices
            .iter()
            .any(|notice| notice.text == "Upgrade for more usage")
    );
}

#[tokio::test]
async fn codex_http_reports_limit_spend_promo_and_overage_notices() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000}},
            "rate_limit_reached_type":{"type":"workspace_owner_credits_depleted"},
            "spend_control":{"reached":true},
            "promo":{"text":"Double limits this week"},
            "credits":{"overage_limit_reached":true}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({
            "tokens":{"access_token":"codex-fixture"}
        }),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(
        provider.notices.iter().map(|notice| &notice.kind).collect::<Vec<_>>(),
        [
            &NoticeKind::LimitReached,
            &NoticeKind::SpendControl,
            &NoticeKind::Promotion,
            &NoticeKind::SpendControl,
        ]
    );
    assert_eq!(
        provider.notices[0].text,
        "Limit reached: Workspace Owner Credits Depleted."
    );
    assert_eq!(provider.notices[2].text, "Double limits this week");
}

#[tokio::test]
async fn codex_additional_limit_keeps_its_name_for_panel_templates() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "additional_rate_limits":[{"limit_name":"Model Alpha","rate_limit":{
                "primary_window":{"used_percent":25,"limit_window_seconds":18000}
            }}]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({"tokens":{"access_token":"codex-fixture"}}),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.windows[0].scope.as_deref(), Some("Model Alpha"));
}

#[tokio::test]
async fn codex_http_keeps_notices_when_no_quota_is_reported() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit":{"limit_reached":true},"promo":{"title":"Promo"}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({
            "tokens":{"access_token":"codex-fixture"}
        }),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert!(provider.windows.is_empty());
    assert_eq!(
        provider
            .notices
            .iter()
            .map(|notice| notice.text.as_str())
            .collect::<Vec<_>>(),
        ["Usage limit reached.", "Promo"]
    );
}

#[tokio::test]
async fn codex_http_summarizes_unstructured_promotion() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/backend-api/wham/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "rate_limit_reached_type":"odd_type",
            "promo":{"pct":2,"details":[true,"active",null]}
        })))
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({
            "tokens":{"access_token":"codex-fixture"}
        }),
    );
    let provider = collect(&fixture_api(&server), "codex", home.path(), false)
        .await
        .unwrap();
    assert_eq!(provider.notices[0].text, "Limit reached: Odd Type.");
    assert_eq!(provider.notices[1].text, "details: true, active, null, pct: 2");
}

#[tokio::test]
async fn codex_http_names_each_plan_like_the_reference() {
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".codex/auth.json",
        &json!({
            "tokens":{"access_token":"codex-fixture"}
        }),
    );
    for (input, expected) in [
        (None, "ChatGPT"),
        (Some("pro"), "Pro (More)"),
        (Some("prolite"), "Pro"),
        (Some("promax"), "Pro (Max)"),
        (Some("plus"), "Plus"),
        (Some("go"), "Go"),
        (Some("free"), "Free"),
        (Some("team"), "Team"),
        (Some("free_workspace"), "Team"),
        (Some("business"), "Business"),
        (Some("self_serve_business_prolite"), "Business"),
        (Some("enterprise"), "Enterprise"),
        (Some("edu"), "Education"),
        (Some("education"), "Education"),
        (Some("k12"), "K12"),
        (Some("quorum_plus"), "Quorum Plus"),
        (Some("unknown_tier"), "Unknown Tier"),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/backend-api/wham/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"plan_type":input})))
            .expect(1)
            .mount(&server)
            .await;
        let provider = collect(&fixture_api(&server), "codex", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.plan, expected, "{input:?}");
    }
}
