use super::*;
use crate::snapshot::LocalSnapshot as Source;
use jiff::{SignedDuration, Timestamp};
use quotaline_export::UsageExport;
use quotaline_quota::{Provider, Window};
use ratatui::{Terminal, backend::TestBackend};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::tempdir;

#[test]
fn renders_demo_from_the_export() {
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::Demo);
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("QUOTALINE"));
    assert!(screen.contains("› Antigravity"));
    assert!(screen.contains("Gemini weekly"));
    assert!(screen.contains("Observed "));
    assert!(screen.contains("Resets in "));
}

#[test]
fn renders_observation_and_meter_from_a_real_export() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    let reset = Timestamp::now() - SignedDuration::from_mins(2);
    write_export(
        &path,
        [provider(
            "codex",
            "Pro",
            vec![Window::new("weekly", "Weekly", 78.5, Some(reset.to_string()), "weekly")],
        )],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    for text in [
        "Codex · Pro",
        "Weekly",
        "78.5%",
        "Observed 5 min ago",
        "Reset was due 2 min ago",
    ] {
        assert!(screen.contains(text), "{screen}");
    }
}

#[test]
fn meter_tracks_remain_contiguous() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [
            provider("zero", "", vec![Window::new("weekly", "Weekly", 0.0, None, "weekly")]),
            provider("half", "", vec![Window::new("weekly", "Weekly", 50.0, None, "weekly")]),
            provider("full", "", vec![Window::new("weekly", "Weekly", 100.0, None, "weekly")]),
        ],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    let meters = screen
        .lines()
        .filter(|line| line.contains("Weekly"))
        .collect::<Vec<_>>();
    assert_eq!(meters.len(), 3);
    assert!(meters[0].contains("█"));
    assert!(!meters[0].contains("░"));
    assert!(meters[0].contains("100.0%"));
    assert!(meters[1].contains("█░"));
    assert!(meters[1].contains("50.0%"));
    assert!(meters[2].contains("░"));
    assert!(!meters[2].contains("█"));
    assert!(meters[2].contains("0.0%"));
}

#[test]
fn overview_lists_all_windows_with_their_usage() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [provider(
            "codex",
            "",
            vec![
                Window::new("short", "Short", 10.0, None, "session"),
                Window::new("long", "Long", 101.5, None, "weekly"),
            ],
        )],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert!(
        screen
            .lines()
            .any(|line| line.contains("Long") && line.contains("100.0%")),
        "{screen}"
    );
    assert!(
        screen
            .lines()
            .any(|line| line.contains("Short") && line.contains("10.0%")),
        "{screen}"
    );
}

#[test]
fn overview_keeps_window_values_at_narrow_and_wide_sizes() {
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::Demo);
    for (width, height) in [(34, 14), (74, 14), (75, 14), (79, 24), (80, 14), (80, 24)] {
        let screen = render(&dashboard, width, height);
        assert!(screen.contains("Quota used"));
        assert!(screen.contains("Gemini 5-hour"));
        assert!(screen.contains("Observed "));
        assert!(screen.contains("Resets in "));
    }
}

#[test]
fn guides_users_to_resize_tiny_terminals() {
    let dashboard = Dashboard::new(true);
    for (width, height) in [(33, 14), (34, 13), (80, 13)] {
        assert!(render(&dashboard, width, height).starts_with("Resize to at least 34 × 14"));
    }
}

#[test]
fn window_navigation_scrolls_to_the_last_row() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [provider(
            "codex",
            "",
            (1..=12)
                .map(|number| Window::new("weekly", format!("Window {number}"), 0.0, None, "weekly"))
                .collect(),
        )],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    for _ in 0..15 {
        dashboard.navigate(Navigation::Down);
    }
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("› Window 12"));
    assert!(!render(&dashboard, 80, 14).contains("Window 1 "));
}

