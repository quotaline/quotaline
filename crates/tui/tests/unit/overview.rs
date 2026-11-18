use super::*;
use serde_json::{Value, json};

fn reading(id: &str, fields: Value) -> Provider {
    let mut provider = serde_json::to_value(Provider::empty(id, "ready")).unwrap();
    provider
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    serde_json::from_value(provider).unwrap()
}

#[test]
fn shows_reported_credits_and_next_reset_expiry() {
    let mut provider = reading(
        "codex",
        json!({
            "credit_balance": 0,
            "usage_credits": {
                "unlimited": false, "has_credits": true, "overage_limit_reached": true
            },
            "reset_credits": {"available": 3, "applicable": 1, "purchase_eligible": true, "details":[]},
            "reset_credit_expiries": [{"at": 180, "id": "later"}, {"at": 120, "id": "soon"}]
        }),
    );
    assert_eq!(
        provider_overview(&provider, |_| true, 60),
        [
            "Credits · $0.00",
            "Credit overage limit reached",
            "Limit resets · 3 available (1 applicable now)",
            "Next reset credit expires in 1 min",
        ],
    );
    provider.credit_balance = None;
    assert_eq!(
        provider_overview(&provider, |_| true, 180),
        [
            "Credits · Balance not reported",
            "Credit overage limit reached",
            "Limit resets · 3 available (1 applicable now)",
            "Reset credit expired 1 min ago",
        ],
    );

    let legacy = reading("codex", json!({"credit_balance": 42.5}));
    assert_eq!(provider_overview(&legacy, |_| true, 0), ["Credits · $42.50 remaining"]);
}

#[test]
fn cloud_credits_report_availability_even_at_zero() {
    let mut provider = reading(
        "codex",
        json!({"cloud_credits": {"remaining":0,"limit":50,"expires_at":"1970-01-01T00:02:00Z"}}),
    );
    assert_eq!(
        provider_overview(&provider, |_| true, 60),
        ["Cloud session credits · $0.00 of $50.00 remaining"]
    );
    assert_eq!(
        provider_overview(&provider, |_| true, 120),
        ["Cloud session credits · $0.00 of $50.00 remaining · Expired"]
    );
    provider.cloud_credits.as_mut().unwrap().locked_reason = Some("Private workspace".into());
    assert_eq!(
        provider_overview(&provider, |_| true, 60),
        ["Cloud session credits · $0.00 of $50.00 remaining · Unavailable"]
    );
}

#[test]
fn spend_uses_reported_units_and_deduplicates_legacy_balance() {
    let provider = reading(
        "cursor",
        json!({
            "credit_balance": 98.76,
            "spend": {
                "currency":"USD", "used":50, "limit":100, "balance":98.76,
                "balance_money":{"currency":"EUR","amount_minor": 125,"exponent":3},
                "credit_allowance":{"used":0,"limit":5,"remaining":5},
                "enabled":false,"limit_reached":true
            }
        }),
    );
    assert_eq!(
        provider_overview(&provider, |_| true, 0),
        [
            "Spend control · 0 credits of 5 credits",
            "Spend allowance · 5 credits remaining",
            "Spend credit balance · 0.125 EUR",
            "Spend limit reached",
            "Spend control · Off",
        ],
    );
    for (spend, expected) in [
        (json!({"used":25,"currency":"EUR"}), "Spend used · 25.00 EUR"),
        (
            json!({"limit_money":{"amount_minor":50,"currency":"JPY","exponent":0}}),
            "Spend limit · 50 JPY",
        ),
        (
            json!({"used":25,"limit":100,"currency":"USD"}),
            "Spend control · $25.00 of $100.00",
        ),
        (
            json!({"used":10,"limit":100,"balance":30,"percent":10}),
            "Spend control · 10.0% used",
        ),
        (json!({"balance":0,"currency":"USD"}), "Spend credit balance · $0.00"),
    ] {
        let mut spend = spend;
        spend["limit_reached"] = json!(false);
        let provider = reading("cursor", json!({"spend":spend}));
        assert_eq!(provider_overview(&provider, |_| true, 0), [expected]);
    }
}

#[test]
fn only_visible_copilot_quota_accounting_appears_in_usage() {
    let provider = reading(
        "copilot",
        json!({"details":[
            {"id":"quota:premium_interactions","title":"Premium credits",
             "value":"Included: 500 credits · Credits used: 200 credits · Remaining: 300 credits · Overage used: 0 credits"},
            {"id":"quota:chat","title":"Chat","value":"Unlimited · Included: 0 credits"},
            {"id":"quota:completions","title":"Completions","value":"Overage permitted"},
            {"id":"account", "title":"Account","value":"private@example.com"}
        ]}),
    );
    assert_eq!(
        provider_overview(&provider, |id| id == "premium_interactions", 0),
        ["Premium credits · Remaining: 300 credits · Included: 500 credits · Credits used: 200 credits"]
    );
    assert_eq!(
        provider_overview(&provider, |id| id != "premium_interactions", 0),
        ["Chat · Unlimited"]
    );
}

#[test]
fn shows_known_notices_without_exposing_account_text() {
    let mut provider = reading(
        "codex",
        json!({
            "identity":"private@example.com",
            "status":"stale",
            "notices":[
                {"kind":"limitReached","text":"Usage limit reached."},
                {"kind":"limitReached","text":"Limit reached for private@example.com"},
                {"kind":"promotion","text":"Offer for private@example.com"},
                {"kind":"spendControl","text":"Workspace spend limit reached."},
                {"kind":"spendControl","text":"Credit overage limit reached."},
                {"kind":"info","text":"Please send diagnostics from /home/private"},
                {"kind":"promotion","text":"Offer for organization"}
            ],
            "warnings":["Plan details unavailable: private@example.com","/home/private/debug"]
        }),
    );
    let selected = |_: &str| true;
    assert_eq!(
        provider_overview(&provider, selected, 0),
        [
            "Status · Cached reading",
            "Credit overage limit reached.",
            "Plan details unavailable",
            "Provider warnings · See details",
            "Quota limit reached",
            "Quota promotion · See details",
            "Usage limit reached.",
            "Workspace spend limit reached.",
        ],
    );

    provider.spend = Some(serde_json::from_value(json!({"limit_reached":true})).unwrap());
    provider.usage_credits = Some(
        serde_json::from_value(json!({"unlimited":true,"has_credits":true,"overage_limit_reached":true})).unwrap(),
    );
    assert_eq!(
        provider_overview(&provider, selected, 0),
        [
            "Credits · Unlimited",
            "Credit overage limit reached",
            "Spend limit reached",
            "Status · Cached reading",
            "Plan details unavailable",
            "Provider warnings · See details",
            "Quota limit reached",
            "Quota promotion · See details",
            "Usage limit reached.",
        ],
    );
}
