use super::*;
use crate::desktop::History;
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
        dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
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
        dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
    }
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 40, 16).contains("› Gemini 2.5 Pro"));
    dashboard.navigate(Navigation::Up, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
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
    dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
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
    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
    dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
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
    let mut dashboard = Dashboard::new(false);
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
    render_buffer(dashboard, width, height, source)
        .content
        .chunks(usize::from(width))
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_buffer(dashboard: &Dashboard, width: u16, height: u16, source: &str) -> ratatui::buffer::Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| dashboard.draw(frame, source)).unwrap();
    terminal.backend().buffer().clone()
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
            details: vec!["Secondary account detail".into()],
            overview: vec!["Credits · $25.00 remaining".into()],
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
    assert!(screen.contains("Secondary account detail"));
    assert!(screen.contains("Weekly  Expected 35% · on pace"));
}

#[test]
fn history_navigation_inspects_actual_readings_and_retains_hidden_series() {
    let mut dashboard = Dashboard::new(true);
    assert!(dashboard.visibility(false).is_none());
    dashboard.show(Page::History);
    dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24));
    assert!(dashboard.visibility(false).is_none());

    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    assert!(render(&dashboard, 100, 24).contains("Latest 78.00"));
    dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 100, 24).contains("2026-05-28T20:26:45Z · 78.00"));
    dashboard.show(Page::History);
    dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 100, 24).contains("2026-05-28T20:26:40Z · 33.00"));
    dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24));
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

    dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 100, 24).contains("Series hidden"));
    assert_eq!(dashboard.visibility(false), Some(visibility_command("toggle", "daily")));
    assert_eq!(dashboard.visibility(true), Some(visibility_command("isolate", "daily")));
}

#[test]
fn settings_keyboard_activates_controls_and_keeps_the_command_editor() {
    let mut dashboard = Dashboard::new(true);
    dashboard.edit_setting();
    assert!(!dashboard.editing());
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":false}),
        providers: Vec::new(),
    }));
    dashboard.show(Page::Settings);
    assert!(render(&dashboard, 80, 24).contains("Account identity"));
    assert_eq!(
        dashboard.edit_setting(),
        Some(Command::Set("hide_identity".into(), "true".into()))
    );
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
    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
    dashboard.apply_output(
        Page::Logs,
        Ok(Output::Text(
            (0..40).map(|index| format!("Log message {index}")).collect(),
        )),
    );
    for _ in 0..35 {
        dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
    }
    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    dashboard.apply_output(Page::History, Err(std::io::Error::other("Old error")));
    dashboard.apply_output(Page::History, Ok(Output::Done("Old response".into())));
    assert!(render(&dashboard, 80, 24).contains("Log message 35"));
    assert!(!render(&dashboard, 80, 24).contains("Old error"));
    assert!(!render(&dashboard, 80, 24).contains("Old response"));
    dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24));
    assert!(!render(&dashboard, 80, 24).contains("Log message 35"));
    dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24));
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
    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
    assert!(dashboard.edit_setting().is_none());
    dashboard.apply_snapshot(Ok(State {
        settings: json!({
            "providers":{"cursor":false},"interval_minutes":{"codex":2,"cursor":10},"show_as":"used","decimals":0,"history_stacked":false,
            "notifications":false,"notify_sound":true
        }),
        providers: Vec::new(),
    }));
    assert_eq!(
        dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24)),
        Some(Command::StepInterval("codex".into(), 1))
    );
    assert!(!render(&dashboard, 80, 24).contains("Cursor refresh interval"));

    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
    assert_eq!(
        dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24)),
        Some(Command::Set("show_as".into(), r#""remaining""#.into()))
    );
    dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
    assert_eq!(
        dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24)),
        Some(Command::Set("decimals".into(), "1".into()))
    );
    assert_eq!(
        dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24)),
        Some(Command::Set("decimals".into(), "0".into()))
    );
    assert!(!render(&dashboard, 80, 24).contains("Notification sound"));
    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 80, 24).contains("› Stack compatible history series"));
    dashboard.navigate(Navigation::NextGroup, Rect::new(0, 0, 80, 24));
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
    assert!(dashboard.navigate(Navigation::Right, Rect::new(0, 0, 80, 24)).is_none());
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
            overview: Vec::new(),
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
    assert!(
        render(&dashboard, 80, 24).contains("Settings need live data: run quotaline-tui without --demo or --file.")
    );
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

