//! Checks GNOME snapshots through the shared quota projection.

use super::*;
use quotaline_quota::Window;

#[test]
fn selected_status_entry_retains_scope_label_and_blocking_quota() {
    let mut session = Window::new("session", "Current session", 30.0, None, "session");
    session.scope = Some("Opus".into());
    let weekly = Window::new(
        "weekly",
        "All models",
        100.0,
        Some("2026-10-14T22:00:00Z".into()),
        "weekly",
    );
    let provider = Provider::connected(
        "claude",
        "Max".into(),
        "fixture".into(),
        String::new(),
        vec![session, weekly],
    );
    let providers = BTreeMap::from([("claude".into(), provider)]);
    let settings = Settings {
        has_custom_selection: Some(true),
        selected_windows: vec!["invalid".into(), "claude:session".into()],
        short_labels: BTreeMap::from([("claude:session".into(), "EDIT".into())]),
        ..Settings::default()
    };
    let entries = project(&providers, &settings, 1_792_015_000.0).entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].short_label, "EDIT");
    assert_eq!(entries[0].used, 100.0);
    assert_eq!(entries[0].restrictions[0].label, "All models");
    assert_eq!(entries[0].reset_at, Some(1_792_015_200.0));
}

#[test]
fn missing_reset_and_selection_keep_raw_quota() {
    let provider = Provider::connected(
        "codex",
        "Pro".into(),
        "fixture".into(),
        String::new(),
        vec![Window::new("session", "Current session", 30.0, None, "session")],
    );
    let providers = BTreeMap::from([("codex".into(), provider)]);
    let entries = project(&providers, &Settings::default(), 100.0).entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].used, 30.0);
    assert_eq!(entries[0].reset_at, None);
}

fn ordered_projection() -> StatusProjection {
    let providers = BTreeMap::from([
        (
            "claude".into(),
            Provider::connected(
                "claude",
                "Max".into(),
                "fixture".into(),
                String::new(),
                vec![
                    Window::new("session", "Current session", 0.0, None, "session"),
                    Window::new("weekly", "All models", 70.0, None, "weekly"),
                ],
            ),
        ),
        (
            "codex".into(),
            Provider::connected(
                "codex",
                "Pro".into(),
                "fixture".into(),
                String::new(),
                vec![Window::new("session", "Current session", 40.0, None, "session")],
            ),
        ),
    ]);
    let settings = Settings {
        has_custom_selection: Some(true),
        selected_windows: vec!["claude:session".into(), "claude:weekly".into(), "codex:session".into()],
        provider_order: vec!["codex".into(), "claude".into()],
        model_order: vec!["claude:weekly".into(), "claude:session".into()],
        hide_zero: true,
        ..Settings::default()
    };
    project(&providers, &settings, 100.0)
}

#[test]
fn projected_status_orders_visible_windows() {
    assert_eq!(
        ordered_projection()
            .entries
            .iter()
            .map(|entry| format!("{}:{}", entry.provider, entry.window_id))
            .collect::<Vec<_>>(),
        vec!["codex:session", "claude:weekly"]
    );
}

#[test]
fn projected_status_selects_one_window_per_provider() {
    assert_eq!(ordered_projection().provider_indices, vec![0, 1]);
}

#[test]
fn projected_status_groups_mini_bars_by_provider() {
    assert_eq!(
        ordered_projection()
            .mini_groups
            .iter()
            .map(|group| (group.provider.as_str(), group.indices.as_slice()))
            .collect::<Vec<_>>(),
        vec![("codex", &[0][..]), ("claude", &[1][..])]
    );
}

#[test]
fn projected_status_filters_unavailable_and_disabled_providers() {
    for (status, enabled) in [("offline", true), ("connected", false)] {
        let mut provider = Provider::connected(
            "claude",
            "Max".into(),
            "fixture".into(),
            String::new(),
            vec![Window::new("session", "Current session", 40.0, None, "session")],
        );
        provider.status = status.into();
        let providers = BTreeMap::from([("claude".into(), provider)]);
        let settings = Settings {
            providers: BTreeMap::from([("claude".into(), enabled)]),
            ..Settings::default()
        };
        assert!(
            project(&providers, &settings, 100.0).entries.is_empty(),
            "{status} enabled={enabled}"
        );
    }
}

#[test]
fn projected_status_keeps_stale_provider_readings() {
    let mut provider = Provider::connected(
        "codex",
        "Pro".into(),
        "fixture".into(),
        String::new(),
        vec![Window::new("session", "Current session", 40.0, None, "session")],
    );
    provider.status = "stale".into();
    let providers = BTreeMap::from([("codex".into(), provider)]);
    assert_eq!(project(&providers, &Settings::default(), 100.0).entries[0].used, 40.0);
}
