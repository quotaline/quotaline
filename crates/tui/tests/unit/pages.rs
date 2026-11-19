use super::*;

#[test]
fn translates_history_controls_to_backend_actions() {
    for (text, action) in [
        ("period 1", json!({"kind":"period","days":1})),
        ("period 7", json!({"kind":"period","days":7})),
        ("period 30", json!({"kind":"period","days":30})),
        ("period 60", json!({"kind":"period","days":60})),
        (
            "range 2026-01-01 2026-01-07",
            json!({"kind":"custom","from":"2026-01-01","to":"2026-01-07"}),
        ),
        ("metric tokenUsage", json!({"kind":"metric","metric":"tokenUsage"})),
        ("rollup hour", json!({"kind":"rollup","rollup":"hour"})),
        ("zone local", json!({"kind":"utc","utc":false})),
        ("zone utc", json!({"kind":"utc","utc":true})),
    ] {
        assert_eq!(parse(text, None).unwrap(), Command::History(action));
    }
}

#[test]
fn rejects_incomplete_or_unsafe_commands() {
    for (text, error) in [
        ("set", "Use set KEY JSON"),
        ("set hide_identity", "Use set KEY JSON"),
        ("set hide_identity maybe", "expected value"),
        ("period week", "invalid digit"),
        ("period 3", "Period takes 1, 7, 30 or 60 days, not 3"),
        ("interval codex up", "invalid digit"),
        ("threshold up", "invalid digit"),
        ("visibility invalid", "expected value"),
        ("export /tmp/readings", "Open History before exporting a metric"),
        ("export-all report.csv", "Use an absolute export path"),
        ("clear history", "Unknown command"),
    ] {
        assert!(parse(text, None).unwrap_err().to_string().contains(error));
    }
}

#[test]
fn history_filters_and_reordering_preserve_values() {
    assert_eq!(
        parse("reorder model codex:weekly codex:session after", None).unwrap(),
        Command::Reorder("model".into(), "codex:weekly".into(), "codex:session".into(), true),
    );
    assert_eq!(parse("sign-in codex", None).unwrap(), Command::SignIn("codex".into()));
}

#[test]
fn history_draws_each_visible_usage_series_and_highlights_selected_reading() {
    let history = chart_fixture(
        "usage",
        false,
        86_400,
        [
            (
                "week",
                "Weekly",
                json!([
                    [0, 25.0, null, false],
                    [43_200, 75.0, null, false],
                    [86_400, 75.0, null, true]
                ]),
            ),
            (
                "day",
                "Daily",
                json!([[0, 50.0, null, false], [86_400, 50.0, null, false]]),
            ),
        ],
    );
    let buffer = history_buffer(&history, (0, Some(43_200)), (60, 22));
    let selected = Color::Indexed(81);
    let other = Color::Indexed(215);
    assert_eq!((buffer[(20, 13)].fg, buffer[(20, 10)].fg), (selected, other));
    assert_eq!(buffer[(32, 6)].symbol(), "◆");
    assert!(history_rows(&buffer).contains("1970-01-01T12:00:00Z · 75.00%"));

    let buffer = history_buffer(&history, (1, None), (60, 22));
    assert_eq!((buffer[(20, 13)].fg, buffer[(20, 10)].fg), (selected, other));
    assert!(history_rows(&buffer).contains("›● Codex · Daily"));
}

#[test]
fn history_keeps_chart_visible_when_focus_moves_to_hidden_series() {
    let mut history = chart_fixture(
        "usage",
        false,
        86_400,
        [
            (
                "week",
                "Weekly",
                json!([[0, 25.0, null, false], [86_400, 25.0, null, false]]),
            ),
            ("day", "Daily", json!([[43_200, 50.0, null, false]])),
        ],
    );
    history.hidden.insert("day".into());
    let buffer = history_buffer(&history, (1, None), (60, 22));
    assert_eq!(buffer[(20, 13)].fg, Color::Indexed(81));
    assert!(history_rows(&buffer).contains("›○ Codex · Daily"));
    assert!(history_rows(&buffer).contains("Series hidden"));
    assert_eq!(buffer[(32, 10)].fg, Color::DarkGray);

    history.hidden.insert("week".into());
    let buffer = history_buffer(&history, (1, None), (60, 22));
    assert!(history_rows(&buffer).contains("No visible readings in this range"));
}

#[test]
fn history_scrolls_narrow_legend_without_dropping_unlisted_series_from_chart() {
    let history = chart_fixture(
        "usage",
        false,
        86_400,
        [
            (
                "week",
                "Weekly",
                json!([[0, 25.0, null, true], [86_400, 25.0, null, true]]),
            ),
            ("day", "Daily", json!([])),
            ("other", "Other", json!([[43_200, 80.0, null, false]])),
            ("last", "Last", json!([[43_200, 50.0, null, false]])),
        ],
    );
    let buffer = history_buffer(&history, (3, None), (35, 10));
    let rows = history_rows(&buffer);
    assert!(rows.contains("›● Codex · Last"));
    assert!(!rows.contains("Weekly"));
    assert_eq!(buffer[(12, 4)].fg, Color::Indexed(81));
}