#[test]
fn history_mouse_inspects_the_plot_and_restores_latest_on_exit() {
    let mut dashboard = Dashboard::new(true);
    let area = Rect::new(0, 0, 100, 24);
    dashboard.show(Page::History);
    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    assert!(render(&dashboard, 100, 24).contains("Latest 78.00%"));
    assert!(dashboard.mouse(MouseEvent::Move(8, 7), area).is_none());
    let screen = render(&dashboard, 100, 24);
    assert!(screen.contains("2026-05-28T20:26:40Z · 33.00%"));
    assert!(
        screen
            .lines()
            .find(|line| line.contains("Daily"))
            .unwrap()
            .contains("—")
    );

    assert!(dashboard.mouse(MouseEvent::Click(49, 7), area).is_none());
    assert!(render(&dashboard, 100, 24).contains("2026-05-28T20:26:45Z · 78.00%"));
    assert!(dashboard.mouse(MouseEvent::Move(8, 21), area).is_none());
    assert!(render(&dashboard, 100, 24).contains("Latest 78.00%"));

    dashboard.mouse(MouseEvent::Move(8, 7), area);
    dashboard.mouse(MouseEvent::Leave, Rect::new(0, 0, 20, 10));
    assert!(render(&dashboard, 100, 24).contains("Latest 78.00%"));
}

#[test]
fn history_mouse_only_toggles_legend_rows_on_click() {
    let mut dashboard = Dashboard::new(true);
    let area = Rect::new(0, 0, 100, 24);
    let click = MouseEvent::Click(79, 5);
    assert!(dashboard.mouse(click, area).is_none());
    dashboard.show(Page::History);
    assert!(dashboard.mouse(click, area).is_none());
    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    assert!(dashboard.mouse(click, Rect::new(0, 0, 100, 12)).is_none());
    assert!(dashboard.mouse(click, Rect::new(0, 0, 20, 24)).is_none());
    dashboard.start_command();
    assert!(dashboard.mouse(click, area).is_none());
    dashboard.edit(27);
    assert!(dashboard.mouse(MouseEvent::Click(79, 7), area).is_none());
    assert!(dashboard.mouse(MouseEvent::Move(79, 5), area).is_none());
    assert_eq!(
        dashboard.visibility(false),
        Some(visibility_command("toggle", "weekly"))
    );
    assert_eq!(
        dashboard.mouse(click, area),
        Some(MouseAction::Command(visibility_command("toggle", "daily")))
    );
    assert!(render(&dashboard, 100, 24).contains("›● Codex · Daily"));
}

#[test]
fn history_mouse_leave_preserves_keyboard_reading() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::History);
    let mut history = history_fixture();
    history.series[1].points.insert(0, (1_780_000_003, 45.0, None, false));
    dashboard.apply_output(Page::History, Ok(Output::History(history)));
    dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24));
    let area = Rect::new(0, 0, 100, 24);
    assert!(dashboard.mouse(MouseEvent::Leave, area).is_none());
    let screen = render(&dashboard, 100, 24);
    assert!(screen.contains("2026-05-28T20:26:43Z · 33.00%"));
    assert!(
        screen
            .lines()
            .find(|line| line.contains("Daily"))
            .unwrap()
            .contains("45.00%")
    );
}

