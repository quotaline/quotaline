//! Renders provider summaries and quota windows.

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Stylize,
    text::Line,
    widgets::Paragraph,
};

use serde_json::{Value, json};

use crate::{
    desktop::{Command, History, Output, State},
    keys::Navigation,
    pages::{self, Page},
    preferences::{self, Control},
    snapshot::Provider,
    usage,
};

pub(crate) struct Dashboard {
    providers: Vec<Provider>,
    warning: Option<String>,
    settings: Value,
    controls: Vec<Control>,
    settings_editable: bool,
    page: Page,
    history: Option<History>,
    logs: Vec<String>,
    response: Vec<String>,
    scroll: usize,
    column: u16,
    reading: Option<usize>,
    command: Option<Vec<u8>>,
    message: Option<String>,
    provider: usize,
    window: usize,
}

impl Dashboard {
    pub(crate) fn new(settings_editable: bool) -> Self {
        Self {
            providers: Vec::new(),
            warning: None,
            settings: Value::Null,
            controls: Vec::new(),
            settings_editable,
            page: Page::Usage,
            history: None,
            logs: Vec::new(),
            response: Vec::new(),
            scroll: 0,
            column: 0,
            reading: None,
            command: None,
            message: None,
            provider: 0,
            window: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn refresh(&mut self, source: &crate::snapshot::LocalSnapshot) {
        self.apply_snapshot(source.read());
    }

    pub(crate) fn apply_snapshot(&mut self, snapshot: std::io::Result<State>) {
        let mut state = match snapshot {
            Ok(state) => state,
            Err(error) => {
                self.warning = Some(error.to_string());
                return;
            }
        };
        self.warning = None;
        state.sanitize();
        let selected = self.providers.get(self.provider).map(|provider| provider.id.clone());
        self.providers = state.providers;
        self.settings = state.settings;
        self.controls = preferences::controls(&self.settings);
        if self.page == Page::Settings {
            self.scroll = self.scroll.min(self.controls.len().saturating_sub(1));
        }
        self.provider = selected
            .as_ref()
            .and_then(|id| self.providers.iter().position(|provider| provider.id == *id))
            .unwrap_or_else(|| self.provider.min(self.providers.len().saturating_sub(1)));
        let provider = self.providers.get(self.provider);
        if provider.map(|provider| &provider.id) != selected.as_ref() {
            self.window = 0;
        }
        if let Some(provider) = provider {
            self.window = self.window.min(provider.windows.len());
        }
    }

    pub(crate) fn navigate(&mut self, action: Navigation) -> Option<Command> {
        match action {
            Navigation::Left => return self.move_horizontal(false),
            Navigation::Right => return self.move_horizontal(true),
            Navigation::NextGroup => self.next_group(),
            Navigation::Up | Navigation::Down => self.move_vertical(action),
        }
        None
    }

    fn next_group(&mut self) {
        match self.page {
            Page::Usage => {
                self.provider = (self.provider + 1) % self.providers.len().max(1);
                self.window = 0;
            }
            Page::Settings => {
                if let Some(current) = self.controls.get(self.scroll) {
                    self.scroll = self
                        .controls
                        .iter()
                        .enumerate()
                        .skip(self.scroll + 1)
                        .find(|(_, control)| control.group != current.group)
                        .map_or(0, |(index, _)| index);
                }
            }
            _ => {}
        }
    }

    fn move_vertical(&mut self, action: Navigation) {
        if self.page == Page::Usage {
            let rows = usage::positions(&self.providers).collect::<Vec<_>>();
            let current = rows
                .iter()
                .position(|&pair| pair == (self.provider, self.window))
                .unwrap_or(0);
            if let Some(&(provider, window)) = rows.get(next_index(current, rows.len(), action)) {
                self.provider = provider;
                self.window = window;
            }
        } else {
            self.reading = None;
            let length = match self.page {
                Page::Settings => self.controls.len(),
                Page::History => self.history.as_ref().map_or(0, |history| history.series.len()),
                _ => self.text().len(),
            };
            self.scroll = next_index(self.scroll, length, action);
        }
    }

    pub(crate) fn show(&mut self, page: Page) {
        self.page = page;
        self.scroll = 0;
        self.column = 0;
        self.reading = None;
    }

    fn move_horizontal(&mut self, forward: bool) -> Option<Command> {
        match self.page {
            Page::Usage => {}
            Page::Settings => {
                if self.settings_editable {
                    return self
                        .controls
                        .get(self.scroll)
                        .and_then(|control| control.adjust(if forward { 1 } else { -1 }));
                }
            }
            Page::History => {
                if let Some(series) = self
                    .history
                    .as_ref()
                    .and_then(|history| history.series.get(self.scroll))
                {
                    let last = series.points.iter().filter(|point| !point.3).count().saturating_sub(1);
                    let current = self.reading.unwrap_or(last);
                    self.reading = Some(if forward {
                        (current + 1).min(last)
                    } else {
                        current.saturating_sub(1)
                    });
                }
            }
            _ => {
                self.column = if forward {
                    self.column.saturating_add(8)
                } else {
                    self.column.saturating_sub(8)
                };
            }
        }
        None
    }

    pub(crate) fn edit_setting(&mut self) -> Option<Command> {
        if self.page != Page::Settings || !self.settings_editable {
            return None;
        }
        let control = self.controls.get(self.scroll)?;
        if let Some(text) = control.answer() {
            self.command = Some(text.into_bytes());
            None
        } else {
            control.adjust(1)
        }
    }

    pub(crate) fn visibility(&self, isolate: bool) -> Option<Command> {
        if self.page != Page::History {
            return None;
        }
        let history = self.history.as_ref()?;
        let current = history.series.get(self.scroll)?;
        let kind = if isolate {
            if history
                .series
                .iter()
                .filter(|series| !history.hidden.contains(&series.id))
                .count()
                == 1
                && !history.hidden.contains(&current.id)
            {
                "isolate_undo"
            } else {
                "isolate"
            }
        } else {
            "toggle"
        };
        Some(Command::Visibility(
            json!({
                "kind": kind,
                "metric": history.metric,
                "id": current.id,
                "series": history.series.iter().map(|series| series.id.as_str()).collect::<Vec<_>>()
            })
            .to_string(),
        ))
    }

    pub(crate) fn selected_provider(&self) -> Option<&str> {
        self.providers.get(self.provider).map(|provider| provider.id.as_str())
    }

    pub(crate) fn page(&self) -> Page {
        self.page
    }

    pub(crate) fn start_command(&mut self) {
        self.command = Some(Vec::new());
    }

    pub(crate) fn editing(&self) -> bool {
        self.command.is_some()
    }

    pub(crate) fn edit(&mut self, byte: u8) -> Option<String> {
        let command = self.command.as_mut().expect("editor is active");
        match byte {
            27 => self.command = None,
            b'\r' | b'\n' => {
                return self
                    .command
                    .take()
                    .map(|text| String::from_utf8_lossy(&text).into_owned());
            }
            8 | 127 => while command.pop().is_some_and(|last| last & 0xc0 == 0x80) {},
            32..=126 | 128..=255 => command.push(byte),
            _ => {}
        }
        None
    }

    pub(crate) fn parse(&mut self, text: &str) -> Option<Command> {
        if text.trim() == "metrics" {
            self.response = std::iter::once("usage · Quota usage".into())
                .chain(
                    quotaline_analytics::METRICS
                        .iter()
                        .map(|(id, label, _, _)| format!("{id} · {label}")),
                )
                .collect();
            self.show(Page::Result);
            return None;
        }
        match pages::parse(text, self.history.as_ref()) {
            Ok(command) => Some(command),
            Err(error) => {
                self.message = Some(error.to_string());
                None
            }
        }
    }

    pub(crate) fn history_shift(&mut self, forward: Option<bool>) -> Command {
        self.show(Page::History);
        Command::History(forward.map_or_else(
            || json!({"kind":"now"}),
            |forward| json!({"kind":"page","forward":forward}),
        ))
    }

    pub(crate) fn apply_output(&mut self, page: Page, output: std::io::Result<Output>) {
        let active = self.page == page;
        if active {
            self.message = None;
        }
        match output {
            Ok(Output::History(history)) => {
                if active {
                    self.scroll = self.scroll.min(history.series.len().saturating_sub(1));
                }
                self.history = Some(history);
            }
            Ok(Output::Text(lines)) if page == Page::Logs => self.logs = lines,
            Ok(Output::Text(lines)) => self.response = lines,
            Ok(Output::Done(message)) if active => self.message = Some(message),
            Err(error) if active => self.message = Some(error.to_string()),
            _ => {}
        }
    }

    fn text(&self) -> Vec<String> {
        match self.page {
            Page::Usage | Page::History | Page::Settings => Vec::new(),
            Page::Help => pages::help(),
            Page::Logs => self.logs.clone(),
            Page::Result => self.response.clone(),
            Page::Detail => self.providers.get(self.provider).map_or_else(Vec::new, |provider| {
                std::iter::once(format!("{} · {}", provider.name(), provider.status))
                    .chain(provider.details.iter().cloned())
                    .chain(
                        provider
                            .windows
                            .iter()
                            .filter_map(|window| window.pace.as_ref().map(|pace| format!("{}  {pace}", window.label))),
                    )
                    .collect()
            }),
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
        let mut heading = Line::from(vec![
            " QUOTALINE ".black().on_cyan().bold(),
            format!(" · {}", self.page.title()).bold(),
        ]);
        if !source.is_empty() {
            heading.push_span(format!(" · {source}").dim());
        }
        frame.render_widget(heading, title);
        match self.page {
            Page::Usage => usage::draw(
                frame,
                content,
                &self.providers,
                (self.provider, self.window),
                &self.settings,
                self.warning.is_some(),
            ),
            Page::Settings => preferences::draw(frame, content, &self.controls, self.scroll, self.settings_editable),
            Page::History if self.history.is_some() => {
                pages::draw_history(
                    frame,
                    content,
                    self.history.as_ref().expect("history is loaded"),
                    (self.scroll, self.reading),
                );
            }
            _ => pages::draw_text(frame, content, self.page, &self.text(), (self.scroll, self.column)),
        }

        if let Some(error) = &self.warning {
            let prefix = if self.providers.is_empty() {
                "Read failed"
            } else {
                "Stale"
            };
            frame.render_widget(
                Line::from(format!(" {prefix} · {error}").replace(char::is_control, "")).yellow(),
                status,
            );
        } else if let Some(message) = &self.message {
            frame.render_widget(Line::from(message.replace(char::is_control, "")).yellow(), status);
        }

        let help_text = if area.width >= 62 {
            " 1 Usage  2 History  3 Settings  4 Logs    ? Help  q Quit"
        } else {
            " 1 Usage  2 Hist  3 Set  4 Log  ?"
        };
        let line = self.command.as_ref().map_or_else(
            || Line::from(help_text).dim(),
            |input| Line::from(format!(":{}", String::from_utf8_lossy(input))).cyan(),
        );
        frame.render_widget(line, help);
    }
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
