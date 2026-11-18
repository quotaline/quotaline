//! Runs the dashboard in an alternate terminal screen.

use ratatui::{Terminal, backend::TermionBackend, layout::Rect};
use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    io::{dup, retry_on_intr},
};
use std::{
    fs::File,
    io::{self, IsTerminal, Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Instant,
};
use termion::{raw::IntoRawMode, screen::IntoAlternateScreen};

use super::Source;
use crate::{
    dashboard::{Dashboard, MouseAction, MouseEvent},
    desktop::{Backend, Command, Output, State},
    keys::{Action, Keys},
    pages::{self, Page},
    snapshot::LocalSnapshot,
    snapshot_schedule::SnapshotSchedule,
};

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
    let mut mouse = MouseTracking {
        output: File::from(dup(io::stdout())?),
        mode: MouseMode::Disabled,
    };
    let source_label = match &source {
        Source::Desktop => String::new(),
        Source::Local(LocalSnapshot::File(path)) => path.to_string_lossy().replace(char::is_control, ""),
        Source::Local(LocalSnapshot::Demo) => "Demo data".into(),
    };
    #[cfg(target_os = "linux")]
    let settings_editable = matches!(source, Source::Desktop);
    #[cfg(target_os = "macos")]
    let settings_editable = false;
    let mut dashboard = Dashboard::new(settings_editable);
    let mut keys = Keys::default();
    let (jobs, responses) = worker(source);
    let mut snapshots = SnapshotSchedule::new(Instant::now());
    let mut last_size = None;
    let mut redraw = true;

    while !interrupted.load(Ordering::Relaxed) {
        while let Ok(response) = responses.try_recv() {
            match response {
                Response::Snapshot(snapshot) => {
                    dashboard.apply_snapshot(snapshot);
                    snapshots.finish();
                }
                Response::Command(page, output) => {
                    dashboard.apply_output(page, output);
                    snapshots.reload(Instant::now());
                }
            }
            redraw = true;
        }
        if snapshots.start(Instant::now()) {
            jobs.send(Job::Snapshot).map_err(io::Error::other)?;
        }

        let size = terminal.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        let mouse_mode = if dashboard.page() == Page::History && !dashboard.editing() {
            MouseMode::Motion
        } else {
            MouseMode::Buttons
        };
        let resized = last_size != Some(size);
        if resized || (mouse.mode == MouseMode::Motion && mouse_mode != MouseMode::Motion) {
            dashboard.mouse(MouseEvent::Leave, area);
        }
        if mouse_mode != mouse.mode {
            mouse.set(mouse_mode)?;
        }
        if redraw || resized {
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
            if keys.cancel_escape(dashboard.editing()) {
                dashboard.edit(27);
                redraw = true;
            }
            continue;
        }
        let mut byte = [0];
        input.read_exact(&mut byte)?;
        if byte[0] == 3 {
            break;
        }
        let (action, canceled) = keys.push(byte[0], dashboard.editing());
        if canceled {
            dashboard.edit(27);
            redraw = true;
        }
        let Some(action) = action else {
            continue;
        };
        if dashboard.editing() && matches!(action, Action::Navigate(_)) {
            continue;
        }
        let command = match action {
            Action::Quit => break,
            Action::Reload => {
                snapshots.reload(Instant::now());
                dashboard.page().command()
            }
            Action::Navigate(navigation) => dashboard.navigate(navigation, area),
            Action::Show(page) => show(&mut dashboard, page),
            Action::StartCommand => {
                dashboard.start_command();
                None
            }
            Action::EditSetting => dashboard.edit_setting(),
            Action::Visibility(isolate) => dashboard.visibility(isolate),
            Action::Refresh => Some(Command::Refresh(dashboard.selected_provider().unwrap_or("all").into())),
            Action::SignIn => dashboard
                .selected_provider()
                .map(|provider| Command::SignIn(provider.into())),
            Action::History(shift) => Some(dashboard.history_shift(shift)),
            Action::Mouse(event) => match dashboard.mouse(event, area) {
                Some(MouseAction::Quit) => break,
                Some(MouseAction::Show(page)) => show(&mut dashboard, page),
                Some(MouseAction::Command(command)) => Some(command),
                None => None,
            },
            Action::Edit(byte) => dashboard.edit(byte).and_then(|text| dashboard.parse(&text)),
        };
        if let Some(command) = command {
            send(&jobs, &mut dashboard, command)?;
        }
        redraw = true;
    }
    Ok(())
}

struct MouseTracking {
    output: File,
    mode: MouseMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MouseMode {
    Disabled,
    Buttons,
    Motion,
}

impl MouseTracking {
    fn set(&mut self, mode: MouseMode) -> io::Result<()> {
        self.mode = mode;
        self.output.write_all(match mode {
            MouseMode::Disabled => b"\x1b[?1000l\x1b[?1003l\x1b[?1006l",
            MouseMode::Buttons => b"\x1b[?1003l\x1b[?1006h\x1b[?1000h",
            MouseMode::Motion => b"\x1b[?1000l\x1b[?1006h\x1b[?1003h",
        })
    }
}

impl Drop for MouseTracking {
    fn drop(&mut self) {
        let _ = self.set(MouseMode::Disabled);
    }
}

fn show(dashboard: &mut Dashboard, page: Page) -> Option<Command> {
    dashboard.show(page);
    page.command()
}

fn send(jobs: &mpsc::Sender<Job>, dashboard: &mut Dashboard, command: Command) -> io::Result<()> {
    let page = pages::output_page(&command, dashboard.page());
    if page != dashboard.page() {
        dashboard.show(page);
    }
    jobs.send(Job::Command(page, command)).map_err(io::Error::other)
}

fn worker(source: Source) -> (mpsc::Sender<Job>, mpsc::Receiver<Response>) {
    let (send_job, jobs) = mpsc::channel();
    let (send_response, responses) = mpsc::channel();
    thread::spawn(move || {
        let mut backend = Backend::default();
        while let Ok(job) = jobs.recv() {
            let response = match job {
                Job::Snapshot => Response::Snapshot(match &source {
                    Source::Desktop => backend.usage(),
                    Source::Local(local) => local.read(),
                }),
                Job::Command(page, command) => Response::Command(
                    page,
                    if matches!(source, Source::Desktop) {
                        backend.execute(command)
                    } else {
                        Err(io::Error::other("This action requires the desktop source"))
                    },
                ),
            };
            let _ = send_response.send(response);
        }
    });
    (send_job, responses)
}

enum Job {
    Snapshot,
    Command(Page, Command),
}

enum Response {
    Snapshot(io::Result<State>),
    Command(Page, io::Result<Output>),
}