#[test]
fn history_mouse_uses_the_scrolled_legend_after_resize() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::History);
    let mut history = history_fixture();
    history.series = (0..9)
        .map(|index| crate::desktop::Series {
            id: format!("model-{index}"),
            provider: "Codex".into(),
            label: format!("Model {index}"),
            points: vec![(1_780_000_000, f64::from(index), None, false)],
        })
        .collect();
    dashboard.apply_output(Page::History, Ok(Output::History(history)));
    for _ in 0..8 {
        dashboard.navigate(Navigation::Down, Rect::new(0, 0, 80, 24));
    }
    for (width, column, row, expected) in [(34, 26, 7, "model-6"), (80, 77, 4, "model-3")] {
        let command = dashboard.mouse(MouseEvent::Click(column, row), Rect::new(0, 0, width, 14));
        let action = command.unwrap();
        assert_eq!(
            action,
            MouseAction::Command(Command::Visibility(
                json!({"kind":"toggle","metric":"usage","id":expected,"series":(0..9).map(|index| format!("model-{index}")).collect::<Vec<_>>()})
                    .to_string(),
            ))
        );
    }
}

#[test]
fn history_reload_drops_selection_when_the_reading_disappears() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::History);
    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    dashboard.navigate(Navigation::Left, Rect::new(0, 0, 80, 24));
    assert!(render(&dashboard, 100, 24).contains("2026-05-28T20:26:40Z"));
    let mut history = history_fixture();
    history.series[0].points.remove(0);
    dashboard.apply_output(Page::History, Ok(Output::History(history)));
    assert!(render(&dashboard, 100, 24).contains("Latest 78.00%"));
}

#[test]
fn old_exports_hide_only_copilots_epoch_reset() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [
            provider(
                "copilot",
                "",
                vec![
                    Window::new(
                        "premium",
                        "Premium",
                        40.0,
                        Some("1970-01-01T00:00:00+00:00".into()),
                        "monthly",
                    ),
                    Window::new(
                        "older",
                        "Older reading",
                        70.0,
                        Some("1970-01-02T00:00:00Z".into()),
                        "monthly",
                    ),
                ],
            ),
            provider(
                "codex",
                "",
                vec![Window::new(
                    "weekly",
                    "Weekly",
                    50.0,
                    Some("1970-01-01T00:00:00Z".into()),
                    "weekly",
                )],
            ),
        ],
    );
    let mut dashboard = Dashboard::new(true);
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 100, 24);
    let premium = screen.lines().find(|line| line.contains("Premium")).unwrap();
    assert!(premium.contains("40.0%"));
    assert!(premium.contains("Reset unavailable"));
    for label in ["Weekly", "Older reading"] {
        assert!(
            screen
                .lines()
                .find(|line| line.contains(label))
                .unwrap()
                .contains("Reset was due")
        );
    }
}

#[test]
fn mouse_footer_uses_visible_links_at_minimum_and_full_width() {
    let mut dashboard = Dashboard::new(true);
    for (width, quit) in [(34, "q"), (80, "Quit")] {
        let size = (width, 14);
        for (label, page) in [
            ("1 Us", Page::Usage),
            ("2 Hist", Page::History),
            ("3 Set", Page::Settings),
            ("4 Log", Page::Logs),
            ("?", Page::Help),
        ] {
            assert_eq!(click_footer(&mut dashboard, size, label), Some(MouseAction::Show(page)));
        }
        assert_eq!(click_footer(&mut dashboard, size, quit), Some(MouseAction::Quit));
        assert!(click_footer(&mut dashboard, size, ":").is_none());
        dashboard.edit(b'x');
        assert!(render(&dashboard, width, 14).contains(":x"));
        dashboard.edit(27);
        assert!(
            dashboard
                .mouse(MouseEvent::Click(width - 1, 13), Rect::new(0, 0, width, 14))
                .is_none()
        );
    }
}

