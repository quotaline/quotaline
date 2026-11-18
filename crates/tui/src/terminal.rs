//! Runs the dashboard in an alternate terminal screen.

use ratatui::{Terminal, backend::TermionBackend};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    io::{dup, retry_on_intr},
};
use std::{
    fs::File,
    io::{self, IsTerminal, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use termion::{raw::IntoRawMode, screen::IntoAlternateScreen};

use crate::{Source, dashboard::Dashboard};

pub(crate) fn run(source: Source) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("standard input and output must be terminals"));
    }

    let interrupted = Arc::new(AtomicBool::new(false));
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&interrupted))?;
    }

    let mut input = File::from(dup(io::stdin())?);
    let output = File::from(dup(io::stdout())?)
        .into_raw_mode()?
        .into_alternate_screen()?;
    let mut terminal = Terminal::new(TermionBackend::new(output))?;
    let source_label = source.to_string();
    let mut dashboard = Dashboard::new();
    let mut keys = Keys::default();
    let mut last_size = None;
    let mut refresh_at = Instant::now();
    let mut redraw = true;

    while !interrupted.load(Ordering::Relaxed) {
        if Instant::now() >= refresh_at {
            dashboard.refresh(&source);
            refresh_at = Instant::now() + Duration::from_secs(2);
            redraw = true;
        }

        let size = terminal.size()?;
        if redraw || last_size != Some(size) {
            terminal.draw(|frame| dashboard.draw(frame, &source_label))?;
            last_size = Some(size);
            redraw = false;
        }

        let mut reader = [PollFd::new(&input, PollFlags::IN)];
        let timeout = Timespec {
            tv_sec: 0,
            tv_nsec: 100_000_000,
        };
        if retry_on_intr(|| poll(&mut reader, Some(&timeout)))? == 0 {
            continue;
        }
        let mut byte = [0];
        input.read_exact(&mut byte)?;
        match keys.push(byte[0]) {
            Some(Action::Quit) => break,
            Some(Action::Reload) => refresh_at = Instant::now(),
            Some(Action::Navigate(navigation)) => {
                dashboard.navigate(navigation);
                redraw = true;
            }
            None => {}
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Navigate(Navigation),
    Reload,
    Quit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Navigation {
    Up,
    Down,
    TogglePanel,
}

#[derive(Default)]
enum Keys {
    #[default]
    Plain,
    Escape,
    Sequence,
    IgnoreSequence,
}

impl Keys {
    fn push(&mut self, byte: u8) -> Option<Action> {
        match std::mem::take(self) {
            Self::Plain => match byte {
                b'\x1b' => {
                    *self = Self::Escape;
                    None
                }
                b'k' => Some(Action::Navigate(Navigation::Up)),
                b'j' => Some(Action::Navigate(Navigation::Down)),
                b'\t' => Some(Action::Navigate(Navigation::TogglePanel)),
                b'r' => Some(Action::Reload),
                b'q' | 3 => Some(Action::Quit),
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
            Self::Sequence if byte == b'Z' => Some(Action::Navigate(Navigation::TogglePanel)),
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
#[path = "../tests/unit/terminal.rs"]
mod tests;
