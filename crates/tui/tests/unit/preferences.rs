use super::*;
use ratatui::{Terminal, backend::TestBackend};

#[test]
fn step_arrows_wrap_the_display_value_in_terminal_cells() {
    for (width, label, value, gap) in [
        (140, "Quota setting", "界e\u{301}", 6),
        (
            34,
            "A control label long enough to reach the value column",
            "界e\u{301}",
            6,
        ),
        (80, "Empty choice", "", 3),
    ] {
        let controls = [choice(label, value)];
        let mut terminal = Terminal::new(TestBackend::new(width, 12)).unwrap();
        let area = Rect::new(0, 0, width, 12);
        terminal.draw(|frame| draw(frame, area, &controls, 0, true)).unwrap();
        let buffer = terminal.backend().buffer();
        let previous = arrow_column(buffer, "‹");
        let next = arrow_column(buffer, "›");
        assert_eq!(next - previous, gap);
        assert_eq!(buffer.cell((previous + 1, 2)).unwrap().symbol(), " ");
        assert_eq!(buffer.cell((next - 1, 2)).unwrap().symbol(), " ");
        assert_eq!(
            hit(area, &controls, 0, Position::new(previous, 2)),
            Some((0, SettingsHit::Adjust(-1)))
        );
        assert_eq!(
            hit(area, &controls, 0, Position::new(next, 2)),
            Some((0, SettingsHit::Adjust(1)))
        );
        assert_eq!(
            hit(area, &controls, 0, Position::new(width - 2, 2)),
            Some((0, SettingsHit::Focus))
        );
    }
}

#[test]
fn narrow_settings_clip_the_value_before_the_right_arrow() {
    let controls = [choice("Long settings label", "abcdefghijklmnopqrstuvwxyz")];
    let mut terminal = Terminal::new(TestBackend::new(34, 12)).unwrap();
    let area = Rect::new(0, 0, 34, 12);
    terminal.draw(|frame| draw(frame, area, &controls, 0, true)).unwrap();
    let buffer = terminal.backend().buffer();
    let previous = arrow_column(buffer, "‹");
    let next = arrow_column(buffer, "›");
    assert_eq!(buffer.cell((previous + 2, 2)).unwrap().symbol(), "a");
    assert_eq!(buffer.cell((next - 1, 2)).unwrap().symbol(), " ");
    assert_eq!(next, 32);
    assert_eq!(
        hit(area, &controls, 0, Position::new(next, 2)),
        Some((0, SettingsHit::Adjust(1)))
    );
}

fn choice(label: &str, value: &str) -> Control {
    Control {
        group: "Display",
        label: label.into(),
        key: "choice".into(),
        value: json!(value),
        editor: Editor::Choice(&[
            ("界e\u{301}", "界e\u{301}"),
            ("", ""),
            ("abcdefghijklmnopqrstuvwxyz", "abcdefghijklmnopqrstuvwxyz"),
        ]),
    }
}

fn arrow_column(buffer: &ratatui::buffer::Buffer, arrow: &str) -> u16 {
    (1..buffer.area.width - 1)
        .rev()
        .find(|&column| buffer.cell((column, 2)).unwrap().symbol() == arrow)
        .unwrap()
}

#[test]
fn controls_keep_operational_preferences_and_hide_desktop_chrome() {
    let settings = json!({
        "providers":{"codex":true},"interval_minutes":{"codex":5},
        "show_as":"used","decimals":1,"hide_identity":true,"hide_unused_models":true,
        "retention_days":60,"analytics_interval_minutes":15,"workdays":5,
        "allow_token_refresh":true,"demo":false,"history_stacked":false,"detailed_logging":false,
        "order":"stable","hide_zero":true,"show_all_providers":true,
        "format":"custom","template":"unused","fit_to_space":true,"notifications":true,
        "notify_resets":true,"notify_sign_in":true,"notify_pace":true,"notify_expiring_credits":true,
        "notify_sound":true,"write_usage_json":true,
    });
    let mut expected = vec![
        "provider:codex",
        "codex",
        "show_as",
        "decimals",
        "hide_identity",
        "hide_unused_models",
        "retention_days",
        "analytics_interval_minutes",
        "workdays",
        "allow_token_refresh",
        "demo",
        "history_stacked",
        "detailed_logging",
    ];
    let keys = |show_model_filter| {
        controls(&settings, show_model_filter)
            .into_iter()
            .map(|control| control.key)
            .collect::<Vec<_>>()
    };
    assert_eq!(keys(true), expected);
    expected.remove(5);
    assert_eq!(keys(false), expected);
}