#[test]
fn provider_navigation_scrolls_to_keep_selection_visible() {
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::Demo);
    for _ in 0..5 {
        dashboard.navigate(Navigation::NextGroup);
    }
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::Left);
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::Down);
    assert!(render(&dashboard, 40, 16).contains("› Gemini 2.5 Pro"));
    dashboard.navigate(Navigation::Up);
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::NextGroup);
    assert!(render(&dashboard, 40, 16).contains("› Antigravity"));
}

#[test]
fn preserves_selection_by_provider_id_when_the_export_is_reordered() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [provider("codex", "", Vec::new()), provider("gemini", "", Vec::new())],
    );
    let source = Source::File(path.clone());
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&source);
    dashboard.navigate(Navigation::Down);
    let mut export: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    export["providers"].as_array_mut().unwrap().reverse();
    fs::write(path, export.to_string()).unwrap();
    dashboard.refresh(&source);
    assert!(render(&dashboard, 80, 24).contains("› Gemini"));
}

#[test]
fn reports_an_absent_snapshot_with_instructions() {
    let directory = tempdir().unwrap();
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(directory.path().join("usage.json")));
    dashboard.navigate(Navigation::NextGroup);
    dashboard.navigate(Navigation::Down);
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("Read failed"));
    assert!(screen.contains("Check the source"));
}

#[test]
fn keeps_last_valid_snapshot_through_bad_data_and_recovers_on_replacement() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(&path, [provider("codex", "", Vec::new())]);
    let mut dashboard = Dashboard::new(true);
    let source = Source::File(path.clone());
    dashboard.refresh(&source);
    fs::write(&path, "{").unwrap();
    dashboard.refresh(&source);
    assert!(render(&dashboard, 80, 24).contains("Stale"));
    assert!(render(&dashboard, 80, 24).contains("› Codex"));

    let replacement = path.with_extension("tmp");
    write_export(&replacement, [provider("gemini", "", Vec::new())]);
    fs::rename(replacement, path).unwrap();
    dashboard.refresh(&source);
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("› Gemini"));
    assert!(!screen.contains("Stale"));
}

#[test]
fn valid_empty_export_clears_previous_readings() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::Demo);
    write_export(&path, []);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("No providers in snapshot"));
    assert!(!screen.contains("› Antigravity"));
}

#[test]
fn unavailable_usage_and_reset_do_not_become_zeroes() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [provider(
            "codex",
            "",
            vec![Window::new("weekly", "Weekly", f64::NAN, None, "weekly")],
        )],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert_eq!(screen.matches("Usage unavailable").count(), 1);
    assert!(screen.contains("Reset unavailable"));
    assert!(!screen.contains("0.0%"));
}

#[test]
fn rejects_directories_as_snapshot_files() {
    let directory = tempdir().unwrap();
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(directory.path().into()));
    assert!(render(&dashboard, 80, 24).contains("expected a regular snapshot file"));
}

#[test]
fn removes_control_characters_from_snapshot_display_text() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [provider(
            "custom\x1b\r-provider",
            "Plan\n name",
            vec![Window::new("weekly", "Week\tly", 10.0, None, "weekly")],
        )],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("custom-provider · Plan name"));
    assert!(screen.contains("Weekly"));
}

fn render(dashboard: &Dashboard, width: u16, height: u16) -> String {
    render_source(dashboard, width, height, "Snapshot")
}

fn render_source(dashboard: &Dashboard, width: u16, height: u16, source: &str) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| dashboard.draw(frame, source)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width))
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn provider(id: &str, plan: &str, windows: Vec<Window>) -> Provider {
    let mut provider = Provider::connected(id, plan.into(), String::new(), String::new(), windows);
    provider.updated = Some(Timestamp::now().as_second() - 330);
    provider
}

fn write_export(path: &Path, providers: impl IntoIterator<Item = Provider>) {
    let providers = providers
        .into_iter()
        .map(|provider| (provider.id.clone(), provider))
        .collect::<BTreeMap<_, _>>();
    fs::write(path, UsageExport::from(&providers).json()).unwrap();
}

