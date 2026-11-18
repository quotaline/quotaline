use std::collections::{BTreeMap, BTreeSet};

use quotaline_quota::AccountSelection;
use quotaline_quota::Provider;
use quotaline_quota::Window;
use quotaline_settings::Settings;

use super::{StateInput, project};

#[test]
fn private_state_masks_named_sources_with_account_selection() {
    let settings = Settings {
        hide_identity: true,
        ..Settings::default()
    };
    let mut provider = Provider::empty("codex", "connected");
    provider.source = "Codex CLI".into();
    provider.account_selection = Some(AccountSelection {
        count: 2,
        label: "private@example.test".into(),
    });
    let providers = BTreeMap::from([("codex".into(), provider)]);
    let state = project(StateInput {
        settings: &settings,
        providers: &providers,
        running: &BTreeSet::new(),
        last_refresh: None,
        first_run_pending: false,
        claude_local_usage: None,
        recent_samples: &BTreeMap::new(),
        now: 0.0,
    });
    let state: serde_json::Value = serde_json::from_str(&state).unwrap();

    assert_eq!(state["providers"]["codex"]["source"], "Local credential");
}

#[test]
fn state_exposes_effective_window_selection() {
    let providers = BTreeMap::from([(
        "claude".into(),
        Provider::connected(
            "claude",
            "Pro".into(),
            "test".into(),
            String::new(),
            vec![Window::new("session", "Session", 12.0, None, "session")],
        ),
    )]);
    for (name, settings, expected) in [
        ("default", Settings::default(), vec!["claude:session"]),
        (
            "custom empty",
            Settings {
                has_custom_selection: Some(true),
                ..Settings::default()
            },
            vec![],
        ),
    ] {
        let state = project(StateInput {
            settings: &settings,
            providers: &providers,
            running: &BTreeSet::new(),
            last_refresh: None,
            first_run_pending: false,
            claude_local_usage: None,
            recent_samples: &BTreeMap::new(),
            now: 0.0,
        });
        let state: serde_json::Value = serde_json::from_str(&state).unwrap();

        assert_eq!(state["selected_window_keys"], serde_json::json!(expected), "{name}");
    }
}