#[test]
fn history_can_plot_carried_points_without_offering_a_reading() {
    let mut history = chart_fixture(
        "usage",
        false,
        86_400,
        [(
            "week",
            "Weekly",
            json!([[-43_200, 18.0, null, true], [86_400, 18.0, null, true]]),
        )],
    );
    history.utc = false;
    let buffer = history_buffer(&history, (0, None), (60, 22));
    assert_eq!(buffer[(6, 15)].fg, Color::Indexed(81));
    assert!(history_rows(&buffer).contains("No readings for this series"));
}

#[test]
fn history_distinguishes_grouped_and_stacked_daily_totals() {
    let mut history = chart_fixture(
        "projectCost",
        false,
        259_200,
        [
            (
                "one",
                "Project 1",
                json!([[0, 1250.0, null, false], [86_400, 0.0, null, false]]),
            ),
            ("two", "Project 2", json!([[0, 750.0, null, false]])),
        ],
    );
    history.canonical_total = Some(2000.0);
    history.utc = false;
    let grouped = history_buffer(&history, (0, Some(0)), (60, 22));
    let rows = history_rows(&grouped);
    assert!(rows.contains("Total $2000.00"));
    assert!(rows.contains("$1.4k"));
    assert!(rows.contains("01-01"));
    assert!(rows.contains("01-04"));
    assert!(rows.contains("Project 1"));
    assert!(rows.contains("1970-01-01T00:00:00Z · $1250.00"));
    assert!(rows.contains("◆"));
    let grouped_columns = chart_columns(&grouped);
    assert!(grouped_columns.0.first().unwrap() < grouped_columns.1.first().unwrap());
    assert!(grouped_columns.0.last().unwrap() < grouped_columns.1.last().unwrap());

    history.stacked = true;
    let stacked = history_buffer(&history, (0, None), (60, 22));
    assert!(history_rows(&stacked).contains("$2.2k"));
    let stacked_columns = chart_columns(&stacked);
    assert_eq!(stacked_columns.0, stacked_columns.1);
    assert!(!stacked_columns.0.is_empty());
}

#[test]
fn history_has_one_daily_group_for_each_nonstackable_series() {
    let mut history = chart_fixture(
        "turns",
        true,
        259_200,
        [
            ("one", "Project 1", json!([[0, 25.0, null, false]])),
            ("two", "Project 2", json!([[0, 75.0, null, false]])),
        ],
    );
    let stacked = history_buffer(&history, (0, None), (60, 22));
    history.stacked = false;
    assert_eq!(stacked, history_buffer(&history, (0, None), (60, 22)));
    let columns = chart_columns(&stacked);
    assert!(!columns.0.is_empty());
    assert!(!columns.1.is_empty());
    assert!(columns.0.is_disjoint(&columns.1));
}

#[test]
fn history_joins_analytics_line_points_without_steps() {
    let history = chart_fixture(
        "creditBalance",
        false,
        259_200,
        [(
            "balance",
            "Balance",
            json!([[0, 15.0, null, false], [259_200, 45.0, null, false]]),
        )],
    );
    let buffer = history_buffer(&history, (0, None), (60, 22));
    assert_eq!(buffer[(32, 8)].fg, Color::Indexed(81));
    assert!(history_rows(&buffer).contains("Latest 45.00 credits"));
}

#[test]
fn history_keeps_metric_heading_and_axis_at_minimum_width() {
    let history = chart_fixture(
        "usage",
        false,
        86_400,
        [("week", "Weekly", json!([[0, 70.0, null, false]]))],
    );
    let rows = history_rows(&history_buffer(&history, (0, None), (16, 10)));
    assert!(rows.contains("Usage %"));
    assert!(rows.contains("100%"));
    assert!(rows.contains("00:00"));
}