#[test]
fn uses_desktop_meter_preferences_and_provider_precision() {
    let mut dashboard = Dashboard::new(true);
    let reset = Timestamp::now() - SignedDuration::from_hours(72);
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"show_as":"remaining","decimals":2}),
        providers: vec![crate::snapshot::Provider {
            id: "codex".into(),
            plan: None,
            fetched_at: None,
            status: "Cached".into(),
            details: vec!["Credit balance  25.00".into()],
            windows: vec![crate::snapshot::Window {
                label: "Weekly".into(),
                used_percent: Some(28.75),
                resets_at: Some(reset),
                reset_by_day: true,
                pace: Some("Expected 35% · on pace".into()),
            }],
        }],
    }));
    let screen = render(&dashboard, 100, 24);
    for label in ["Quota remaining", "71.25%", "Reset was due", "Cached"] {
        assert!(screen.contains(label), "{screen}");
    }
    assert!(!screen.contains("days ago ·"));
    assert!(render(&dashboard, 140, 24).contains("days ago · Expected 35% · on pace"));
    dashboard.show(Page::Detail);
    let screen = render(&dashboard, 100, 24);
    assert!(screen.contains("Credit balance  25.00"));
    assert!(screen.contains("Weekly  Expected 35% · on pace"));
}

#[test]
fn history_navigation_inspects_actual_readings_and_retains_hidden_series() {
    let mut dashboard = Dashboard::new(true);
    assert!(dashboard.visibility(false).is_none());
    dashboard.show(Page::History);
    dashboard.navigate(Navigation::Right);
    assert!(dashboard.visibility(false).is_none());

    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    assert!(render(&dashboard, 100, 24).contains("Latest 78.00"));
    dashboard.navigate(Navigation::Left);
    assert!(render(&dashboard, 100, 24).contains("2026-05-28T20:26:40Z · 33.00"));
    dashboard.navigate(Navigation::Right);
    assert!(render(&dashboard, 100, 24).contains("2026-05-28T20:26:45Z · 78.00"));

    assert_eq!(
        dashboard.visibility(true),
        Some(visibility_command("isolate", "weekly"))
    );
    let mut history = history_fixture();
    history.hidden.insert("daily".into());
    dashboard.apply_output(Page::History, Ok(Output::History(history)));
    assert_eq!(
        dashboard.visibility(true),
        Some(visibility_command("isolate_undo", "weekly"))
    );

    dashboard.navigate(Navigation::Down);
    assert!(render(&dashboard, 100, 24).contains("Series hidden"));
    assert_eq!(dashboard.visibility(false), Some(visibility_command("toggle", "daily")));
    assert_eq!(dashboard.visibility(true), Some(visibility_command("isolate", "daily")));
}