#[test]
fn mouse_footer_highlights_the_active_destination() {
    let mut dashboard = Dashboard::new(true);
    dashboard.show(Page::Logs);
    let buffer = render_buffer(&dashboard, 80, 14, "Snapshot");
    use ratatui::style::{Color, Modifier};
    assert_eq!(buffer[(35, 13)].fg, Color::Cyan);
    assert!(buffer[(35, 13)].modifier.contains(Modifier::BOLD));
    assert!(buffer[(11, 13)].modifier.contains(Modifier::DIM));
}

#[test]
fn mouse_usage_opens_the_detail_for_the_clicked_provider_or_window() {
    let mut dashboard = Dashboard::new(true);
    let size = (45, 16);
    let untouched = render(&dashboard, size.0, size.1);
    assert!(
        dashboard
            .mouse(MouseEvent::Move(3, 3), Rect::new(0, 0, 45, 16))
            .is_none()
    );
    assert_eq!(render(&dashboard, size.0, size.1), untouched);
    assert!(
        dashboard
            .mouse(MouseEvent::Click(2, 3), Rect::new(0, 0, 45, 16))
            .is_none()
    );
    dashboard.apply_snapshot(Ok(State {
        providers: vec![
            crate::snapshot::Provider {
                id: "codex".into(),
                plan: None,
                fetched_at: None,
                status: "Cached".into(),
                details: vec!["Codex account".into()],
                overview: Vec::new(),
                windows: vec![
                    crate::snapshot::Window {
                        label: "Weekly".into(),
                        used_percent: Some(31.0),
                        resets_at: None,
                        reset_by_day: false,
                        pace: None,
                    },
                    crate::snapshot::Window {
                        label: "Session".into(),
                        used_percent: None,
                        resets_at: None,
                        reset_by_day: false,
                        pace: None,
                    },
                ],
            },
            crate::snapshot::Provider {
                id: "gemini".into(),
                plan: None,
                fetched_at: None,
                status: "Ready".into(),
                details: vec!["Gemini account".into()],
                overview: Vec::new(),
                windows: vec![],
            },
        ],
        settings: json!({"show_as":"remaining"}),
    }));
    let area = Rect::new(0, 0, 45, 16);
    for (target, detail) in [
        ("Weekly", "Weekly · 69.0% remaining"),
        ("Session", "Session · Usage unavailable"),
        ("Gemini", "Gemini account"),
    ] {
        let at = screen_position(&render(&dashboard, size.0, size.1), target);
        assert_eq!(
            dashboard.mouse(MouseEvent::Click(at.x, at.y + 1), area),
            Some(MouseAction::Show(Page::Detail))
        );
        dashboard.show(Page::Detail);
        assert!(render(&dashboard, size.0, size.1).contains(detail));
        dashboard.show(Page::Usage);
    }
}

#[test]
fn mouse_settings_toggles_a_switch_from_its_row() {
    let mut dashboard = Dashboard::new(true);
    let size = (80, 24);
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":false}),
        providers: vec![],
    }));
    dashboard.show(Page::Settings);
    assert!(click_label(&mut dashboard, size, "Display").is_none());
    assert_eq!(
        click_label(&mut dashboard, size, "Account identity"),
        Some(MouseAction::Command(Command::Set(
            "hide_identity".into(),
            "true".into()
        )))
    );
}

#[test]
fn mouse_settings_uses_visible_step_arrows_in_both_directions() {
    let mut dashboard = Dashboard::new(true);
    let size = (80, 24);
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"decimals":1,"show_as":"used"}),
        providers: vec![],
    }));
    dashboard.show(Page::Settings);
    for (label, key, previous, next) in [
        ("Quota decimal places", "decimals", "0", "2"),
        ("Quota numbers", "show_as", r#""remaining""#, r#""remaining""#),
    ] {
        assert!(click_label(&mut dashboard, size, label).is_none());
        let screen = render(&dashboard, size.0, size.1);
        let at = screen_position(&screen, label);
        let row = screen.lines().nth(usize::from(at.y)).unwrap();
        for (arrow, value) in [('‹', previous), ('›', next)] {
            let column = row[..row.rfind(arrow).unwrap()].chars().count() as u16;
            assert_eq!(
                dashboard.mouse(MouseEvent::Click(column, at.y), Rect::new(0, 0, size.0, size.1)),
                Some(MouseAction::Command(Command::Set(key.into(), value.into())))
            );
        }
    }
}

