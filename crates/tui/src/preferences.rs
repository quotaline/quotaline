//! Presents typed controls for the desktop settings document.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
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
    Text,
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
            Editor::Text => None,
        };
        value.map(|value| Command::Set(self.key.clone(), value.to_string()))
    }

    pub(crate) fn answer(&self) -> Option<String> {
        matches!(self.editor, Editor::Text).then(|| format!("set {} {}", self.key, self.value))
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
            Editor::Text => self.value.as_str().unwrap_or_default().replace(char::is_control, " "),
        }
    }
}

pub(crate) fn controls(settings: &Value) -> Vec<Control> {
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
            "Provider order",
            "order",
            Editor::Choice(&[("stable", "Configured"), ("usage", "Usage")]),
        ),
        (
            "Display",
            "Zero readings",
            "hide_zero",
            Editor::Switch("Hidden", "Visible"),
        ),
        (
            "Display",
            "Unused models",
            "hide_unused_models",
            Editor::Switch("Hidden", "Visible"),
        ),
        ("Display", "All providers in settings", "show_all_providers", toggled),
        (
            "Display",
            "Desktop menu-bar layout",
            "format",
            Editor::Choice(&[
                ("stacked", "Stacked"),
                ("inline", "Inline"),
                ("mini-bars", "Mini-bars"),
                ("countdown", "Countdown"),
                ("exhausted", "Exhausted"),
                ("custom", "Custom"),
            ]),
        ),
        ("Display", "Desktop menu-bar template", "template", Editor::Text),
        ("Display", "Fit desktop menu-bar space", "fit_to_space", toggled),
        (
            "Notifications",
            "Enable desktop notifications",
            "notifications",
            toggled,
        ),
        ("Notifications", "Quota reset alerts", "notify_resets", toggled),
        ("Notifications", "Sign-in alerts", "notify_sign_in", toggled),
        ("Notifications", "Pace alerts", "notify_pace", toggled),
        (
            "Notifications",
            "Credit expiry alerts",
            "notify_expiring_credits",
            toggled,
        ),
        ("Notifications", "Notification sound", "notify_sound", toggled),
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
        ("Data", "Write usage.json", "write_usage_json", toggled),
        ("Data", "Desktop demo data", "demo", toggled),
        ("History", "Stack compatible history series", "history_stacked", toggled),
        ("Diagnostics", "Detailed logging", "detailed_logging", toggled),
    ];
    controls.extend(choices.into_iter().filter_map(|(group, label, key, editor)| {
        if key == "template" && settings["format"] != "custom" {
            return None;
        }
        if group == "Notifications" && key != "notifications" && settings["notifications"] == false {
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

    let [rows, hints] = Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
    let mut items = Vec::new();
    let mut group = "";
    let mut focused_row = 0;
    for (index, control) in controls.iter().enumerate() {
        if group != control.group {
            group = control.group;
            items.push((Line::from(format!("  {group}")).cyan().bold(), Line::default()));
        }
        let label = format!("{} {}", if index == selected { "›" } else { " " }, control.label);
        let value = Line::from(control.formatted());
        if index == selected {
            focused_row = items.len();
            items.push((Line::from(label).cyan().bold(), value.bold()));
        } else {
            items.push((Line::from(label), value));
        }
    }
    let start = focused_row.saturating_sub(usize::from(rows.height).saturating_sub(2));
    for (offset, (label, value)) in items.into_iter().skip(start).take(rows.height.into()).enumerate() {
        let row = Rect::new(rows.x, rows.y + offset as u16, rows.width, 1);
        let [label_area, value_area] = Layout::horizontal([Constraint::Percentage(62), Constraint::Fill(1)])
            .spacing(2)
            .areas(row);
        frame.render_widget(label, label_area);
        frame.render_widget(value, value_area);
    }
    let hint = if editable {
        "↑↓ Move   Tab Next group\n←→ Adjust   Enter Toggle or edit"
    } else {
        "↑↓ Move   Tab Next group\nChange these settings in the desktop app"
    };
    frame.render_widget(Paragraph::new(hint).dim(), hints);
}