#[test]
fn settings_keyboard_edits_preselect_values_and_handles_text() {
    let mut dashboard = Dashboard::new(true);
    dashboard.edit_setting();
    assert!(!dashboard.editing());
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":false,"format":"custom","template":"a"}),
        providers: Vec::new(),
    }));
    dashboard.show(Page::Settings);
    assert!(render(&dashboard, 80, 24).contains("Account identity"));
    assert_eq!(
        dashboard.edit_setting(),
        Some(Command::Set("hide_identity".into(), "true".into()))
    );
    dashboard.navigate(Navigation::Down);
    dashboard.navigate(Navigation::Down);
    dashboard.edit_setting();
    assert!(render(&dashboard, 80, 24).contains(r#":set template "a""#));
    dashboard.edit(27);
    assert!(!dashboard.editing());

    dashboard.start_command();
    for &byte in "testé".as_bytes() {
        dashboard.edit(byte);
    }
    dashboard.edit(127);
    dashboard.edit(1);
    assert_eq!(dashboard.edit(b'\n').as_deref(), Some("test"));
}

#[test]
fn late_history_response_preserves_the_active_log_position() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::Logs);
    dashboard.navigate(Navigation::NextGroup);
    dashboard.apply_output(
        Page::Logs,
        Ok(Output::Text(
            (0..40).map(|index| format!("Log message {index}")).collect(),
        )),
    );
    for _ in 0..35 {
        dashboard.navigate(Navigation::Down);
    }
    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    dashboard.apply_output(Page::History, Err(std::io::Error::other("Old error")));
    dashboard.apply_output(Page::History, Ok(Output::Done("Old response".into())));
    assert!(render(&dashboard, 80, 24).contains("Log message 35"));
    assert!(!render(&dashboard, 80, 24).contains("Old error"));
    assert!(!render(&dashboard, 80, 24).contains("Old response"));
    dashboard.navigate(Navigation::Right);
    assert!(!render(&dashboard, 80, 24).contains("Log message 35"));
    dashboard.navigate(Navigation::Left);
    assert!(render(&dashboard, 80, 24).contains("Log message 35"));
}

#[test]
fn empty_history_does_not_draw_readings() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::History);
    let mut history = history_fixture();
    history.series.clear();
    history.canonical_total = None;
    history.start = i64::MIN;
    dashboard.apply_output(Page::History, Ok(Output::History(history)));
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("No history in this range"));
    assert!(screen.contains("Unknown date"));
    assert!(dashboard.visibility(true).is_none());
}

fn history_fixture() -> History {
    serde_json::from_value(json!({
        "metric":"usage","start":1_780_000_000_i64,"end":1_780_000_007_i64,
        "canonicalTotal":78.0,"utc":true,
        "series":[
            {"id":"weekly","provider":"Codex","label":"Weekly","points":[[1_780_000_000_i64,33.0,null,false],[1_780_000_005_i64,78.0,null,false],[1_780_000_007_i64,90.0,null,true]]},
            {"id":"daily","provider":"Codex","label":"Daily","points":[[1_780_000_005_i64,76.0,null,false]]}
        ]
    }))
    .unwrap()
}

fn visibility_command(kind: &str, id: &str) -> Command {
    Command::Visibility(json!({"kind":kind,"metric":"usage","id":id,"series":["weekly","daily"]}).to_string())
}

#[test]
fn settings_navigation_changes_numbers_choices_and_provider_intervals() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::Settings);
    dashboard.navigate(Navigation::NextGroup);
    assert!(dashboard.edit_setting().is_none());
    dashboard.apply_snapshot(Ok(State {
        settings: json!({
            "providers":{"cursor":false},"interval_minutes":{"codex":2,"cursor":10},"show_as":"used","decimals":0,"format":"custom","template":"active",
            "notifications":false,"notify_sound":true
        }),
        providers: Vec::new(),
    }));
    assert_eq!(
        dashboard.navigate(Navigation::Right),
        Some(Command::StepInterval("codex".into(), 1))
    );
    assert!(!render(&dashboard, 80, 24).contains("Cursor refresh interval"));

    dashboard.navigate(Navigation::NextGroup);
    assert_eq!(
        dashboard.navigate(Navigation::Left),
        Some(Command::Set("show_as".into(), r#""remaining""#.into()))
    );
    dashboard.navigate(Navigation::Down);
    assert_eq!(
        dashboard.navigate(Navigation::Right),
        Some(Command::Set("decimals".into(), "1".into()))
    );
    assert_eq!(
        dashboard.navigate(Navigation::Left),
        Some(Command::Set("decimals".into(), "0".into()))
    );
    dashboard.navigate(Navigation::Down);
    dashboard.navigate(Navigation::Down);
    assert!(dashboard.navigate(Navigation::Left).is_none());
    assert!(!render(&dashboard, 80, 24).contains("Notification sound"));
    dashboard.navigate(Navigation::NextGroup);
    assert!(render(&dashboard, 80, 24).contains("› Enable desktop notifications"));
    dashboard.navigate(Navigation::NextGroup);
    assert!(render(&dashboard, 80, 24).contains("› Codex refresh interval"));

    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":true}),
        providers: Vec::new(),
    }));
    assert!(render(&dashboard, 80, 24).contains("› Account identity"));
}

