//! Checks copilot 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn copilot_http_keeps_unlimited_account_connected() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quota_snapshots":{"chat":{"unlimited":true}}
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
    assert_eq!(provider.status, "connected");
    assert!(provider.windows.is_empty());
    assert_eq!(provider.details[0].value, "Unlimited");
}

#[tokio::test]
async fn copilot_http_names_plans_and_organizations_like_the_reference() {
    let home = tempfile::tempdir().unwrap();
    credential_file(
        home.path(),
        ".copilot/config.json",
        &json!({
            "loggedInUsers":[{"token":"copilot-fixture","host":"github.com"}]
        }),
    );
    for (sku, organizations, expected) in [
        ("individual", json!([]), "Pro"),
        ("individual_pro", json!([]), "Pro+"),
        ("individual_max", json!([]), "Max"),
        ("individual_edu", json!([]), "Student"),
        (
            "enterprise",
            json!(["octo-org", "octo-labs"]),
            "Enterprise via octo-org, octo-labs",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "copilot_plan":sku,
                "organization_login_list":organizations,
                "quota_snapshots":{"chat":{"percent_remaining":40}}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let provider = collect(&fixture_api(&server), "copilot", home.path(), false)
            .await
            .unwrap();
        assert_eq!(provider.plan, expected, "{sku}");
    }
}

#[tokio::test]
async fn copilot_http_orders_quotas_and_reads_legacy_free_quotas() {
    let server = MockServer::start().await;
    Mock::given(matchers::method("GET"))
        .and(matchers::path("/copilot_internal/user"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "quota_reset_date":"2026-10-11T00:00:00Z",
            "quota_snapshots":{
                "chat":{"percent_remaining":40},
                "premium_interactions":{"percent_remaining":75},
                "extra_feature":{"percent_remaining":80}
            },
            "limited_user_quotas":{"completions":75,"chat":80},
            "monthly_quotas":{"completions":100,"chat":100}
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
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["premium_interactions", "chat", "extra_feature", "free:completions"]
    );
    assert_eq!(provider.windows[0].label, "Premium requests");
    assert_eq!(provider.windows[3].used, 25.0);
}

#[tokio::test]
async fn copilot_places_known_categories_before_other_snapshots() {
    let provider = copilot_summary(json!({
        "quota_snapshots":{
            "other":{"percent_remaining":80},
            "completions":{"percent_remaining":70},
            "chat":{"percent_remaining":60},
            "premium_interactions":{"percent_remaining":50}
        }
    }))
    .await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["premium_interactions", "chat", "completions", "other"]
    );
}

#[test]
fn copilot_keeps_categories_and_overage() {
    let data = json!({"copilot_plan":"individual","quota_snapshots":{
        "premium_interactions":{"entitlement":100,"quota_remaining":40,"overage_count":3},
        "chat":{"unlimited":true}
    }});
    let provider = copilot_provider(&data, "fixture".into());
    assert_eq!(provider.windows.len(), 1);
    assert_eq!(provider.windows[0].used, 60.0);
    assert!(
        provider
            .details
            .iter()
            .any(|detail| detail.value.contains("Overage used: 3"))
    );
}
