//! Presents terminal views of desktop history, preferences, and diagnostics.

use std::{collections::BTreeMap, io, path::Path};

use jiff::{Timestamp, tz::TimeZone};
use quotaline_analytics::{MetricDefinition, metric_definitions};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span},
    widgets::{
        Block, Paragraph,
        canvas::{Canvas, Context, FilledLine, Line as PlotLine, Points},
    },
};
use serde_json::{Value, json};

use crate::desktop::{Command, History, Series};

#[derive(Default, Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Page {
    #[default]
    Usage,
    History,
    Settings,
    Logs,
    Help,
    Detail,
    Result,
}

impl Page {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Self::Usage => "Usage",
            Self::History => "History",
            Self::Settings => "Settings",
            Self::Logs => "Logs",
            Self::Help => "Help",
            Self::Detail => "Provider details",
            Self::Result => "Service response",
        }
    }

    pub(crate) fn command(self) -> Option<Command> {
        match self {
            Self::History => Some(Command::History(json!({"kind":"load"}))),
            Self::Logs => Some(Command::Logs(false)),
            _ => None,
        }
    }
}

pub(crate) fn output_page(command: &Command, current: Page) -> Page {
    match command {
        Command::History(_) | Command::Visibility(_) => Page::History,
        Command::Logs(_) => Page::Logs,
        Command::Diagnostics | Command::Models(_) | Command::Spend | Command::Reviews | Command::IssueUrl => {
            Page::Result
        }
        _ => current,
    }
}

