//! Renders provider summaries and quota windows.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Paragraph, Wrap},
};

use crate::{Source, snapshot::Snapshot, terminal::Navigation};

pub(crate) struct Dashboard {
    snapshot: Snapshot,
    provider: usize,
    window: usize,
    windows_focused: bool,
}

impl Dashboard {
    pub(crate) fn new() -> Self {
        Self {
            snapshot: Snapshot {
                providers: Vec::new(),
                warning: None,
            },
            provider: 0,
            window: 0,
            windows_focused: false,
        }
    }

    pub(crate) fn refresh(&mut self, source: &Source) {
        let selected = self
            .snapshot
            .providers
            .get(self.provider)
            .map(|provider| provider.id.clone());
        self.snapshot.refresh(source);
        self.provider = selected
            .as_ref()
            .and_then(|id| self.snapshot.providers.iter().position(|provider| provider.id == *id))
            .unwrap_or_else(|| self.provider.min(self.snapshot.providers.len().saturating_sub(1)));
        let provider = self.snapshot.providers.get(self.provider);
        if provider.map(|provider| &provider.id) != selected.as_ref() {
            self.window = 0;
        }
        if let Some(provider) = provider {
            self.window = self.window.min(provider.windows.len().saturating_sub(1));
        }
    }

    pub(crate) fn navigate(&mut self, action: Navigation) {
        match action {
            Navigation::TogglePanel => self.windows_focused = !self.windows_focused,
            Navigation::Up | Navigation::Down => {
                if self.windows_focused {
                    if let Some(provider) = self.snapshot.providers.get(self.provider) {
                        self.window = next_index(self.window, provider.windows.len(), action);
                    }
                } else {
                    let next = next_index(self.provider, self.snapshot.providers.len(), action);
                    if self.provider != next {
                        self.provider = next;
                        self.window = 0;
                    }
                }
            }
        }
    }

    pub(crate) fn draw(&self, frame: &mut Frame<'_>, source: &str) {
        let area = frame.area();
        if area.width < 34 || area.height < 14 {
            frame.render_widget(Paragraph::new("Resize to at least 34 × 14").yellow(), area);
            return;
        }

        let [title, content, status, help] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        frame.render_widget(
            Line::from(vec![
                " QUOTALINE ".black().on_cyan().bold(),
                format!("  {source}").dim(),
            ]),
            title,
        );
        let [providers, windows] = if area.width >= 80 {
            Layout::horizontal([Constraint::Percentage(44), Constraint::Percentage(56)])
                .spacing(1)
                .areas(content)
        } else {
            let provider_height = (self.snapshot.providers.len() * 2 + 2).clamp(4, usize::from(content.height - 7));
            Layout::vertical([Constraint::Length(provider_height as u16), Constraint::Min(7)]).areas(content)
        };
        self.draw_providers(frame, providers);
        self.draw_windows(frame, windows);

        if let Some(error) = &self.snapshot.warning {
            let prefix = if self.snapshot.providers.is_empty() {
                "Read failed"
            } else {
                "Stale"
            };
            frame.render_widget(Line::from(format!(" {prefix} · {error}")).yellow(), status);
        }
        let help_text = if area.width >= 51 {
            " ↑↓ / j k Move   Tab Switch panel   r Reload   q Quit"
        } else {
            " ↑↓ Move  Tab Panel  r Load  q Quit"
        };
        frame.render_widget(Line::from(help_text).dim(), help);
    }

