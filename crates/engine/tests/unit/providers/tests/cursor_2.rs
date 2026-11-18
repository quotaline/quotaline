//! Checks cursor 2 behavior through the public API.

use super::*;

#[tokio::test]
async fn cursor_summary_preserves_budget_details() {
    let data = serde_json::from_str(include_str!("../../../fixtures/cursor_usage_summary.json")).unwrap();
    let provider = cursor_summary(data, json!({"accessToken":"cursor-fixture"})).await;
    let details = provider
        .details
        .iter()
        .map(|detail| (detail.title.as_str(), detail.value.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    let included = details["Included plan"];
    assert!(included.contains("Included: $0.00"));
    assert!(included.contains("Bonus: $0.00"));
    assert_eq!(
        provider.details[0].explanation,
        "Provider-reported budget in USD. Individual and team allowances remain separate."
    );
    assert!(
        provider
            .details
            .iter()
            .any(|detail| { detail.title == "Individual on-demand" && detail.value.contains("Remaining: $95.00") })
    );
    assert!(
        provider
            .details
            .iter()
            .any(|detail| { detail.title == "Overall" && detail.value.starts_with("Off ·") })
    );
}

#[tokio::test]
async fn cursor_team_details_put_on_demand_before_pool() {
    let provider = cursor_summary(
        json!({"teamUsage":{"pooled":{"used":100},"onDemand":{"used":200}}}),
        json!({"accessToken":"cursor-fixture"}),
    )
    .await;
    assert_eq!(
        provider
            .details
            .iter()
            .map(|detail| detail.title.as_str())
            .collect::<Vec<_>>(),
        ["Team on-demand", "Team pool"]
    );
}

#[tokio::test]
async fn cursor_summary_exposes_spend_control() {
    let data = serde_json::from_str(include_str!("../../../fixtures/cursor_usage_summary.json")).unwrap();
    let provider = cursor_summary(data, json!({"accessToken":"cursor-fixture"})).await;
    assert_eq!(provider.spend.as_ref().unwrap().used, Some(5.0));
    assert_eq!(provider.spend.as_ref().unwrap().limit, Some(100.0));
    assert_eq!(provider.spend.as_ref().unwrap().percent, Some(5.0));
    assert_eq!(provider.spend.as_ref().unwrap().currency.as_deref(), Some("USD"));
}

#[tokio::test]
async fn cursor_pro_omits_disabled_on_demand_window() {
    let data = serde_json::from_str(include_str!("../../../fixtures/cursor_usage_summary_pro.json")).unwrap();
    let provider = cursor_summary(data, json!({"accessToken":"cursor-fixture"})).await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["plan", "overall"]
    );
    assert_eq!(provider.windows[0].detail, "Auto 18% · API 62%");
    assert_eq!(provider.spend.as_ref().unwrap().enabled, Some(false));
}

#[tokio::test]
async fn cursor_preserves_component_windows_without_an_aggregate() {
    let provider = cursor_summary(
        json!({"individualUsage":{"plan":{"autoPercentUsed":10,"apiPercentUsed":20}}}),
        json!({"accessToken":"cursor-fixture"}),
    )
    .await;
    assert_eq!(
        provider
            .windows
            .iter()
            .map(|window| window.id.as_str())
            .collect::<Vec<_>>(),
        ["plan:auto", "plan:api"]
    );
    assert_eq!(provider.windows[0].label, "Plan usage (Auto)");
    assert!((provider.windows[0].used - 10.0).abs() < 0.000_001);
}

#[tokio::test]
async fn cursor_keeps_disabled_spend_visible_without_a_window() {
    let provider = cursor_summary(
        json!({"individualUsage":{"onDemand":{"enabled":false,"used":100,"limit":100,"remaining":0}}}),
        json!({"accessToken":"cursor-fixture"}),
    )
    .await;
    assert!(provider.windows.is_empty());
    let details = provider
        .details
        .iter()
        .map(|detail| (detail.title.as_str(), detail.value.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    assert!(details["Individual on-demand"].starts_with("Off ·"));
    assert_eq!(provider.spend.as_ref().unwrap().enabled, Some(false));
    assert!(provider.spend.as_ref().unwrap().limit_reached);
    assert!(provider.notices.is_empty());
}

#[tokio::test]
async fn cursor_ignores_invalid_billing_dates() {
    let provider = cursor_summary(
        json!({
            "billingCycleStart":"bad",
            "billingCycleEnd":"bad",
            "individualUsage":{"onDemand":{"used":100,"limit":200}}
        }),
        json!({"accessToken":"cursor-fixture"}),
    )
    .await;
    assert_eq!(provider.windows[0].reset, None);
    assert_eq!(provider.windows[0].duration_seconds, None);
    assert_eq!(provider.spend.as_ref().unwrap().reset, None);
}

#[tokio::test]
async fn cursor_maps_provider_allowance_descriptions() {
    let provider = cursor_summary(
        json!({
            "isUnlimited":true,
            "limitType":"user",
            "autoModelSelectedDisplayMessage":"Auto pool",
            "namedModelSelectedDisplayMessage":"Named pool"
        }),
        json!({"accessToken":"cursor-fixture"}),
    )
    .await;
    assert_eq!(
        provider
            .details
            .iter()
            .map(|detail| detail.title.as_str())
            .collect::<Vec<_>>(),
        ["Limit type", "Auto model allowance", "Named model allowance"]
    );
    assert_eq!(provider.details[2].value, "Named pool");
    assert_eq!(
        provider.details[2].explanation,
        "The provider's description of this allowance. Model pools may have different limits."
    );
}
