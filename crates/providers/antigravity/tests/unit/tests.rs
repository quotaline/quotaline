//! Checks the public behavior of the adjacent module.

use super::*;
use serde_json::json;

fn provider(document: &Value) -> Provider {
    project(document, identity(&Value::Null), "fixture".into()).unwrap()
}

#[test]
fn identity_prefers_local_tier_and_keeps_email() {
    assert_eq!(
        identity(&json!({"userStatus":{"userTier":{"id":"ultra","name":"Ultra"},
                "planStatus":{"planInfo":{"planName":"Pro"}},"email":"a@b"}})),
        Identity {
            plan_name: "Ultra".into(),
            tier: Some("ultra".into()),
            email: Some("a@b".into())
        }
    );
}

#[test]
fn identity_uses_plan_and_cloud_fallbacks() {
    assert_eq!(
        identity(&json!({"userStatus":{"planStatus":{"planInfo":{"planName":"Pro"}}}})).plan_name,
        "Pro"
    );
    assert_eq!(
        identity(&json!({"currentTier":{"id":"plus","name":"Plus"}}))
            .tier
            .as_deref(),
        Some("plus")
    );
    assert_eq!(
        identity(&json!({"planName":"Legacy","email":"a@b"})).plan_name,
        "Legacy"
    );
    assert_eq!(identity(&json!({"email":"a@b"})).email.as_deref(), Some("a@b"));
    assert_eq!(identity(&Value::Null).plan_name, "Antigravity");
}

#[test]
fn quota_rpc_errors_have_distinct_outcomes() {
    for code in [json!(16), json!("16"), json!("UNAUTHENTICATED")] {
        assert_eq!(validate(&json!({"code":code})), Err(QuotaError::Unauthenticated));
    }
    for code in [json!(7), json!("7"), json!("denied")] {
        assert_eq!(validate(&json!({"code":code})), Err(QuotaError::RpcFailure));
    }
    for code in [json!(0), json!("0"), json!("OK"), json!("Success"), Value::Null] {
        assert_eq!(validate(&json!({"code":code,"groups":[]})), Ok(()));
    }
    assert_eq!(validate(&json!({})), Err(QuotaError::MissingGroups));
}

#[test]
fn wrapped_and_bare_groups_project_quota_windows() {
    for document in [
        json!({"response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.25,
                "resetTime":"2026-10-10T00:00:00Z","description":"Local"}]}]}}),
        json!({"summary":{"groups":[{"buckets":[{"bucketId":"gemini-5h",
                "remaining":{"remainingFraction":"0.25"},"resetTime":"invalid"}]}]}}),
        json!({"groups":[{"buckets":[{"bucketId":"gemini-5h",
                "remaining":{"case":"remainingFraction","value":"0.25"}}]}]}),
    ] {
        let projection = provider(&document);
        assert_eq!(projection.windows[0].id, "gemini:session");
        assert_eq!(projection.windows[0].used, 75.0);
    }
    let window = &provider(&json!({"groups":[{"buckets":[{"bucketId":"gemini-5h",
            "remainingFraction":0.25,"resetTime":"2026-10-10T00:00:00Z","description":"Local"}]}]}))
    .windows[0];
    assert_eq!(window.group, "session");
    assert_eq!(window.duration_seconds, Some(18_000));
    assert_eq!(window.reset.as_deref(), Some("2026-10-10T00:00:00Z"));
    assert_eq!(window.detail, "Local");
}

#[test]
fn known_and_custom_pools_keep_labels_and_durations() {
    let projection = provider(&json!({"groups":[{"buckets":[
        {"bucketId":"gemini-weekly","remainingFraction":0.5},
        {"bucketId":"3p-5h","remainingFraction":0.5},
        {"bucketId":"3p-weekly","remainingFraction":0.5},
        {"bucketId":"custom","displayName":"Custom","remainingFraction":0.5},
        {"displayName":"Named","remainingFraction":0.5},
        {"remainingFraction":0.5}
    ]}]}));
    assert_eq!(
        projection
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["gemini:weekly", "3p:session", "3p:weekly", "custom", "Named", "quota"]
    );
    assert_eq!(
        projection
            .windows
            .iter()
            .map(|window| window.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Gemini weekly",
            "Claude and GPT 5-hour",
            "Claude and GPT weekly",
            "Custom",
            "Named",
            "quota"
        ]
    );
    assert_eq!(
        projection
            .windows
            .iter()
            .map(|window| window.duration_seconds)
            .collect::<Vec<_>>(),
        [Some(604_800), Some(18_000), Some(604_800), None, None, None]
    );
}

