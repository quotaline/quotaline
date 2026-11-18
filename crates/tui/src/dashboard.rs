//! Renders provider summaries and quota windows.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::Stylize,
    text::{Line, Span},
    widgets::Paragraph,
};

use serde_json::{Value, json};

use crate::{
    desktop::{Command, Output, State},
    keys::Navigation,
    pages::{self, HistoryHit, HistoryView, Page},
    preferences::{self, Control, SettingsHit},
    snapshot::Provider,
    usage::{self, UsageRow},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MouseEvent {
    Move(u16, u16),
    Click(u16, u16),
    Scroll(u16, u16, Navigation),
    Leave,
}

#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) enum MouseAction {
    Quit,
    Show(Page),
    Command(Command),
}

#[derive(Clone, Copy)]
enum HistoryCursor {
    Keyboard(i64),
    Mouse(i64),
}

impl HistoryCursor {
    fn timestamp(self) -> i64 {
        match self {
            Self::Keyboard(timestamp) | Self::Mouse(timestamp) => timestamp,
        }
    }
}

pub(crate) struct Dashboard {
    providers: Vec<Provider>,
    warning: Option<String>,
    settings: Value,
    controls: Vec<Control>,
    settings_editable: bool,
    page: Page,
    history: Option<HistoryView>,
    logs: Vec<String>,
    response: Vec<String>,
    scroll: usize,
    column: u16,
    reading: Option<HistoryCursor>,
    command: Option<Vec<u8>>,
    message: Option<String>,
    provider: usize,
    usage_row: UsageRow,
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
            usage_row: UsageRow::Provider,
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
        let selected = self.providers.get(self.provider);
        let selected_id = selected.map(|provider| provider.id.clone());
        let selected_window = selected
            .and_then(|provider| self.selected_window(provider))
            .map(|window| window.label.clone());
        self.providers = state.providers;
        self.settings = state.settings;
        self.controls = preferences::controls(&self.settings, self.settings_editable);
        if self.page == Page::Settings {
            self.scroll = self.scroll.min(self.controls.len().saturating_sub(1));
        }
        self.provider = selected_id
            .as_ref()
            .and_then(|id| self.providers.iter().position(|provider| provider.id == *id))
            .unwrap_or_else(|| self.provider.min(self.providers.len().saturating_sub(1)));
        self.usage_row = self
            .providers
            .get(self.provider)
            .filter(|provider| Some(&provider.id) == selected_id.as_ref())
            .map_or(UsageRow::Provider, |provider| match self.usage_row {
                UsageRow::Window(_) => selected_window
                    .as_ref()
                    .and_then(|label| provider.windows.iter().position(|window| window.label == *label))
                    .map_or(UsageRow::Provider, UsageRow::Window),
                UsageRow::Overview(fact, _) if fact >= provider.overview.len() => UsageRow::Provider,
                row => row,
            });
    }

    fn selected_window<'a>(&self, provider: &'a Provider) -> Option<&'a crate::snapshot::Window> {
        match self.usage_row {
            UsageRow::Window(index) => provider.windows.get(index),
            _ => None,
        }
    }

    pub(crate) fn navigate(&mut self, action: Navigation, area: Rect) -> Option<Command> {
        match action {
            Navigation::Left => return self.move_horizontal(false),
            Navigation::Right => return self.move_horizontal(true),
            Navigation::NextGroup => self.next_group(),
            Navigation::Up | Navigation::Down => self.move_vertical(action, dashboard_sections(area)[1]),
        }
        None
    }

    fn next_group(&mut self) {
        match self.page {
            Page::Usage => {
                self.provider = (self.provider + 1) % self.providers.len().max(1);
                self.usage_row = UsageRow::Provider;
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

    fn move_vertical(&mut self, action: Navigation, content: Rect) {
        if self.page == Page::Usage {
            (self.provider, self.usage_row) = usage::step(
                content,
                &self.providers,
                (self.provider, self.usage_row),
                action == Navigation::Down,
            );
        } else {
            self.reading = None;
            let length = match self.page {
                Page::Settings => self.controls.len(),
                Page::History => self.history.as_ref().map_or(0, |history| history.data.series.len()),
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
                if let Some(history) = &self.history {
                    self.reading = history
                        .step(self.reading.map(HistoryCursor::timestamp), if forward { 1 } else { -1 })
                        .map(HistoryCursor::Keyboard);
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

    pub(crate) fn mouse(&mut self, event: MouseEvent, area: Rect) -> Option<MouseAction> {
        let (MouseEvent::Move(column, row) | MouseEvent::Click(column, row) | MouseEvent::Scroll(column, row, _)) =
            event
        else {
            self.clear_history_hover();
            return None;
        };
        if !usable(area) {
            return None;
        }
        let at = Position::new(column, row);
        if !area.contains(at) {
            self.clear_history_hover();
            return None;
        }
        let [_, content, _, footer] = dashboard_sections(area);
        if self.editing() {
            return match event {
                MouseEvent::Click(..) => self.click_editor(footer, at),
                _ => None,
            };
        }
        match event {
            MouseEvent::Scroll(.., navigation) => {
                self.scroll_mouse(navigation, content);
                None
            }
            MouseEvent::Click(..) if footer.contains(at) => self.click_footer(footer, at),
            MouseEvent::Click(..) => self.click_content(content, at),
            MouseEvent::Move(..) | MouseEvent::Leave => self.inspect_history(content, at),
        }
    }

    fn click_editor(&mut self, footer: Rect, at: Position) -> Option<MouseAction> {
        let [_, run, cancel] = editor_sections(footer);
        if run.contains(at) {
            return self
                .edit(b'\r')
                .and_then(|text| self.parse(&text))
                .map(MouseAction::Command);
        }
        if cancel.contains(at) {
            self.edit(27);
        }
        None
    }

    fn scroll_mouse(&mut self, navigation: Navigation, content: Rect) {
        match navigation {
            Navigation::Up | Navigation::Down => self.move_vertical(navigation, content),
            Navigation::Left | Navigation::Right
                if matches!(self.page, Page::Logs | Page::Help | Page::Detail | Page::Result) =>
            {
                self.move_horizontal(navigation == Navigation::Right);
            }
            _ => {}
        }
    }

    fn click_footer(&mut self, area: Rect, at: Position) -> Option<MouseAction> {
        self.clear_history_hover();
        let (_, _, target) = footer_links(area).find(|(hit, _, _)| hit.contains(at))?;
        match target {
            FooterTarget::Page(page) => Some(MouseAction::Show(page)),
            FooterTarget::Command => {
                self.start_command();
                None
            }
            FooterTarget::Quit => Some(MouseAction::Quit),
        }
    }

    fn click_content(&mut self, content: Rect, at: Position) -> Option<MouseAction> {
        match self.page {
            Page::Usage => {
                (self.provider, self.usage_row) =
                    usage::hit(content, &self.providers, (self.provider, self.usage_row), at)?;
                Some(MouseAction::Show(Page::Detail))
            }
            Page::Settings if self.settings_editable => {
                let (index, hit) = preferences::hit(content, &self.controls, self.scroll, at)?;
                self.scroll = index;
                let command = match hit {
                    SettingsHit::Edit => self.edit_setting(),
                    SettingsHit::Adjust(direction) => self.controls[index].adjust(direction),
                    SettingsHit::Focus => None,
                };
                command.map(MouseAction::Command)
            }
            Page::History => {
                let history = self.history.as_ref()?;
                match pages::history_hit(content, history, self.scroll, at) {
                    Some(HistoryHit::Series(index)) => {
                        self.scroll = index;
                        self.reading = None;
                        self.visibility(false).map(MouseAction::Command)
                    }
                    hit => {
                        self.inspect_history_hit(hit);
                        None
                    }
                }
            }
            _ => None,
        }
    }

    fn inspect_history(&mut self, content: Rect, at: Position) -> Option<MouseAction> {
        if self.page == Page::History {
            let history = self.history.as_ref()?;
            self.inspect_history_hit(pages::history_hit(content, history, self.scroll, at));
        }
        None
    }

    fn inspect_history_hit(&mut self, hit: Option<HistoryHit>) {
        if let Some(HistoryHit::Reading(timestamp)) = hit {
            self.reading = Some(HistoryCursor::Mouse(timestamp));
        } else {
            self.clear_history_hover();
        }
    }

    fn clear_history_hover(&mut self) {
        if matches!(self.reading, Some(HistoryCursor::Mouse(_))) {
            self.reading = None;
        }
    }

    pub(crate) fn edit_setting(&mut self) -> Option<Command> {
        if self.page != Page::Settings || !self.settings_editable {
            return None;
        }
        self.controls.get(self.scroll)?.adjust(1)
    }

    pub(crate) fn visibility(&self, isolate: bool) -> Option<Command> {
        if self.page != Page::History {
            return None;
        }
        let history = &self.history.as_ref()?.data;
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
        match pages::parse(text, self.history.as_ref().map(|view| &view.data)) {
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
                let view = HistoryView::new(history);
                if active {
                    self.scroll = self.scroll.min(view.data.series.len().saturating_sub(1));
                    self.reading = self
                        .reading
                        .filter(|reading| view.reading_index(reading.timestamp()).is_some());
                }
                self.history = Some(view);
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
                let window = self.selected_window(provider);
                std::iter::once(format!("{} · {}", provider.name(), provider.status))
                    .chain(
                        window
                            .into_iter()
                            .flat_map(|window| usage::window_details(window, &self.settings)),
                    )
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
        if !usable(area) {
            frame.render_widget(Paragraph::new("Resize to at least 34 × 14").yellow(), area);
            return;
        }

        let [title, content, status, help] = dashboard_sections(area);
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
                (self.provider, self.usage_row),
                &self.settings,
                self.warning.is_some(),
            ),
            Page::Settings => preferences::draw(frame, content, &self.controls, self.scroll, self.settings_editable),
            Page::History if self.history.is_some() => {
                pages::draw_history(
                    frame,
                    content,
                    self.history.as_ref().expect("history is loaded"),
                    (self.scroll, self.reading.map(HistoryCursor::timestamp)),
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

        if let Some(input) = &self.command {
            let [draft, run, cancel] = editor_sections(help);
            let draft_text = Line::from(format!(":{}", String::from_utf8_lossy(input)));
            let scroll = draft_text.width().saturating_sub(usize::from(draft.width)) as u16;
            frame.render_widget(Paragraph::new(draft_text).scroll((0, scroll)).cyan(), draft);
            frame.render_widget(Line::from(" Run ").cyan().bold(), run);
            frame.render_widget(Line::from(" Cancel ").cyan().bold(), cancel);
        } else {
            let tabs = footer_links(help).map(|(_, text, target)| {
                let label = Span::raw(text);
                if matches!(target, FooterTarget::Page(page) if page == self.page) {
                    label.cyan().bold()
                } else {
                    label.dim()
                }
            });
            let line = Line::from(tabs.collect::<Vec<_>>());
            frame.render_widget(line, help);
        }
    }
}

fn usable(area: Rect) -> bool {
    area.width >= 34 && area.height >= 14
}

#[derive(Clone, Copy)]
enum FooterTarget {
    Page(Page),
    Command,
    Quit,
}

fn footer_links(area: Rect) -> impl Iterator<Item = (Rect, &'static str, FooterTarget)> {
    let labels = if area.width >= 66 {
        [
            " 1 Usage",
            "  2 History",
            "  3 Settings",
            "  4 Logs",
            "  ? Help",
            "  : Cmd",
            "  q Quit",
        ]
    } else {
        [" 1 Use", " 2 Hist", " 3 Set", " 4 Log", " ?", " :", " q"]
    };
    let actions = [
        FooterTarget::Page(Page::Usage),
        FooterTarget::Page(Page::History),
        FooterTarget::Page(Page::Settings),
        FooterTarget::Page(Page::Logs),
        FooterTarget::Page(Page::Help),
        FooterTarget::Command,
        FooterTarget::Quit,
    ];
    labels
        .into_iter()
        .zip(actions)
        .scan(area.x, move |column, (label, action)| {
            let width = label.len() as u16;
            let hit = Rect::new(*column, area.y, width, area.height);
            *column += width;
            Some((hit, label, action))
        })
}

fn editor_sections(area: Rect) -> [Rect; 3] {
    Layout::horizontal([Constraint::Min(0), Constraint::Length(5), Constraint::Length(8)]).areas(area)
}

fn dashboard_sections(area: Rect) -> [Rect; 4] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area)
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
