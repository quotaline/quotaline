use super::*;
use quotaline_export::UsageExport;
use quotaline_quota::{Provider, Window};
use ratatui::{Terminal, backend::TestBackend};
use std::{collections::BTreeMap, fs, path::Path};
use tempfile::tempdir;

#[test]
fn renders_demo_from_the_export() {
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::Demo);
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("QUOTALINE"));
    assert!(screen.contains("› Antigravity"));
    assert!(screen.contains("Gemini weekly"));
    assert!(screen.contains("Observed "));
    assert!(screen.contains("Reset "));
}

#[test]
fn renders_observation_and_meter_from_a_real_export() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(
        &path,
        [provider(
            "codex",
            "Pro",
            vec![Window::new(
                "weekly",
                "Weekly",
                78.5,
                Some("2026-05-29T20:26:40Z".into()),
                "weekly",
            )],
        )],
    );
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    for text in [
        "Codex · Pro",
        "Weekly",
        "78.5%",
        "Observed 2026-05-28T20:26:40Z",
        "Reset 2026-05-29T20:26:40Z",
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
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    for meter in ["░░░░░░░░ 0.0%", "████░░░ 50.0%", "██████ 100.0%"] {
        assert!(screen.contains(meter), "{screen}");
    }
}

#[test]
fn overview_uses_the_most_used_window() {
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
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert!(
        screen
            .lines()
            .any(|line| line.starts_with("│  Long") && line.contains("100.0%")),
        "{screen}"
    );
}

#[test]
fn stacks_panels_below_80_columns() {
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::Demo);
    for (width, height, row) in [(34, 14, 5), (79, 14, 5), (79, 24, 15), (80, 14, 1), (80, 24, 1)] {
        let screen = render(&dashboard, width, height);
        assert_eq!(
            screen.lines().position(|line| line.contains("Quota windows")),
            Some(row),
            "{screen}"
        );
        assert!(screen.contains("Observed "));
    }
}

#[test]
fn guides_users_to_resize_tiny_terminals() {
    let dashboard = Dashboard::new();
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
            (1..=9)
                .map(|number| Window::new("weekly", format!("Window {number}"), 0.0, None, "weekly"))
                .collect(),
        )],
    );
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(path));
    dashboard.navigate(Navigation::TogglePanel);
    for _ in 0..12 {
        dashboard.navigate(Navigation::Down);
    }
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("› Window 9"));
    assert!(screen.contains("Quota windows 5–9 / 9"));
}

#[test]
fn provider_navigation_scrolls_to_keep_selection_visible() {
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::Demo);
    for _ in 0..8 {
        dashboard.navigate(Navigation::Down);
    }
    assert!(render(&dashboard, 40, 16).contains("› Gemini"));
    dashboard.navigate(Navigation::Up);
    assert!(render(&dashboard, 40, 16).contains("› Cursor"));
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
    let mut dashboard = Dashboard::new();
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
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(directory.path().join("usage.json")));
    dashboard.navigate(Navigation::TogglePanel);
    dashboard.navigate(Navigation::Down);
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("Read failed"));
    assert!(screen.contains("Enable Write usage.json"));
}

#[test]
fn keeps_last_valid_snapshot_through_bad_data_and_recovers_on_replacement() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    write_export(&path, [provider("codex", "", Vec::new())]);
    let mut dashboard = Dashboard::new();
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
    let mut dashboard = Dashboard::new();
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
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert_eq!(screen.matches("Usage unavailable").count(), 2);
    assert!(screen.contains("Reset unavailable"));
    assert!(!screen.contains("0.0%"));
}

#[test]
fn rejects_directories_as_snapshot_files() {
    let directory = tempdir().unwrap();
    let mut dashboard = Dashboard::new();
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
    let mut dashboard = Dashboard::new();
    dashboard.refresh(&Source::File(path));
    let screen = render(&dashboard, 80, 24);
    assert!(screen.contains("custom-provider · Plan name"));
    assert!(screen.contains("Weekly"));
}

fn render(dashboard: &Dashboard, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| dashboard.draw(frame, "Snapshot")).unwrap();
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
    provider.updated = Some(1_780_000_000);
    provider
}

fn write_export(path: &Path, providers: impl IntoIterator<Item = Provider>) {
    let providers = providers
        .into_iter()
        .map(|provider| (provider.id.clone(), provider))
        .collect::<BTreeMap<_, _>>();
    fs::write(path, UsageExport::from(&providers).json()).unwrap();
}
