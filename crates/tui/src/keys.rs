//! Decodes terminal keys and mouse reports.

use ratatui::crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};

use crate::{dashboard::MouseEvent, pages::Page};

/// Encodes a terminal event as the xterm bytes `Keys` decodes, so every platform shares one decoder. `motion`
/// passes pointer movement, which only the history chart asks for.
pub(crate) fn encode(event: &Event, motion: bool) -> Vec<u8> {
    match event {
        // Windows reports key releases as well as presses.
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                vec![(character as u8) & 0x1f]
            }
            // crossterm reads an escape followed by a key as that key with Alt held.
            KeyCode::Char(character) if key.modifiers.contains(KeyModifiers::ALT) => {
                format!("\x1b{character}").into_bytes()
            }
            KeyCode::Char(character) => character.to_string().into_bytes(),
            KeyCode::Enter => b"\r".to_vec(),
            KeyCode::Tab => b"\t".to_vec(),
            KeyCode::BackTab => b"\x1b[Z".to_vec(),
            KeyCode::Backspace => vec![0x7f],
            KeyCode::Esc => vec![0x1b],
            KeyCode::Up => b"\x1b[A".to_vec(),
            KeyCode::Down => b"\x1b[B".to_vec(),
            KeyCode::Right => b"\x1b[C".to_vec(),
            KeyCode::Left => b"\x1b[D".to_vec(),
            _ => Vec::new(),
        },
        Event::Mouse(report) => {
            let button = match report.kind {
                MouseEventKind::Down(MouseButton::Left) => 0,
                MouseEventKind::Moved | MouseEventKind::Drag(_) if motion => 35,
                MouseEventKind::ScrollUp => 64,
                MouseEventKind::ScrollDown => 65,
                MouseEventKind::ScrollLeft => 66,
                MouseEventKind::ScrollRight => 67,
                _ => return Vec::new(),
            };
            format!("\x1b[<{button};{};{}M", report.column + 1, report.row + 1).into_bytes()
        }
        _ => Vec::new(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Action {
    Navigate(Navigation),
    Reload,
    Quit,
    Show(Page),
    StartCommand,
    EditSetting,
    Visibility(bool),
    Refresh,
    SignIn,
    History(Option<bool>),
    Edit(u8),
    Mouse(MouseEvent),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Navigation {
    Up,
    Down,
    Left,
    Right,
    NextGroup,
}

#[derive(Default)]
pub(crate) enum Keys {
    #[default]
    Plain,
    Escape,
    Sequence,
    Application,
    IgnoreSequence,
    Mouse(Option<Vec<u8>>),
    LegacyMouse(u8),
}

impl Keys {
    pub(crate) fn push(&mut self, byte: u8, editing: bool) -> (Option<Action>, bool) {
        let cancel = editing && matches!(self, Self::Escape) && !matches!(byte, b'[' | b'O');
        if cancel {
            *self = Self::Plain;
        }
        (self.decode(byte, editing && !cancel), cancel)
    }

    pub(crate) fn cancel_escape(&mut self, editing: bool) -> bool {
        if editing && matches!(self, Self::Escape) {
            *self = Self::Plain;
            return true;
        }
        false
    }

    fn decode(&mut self, byte: u8, editing: bool) -> Option<Action> {
        match std::mem::take(self) {
            Self::Plain => match byte {
                b'\x1b' => {
                    *self = Self::Escape;
                    None
                }
                byte if editing => Some(Action::Edit(byte)),
                b'k' => Some(Action::Navigate(Navigation::Up)),
                b'j' => Some(Action::Navigate(Navigation::Down)),
                b'\t' => Some(Action::Navigate(Navigation::NextGroup)),
                b'r' => Some(Action::Reload),
                b'q' => Some(Action::Quit),
                b'1' => Some(Action::Show(Page::Usage)),
                b'2' => Some(Action::Show(Page::History)),
                b'3' => Some(Action::Show(Page::Settings)),
                b'4' => Some(Action::Show(Page::Logs)),
                b'?' => Some(Action::Show(Page::Help)),
                b'd' => Some(Action::Show(Page::Detail)),
                b':' => Some(Action::StartCommand),
                b'\r' | b'\n' => Some(Action::EditSetting),
                b'v' => Some(Action::Visibility(false)),
                b'i' => Some(Action::Visibility(true)),
                b'R' => Some(Action::Refresh),
                b's' => Some(Action::SignIn),
                b'[' => Some(Action::History(Some(false))),
                b']' => Some(Action::History(Some(true))),
                b'n' => Some(Action::History(None)),
                _ => None,
            },
            Self::Escape => {
                *self = match byte {
                    b'[' => Self::Sequence,
                    b'O' => Self::Application,
                    _ => Self::Plain,
                };
                None
            }
            sequence @ (Self::Sequence | Self::Application) => match byte {
                b'A' => Some(Action::Navigate(Navigation::Up)),
                b'B' => Some(Action::Navigate(Navigation::Down)),
                b'C' => Some(Action::Navigate(Navigation::Right)),
                b'D' => Some(Action::Navigate(Navigation::Left)),
                b'Z' => Some(Action::Navigate(Navigation::NextGroup)),
                b'<' if matches!(sequence, Self::Sequence) => {
                    *self = Self::Mouse(Some(Vec::with_capacity(17)));
                    None
                }
                b'M' if matches!(sequence, Self::Sequence) => {
                    *self = Self::LegacyMouse(3);
                    None
                }
                _ => self.ignore_sequence(byte),
            },
            Self::LegacyMouse(1) => None,
            Self::LegacyMouse(remaining) => {
                *self = Self::LegacyMouse(remaining - 1);
                None
            }
            Self::Mouse(report) => {
                *self = match byte {
                    b'M' => return report.as_deref().and_then(mouse_report).map(Action::Mouse),
                    b'\x1b' => Self::Escape,
                    0x20..=0x3f => Self::Mouse(report.filter(|bytes| bytes.len() < 17).map(|mut bytes| {
                        bytes.push(byte);
                        bytes
                    })),
                    0x40..=0x7e => Self::Plain,
                    _ => Self::Mouse(None),
                };
                None
            }
            Self::IgnoreSequence => self.ignore_sequence(byte),
        }
    }

    fn ignore_sequence(&mut self, byte: u8) -> Option<Action> {
        if (0x20..=0x3f).contains(&byte) {
            *self = Self::IgnoreSequence;
        }
        None
    }
}

fn mouse_report(report: &[u8]) -> Option<MouseEvent> {
    if !report.iter().all(|byte| byte.is_ascii_digit() || *byte == b';') {
        return None;
    }
    let (button, position) = std::str::from_utf8(report)
        .expect("mouse parameters are ASCII")
        .split_once(';')?;
    let (column, row) = position.split_once(';')?;
    let column = column.parse::<u16>().ok()?.checked_sub(1)?;
    let row = row.parse::<u16>().ok()?.checked_sub(1)?;
    match button.parse::<u8>().ok()? & 0b1110_0011 {
        0 => Some(MouseEvent::Click(column, row)),
        32..=35 => Some(MouseEvent::Move(column, row)),
        64 => Some(MouseEvent::Scroll(column, row, Navigation::Up)),
        65 => Some(MouseEvent::Scroll(column, row, Navigation::Down)),
        66 => Some(MouseEvent::Scroll(column, row, Navigation::Left)),
        67 => Some(MouseEvent::Scroll(column, row, Navigation::Right)),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../tests/unit/keys.rs"]
mod tests;
