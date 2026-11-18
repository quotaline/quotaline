//! Shows every visible provider and quota window in one scrolling table.

use jiff::{
    Timestamp,
    civil::Date,
    tz::{Offset, TimeZone},
};
use quotaline_text::time::{countdown, relative_age};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Wrap},
};
use serde_json::Value;

use crate::snapshot::{Provider, Window};

pub(crate) fn positions(providers: &[Provider]) -> impl Iterator<Item = (usize, usize)> + '_ {
    providers
        .iter()
        .enumerate()
        .flat_map(|(provider, data)| (0..=data.windows.len()).map(move |window| (provider, window)))
}

pub(crate) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    providers: &[Provider],
    selected: (usize, usize),
    settings: &Value,
    read_failed: bool,
) {
    let remaining = settings["show_as"] == "remaining";
    let title = if remaining { " Quota remaining " } else { " Quota used " };
    let panel = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .border_style(Color::Cyan);
    let inner = panel.inner(area);
    frame.render_widget(panel, area);
    if providers.is_empty() {
        let message = if read_failed {
            "Check the source, open Quotaline or use --demo."
        } else {
            "No providers in snapshot"
        };
        frame.render_widget(Paragraph::new(message).dim().wrap(Wrap { trim: true }), inner);
        return;
    }

    let decimals = settings["decimals"].as_u64().unwrap_or(1).min(2) as usize;
    let clock = ReadingTime::at(Timestamp::now());
    for (row, index @ (provider, window)) in visible_rows(inner, providers, selected) {
        let active = index == selected;
        let data = &providers[provider];
        if let Some(window) = window.checked_sub(1).and_then(|index| data.windows.get(index)) {
            draw_window(frame, row, window, active, (remaining, decimals), clock);
        } else {
            draw_provider(frame, row, data, active, clock);
        }
    }
}

pub(crate) fn hit(
    area: Rect,
    providers: &[Provider],
    selected: (usize, usize),
    at: Position,
) -> Option<(usize, usize)> {
    visible_rows(Block::bordered().inner(area), providers, selected)
        .find_map(|(row, index)| row.contains(at).then_some(index))
}

fn visible_rows(
    inner: Rect,
    providers: &[Provider],
    selected: (usize, usize),
) -> impl Iterator<Item = (Rect, (usize, usize))> {
    let line_height = if inner.width < 73 { 2 } else { 1 };
    let visible = usize::from(inner.height / line_height);
    let rows = positions(providers).collect::<Vec<_>>();
    let selected = rows.iter().position(|&row| row == selected).unwrap_or(0);
    let start = selected
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible));
    rows.into_iter()
        .skip(start)
        .take(visible)
        .enumerate()
        .map(move |(offset, index)| {
            (
                Rect::new(inner.x, inner.y + offset as u16 * line_height, inner.width, line_height),
                index,
            )
        })
}

pub(crate) fn window_details(window: &Window, settings: &Value) -> [String; 2] {
    let remaining = settings["show_as"] == "remaining";
    let decimals = settings["decimals"].as_u64().unwrap_or(1).min(2) as usize;
    let reading = window.used_percent.map_or_else(
        || "Usage unavailable".into(),
        |used| {
            let state = if remaining { "remaining" } else { "used" };
            format!("{} {state}", percentage(used, remaining, decimals))
        },
    );
    [
        format!("{} · {reading}", window.label),
        ReadingTime::at(Timestamp::now()).deadline(window),
    ]
}

fn draw_provider(frame: &mut Frame<'_>, area: Rect, provider: &Provider, active: bool, clock: ReadingTime) {
    let source = provider
        .fetched_at
        .map_or_else(|| provider.status.clone(), |stamp| clock.observed(stamp));
    let secondary = if provider.windows.is_empty() && provider.status.is_empty() {
        "No quota windows reported".into()
    } else {
        source
    };
    let mut title = Line::from(Span::raw(format!("{} {}", if active { "›" } else { " " }, provider.name())).bold());
    if let Some(plan) = &provider.plan {
        title.push_span(format!(" · {plan}").dim());
    }
    let [name, status] = if area.height == 1 {
        Layout::horizontal([Constraint::Fill(1), Constraint::Length(32)]).areas(area)
    } else {
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area)
    };
    frame.render_widget(title.cyan(), name);
    frame.render_widget(Line::from(secondary).dim(), status);
}