#[test]
fn identified_projection_omits_unnamed_buckets() {
    let document = json!({"groups":[{"buckets":[
        {"bucketId":"known","remainingFraction":0.5},
        {"displayName":"Unnamed bucket","remainingFraction":0.5},
        {"displayName":"Unreported"}
    ]}]});
    assert_eq!(provider(&document).windows.len(), 2);
    let identified = project_identified(&document, identity(&Value::Null), "fixture".into()).unwrap();
    assert_eq!(
        identified
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["known"]
    );
    assert!(identified.details.is_empty());
}

#[test]
fn unmeasured_bucket_without_identity_is_ignored() {
    let projection = provider(&json!({"groups":[{"buckets":[
        {"remaining":{"remainingAmount":"9"}},
        {"displayName":"Named","remaining":{"remainingAmount":"9"}}
    ]}]}));
    assert_eq!(
        projection
            .details
            .iter()
            .map(|detail| detail.title.as_str())
            .collect::<Vec<_>>(),
        ["Named"]
    );
}

#[test]
fn unmeasured_pools_show_amounts_and_explanations() {
    let projection = provider(&json!({"groups":[{"buckets":[
        {"bucketId":"small","remaining":{"case":"remainingAmount","value":250}},
        {"bucketId":"thousands","remaining":{"remainingAmount":1200}},
        {"bucketId":"large-thousands","remaining":{"remainingAmount":12000}},
        {"bucketId":"millions","remaining":{"remainingAmount":1500000}},
        {"bucketId":"large-millions","remaining":{"remainingAmount":12000000}},
        {"bucketId":"billions","remaining":{"remainingAmount":1500000000}},
        {"bucketId":"large-billions","remaining":{"remainingAmount":12000000000_i64}},
        {"bucketId":"literal","remaining":{"remainingAmount":"many"}},
        {"bucketId":"missing","description":"Limited.","resetTime":"tomorrow"},
        {"bucketId":"disabled","disabled":true,"remaining":{"remainingAmount":1500},
            "remainingFraction":0.5}
    ]}]}));
    assert_eq!(
        projection
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
            "many remaining",
            "Usage percentage unavailable",
            "Disabled · 1.5K remaining"
        ]
    );
    assert_eq!(
        projection.details[8].explanation,
        "Limited. Resets tomorrow. The local source did not report a usable percentage for this quota."
    );
    assert_eq!(projection.details[9].title, "disabled");
}

#[test]
fn disabled_and_invalid_quota_values_remain_details() {
    let projection = provider(&json!({"groups":[{"buckets":[
        {"bucketId":"disabled","disabled":true},
        {"bucketId":"invalid","remainingFraction":"invalid"},
        {"bucketId":"infinite","remainingFraction":"inf"},
        {"bucketId":"missing"}
    ]}]}));
    assert!(projection.windows.is_empty());
    assert_eq!(
        projection
            .details
            .iter()
            .map(|detail| detail.value.as_str())
            .collect::<Vec<_>>(),
        [
            "Disabled",
            "Usage percentage unavailable",
            "Usage percentage unavailable",
            "Usage percentage unavailable"
        ]
    );
}

#[test]
fn empty_groups_are_a_connected_provider_without_quota() {
    let result = provider(&json!({"groups":[]}));
    assert_eq!(
        (result.status.as_str(), result.windows.len(), result.details.len()),
        ("connected", 0, 0)
    );
}

#[test]
fn projection_rejects_invalid_quota() {
    assert_eq!(
        project(&json!({"code":16}), identity(&Value::Null), "fixture".into()).err(),
        Some(QuotaError::Unauthenticated)
    );
}

#[test]
fn provider_keeps_account_identity() {
    let result = project(
        &json!({"groups":[]}),
        identity(&json!({"userStatus":{"userTier":{"name":"Ultra","id":"ultra"},"email":"user@example.com"}})),
        "fixture".into(),
    )
    .unwrap();
    assert_eq!(
        (
            result.plan.as_str(),
            result.tier.as_deref(),
            result.identity.as_str(),
            result.source.as_str()
        ),
        ("Ultra", Some("ultra"), "user@example.com", "fixture")
    );
}
