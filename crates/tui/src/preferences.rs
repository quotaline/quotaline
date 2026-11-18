//! Presents typed controls for the desktop settings document.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::Stylize,
    text::Line,
    widgets::{Block, Paragraph, Wrap},
};
use serde_json::{Value, json};

use crate::desktop::Command;

pub(crate) struct Control {
    pub(crate) group: &'static str,
    label: String,
    key: String,
    value: Value,
    editor: Editor,
}

#[derive(Clone, Copy)]
enum Editor {
    Switch(&'static str, &'static str),
    Number {
        min: i64,
        max: i64,
        step: i64,
        unit: &'static str,
    },
    Choice(&'static [(&'static str, &'static str)]),
    Interval(&'static str),
}

impl Control {
    pub(crate) fn adjust(&self, direction: i32) -> Option<Command> {
        let value = match self.editor {
            Editor::Switch(_, _) => self.value.as_bool().map(|enabled| json!(!enabled)),
            Editor::Number { min, max, step, .. } => self
                .value
                .as_i64()
                .map(|value| json!((value + step * i64::from(direction)).clamp(min, max))),
            Editor::Choice(choices) => choices.iter().position(|(id, _)| self.value == *id).map(|current| {
                let next = (current as i32 + direction).rem_euclid(choices.len() as i32) as usize;
                json!(choices[next].0)
            }),
            Editor::Interval(id) => return Some(Command::StepInterval(id.into(), direction)),
        };
        value.map(|value| Command::Set(self.key.clone(), value.to_string()))
    }

    fn stepped(&self) -> bool {
        matches!(
            self.editor,
            Editor::Number { .. } | Editor::Choice(_) | Editor::Interval(_)
        )
    }

    fn formatted(&self) -> String {
        match self.editor {
            Editor::Switch(on, off) => self
                .value
                .as_bool()
                .map_or("Unknown", |enabled| if enabled { on } else { off })
                .into(),
            Editor::Number { unit, .. } => format!("{} {unit}", self.value),
            Editor::Interval(_) => format!("{} min", self.value),
            Editor::Choice(choices) => choices
                .iter()
                .find(|(id, _)| self.value == *id)
                .map_or("Unknown", |(_, label)| *label)
                .into(),
        }
    }
}

pub(crate) fn controls(settings: &Value, show_model_filter: bool) -> Vec<Control> {
    let toggled = Editor::Switch("On", "Off");
    let mut controls = Vec::new();
    for provider in quotaline_catalog::ALL {
        if let Some(enabled) = settings["providers"].get(provider.id) {
            controls.push(Control {
                group: "Providers",
                label: format!("{} collection", provider.name),
                key: format!("provider:{}", provider.id),
                value: enabled.clone(),
                editor: toggled,
            });
        }
        if let Some(interval) = settings["interval_minutes"].get(provider.id)
            && settings["providers"][provider.id] != false
        {
            controls.push(Control {
                group: "Providers",
                label: format!("{} refresh interval", provider.name),
                key: provider.id.into(),
                value: interval.clone(),
                editor: Editor::Interval(provider.id),
            });
        }
    }
    let choices = [
        (
            "Display",
            "Quota numbers",
            "show_as",
            Editor::Choice(&[("used", "Used"), ("remaining", "Remaining")]),
        ),
        (
            "Display",
            "Quota decimal places",
            "decimals",
            Editor::Number {
                min: 0,
                max: 2,
                step: 1,
                unit: "places",
            },
        ),
        (
            "Display",
            "Account identity",
            "hide_identity",
            Editor::Switch("Hidden", "Visible"),
        ),
        (
            "Display",
            "Unused models in model list",
            "hide_unused_models",
            Editor::Switch("Hidden", "Visible"),
        ),
        (
            "Data",
            "Keep history for",
            "retention_days",
            Editor::Number {
                min: 7,
                max: 365,
                step: 1,
                unit: "days",
            },
        ),
        (
            "Data",
            "Refresh analytics every",
            "analytics_interval_minutes",
            Editor::Number {
                min: 5,
                max: 120,
                step: 5,
                unit: "min",
            },
        ),
        (
            "Data",
            "Expected usage each week",
            "workdays",
            Editor::Number {
                min: 4,
                max: 7,
                step: 1,
                unit: "days",
            },
        ),
        ("Data", "Allow credential refresh", "allow_token_refresh", toggled),
        ("Data", "Use shared demo data", "demo", toggled),
        ("History", "Stack compatible history series", "history_stacked", toggled),
        ("Diagnostics", "Detailed logging", "detailed_logging", toggled),
    ];
    controls.extend(choices.into_iter().filter_map(|(group, label, key, editor)| {
        if key == "hide_unused_models" && !show_model_filter {
            return None;
        }
        Some(Control {
            group,
            label: label.into(),
            key: key.into(),
            value: settings.get(key)?.clone(),
            editor,
        })
    }));
    controls
}

pub(crate) fn draw(frame: &mut Frame<'_>, area: Rect, controls: &[Control], selected: usize, editable: bool) {
    let title = if editable {
        " Settings "
    } else {
        " Settings · read only "
    };
    let panel = Block::bordered().title(title);
    let inner = panel.inner(area);
    frame.render_widget(panel, area);
    if controls.is_empty() {
        frame.render_widget(
            Paragraph::new("Open the desktop source to see its settings.").wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }

    let [rows, hints] = settings_sections(inner);
    for (row, item) in visible_rows(rows, controls, selected) {
        match item {
            SettingsRow::Group(group) => frame.render_widget(Line::from(format!("  {group}")).cyan().bold(), row),
            SettingsRow::Control(index) => draw_control(frame, row, &controls[index], index == selected, editable),
        }
    }
    let hint = if editable {
        "↑↓ Move   Tab Next group\n←→ Adjust   Enter Activate"
    } else {
        "↑↓ Move   Tab Next group\nChange these settings in the desktop app"
    };
    frame.render_widget(Paragraph::new(hint).dim(), hints);
}

fn settings_sections(inner: Rect) -> [Rect; 2] {
    Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner)
}

enum SettingsRow {
    Group(&'static str),
    Control(usize),
}

fn visible_rows(rows: Rect, controls: &[Control], selected: usize) -> impl Iterator<Item = (Rect, SettingsRow)> {
    let mut items = Vec::new();
    let mut group = "";
    let mut focused_row = 0;
    for (index, control) in controls.iter().enumerate() {
        if group != control.group {
            group = control.group;
            items.push(SettingsRow::Group(group));
        }
        if index == selected {
            focused_row = items.len();
        }
        items.push(SettingsRow::Control(index));
    }
    let start = focused_row.saturating_sub(usize::from(rows.height).saturating_sub(2));
    items
        .into_iter()
        .skip(start)
        .take(rows.height.into())
        .enumerate()
        .map(move |(offset, item)| (Rect::new(rows.x, rows.y + offset as u16, rows.width, 1), item))
}

fn draw_control(frame: &mut Frame<'_>, row: Rect, control: &Control, active: bool, editable: bool) {
    let label = format!("{} {}", if active { "›" } else { " " }, control.label);
    let label = Line::from(label);
    let value = Line::from(control.formatted());
    let [label_area, value_area, previous, next] = control_sections(row, editable && control.stepped(), value.width());
    frame.render_widget(if active { label.cyan().bold() } else { label }, label_area);
    frame.render_widget(if active { value.bold() } else { value }, value_area);
    frame.render_widget(Line::from("‹").cyan(), previous);
    frame.render_widget(Line::from("›").cyan(), next);
}

fn control_sections(row: Rect, stepped: bool, value_width: usize) -> [Rect; 4] {
    let [label, value] = Layout::horizontal([Constraint::Percentage(62), Constraint::Fill(1)])
        .spacing(2)
        .areas(row);
    if stepped {
        let cluster = Rect {
            width: value_width.saturating_add(4).min(usize::from(value.width)) as u16,
            ..value
        };
        let [previous, value, next] =
            Layout::horizontal([Constraint::Length(1), Constraint::Fill(1), Constraint::Length(1)])
                .spacing(1)
                .areas(cluster);
        [label, value, previous, next]
    } else {
        [label, value, Rect::ZERO, Rect::ZERO]
    }
}

#[cfg_attr(test, derive(Debug, PartialEq, Eq))]
pub(crate) enum SettingsHit {
    Edit,
    Adjust(i32),
    Focus,
}

pub(crate) fn hit(area: Rect, controls: &[Control], selected: usize, at: Position) -> Option<(usize, SettingsHit)> {
    let [rows, _] = settings_sections(Block::bordered().inner(area));
    let (row, SettingsRow::Control(index)) =
        visible_rows(rows, controls, selected).find(|(row, _)| row.contains(at))?
    else {
        return None;
    };
    let control = &controls[index];
    let action = if control.stepped() {
        let [_, _, previous, next] = control_sections(row, true, Line::from(control.formatted()).width());
        if previous.contains(at) {
            SettingsHit::Adjust(-1)
        } else if next.contains(at) {
            SettingsHit::Adjust(1)
        } else {
            SettingsHit::Focus
        }
    } else {
        SettingsHit::Edit
    };
    Some((index, action))
}

#[cfg(test)]
#[path = "../tests/unit/preferences.rs"]
mod tests;
