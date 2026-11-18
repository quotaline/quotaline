//! Decodes keystrokes for terminal navigation.

use crate::pages::Page;

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
    IgnoreSequence,
}

impl Keys {
    pub(crate) fn push(&mut self, byte: u8) -> Option<Action> {
        match std::mem::take(self) {
            Self::Plain => match byte {
                b'\x1b' => {
                    *self = Self::Escape;
                    None
                }
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
                if matches!(byte, b'[' | b'O') {
                    *self = Self::Sequence;
                }
                None
            }
            Self::Sequence if byte == b'A' => Some(Action::Navigate(Navigation::Up)),
            Self::Sequence if byte == b'B' => Some(Action::Navigate(Navigation::Down)),
            Self::Sequence if byte == b'C' => Some(Action::Navigate(Navigation::Right)),
            Self::Sequence if byte == b'D' => Some(Action::Navigate(Navigation::Left)),
            Self::Sequence if byte == b'Z' => Some(Action::Navigate(Navigation::NextGroup)),
            Self::Sequence | Self::IgnoreSequence => {
                if (0x20..=0x3f).contains(&byte) {
                    *self = Self::IgnoreSequence;
                }
                None
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/keys.rs"]
mod tests;
