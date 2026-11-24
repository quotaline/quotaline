//! Checks the public behavior of the adjacent module.

use super::*;
use serde_json::json;

fn provider(quota: &Value, assist: &Value, hosted_domain: Option<&str>) -> Provider {
    project(quota, assist, hosted_domain, String::new(), "fixture".into())
}

#[test]
fn projection_selects_most_consumed_model_and_credit_balance() {
    let result = provider(
        &json!({"buckets":[
            {"modelId":"gemini-2-pro","remainingFraction":0.8,"remainingAmount":"80","tokenType":"requests","resetTime":"2026-11-01T00:00:00Z"},
            {"modelId":"gemini-2-pro","remainingFraction":0.2,"remainingAmount":"20","resetTime":"2026-11-01T00:00:00Z"},
            {"modelId":"gemini-2-pro","remainingFraction":0.9},
            {"modelId":"gemini-3-flash","remainingFraction":0.9}
        ]}),
        &json!({"paidTier":{"name":"Google AI Pro","availableCredits":[
                {"creditType":"GOOGLE_ONE_AI","creditAmount":"2.5"}]},
                "currentTier":{"id":"standard-tier","availableCredits":[
                {"creditType":"OTHER_CREDIT","creditAmount":"3.2500"}]}}),
        None,
    );
    assert_eq!(result.plan, "Google AI Pro");
    assert_eq!(result.tier.as_deref(), Some("standard-tier"));
    assert_eq!(result.windows.len(), 2);
    assert_eq!(result.windows[0].id, "model:gemini-2-pro");
    assert_eq!(result.windows[0].label, "Gemini 2 Pro");
    assert_eq!(result.windows[0].used, 80.0);
    assert_eq!(result.windows[0].scope.as_deref(), Some("gemini-2-pro"));
    assert_eq!(result.windows[0].reset.as_deref(), Some("2026-11-01T00:00:00Z"));
    assert_eq!(result.credit_balance, Some(2.5));
    assert_eq!(result.details[0].id, "credit:GOOGLE_ONE_AI");
    assert_eq!(result.details[1].value, "3.25 credits");
    assert_eq!(
        result.details[2].value,
        "80 requests remaining · 20% used · Resets 2026-11-01T00:00:00Z"
    );
    assert_eq!(result.details[3].id, "bucket:1");
}

#[test]
fn account_tiers_and_fallback_credit_follow_provider_rules() {
    for (id, domain, expected) in [
        ("standard-tier", None, "Standard"),
        ("legacy-tier", None, "Legacy"),
        ("free-tier", None, "Free"),
        ("free-tier", Some("example.com"), "Workspace"),
    ] {
        assert_eq!(
            provider(&Value::Null, &json!({"currentTier":{"id":id}}), domain).plan,
            expected
        );
    }
    let result = provider(
        &json!({"buckets":[{"remainingAmount":"10"},{"modelId":"unknown","remainingFraction":"invalid"}]}),
        &json!({"currentTier":{"name":"Special","availableCredits":[
                {"creditType":"PROMO","creditAmount":2.5},
                {"creditType":"INVALID","creditAmount":"bad"}]}}),
        None,
    );
    assert_eq!(result.plan, "Special");
    assert_eq!(result.credit_balance, Some(2.5));
    assert!(result.windows.is_empty());
    assert_eq!(result.details[1].id, "bucket:0");
    assert_eq!(result.details[1].title, "Quota");
    assert_eq!(result.details[1].value, "10 units remaining");
    assert_eq!(provider(&Value::Null, &Value::Null, None).plan, "Gemini");
}

#[test]
fn assist_metadata_and_subscription_errors_use_one_contract() {
    assert_eq!(project_id(&json!({"cloudaicompanionProject":"direct"})), Some("direct"));
    assert_eq!(project_id(&json!({"cloudaicompanionProject":{"id":"id"}})), Some("id"));
    assert_eq!(
        project_id(&json!({"cloudaicompanionProject":{"projectId":"project"}})),
        Some("project")
    );
    assert_eq!(project_id(&Value::Null), None);
    assert!(has_credit_balances(&json!({"paidTier":{"availableCredits":[{}]}})));
    assert!(!has_credit_balances(&Value::Null));
    assert_eq!(unsupported_reason(&json!({"currentTier":{}})), None);
    assert_eq!(unsupported_reason(&json!({"paidTier":{}})), None);
    assert_eq!(unsupported_reason(&Value::Null), None);
    assert_eq!(unsupported_reason(&json!({"ineligibleTiers":[]})), None);
    assert_eq!(
        unsupported_reason(
            &json!({"ineligibleTiers":[{"reasonCode":"other","reasonMessage":"Other"},
            {"reasonCode":"unsupported_client","reasonMessage":"Unsupported"}]})
        ),
        Some("Unsupported".into())
    );
    assert_eq!(
        unsupported_reason(&json!({"ineligibleTiers":[{}]})),
        Some(UNSUPPORTED_CLIENT_MESSAGE.into())
    );
    assert!(subscription_required("SUBSCRIPTION_REQUIRED"));
    assert!(subscription_required(r#"{"error":{"status":"subscription_required"}}"#));
    assert!(subscription_required(
        r#"{"error":{"details":[{"reason":"SUBSCRIPTION_REQUIRED"}]}}"#
    ));
    assert!(!subscription_required(r#"{"error":{"status":"PERMISSION_DENIED"}}"#));
}

#[test]
fn provider_keeps_identity_and_hosted_domain() {
    let result = project(
        &Value::Null,
        &Value::Null,
        Some("example.com"),
        "user@example.com".into(),
        "fixture".into(),
    );
    assert_eq!(
        (
            result.identity.as_str(),
            result.organization.as_str(),
            result.source.as_str()
        ),
        ("user@example.com", "example.com", "fixture")
    );
}
