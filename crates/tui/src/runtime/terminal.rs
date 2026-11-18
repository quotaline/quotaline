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
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};
use termion::{raw::IntoRawMode, screen::IntoAlternateScreen};

use super::Source;
use crate::{
    dashboard::Dashboard,
    desktop::{Backend, Command, Output, State},
    keys::{Action, Keys},
    pages::{self, Page},
    snapshot::LocalSnapshot,
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
    let source_label = match &source {
        Source::Desktop => String::new(),
        Source::Local(LocalSnapshot::File(path)) => path.to_string_lossy().replace(char::is_control, ""),
        Source::Local(LocalSnapshot::Demo) => "Demo data".into(),
    };
    let mut dashboard = Dashboard::new(cfg!(target_os = "linux") && matches!(source, Source::Desktop));
    let mut keys = Keys::default();
    let (jobs, responses) = worker(source);
    let mut pending_snapshot = false;
    let mut last_size = None;
    let mut refresh_at = Instant::now();
    let mut redraw = true;

    while !interrupted.load(Ordering::Relaxed) {
        while let Ok(response) = responses.try_recv() {
            match response {
                Response::Snapshot(snapshot) => {
                    dashboard.apply_snapshot(snapshot);
                    pending_snapshot = false;
                }
                Response::Command(page, output) => {
                    dashboard.apply_output(page, output);
                    refresh_at = Instant::now();
                }
            }
            redraw = true;
        }
        if !pending_snapshot && Instant::now() >= refresh_at {
            jobs.send(Job::Snapshot).map_err(io::Error::other)?;
            pending_snapshot = true;
            refresh_at = Instant::now() + Duration::from_secs(2);
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
        if byte[0] == 3 {
            break;
        }
        if dashboard.editing() {
            if let Some(text) = dashboard.edit(byte[0])
                && let Some(command) = dashboard.parse(&text)
            {
                send(&jobs, &mut dashboard, command)?;
            }
            redraw = true;
            continue;
        }
        let Some(action) = keys.push(byte[0]) else {
            continue;
        };
        let command = match action {
            Action::Quit => break,
            Action::Reload => {
                refresh_at = Instant::now();
                dashboard.page().command()
            }
            Action::Navigate(navigation) => dashboard.navigate(navigation),
            Action::Show(page) => {
                dashboard.show(page);
                page.command()
            }
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
        };
        if let Some(command) = command {
            send(&jobs, &mut dashboard, command)?;
        }
        redraw = true;
    }
    Ok(())
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
