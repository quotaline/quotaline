//! Decodes terminal keys and mouse reports.

use crate::{dashboard::MouseEvent, pages::Page};

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