#[test]
fn read_only_settings_show_values_without_editing() {
    let mut dashboard = Dashboard::new(false);
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":true}),
        providers: Vec::new(),
    }));
    dashboard.show(Page::Settings);
    assert!(dashboard.navigate(Navigation::Right).is_none());
    assert!(dashboard.edit_setting().is_none());
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("Settings · read only"));
    assert!(screen.contains("Hidden"));
    assert!(screen.contains("Change these settings in the desktop app"));
}

#[test]
fn desktop_title_contains_the_active_page_only() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::History);
    let screen = render_source(&dashboard, 80, 24, "");
    assert_eq!(screen.lines().next().unwrap().trim(), "QUOTALINE  · History");
}

#[test]
fn provider_status_survives_without_an_observation_or_window() {
    let mut dashboard = Dashboard::new(true);
    dashboard.apply_snapshot(Ok(State {
        settings: Value::Null,
        providers: vec![crate::snapshot::Provider {
            id: "codex".into(),
            plan: None,
            fetched_at: None,
            status: "Sign-in needed".into(),
            details: Vec::new(),
            windows: Vec::new(),
        }],
    }));
    assert!(render(&dashboard, 80, 24).contains("Sign-in needed"));
}

#[test]
fn commands_follow_the_active_provider_and_history_range() {
    let mut dashboard = Dashboard::new(true);
    assert!(dashboard.selected_provider().is_none());
    dashboard.refresh(&Source::Demo);
    assert_eq!(dashboard.selected_provider(), Some("antigravity"));

    assert_eq!(
        dashboard.history_shift(Some(false)),
        Command::History(json!({"kind":"page","forward":false}))
    );
    assert_eq!(dashboard.history_shift(None), Command::History(json!({"kind":"now"})));
    assert_eq!(dashboard.page(), Page::History);
}

#[test]
fn command_reference_lists_the_shared_metric_ids() {
    let mut dashboard = Dashboard::new(true);
    assert!(dashboard.parse("metrics").is_none());
    let screen = render(&dashboard, 100, 24);
    assert!(screen.contains("usage · Quota usage"));
    assert!(screen.contains("inputTokens"));
    assert!(dashboard.parse("unknown").is_none());
    assert!(render(&dashboard, 100, 24).contains("Unknown command"));
    assert_eq!(dashboard.parse("refresh all"), Some(Command::Refresh("all".into())));
}

#[test]
fn settings_explains_the_source_when_there_are_no_controls() {
    let mut dashboard = Dashboard::new(false);
    dashboard.show(Page::Settings);
    assert!(render(&dashboard, 80, 24).contains("Open the desktop source to see its settings."));
}

#[test]
fn displays_diagnostics_in_the_result_view() {
    let mut dashboard = Dashboard::new(true);
    dashboard.parse("metrics");
    dashboard.apply_output(Page::Result, Ok(Output::Text(vec!["Diagnostic report".into()])));
    assert!(render(&dashboard, 80, 24).contains("Diagnostic report"));
}

#[test]
fn displays_action_success_in_the_active_view() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::Settings);
    dashboard.apply_output(Page::Settings, Ok(Output::Done("Settings saved".into())));
    assert!(render(&dashboard, 80, 24).contains("Settings saved"));
}

#[test]
fn displays_backend_errors_in_the_active_view() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::History);
    dashboard.apply_output(Page::History, Err(std::io::Error::other("History unavailable")));
    assert!(render(&dashboard, 80, 24).contains("History unavailable"));
}