#[test]
fn mouse_settings_read_only_controls_have_no_arrows_or_actions() {
    let mut dashboard = Dashboard::new(false);
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":false,"decimals":1}),
        providers: vec![],
    }));
    dashboard.show(Page::Settings);
    let screen = render(&dashboard, 80, 24);
    assert!(!screen.contains('‹'));
    assert!(click_label(&mut dashboard, (80, 24), "Account identity").is_none());
    assert!(!dashboard.editing());
}

#[test]
fn mouse_vertical_wheel_moves_through_every_page_and_horizontal_wheel_only_pans_text() {
    let mut dashboard = Dashboard::new(true);
    let area = Rect::new(0, 0, 80, 15);
    dashboard.apply_snapshot(Ok(State {
        settings: json!({"hide_identity":false,"decimals":1}),
        providers: vec![crate::snapshot::Provider {
            id: "codex".into(),
            plan: None,
            fetched_at: None,
            status: "Ready".into(),
            details: vec!["Account detail".into()],
            overview: Vec::new(),
            windows: vec![crate::snapshot::Window {
                label: "Session".into(),
                used_percent: None,
                resets_at: None,
                reset_by_day: false,
                pace: None,
            }],
        }],
    }));
    dashboard.apply_output(Page::History, Ok(Output::History(history_fixture())));
    let lines = vec!["First entry".into(), "Second entry".into()];
    dashboard.apply_output(Page::Logs, Ok(Output::Text(lines.clone())));
    dashboard.apply_output(Page::Result, Ok(Output::Text(lines)));
    for page in [
        Page::Usage,
        Page::History,
        Page::Settings,
        Page::Logs,
        Page::Help,
        Page::Result,
        Page::Detail,
    ] {
        dashboard.show(page);
        let before = render_buffer(&dashboard, 80, 15, "Snapshot");
        assert!(
            dashboard
                .mouse(MouseEvent::Scroll(5, 5, Navigation::Down), area)
                .is_none()
        );
        assert_ne!(render_buffer(&dashboard, 80, 15, "Snapshot"), before, "{page:?}");
        assert!(
            dashboard
                .mouse(MouseEvent::Scroll(5, 5, Navigation::Up), area)
                .is_none()
        );
        assert_eq!(render_buffer(&dashboard, 80, 15, "Snapshot"), before, "{page:?}");
    }

    dashboard.show(Page::Logs);
    let before = render(&dashboard, 80, 15);
    assert!(
        dashboard
            .mouse(MouseEvent::Scroll(5, 5, Navigation::Right), area)
            .is_none()
    );
    assert!(!render(&dashboard, 80, 15).contains("First entry"));
    assert!(
        dashboard
            .mouse(MouseEvent::Scroll(5, 5, Navigation::Left), area)
            .is_none()
    );
    assert_eq!(render(&dashboard, 80, 15), before);
    dashboard.show(Page::Settings);
    let before = render(&dashboard, 80, 15);
    assert!(
        dashboard
            .mouse(MouseEvent::Scroll(5, 5, Navigation::Right), area)
            .is_none()
    );
    assert_eq!(render(&dashboard, 80, 15), before);
    assert!(dashboard.mouse(MouseEvent::Move(5, 30), area).is_none());
}

#[test]
fn mouse_usage_detail_shows_the_percentage_used_by_default() {
    let mut dashboard = Dashboard::new(true);
    dashboard.apply_snapshot(Ok(State {
        providers: vec![
            serde_json::from_value(json!({
                "id":"codex",
                "windows":[{"label":"Weekly","usedPercent":31.0,"resetsAt":null}]
            }))
            .unwrap(),
        ],
        settings: Value::Null,
    }));
    assert_eq!(
        click_label(&mut dashboard, (80, 24), "Weekly"),
        Some(MouseAction::Show(Page::Detail))
    );
    dashboard.show(Page::Detail);
    assert!(render(&dashboard, 80, 24).contains("Weekly · 31.0% used"));
}