    fn draw_providers(&self, frame: &mut Frame<'_>, area: Rect) {
        let panel = Block::bordered()
            .title(" Providers · most used ")
            .border_type(BorderType::Rounded)
            .border_style(if self.windows_focused {
                Color::DarkGray
            } else {
                Color::Cyan
            });
        let inner = panel.inner(area);
        frame.render_widget(panel, area);
        if self.snapshot.providers.is_empty() {
            let message = if self.snapshot.warning.is_some() {
                "Enable Write usage.json in Quotaline’s Data settings."
            } else {
                "No providers in snapshot"
            };
            frame.render_widget(Paragraph::new(message).dim().wrap(Wrap { trim: true }), inner);
            return;
        }
        let visible = usize::from(inner.height / 2);
        let start = visible_start(self.provider, self.snapshot.providers.len(), visible);
        for (offset, provider) in self.snapshot.providers.iter().skip(start).take(visible).enumerate() {
            let y = inner.y + offset as u16 * 2;
            let name = Rect::new(inner.x, y, inner.width, 1);
            let style = if start + offset == self.provider {
                Line::from(format!("› {}", provider.name())).cyan().bold()
            } else {
                Line::from(format!("  {}", provider.name()))
            };
            frame.render_widget(style, name);

            let bar = Rect::new(inner.x + 2, y + 1, inner.width - 3, 1);
            if let Some((window, used)) = provider.most_used() {
                let [label, meter] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(13)]).areas(bar);
                frame.render_widget(Line::from(window.label.as_str()).dim(), label);
                draw_meter(frame, meter, Some(used));
            } else {
                frame.render_widget(Line::from("Usage unavailable").dim(), bar);
            }
        }
    }

    fn draw_windows(&self, frame: &mut Frame<'_>, area: Rect) {
        let panel = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(if self.windows_focused {
                Color::Cyan
            } else {
                Color::DarkGray
            });
        let inner = panel.inner(area);
        let Some(provider) = self.snapshot.providers.get(self.provider) else {
            frame.render_widget(panel.title(" Quota windows "), area);
            return;
        };
        let visible = usize::from((inner.height - 2) / 3);
        let start = visible_start(self.window, provider.windows.len(), visible);
        let title = if provider.windows.len() > visible {
            format!(
                " Quota windows {}–{} / {} ",
                start + 1,
                start + visible,
                provider.windows.len()
            )
        } else {
            " Quota windows ".into()
        };
        frame.render_widget(panel.title(title), area);

        let [name, observed, rows] =
            Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)]).areas(inner);
        let mut title = Line::from(Span::raw(provider.name()).bold());
        if let Some(plan) = &provider.plan {
            title.push_span(format!(" · {plan}").dim());
        }
        frame.render_widget(title, name);
        frame.render_widget(Line::from(format!("Observed {}", provider.fetched_at)).dim(), observed);
        if provider.windows.is_empty() {
            frame.render_widget(Line::from("No quota windows reported").dim(), rows);
            return;
        }

        for (offset, window) in provider.windows.iter().skip(start).take(visible).enumerate() {
            let y = rows.y + offset as u16 * 3;
            let label = Rect::new(rows.x, y, rows.width, 1);
            let meter = Rect::new(rows.x, y + 1, rows.width, 1);
            let reset = Rect::new(rows.x, y + 2, rows.width, 1);
            let selected = self.windows_focused && self.window == start + offset;
            let label_text = if selected {
                Line::from(format!("› {}", window.label)).cyan().bold()
            } else {
                Line::from(window.label.as_str()).bold()
            };
            frame.render_widget(label_text, label);
            draw_meter(frame, meter, window.used_percent);
            let resetting = window
                .resets_at
                .map_or_else(|| "Reset unavailable".into(), |reset| format!("Reset {reset}"));
            frame.render_widget(Line::from(resetting).dim(), reset);
        }
    }
}

fn draw_meter(frame: &mut Frame<'_>, area: Rect, used: Option<f64>) {
    let Some(used) = used else {
        frame.render_widget(Line::from("Usage unavailable").dim(), area);
        return;
    };
    let percent = format!("{used:.1}%");
    let [bar, label] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(percent.len().min(usize::from(area.width)) as u16),
    ])
    .areas(area);
    let width = usize::from(bar.width.saturating_sub(1));
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
        bar,
    );
    frame.render_widget(Line::from(percent).bold().right_aligned(), label);
}

fn visible_start(selected: usize, length: usize, visible: usize) -> usize {
    selected.saturating_sub(visible / 2).min(length.saturating_sub(visible))
}

fn next_index(selected: usize, length: usize, action: Navigation) -> usize {
    if action == Navigation::Up {
        selected.saturating_sub(1)
    } else {
        (selected + 1).min(length.saturating_sub(1))
    }
}

#[cfg(test)]
#[path = "../tests/unit/dashboard.rs"]
mod tests;