pub(crate) fn help() -> Vec<String> {
    [
        "1 Usage   2 History   3 Settings   4 Logs   ? Help",
        "↑↓ / j k Move or scroll   Tab Next provider or setting group",
        "r Reload cached state   R Request provider refresh",
        "d Provider details   s Open provider sign-in   q Quit",
        "[ ] Page history   n Return to the current range",
        "← → Adjust a setting, inspect readings, or pan text",
        "v Toggle chart series   i Isolate or restore chart series",
        "Enter Edit the selected setting",
        ": Enter a command   Enter Run   Esc Cancel",
        "",
        "History commands",
        "period DAYS             1, 7, 30 or 60",
        "range FROM TO           ISO dates, YYYY-MM-DD",
        "metric ID               Use :metrics to list IDs",
        "rollup minute|hour|day  Set usage chart grouping",
        "zone utc|local          Select history time zone",
        "visibility JSON         Toggle or isolate history series",
        "export PATH             Export the current history metric",
        "export-all PATH         Export all desktop history",
        "",
        "Live commands",
        "refresh ID|all          Request a refresh",
        "sign-in ID              Open the provider’s sign-in",
        "set KEY JSON            Change a key from Settings",
        "set provider:ID true    Enable a provider",
        "models [QUERY]          Show model selection",
        "toggle-model KEY        Toggle a quota window",
        "toggle-provider ID      Toggle all its quota windows",
        "interval ID -1|1        Step its refresh interval",
        "threshold PERCENT       Toggle a notification threshold",
        "reorder KIND FROM TO before|after",
        "spend / reviews         Show analytics summaries",
        "diagnostics / issue / logs full",
        "clear logs confirm / clear history confirm",
        "reset settings confirm",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

pub(crate) fn parse(text: &str, history: Option<&History>) -> io::Result<Command> {
    let text = text.trim();
    let (verb, arguments) = text.split_once(' ').unwrap_or((text, ""));
    let arguments = arguments.trim();
    let words = arguments.split_whitespace().collect::<Vec<_>>();
    let numbers = || arguments.parse::<i32>().map_err(io::Error::other);
    match (verb, words.as_slice()) {
        ("period", [_]) => Ok(Command::History(json!({"kind":"period", "days":numbers()?}))),
        ("range", [from, to]) => Ok(Command::History(json!({"kind":"custom", "from":from, "to":to}))),
        ("metric", [metric]) => Ok(Command::History(json!({"kind":"metric", "metric":metric}))),
        ("rollup", [rollup]) => Ok(Command::History(json!({"kind":"rollup", "rollup":rollup}))),
        ("zone", [zone @ ("utc" | "local")]) => Ok(Command::History(json!({"kind":"utc", "utc":*zone == "utc"}))),
        ("visibility", _) => {
            let action = serde_json::from_str::<Value>(arguments)?;
            Ok(Command::Visibility(action.to_string()))
        }
        ("refresh", [provider]) => Ok(Command::Refresh((*provider).into())),
        ("sign-in", [provider]) => Ok(Command::SignIn((*provider).into())),
        ("set", _) => {
            let (key, value) = arguments
                .split_once(' ')
                .ok_or_else(|| io::Error::other("Use set KEY JSON"))?;
            serde_json::from_str::<Value>(value)?;
            Ok(Command::Set(key.into(), value.into()))
        }
        ("models", _) => Ok(Command::Models(arguments.into())),
        ("toggle-model", [key]) => Ok(Command::ToggleModel((*key).into())),
        ("toggle-provider", [id]) => Ok(Command::ToggleProviderModels((*id).into())),
        ("interval", [id, delta]) => Ok(Command::StepInterval(
            (*id).into(),
            delta.parse().map_err(io::Error::other)?,
        )),
        ("threshold", [_]) => Ok(Command::ToggleThreshold(numbers()?)),
        ("reorder", [kind, from, to, position @ ("before" | "after")]) => Ok(Command::Reorder(
            (*kind).into(),
            (*from).into(),
            (*to).into(),
            *position == "after",
        )),
        ("spend", []) => Ok(Command::Spend),
        ("reviews", []) => Ok(Command::Reviews),
        ("diagnostics", []) => Ok(Command::Diagnostics),
        ("logs", ["full"]) => Ok(Command::Logs(true)),
        ("issue", []) => Ok(Command::IssueUrl),
        ("export-all", _) => Ok(Command::ExportAll(export_path(arguments)?)),
        ("export", _) => {
            let history = history.ok_or_else(|| io::Error::other("Open History before exporting a metric"))?;
            Ok(Command::ExportMetric(
                history.metric.clone(),
                history.start,
                history.end,
                export_path(arguments)?,
            ))
        }
        ("clear", ["logs", "confirm"]) => Ok(Command::ClearLogs),
        ("clear", ["history", "confirm"]) => Ok(Command::ClearHistory),
        ("reset", ["settings", "confirm"]) => Ok(Command::ResetSettings),
        _ => Err(io::Error::other("Unknown command. Press ? for help.")),
    }
}

fn export_path(path: &str) -> io::Result<String> {
    if !Path::new(path).is_absolute() {
        return Err(io::Error::other("Use an absolute export path"));
    }
    Ok(path.into())
}

pub(crate) fn draw_text(frame: &mut Frame<'_>, area: Rect, page: Page, lines: &[String], position: (usize, u16)) {
    let block = Block::bordered().title(format!(" {} ", page.title())).cyan();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if lines.is_empty() {
        frame.render_widget(Line::from("No entries").dim(), inner);
        return;
    }
    let (selected, x) = position;
    let start = selected.saturating_sub(usize::from(inner.height) / 2);
    for (offset, text) in lines.iter().skip(start).take(inner.height.into()).enumerate() {
        let line = Line::from(text.replace(char::is_control, ""));
        let line = if start + offset == selected { line.cyan() } else { line };
        frame.render_widget(
            Paragraph::new(line).scroll((0, x)),
            Rect::new(inner.x, inner.y + offset as u16, inner.width, 1),
        );
    }
}

pub(crate) fn draw_history(frame: &mut Frame<'_>, area: Rect, history: &History, position: (usize, Option<usize>)) {
    let block = Block::bordered().title(" History ").cyan();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [heading, range, body, reading] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);
    let metric = if history.metric == "usage" {
        "windowUsagePercent"
    } else {
        &history.metric
    };
    let definition = metric_definitions()
        .into_iter()
        .find(|definition| definition.id == metric);
    let title = definition
        .as_ref()
        .map_or("History metric", |definition| &definition.title);
    let unit = definition
        .as_ref()
        .map_or("", |definition| definition.display_unit.as_str());
    let (zone, reading_format) = if history.utc || definition.as_ref().is_some_and(|definition| definition.daily_utc) {
        (TimeZone::UTC, "%Y-%m-%dT%H:%M:%SZ")
    } else {
        (TimeZone::system(), "%Y-%m-%dT%H:%M:%S%:z")
    };
    let total = history
        .canonical_total
        .map_or_else(String::new, |total| format!(" · Total {}", history_value(total, unit)));
    frame.render_widget(Line::from(format!("{title}{total}")).bold(), heading);
    let start = history.start;
    let end = history.end;
    frame.render_widget(
        Line::from(format!(
            "{} to {}",
            history_date(start, &zone, "%Y-%m-%d"),
            history_date(end, &zone, "%Y-%m-%d")
        ))
        .dim(),
        range,
    );

    let (selected, reading_index) = position;
    let Some(series) = history.series.get(selected) else {
        frame.render_widget(Line::from("No history in this range").dim(), body);
        return;
    };
    let wide = body.width >= 76;
    let [chart, legend] = if wide {
        Layout::horizontal([Constraint::Min(0), Constraint::Length((body.width / 3).min(44))]).areas(body)
    } else {
        Layout::vertical([
            Constraint::Min(0),
            Constraint::Length((history.series.len().min(usize::from(body.height / 2))) as u16),
        ])
        .areas(body)
    };
    let visible = draw_history_legend(frame, legend, history, selected);
    let hidden = history.hidden.contains(&series.id);
    let cursor = if hidden {
        frame.render_widget(Line::from("Series hidden · v to show it").dim(), reading);
        None
    } else {
        draw_history_reading(
            frame,
            reading,
            series,
            (reading_index, selected),
            unit,
            (&zone, reading_format),
        )
    };
    if visible.is_empty() {
        frame.render_widget(Line::from("No visible readings in this range").dim(), chart);
        return;
    }
    draw_history_chart(
        frame,
        chart,
        history,
        &visible,
        (selected, cursor),
        (definition.as_ref(), unit, &zone),
    );
}

fn history_value(value: f64, unit: &str) -> String {
    match unit {
        "%" => format!("{value:.2}%"),
        "USD" => format!("${value:.2}"),
        "" | "count" => format!("{value:.2}"),
        _ => format!("{value:.2} {unit}"),
    }
}

fn history_date(seconds: i64, zone: &TimeZone, format: &str) -> String {
    Timestamp::from_second(seconds).map_or_else(
        |_| "Unknown date".into(),
        |stamp| stamp.to_zoned_datetime(zone.clone()).strftime(format).to_string(),
    )
}

fn draw_history_legend<'a>(
    frame: &mut Frame<'_>,
    area: Rect,
    history: &'a History,
    selected: usize,
) -> Vec<(usize, &'a Series)> {
    let mut visible = Vec::new();
    let rows = usize::from(area.height);
    let start = selected
        .saturating_sub(rows / 2)
        .min(history.series.len().saturating_sub(rows));
    for (index, series) in history.series.iter().enumerate() {
        let hidden = history.hidden.contains(&series.id);
        if !hidden && !series.points.is_empty() {
            visible.push((index, series));
        }
        if !(start..start + rows).contains(&index) {
            continue;
        }
        let color = history_color(index);
        let arrow = if index == selected { "›" } else { " " };
        let swatch = if hidden { "○" } else { "●" };
        let provider =
            quotaline_catalog::definition(&series.provider).map_or(series.provider.as_str(), |provider| provider.name);
        let name = format!("{provider} · {}", series.label).replace(char::is_control, "");
        let name = name.fg(color);
        let name = if index == selected { name.bold() } else { name };
        let name = if hidden { name.dim() } else { name };
        frame.render_widget(
            Line::from(vec![Span::raw(format!("{arrow}{swatch} ")).fg(color), name]),
            Rect::new(area.x, area.y + (index - start) as u16, area.width, 1),
        );
    }
    visible
}

