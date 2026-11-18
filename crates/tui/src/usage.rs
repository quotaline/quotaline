//! Shows every visible provider and quota window in one scrolling table.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
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

    let line_height = if inner.width < 73 { 2 } else { 1 };
    let visible = usize::from(inner.height / line_height);
    let rows = positions(providers).collect::<Vec<_>>();
    let selected = rows.iter().position(|&row| row == selected).unwrap_or(0);
    let start = selected
        .saturating_sub(visible / 2)
        .min(rows.len().saturating_sub(visible));
    let decimals = settings["decimals"].as_u64().unwrap_or(1).min(2) as usize;
    for (offset, &(provider, window)) in rows.iter().skip(start).take(visible).enumerate() {
        let row = Rect::new(inner.x, inner.y + offset as u16 * line_height, inner.width, line_height);
        let active = start + offset == selected;
        let data = &providers[provider];
        if let Some(window) = window.checked_sub(1).and_then(|index| data.windows.get(index)) {
            draw_window(frame, row, window, active, (remaining, decimals));
        } else {
            draw_provider(frame, row, data, active);
        }
    }
}

fn draw_provider(frame: &mut Frame<'_>, area: Rect, provider: &Provider, active: bool) {
    let source = provider
        .fetched_at
        .map_or_else(|| provider.status.clone(), |stamp| format!("Observed {stamp}"));
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

fn draw_window(frame: &mut Frame<'_>, area: Rect, window: &Window, active: bool, percent: (bool, usize)) {
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

    let mut deadline = window.resets_at.map_or_else(
        || "Reset unavailable".into(),
        |stamp| {
            if window.reset_by_day {
                format!("Reset {}", stamp.strftime("%Y-%m-%d"))
            } else {
                format!("Reset {stamp}")
            }
        },
    );
    if let Some(pace) = &window.pace
        && deadline.chars().count() + 3 + pace.chars().count() <= usize::from(reset.width)
    {
        deadline.push_str(&format!(" · {pace}"));
    }
    frame.render_widget(Line::from(deadline).dim(), reset);
    draw_meter(frame, meter, window.used_percent, percent);
}

fn draw_meter(frame: &mut Frame<'_>, area: Rect, used: Option<f64>, (remaining, decimals): (bool, usize)) {
    let Some(used) = used else {
        frame.render_widget(Line::from("Usage unavailable").dim(), area);
        return;
    };
    let value = if remaining { 100.0 - used } else { used };
    let label = format!("{value:.decimals$}%");
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