fn click_label(dashboard: &mut Dashboard, (width, height): (u16, u16), label: &str) -> Option<MouseAction> {
    let at = screen_position(&render(dashboard, width, height), label);
    dashboard.mouse(MouseEvent::Click(at.x, at.y), Rect::new(0, 0, width, height))
}

/// Clicks a footer link, which the page above may also mention.
fn click_footer(dashboard: &mut Dashboard, (width, height): (u16, u16), label: &str) -> Option<MouseAction> {
    let screen = render(dashboard, width, height);
    let footer = screen.lines().last().unwrap();
    let column = footer[..footer.find(label).expect(label)].chars().count() as u16;
    dashboard.mouse(MouseEvent::Click(column, height - 1), Rect::new(0, 0, width, height))
}

fn screen_position(screen: &str, label: &str) -> Position {
    let (row, column) = screen
        .lines()
        .enumerate()
        .find_map(|(row, line)| line.find(label).map(|start| (row, line[..start].chars().count())))
        .expect(label);
    Position::new(column as u16, row as u16)
}

#[test]
fn mouse_command_editor_runs_and_cancels_drafts_at_minimum_width() {
    let mut dashboard = Dashboard::new(true);
    let size = (34, 14);
    let area = Rect::new(0, 0, size.0, size.1);
    assert!(click_footer(&mut dashboard, size, ":").is_none());
    for byte in b"logs full" {
        dashboard.edit(*byte);
    }
    let draft = render(&dashboard, size.0, size.1);
    for event in [
        MouseEvent::Click(3, 13),
        MouseEvent::Click(3, 5),
        MouseEvent::Scroll(3, 5, Navigation::Down),
    ] {
        assert!(dashboard.mouse(event, area).is_none());
        assert_eq!(render(&dashboard, size.0, size.1), draft);
    }
    assert_eq!(
        click_footer(&mut dashboard, size, "Run"),
        Some(MouseAction::Command(Command::Logs(true)))
    );
    assert!(!dashboard.editing());

    assert!(click_footer(&mut dashboard, size, ":").is_none());
    dashboard.edit(b'x');
    assert!(click_footer(&mut dashboard, size, "Cancel").is_none());
    assert!(!render(&dashboard, size.0, size.1).contains(":x"));
}

#[test]
fn usage_packs_provider_facts_and_keeps_narrow_values_visible() {
    let mut dashboard = Dashboard::new(true);
    dashboard.apply_snapshot(Ok(State {
        providers: vec![crate::snapshot::Provider {
            id: "codex".into(),
            plan: None,
            fetched_at: None,
            status: String::new(),
            details: vec!["Private account detail".into()],
            overview: vec![
                "Credits · $42.50 remaining".into(),
                "Limit resets · 1 available".into(),
                "Spend control · $27.00 of $120.00".into(),
                " ".into(),
                "Quota limit reached".into(),
            ],
            windows: Vec::new(),
        }],
        settings: Value::Null,
    }));
    let wide = render(&dashboard, 112, 14);
    let fact = wide.lines().find(|line| line.contains("$42.50")).unwrap();
    assert!(fact.contains("Limit resets · 1 available"));
    for label in ["$27.00", "$120.00", "Quota limit reached"] {
        assert!(wide.contains(label));
    }
    assert!(!wide.contains("Private account detail"));
    assert!(!wide.contains("No quota windows reported"));

    let narrow = render(&dashboard, 34, 14);
    assert!(narrow.contains("Credits · $42.50 remaining"));
    assert!(narrow.contains("Limit resets · 1 available"));
    assert!(narrow.contains("$120.00"));
    assert_eq!(
        click_label(&mut dashboard, (34, 14), "$120.00"),
        Some(MouseAction::Show(Page::Detail))
    );
    dashboard.show(Page::Detail);
    assert!(render(&dashboard, 34, 14).contains("Private account detail"));
}

