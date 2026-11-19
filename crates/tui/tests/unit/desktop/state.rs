use super::*;
use serde_json::json;

fn provider(show_in_usage: bool) -> Value {
    json!({
        "plan": "",
        "updated": null,
        "status": "ok",
        "windows": [],
        "show_in_usage": show_in_usage,
        "details": [],
        "notices": [],
        "warnings": [],
        "organization": null,
        "show_organization": false,
        "identity": "",
        "credit_balance": null,
        "usage_credits": null,
        "reset_credits": null,
        "cloud_credits": null,
        "spend": null,
    })
}

#[test]
fn keeps_listed_providers_in_order_with_their_selected_windows_and_details() {
    let mut claude = provider(true);
    claude["plan"] = json!("Max");
    claude["updated"] = json!(1_700_000_000);
    claude["details"] = json!([{"title": "Subscription", "value": "Active"}]);
    claude["notices"] = json!([{"kind": "info", "text": "Limits doubled"}]);
    claude["warnings"] = json!(["Skipped 1 malformed Claude usage limit."]);
    claude["organization"] = json!("Example");
    claude["show_organization"] = json!(true);
    claude["identity"] = json!("me@example.com");
    claude["windows"] = json!([
        {
            "id": "session",
            "label": "Current session",
            "used": 38.0,
            "reset": "2023-11-14T22:13:20Z",
            "reset_precision": "day",
            "pace": {"status": "on_track", "expected_percent": 41.6},
        },
        {"id": "weekly", "label": "All models", "used": null, "reset": "soon", "pace": {"status": "unknown", "expected_percent": null}},
        {"id": "hidden", "label": "Not selected", "used": 1.0, "reset": null, "pace": null},
    ]);
    let mut codex = provider(true);
    codex["organization"] = json!("Hidden workspace");
    let raw = json!({
        "providers": {"claude": claude, "codex": codex, "gemini": provider(false)},
        "ordering": {"providers": ["codex", "gemini", "cursor", "claude"]},
        "selected_window_keys": ["claude:session", "claude:weekly"],
        "settings": {"decimals": 1},
    })
    .to_string();

    let state = project(&raw, 1_700_000_000).unwrap();

    assert_eq!(state.settings, json!({"decimals": 1}));
    let ids: Vec<&str> = state.providers.iter().map(|provider| provider.id.as_str()).collect();
    assert_eq!(ids, ["codex", "claude"]);
    let (codex, claude) = (&state.providers[0], &state.providers[1]);
    assert_eq!(codex.plan, None);
    assert!(codex.details.is_empty());
    assert_eq!(claude.plan.as_deref(), Some("Max"));
    assert_eq!(claude.fetched_at, Some(Timestamp::from_second(1_700_000_000).unwrap()));
    assert_eq!(
        claude.details,
        [
            "Subscription  Active",
            "Limits doubled",
            "Skipped 1 malformed Claude usage limit.",
            "Example",
            "me@example.com",
        ]
    );
    let labels: Vec<&str> = claude.windows.iter().map(|window| window.label.as_str()).collect();
    assert_eq!(labels, ["Current session", "All models"]);
    let (session, weekly) = (&claude.windows[0], &claude.windows[1]);
    assert_eq!(session.used_percent, Some(38.0));
    assert_eq!(session.resets_at, Some(Timestamp::from_second(1_700_000_000).unwrap()));
    assert!(session.reset_by_day);
    assert_eq!(session.pace.as_deref(), Some("Expected 42% · on track"));
    assert_eq!((weekly.used_percent, weekly.resets_at), (None, None));
    assert!(!weekly.reset_by_day);
    assert_eq!(weekly.pace, None);
}

#[test]
fn rejects_state_that_is_not_the_engine_shape() {
    assert!(project("{}", 0).is_err());
}