fn history_color(index: usize) -> Color {
    Color::Indexed([81, 215, 120, 177, 211, 75, 228, 151, 167, 111, 184, 195][index % 12])
}

fn draw_history_reading(
    frame: &mut Frame<'_>,
    area: Rect,
    series: &Series,
    position: (Option<usize>, usize),
    unit: &str,
    time: (&TimeZone, &str),
) -> Option<(i64, f64)> {
    let (reading, index) = position;
    let mut readings = series.points.iter().filter(|point| !point.3);
    let point = match reading {
        Some(offset) => readings.nth(offset),
        None => readings.next_back(),
    };
    let Some(&(timestamp, value, _, _)) = point else {
        frame.render_widget(Line::from("No readings for this series").dim(), area);
        return None;
    };
    let value_label = history_value(value, unit);
    let summary = if reading.is_some() {
        let when = history_date(timestamp, time.0, time.1);
        format!("{when} · {value_label}")
    } else {
        format!("Latest {value_label}")
    };
    frame.render_widget(Line::from(summary).fg(history_color(index)).bold(), area);
    reading.map(|_| (timestamp, value))
}

fn draw_history_chart(
    frame: &mut Frame<'_>,
    area: Rect,
    history: &History,
    visible: &[(usize, &Series)],
    position: (usize, Option<(i64, f64)>),
    presentation: (Option<&MetricDefinition>, &str, &TimeZone),
) {
    let (selected, cursor) = position;
    let (definition, unit, zone) = presentation;
    let mark = definition.map_or("line", |definition| definition.mark.as_str());
    let stacked = history.stacked && definition.is_some_and(|definition| definition.stackable);
    let upper = history_upper(visible, stacked, unit);
    let [body, timeline] = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
    let ratios = [1.0, 0.75, 0.5, 0.25, 0.0];
    let ticks = ratios.map(|ratio| history_tick(upper * ratio, unit));
    let width = ticks.iter().map(String::len).fold(0, usize::max) as u16 + 1;
    let [axis, plot] = Layout::horizontal([Constraint::Length(width), Constraint::Min(0)]).areas(body);
    let tick_stride = if axis.height >= 9 { 1 } else { 4 };
    let plot_rows = f64::from(axis.height.saturating_sub(1));
    for (ratio, tick) in ratios.into_iter().zip(ticks).step_by(tick_stride) {
        let row = ((1.0 - ratio) * plot_rows).round() as u16;
        frame.render_widget(
            Line::from(tick).dim(),
            Rect::new(axis.x, axis.y + row, axis.width, axis.height.min(1)),
        );
    }

    let start = history.start as f64;
    let end = (history.end as f64).max(start + 1.0);
    let canvas = Canvas::default()
        .x_bounds([start, end])
        .y_bounds([0.0, upper])
        .paint(|context| {
            for ratio in ratios {
                context.draw(&PlotLine::new(
                    start,
                    upper * ratio,
                    end,
                    upper * ratio,
                    Color::DarkGray,
                ));
            }
            if let Some((timestamp, _)) = cursor {
                let timestamp = history_x(timestamp, mark);
                context.draw(&PlotLine::new(timestamp, 0.0, timestamp, upper, Color::Gray));
            }
            context.layer();
            if mark == "bars" {
                draw_history_bars(
                    context,
                    visible,
                    (start, end, plot.width, stacked),
                    (selected, cursor.map(|(timestamp, _)| timestamp)),
                );
            } else {
                for &(index, series) in visible
                    .iter()
                    .filter(|&&(index, _)| index != selected)
                    .chain(visible.iter().filter(|&&(index, _)| index == selected))
                {
                    draw_history_line(context, &series.points, history_color(index), mark == "stepLine", upper);
                }
                if let Some((timestamp, value)) = cursor {
                    context.print(
                        timestamp as f64,
                        value.clamp(0.0, upper),
                        Span::raw("◆").fg(history_color(selected)).bold(),
                    );
                }
            }
        });
    frame.render_widget(canvas, plot);
    let format = if end - start > 172_800.0 { "%m-%d" } else { "%H:%M" };
    let timeline = Rect::new(plot.x, timeline.y, plot.width, timeline.height);
    let [first, last] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(timeline);
    frame.render_widget(Line::from(history_date(history.start, zone, format)).dim(), first);
    frame.render_widget(
        Line::from(history_date(history.end, zone, format))
            .dim()
            .right_aligned(),
        last,
    );
}