#[test]
fn history_falls_back_for_an_unknown_metric_and_formats_local_time() {
    let mut history = chart_fixture(
        "custom",
        true,
        86_400,
        [
            ("one", "First", json!([[0, 12.0, null, false]])),
            ("two", "Second", json!([[43_200, 24.0, null, false]])),
        ],
    );
    history.utc = false;
    let buffer = history_buffer(&history, (0, Some(0)), (100, 22));
    let rows = history_rows(&buffer);
    let stamp = Timestamp::from_second(0)
        .unwrap()
        .to_zoned_datetime(TimeZone::system())
        .strftime("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    assert!(rows.contains("History metric"));
    assert!(rows.contains(&format!("{stamp} · 12.00")), "{rows}");
    assert!(rows.contains("›● Codex · First"));
}

#[test]
fn history_export_accepts_only_absolute_paths() {
    let history = chart_fixture("usage", false, 86_400, []);
    assert_eq!(
        parse("export relative.csv", Some(&history)).unwrap_err().to_string(),
        "Use an absolute export path"
    );
    let path = std::env::temp_dir().join("history.csv").display().to_string();
    assert_eq!(
        parse(&format!("export {path}"), Some(&history)).unwrap(),
        Command::ExportMetric("usage".into(), 0, 86_400, path)
    );
}

fn chart_fixture<const COUNT: usize>(
    metric: &str,
    stacked: bool,
    end: i64,
    series: [(&str, &str, Value); COUNT],
) -> History {
    let series = series.map(|(id, label, points)| json!({"id":id,"provider":"Codex","label":label,"points":points}));
    serde_json::from_value(
        json!({"metric":metric,"start":0,"end":end,"utc":true,"stacked":stacked,"series":series.as_slice()}),
    )
    .unwrap()
}

fn history_buffer(
    history: &History,
    position: (usize, Option<i64>),
    (width, height): (u16, u16),
) -> ratatui::buffer::Buffer {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            draw_history(
                frame,
                frame.area(),
                &HistoryView::new(serde_json::from_value(serde_json::to_value(history).unwrap()).unwrap()),
                position,
            )
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn history_rows(buffer: &ratatui::buffer::Buffer) -> String {
    buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn chart_columns(
    buffer: &ratatui::buffer::Buffer,
) -> (std::collections::BTreeSet<u16>, std::collections::BTreeSet<u16>) {
    let plot = (3..16).flat_map(|row| (0..buffer.area.width).map(move |column| (column, &buffer[(column, row)])));
    let columns_with_color = |color| {
        plot.clone()
            .filter(|(_, cell)| cell.fg == Color::Indexed(color))
            .map(|(column, _)| column)
            .collect()
    };
    (columns_with_color(81), columns_with_color(215))
}

#[test]
fn navigation_loads_history_and_logs_from_the_current_source() {
    assert_eq!(Page::History.command(), Some(Command::History(json!({"kind":"load"}))));
    assert_eq!(Page::Logs.command(), Some(Command::Logs(false)));
    assert_eq!(Page::Settings.command(), None);
}

#[test]
fn output_stays_in_the_relevant_view() {
    for (command, page) in [
        (Command::History(json!({"kind":"load"})), Page::History),
        (Command::Logs(true), Page::Logs),
        (Command::Diagnostics, Page::Result),
        (Command::Refresh("all".into()), Page::Settings),
    ] {
        assert_eq!(output_page(&command, Page::Settings), page);
    }
}

#[test]
fn history_legend_uses_last_real_bucket_and_reserves_space_for_values() {
    let history = chart_fixture(
        "projectCost",
        false,
        259_200,
        [
            (
                "one",
                "A long project name that should be clipped",
                json!([
                    [0, 81.02, null, false],
                    [86_400, 31.50, null, false],
                    [172_800, 8000.0, null, true]
                ]),
            ),
            (
                "two",
                "Second",
                json!([[129_600, 50.0, null, false], [172_800, 418.02, null, false]]),
            ),
        ],
    );
    let rows = history_rows(&history_buffer(&history, (0, None), (34, 20)));
    let first = rows.lines().find(|line| line.contains("A long")).unwrap();
    let second = rows.lines().find(|line| line.contains("Second")).unwrap();
    assert!(first.ends_with("$31.50│"), "{first}");
    assert!(second.ends_with("$418.02│"), "{second}");
    assert!(!first.contains("8000"));

    for timestamp in [129_600, 172_800] {
        let rows = history_rows(&history_buffer(&history, (0, Some(timestamp)), (34, 20)));
        let first = rows.lines().find(|line| line.contains("A long")).unwrap();
        assert!(first.ends_with("—│"), "{first}");
    }
}

#[test]
fn history_legend_interpolates_lines_at_the_same_timestamp() {
    let history = chart_fixture(
        "creditBalance",
        false,
        259_200,
        [
            (
                "one",
                "First",
                json!([[0, 10.0, null, false], [259_200, 70.0, null, false]]),
            ),
            ("two", "Second", json!([[86_400, 25.0, null, false]])),
            ("three", "Third", json!([[86_400, 99.0, null, true]])),
            ("four", "Ended", json!([[43_200, 120.0, null, false]])),
        ],
    );
    let rows = history_rows(&history_buffer(&history, (0, Some(86_400)), (80, 24)));
    for (label, value) in [
        ("First", "30.00 credits"),
        ("Second", "25.00 credits"),
        ("Third", "99.00 credits"),
        ("Ended", "—"),
    ] {
        let row = rows.lines().find(|line| line.contains(label)).unwrap();
        assert!(row.contains(value), "{row}");
    }
}