fn draw_window(
    frame: &mut Frame<'_>,
    area: Rect,
    window: &Window,
    active: bool,
    percent: (bool, usize),
    clock: ReadingTime,
) {
    let [label, meter, reset] = if area.height == 1 {
        Layout::horizontal([
            Constraint::Percentage(35),
            Constraint::Fill(1),
            Constraint::Length(if area.width >= 120 { 54 } else { 29 }),
        ])
        .spacing(1)
        .areas(area)
    } else {
        let [top, bottom] = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);
        let [label, meter] = Layout::horizontal([Constraint::Percentage(55), Constraint::Fill(1)]).areas(top);
        [label, meter, bottom]
    };
    let marker = if active { " › " } else { "   " };
    let title = Line::from(format!("{marker}{}", window.label));
    frame.render_widget(if active { title.cyan().bold() } else { title }, label);

    let mut deadline = clock.deadline(window);
    if let Some(pace) = &window.pace
        && deadline.chars().count() + 3 + pace.chars().count() <= usize::from(reset.width)
    {
        deadline.push_str(&format!(" · {pace}"));
    }
    frame.render_widget(Line::from(deadline).dim(), reset);
    draw_meter(frame, meter, window.used_percent, percent);
}

#[derive(Clone, Copy)]
struct ReadingTime {
    now: Timestamp,
    today: Date,
}

impl ReadingTime {
    fn at(now: Timestamp) -> Self {
        Self {
            now,
            today: TimeZone::system().to_datetime(now).date(),
        }
    }

    fn deadline(self, window: &Window) -> String {
        window.resets_at.map_or_else(
            || "Reset unavailable".into(),
            |stamp| self.reset(stamp, window.reset_by_day),
        )
    }

    fn observed(self, stamp: Timestamp) -> String {
        let age = stamp.duration_until(self.now).as_secs_f64();
        if age <= -5.0 {
            format!("Observed {} ahead", countdown(-age))
        } else {
            format!("Observed {}", relative_age(age))
        }
    }

    fn reset(self, stamp: Timestamp, by_day: bool) -> String {
        if by_day {
            // The timestamp's UTC date is the date reported by the provider.
            let day = Offset::UTC.to_datetime(stamp).date();
            return match day.duration_since(self.today).as_secs() / 86_400 {
                -1 => "Reset was due yesterday".into(),
                0 => "Reset due today".into(),
                1 => "Resets tomorrow".into(),
                days if days < 0 => format!("Reset was due {} days ago", -days),
                days => format!("Resets in {days} days"),
            };
        }
        let until = self.now.duration_until(stamp).as_secs_f64();
        if until == 0.0 {
            "Reset due now".into()
        } else if until < 0.0 {
            format!("Reset was due {}", relative_age(-until))
        } else {
            format!("Resets in {}", countdown(until))
        }
    }
}

fn draw_meter(frame: &mut Frame<'_>, area: Rect, used: Option<f64>, (remaining, decimals): (bool, usize)) {
    let Some(used) = used else {
        frame.render_widget(Line::from("Usage unavailable").dim(), area);
        return;
    };
    let label = percentage(used, remaining, decimals);
    let [track, value] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(8)]).areas(area);
    let width = usize::from(track.width.saturating_sub(1));
    let filled = (used.clamp(0.0, 100.0) * width as f64 / 100.0).round() as usize;
    let color = if used >= 90.0 {
        Color::LightRed
    } else {
        Color::LightGreen
    };
    frame.render_widget(
        Line::from(vec![
            "█".repeat(filled).fg(color),
            "░".repeat(width - filled).dark_gray(),
        ]),
        track,
    );
    frame.render_widget(Line::from(label).bold().right_aligned(), value);
}

fn percentage(used: f64, remaining: bool, decimals: usize) -> String {
    let value = if remaining { 100.0 - used } else { used };
    format!("{value:.decimals$}%")
}

#[cfg(test)]
#[path = "../tests/unit/usage.rs"]
mod tests;