fn history_tick(value: f64, unit: &str) -> String {
    let suffix = if unit == "%" { "%" } else { "" };
    let prefix = if unit == "USD" { "$" } else { "" };
    for (scale, magnitude) in [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "k")] {
        if value >= scale {
            return format!("{prefix}{:.1}{magnitude}{suffix}", value / scale);
        }
    }
    if unit == "USD" {
        format!("{prefix}{value:.2}")
    } else {
        format!("{value:.0}{suffix}")
    }
}

fn history_upper(visible: &[(usize, &Series)], stacked: bool, unit: &str) -> f64 {
    if unit == "%" {
        return 100.0;
    }
    if stacked {
        let mut totals = BTreeMap::new();
        for &(_, series) in visible {
            for &(timestamp, value, _, _) in &series.points {
                *totals.entry(timestamp).or_insert(0.0) += value.max(0.0);
            }
        }
        totals.into_values().fold(0.0, f64::max).mul_add(1.08, 0.0).max(1.0)
    } else {
        visible
            .iter()
            .flat_map(|(_, series)| series.points.iter().map(|point| point.1))
            .fold(0.0, f64::max)
            .mul_add(1.08, 0.0)
            .max(1.0)
    }
}

fn history_x(timestamp: i64, mark: &str) -> f64 {
    timestamp as f64 + if mark == "bars" { 43_200.0 } else { 0.0 }
}