#[test]
fn usage_scrolls_a_single_long_overview_and_keeps_fact_hits_aligned() {
    let mut dashboard = Dashboard::new(true);
    let narrow = Rect::new(0, 0, 34, 14);
    let wide = Rect::new(0, 0, 112, 14);
    dashboard.apply_snapshot(Ok(State {
        providers: vec![crate::snapshot::Provider {
            id: "codex".into(),
            plan: None,
            fetched_at: None,
            status: "Cache ready".into(),
            details: vec!["Secondary detail".into()],
            overview: vec![
                "Credits · $300.00".into(),
                "Limit resets · 2 available".into(),
                format!("Provider notice · {} LAST-FACT", "quota warning ".repeat(30)),
            ],
            windows: Vec::new(),
        }],
        settings: Value::Null,
    }));
    assert!(!render(&dashboard, 34, 14).contains("LAST-FACT"));
    for _ in 0..22 {
        dashboard.navigate(Navigation::Down, narrow);
    }
    let screen = render(&dashboard, 34, 14);
    assert!(screen.contains("LAST-FACT"), "{screen}");
    dashboard.mouse(MouseEvent::Scroll(4, 6, Navigation::Up), wide);
    assert!(render(&dashboard, 112, 14).contains("LAST-FACT"));
    assert_eq!(
        click_label(&mut dashboard, (112, 14), "LAST-FACT"),
        Some(MouseAction::Show(Page::Detail))
    );
    dashboard.show(Page::Detail);
    assert!(render(&dashboard, 112, 14).contains("Secondary detail"));
}

#[test]
fn usage_refresh_preserves_the_selected_window_when_windows_reorder() {
    let mut dashboard = Dashboard::new(true);
    let area = Rect::new(0, 0, 80, 24);
    let state = |windows: [&str; 2]| State {
        providers: vec![
            serde_json::from_value(json!({
                "id":"codex",
                "windows":windows.map(|label| json!({"label":label,"usedPercent":31.0}))
            }))
            .unwrap(),
        ],
        settings: Value::Null,
    };
    dashboard.apply_snapshot(Ok(state(["Weekly", "Session"])));
    dashboard.navigate(Navigation::Down, area);
    assert!(render(&dashboard, 80, 24).contains("› Weekly"));
    dashboard.apply_snapshot(Ok(state(["Session", "Weekly"])));
    assert!(render(&dashboard, 80, 24).contains("› Weekly"));
    dashboard.show(Page::Detail);
    assert!(render(&dashboard, 80, 24).contains("Weekly · 31.0% used"));
}

#[test]
fn usage_refresh_returns_to_provider_when_its_last_overview_fact_disappears() {
    let mut dashboard = Dashboard::new(true);
    let area = Rect::new(0, 0, 80, 24);
    let state = |overview: Vec<String>| State {
        providers: vec![crate::snapshot::Provider {
            id: "codex".into(),
            plan: None,
            fetched_at: None,
            status: String::new(),
            details: Vec::new(),
            overview,
            windows: Vec::new(),
        }],
        settings: Value::Null,
    };
    dashboard.apply_snapshot(Ok(state(vec!["Credits · $25.00".into()])));
    dashboard.navigate(Navigation::Down, area);
    assert!(render(&dashboard, 80, 24).contains("› Credits · $25.00"));
    dashboard.apply_snapshot(Ok(state(vec!["Credits · $20.00".into()])));
    assert!(render(&dashboard, 80, 24).contains("› Credits · $20.00"));
    dashboard.apply_snapshot(Ok(state(Vec::new())));
    assert!(render(&dashboard, 80, 24).contains("› Codex"));
    assert!(render(&dashboard, 80, 24).contains("No quota windows reported"));
}
