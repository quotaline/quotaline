//! Shows providers, quota windows, and account usage in one scrolling table.

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UsageRow {
    Provider,
    Window(usize),
    Overview(usize, usize),
}

pub(crate) type UsagePosition = (usize, UsageRow);

pub(crate) fn step(area: Rect, providers: &[Provider], selected: UsagePosition, forward: bool) -> UsagePosition {
    let entries = entries(Block::bordered().inner(area).width, providers);
    let current = active_row(&entries, selected);
    let next = if forward {
        current + 1
    } else {
        current.saturating_sub(1)
    };
    entries
        .get(next.min(entries.len().saturating_sub(1)))
        .map_or(selected, Entry::position)
}

pub(crate) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    providers: &[Provider],
    selected: UsagePosition,
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
    for (area, entry, active) in visible_rows(inner, providers, selected) {
        let provider = &providers[entry.provider];
        match entry.row {
            Row::Provider => draw_provider(frame, area, provider, active, clock),
            Row::Window(index) => draw_window(
                frame,
                area,
                &provider.windows[index],
                active,
                (remaining, decimals),
                clock,
            ),
            Row::Overview { text, .. } => {
                let marker = if active { "  › " } else { "    " };
                let line = Line::from(format!("{marker}{text}"));
                frame.render_widget(if active { line.cyan().bold() } else { line }, area);
            }
        }
    }
}

pub(crate) fn hit(area: Rect, providers: &[Provider], selected: UsagePosition, at: Position) -> Option<UsagePosition> {
    visible_rows(Block::bordered().inner(area), providers, selected)
        .find_map(|(row, entry, _)| row.contains(at).then(|| entry.position()))
}

fn visible_rows(
    inner: Rect,
    providers: &[Provider],
    selected: UsagePosition,
) -> impl Iterator<Item = (Rect, Entry, bool)> {
    let entries = entries(inner.width, providers);
    let active = active_row(&entries, selected);
    let height = usize::from(inner.height);
    let total = entries.iter().map(|entry| entry.height).sum::<usize>();
    let focus = entries[..active].iter().map(|entry| entry.height).sum::<usize>();
    let top = focus.saturating_sub(height / 2).min(total.saturating_sub(height));
    let start = entries
        .iter()
        .scan(0, |offset, entry| {
            let before = *offset;
            *offset += entry.height;
            Some(before)
        })
        .position(|offset| offset >= top)
        .unwrap_or(0);
    entries
        .into_iter()
        .enumerate()
        .skip(start)
        .scan(0, move |offset, (index, entry)| {
            let before = *offset;
            *offset += entry.height;
            (*offset <= height).then(|| {
                (
                    Rect::new(inner.x, inner.y + before as u16, inner.width, entry.height as u16),
                    entry,
                    index == active,
                )
            })
        })
}

struct Entry {
    provider: usize,
    height: usize,
    row: Row,
}

enum Row {
    Provider,
    Window(usize),
    Overview {
        first: usize,
        last: usize,
        fragment: usize,
        text: String,
    },
}

impl Entry {
    fn position(&self) -> UsagePosition {
        let row = match &self.row {
            Row::Provider => UsageRow::Provider,
            Row::Window(index) => UsageRow::Window(*index),
            Row::Overview { first, fragment, .. } => UsageRow::Overview(*first, *fragment),
        };
        (self.provider, row)
    }
}

fn entries(width: u16, providers: &[Provider]) -> Vec<Entry> {
    let window_height = if width < 73 { 2 } else { 1 };
    let mut entries = Vec::new();
    for (provider, data) in providers.iter().enumerate() {
        entries.push(Entry {
            provider,
            height: if provider_subtitle(data).is_some() {
                window_height
            } else {
                1
            },
            row: Row::Provider,
        });
        entries.extend((0..data.windows.len()).map(|index| Entry {
            provider,
            height: window_height,
            row: Row::Window(index),
        }));
        append_overview(
            &mut entries,
            provider,
            &data.overview,
            usize::from(width.saturating_sub(4).max(1)),
        );
    }
    entries
}

fn append_overview(entries: &mut Vec<Entry>, provider: usize, facts: &[String], width: usize) {
    let wrapping = textwrap::Options::new(width).word_splitter(textwrap::WordSplitter::NoHyphenation);
    for (index, fact) in facts.iter().enumerate() {
        if fact.trim().is_empty() {
            continue;
        }
        let lines = textwrap::wrap(fact, &wrapping);
        if let [line] = lines.as_slice()
            && let Some(Entry {
                row:
                    Row::Overview {
                        last,
                        fragment: 0,
                        text,
                        ..
                    },
                ..
            }) = entries.last_mut()
            && Line::raw(text.as_str()).width() + 3 + Line::raw(line.as_ref()).width() <= width
        {
            text.push_str("   ");
            text.push_str(line);
            *last = index;
            continue;
        }
        entries.extend(lines.into_iter().enumerate().map(|(fragment, text)| Entry {
            provider,
            height: 1,
            row: Row::Overview {
                first: index,
                last: index,
                fragment,
                text: text.into_owned(),
            },
        }));
    }
}

fn active_row(entries: &[Entry], (provider, selected): UsagePosition) -> usize {
    let mut fallback = 0;
    for (index, entry) in entries.iter().enumerate() {
        if entry.provider != provider {
            continue;
        }
        match (&entry.row, selected) {
            (Row::Provider, _) => fallback = index,
            (
                Row::Overview {
                    first, last, fragment, ..
                },
                UsageRow::Overview(fact, selected_fragment),
            ) if (*first..=*last).contains(&fact) => {
                fallback = index;
                if *fragment == selected_fragment {
                    return index;
                }
            }
            _ => {}
        }
        if entry.position() == (provider, selected) {
            return index;
        }
    }
    fallback
}

enum ProviderSubtitle<'a> {
    Observed(Timestamp),
    Status(&'a str),
}

fn provider_subtitle(provider: &Provider) -> Option<ProviderSubtitle<'_>> {
    if let Some(stamp) = provider.fetched_at {
        Some(ProviderSubtitle::Observed(stamp))
    } else if !provider.status.is_empty() {
        Some(ProviderSubtitle::Status(&provider.status))
    } else if provider.windows.is_empty() && provider.overview.is_empty() {
        Some(ProviderSubtitle::Status("No quota windows reported"))
    } else {
        None
    }
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
    let mut title = Line::from(Span::raw(format!("{} {}", if active { "›" } else { " " }, provider.name())).bold());
    if let Some(plan) = &provider.plan {
        title.push_span(format!(" · {plan}").dim());
    }
    let Some(subtitle) = provider_subtitle(provider) else {
        frame.render_widget(title.cyan(), area);
        return;
    };
    let secondary = match subtitle {
        ProviderSubtitle::Observed(stamp) => clock.observed(stamp),
        ProviderSubtitle::Status(text) => text.to_owned(),
    };
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