fn draw_history_bars(
    context: &mut Context<'_>,
    visible: &[(usize, &Series)],
    geometry: (f64, f64, u16, bool),
    selection: (usize, Option<i64>),
) {
    let (start, end, columns, stacked) = geometry;
    let span = end - start;
    let group = if stacked { 1.0 } else { visible.len() as f64 };
    let pixel = span / (f64::from(columns) * 2.0);
    let width = (span / (span / 86_400.0).max(1.0) * 0.8 / group).clamp(pixel, 12.0 * pixel);
    let mut bases = BTreeMap::new();
    for (position, &(index, series)) in visible.iter().enumerate() {
        let shift = if stacked { -0.5 } else { position as f64 - group / 2.0 };
        for &(timestamp, value, _, _) in &series.points {
            if value <= 0.0 {
                continue;
            }
            let left = history_x(timestamp, "bars") + shift * width;
            let base = if stacked {
                *bases.entry(timestamp).or_insert(0.0)
            } else {
                0.0
            };
            let top = base + value;
            let color = history_color(index);
            context.draw(&FilledLine::new(left, top, left + width * 0.9, top, base, color));
            if (index, Some(timestamp)) == selection {
                context.print(left + width * 0.45, top, Span::raw("◆").fg(color).bold());
            }
            if stacked {
                bases.insert(timestamp, top);
            }
        }
    }
}

fn draw_history_line(
    context: &mut Context<'_>,
    points: &[(i64, f64, Option<f64>, bool)],
    color: Color,
    steps: bool,
    upper: f64,
) {
    for pair in points.windows(2) {
        let first = pair[0];
        let next = pair[1];
        let first_value = first.1.clamp(0.0, upper);
        let next_value = next.1.clamp(0.0, upper);
        let intermediate = if steps { first_value } else { next_value };
        context.draw(&PlotLine::new(
            first.0 as f64,
            first_value,
            next.0 as f64,
            intermediate,
            color,
        ));
        if steps {
            context.draw(&PlotLine::new(
                next.0 as f64,
                intermediate,
                next.0 as f64,
                next_value,
                color,
            ));
        }
    }
    if let [point] = points {
        context.draw(&Points {
            coords: &[(point.0 as f64, point.1.clamp(0.0, upper))],
            color,
        });
    }
}

#[cfg(test)]
#[path = "../tests/unit/pages.rs"]
mod tests;
