//! Runs the dashboard in an alternate terminal screen.

use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        event, execute,
        terminal::{self as screen, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::Rect,
};
use std::{
    env,
    io::{self, IsTerminal, Stdout},
    path::PathBuf,
    process,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use super::Source;
use crate::{
    dashboard::{Dashboard, MouseAction, MouseEvent},
    desktop::{Backend, Command, Output, State},
    keys::{self, Action, Keys},
    pages::{self, Page},
    snapshot::LocalSnapshot,
    snapshot_schedule::SnapshotSchedule,
};

pub(crate) fn run(source: Source) -> io::Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(io::Error::other("standard input and output must be terminals"));
    }

    let interrupted = Arc::new(AtomicBool::new(false));
    #[cfg(unix)]
    for signal in [
        signal_hook::consts::SIGINT,
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
    ] {
        signal_hook::flag::register(signal, Arc::clone(&interrupted))?;
    }

    let mut session = Session::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let mut mouse = MouseTracking {
        mode: MouseMode::Disabled,
    };
    let source_label = match &source {
        Source::Live => String::new(),
        Source::Local(LocalSnapshot::File(path)) => path.to_string_lossy().replace(char::is_control, ""),
        Source::Local(LocalSnapshot::Demo) => "Demo data".into(),
    };
    let live = matches!(source, Source::Live);
    let mut dashboard = Dashboard::new(live);
    let mut keys = Keys::default();
    let (jobs, responses) = worker(source);
    let mut snapshots = SnapshotSchedule::new(Instant::now());
    let mut last_size = None;
    let mut redraw = true;

    'session: while !interrupted.load(Ordering::Relaxed) {
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

        if !event::poll(Duration::from_millis(100))? {
            if keys.cancel_escape(dashboard.editing()) {
                dashboard.edit(27);
                redraw = true;
            }
            continue;
        }
        for byte in keys::encode(&event::read()?, mouse.mode == MouseMode::Motion) {
            if byte == 3 {
                break 'session;
            }
            let (action, canceled) = keys.push(byte, dashboard.editing());
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
                Action::Quit => break 'session,
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
                    Some(MouseAction::Quit) => break 'session,
                    Some(MouseAction::Show(page)) => show(&mut dashboard, page),
                    Some(MouseAction::Command(command)) => Some(command),
                    None => None,
                },
                Action::Edit(byte) => dashboard.edit(byte).and_then(|text| dashboard.parse(&text)),
            };
            match command {
                // The login command needs this terminal, so it runs here before the engine refreshes the provider.
                Some(Command::SignIn(provider)) if live => {
                    let outcome = match login(&provider) {
                        Ok(login) => session.suspend(&mut terminal, &mut mouse, || login.run())?,
                        Err(error) => Err(error),
                    };
                    let signed_in = outcome.is_ok();
                    dashboard.apply_output(dashboard.page(), outcome.map(Output::Done));
                    if signed_in {
                        send(&jobs, &mut dashboard, Command::SignIn(provider))?;
                    }
                }
                Some(command) => send(&jobs, &mut dashboard, command)?,
                None => {}
            }
            redraw = true;
        }
    }
    Ok(())
}

/// Holds raw mode and the alternate screen, restoring the terminal however the dashboard exits.
struct Session;

impl Session {
    fn enter() -> io::Result<Self> {
        screen::enable_raw_mode()?;
        let session = Self;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(session)
    }

    /// Hands the terminal to `work`, then takes it back and repaints.
    fn suspend<T>(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        mouse: &mut MouseTracking,
        work: impl FnOnce() -> T,
    ) -> io::Result<T> {
        let mode = mouse.mode;
        mouse.set(MouseMode::Disabled)?;
        execute!(io::stdout(), LeaveAlternateScreen)?;
        screen::disable_raw_mode()?;
        let result = work();
        screen::enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        mouse.set(mode)?;
        // A new terminal repaints every cell, without the cursor query `Terminal::clear` makes.
        *terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
        Ok(result)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        let _ = screen::disable_raw_mode();
    }
}

/// A provider's own sign-in command, found on `PATH`.
struct Login {
    text: &'static str,
    program: PathBuf,
}

/// Finds the provider's sign-in command before the dashboard gives up the terminal.
fn login(provider: &str) -> io::Result<Login> {
    let text = quotaline_catalog::login(provider);
    let name = text.split_whitespace().next().unwrap_or_default();
    // Antigravity signs in through its own app, and the catalog says so instead of naming a command.
    let program = program(name).ok_or_else(|| io::Error::other(format!("{text}, then press R")))?;
    Ok(Login { text, program })
}

impl Login {
    fn run(&self) -> io::Result<String> {
        println!("Running {}; Quotaline resumes when it exits.", self.text);
        let status = process::Command::new(&self.program)
            .args(self.text.split_whitespace().skip(1))
            .status()?;
        if !status.success() {
            return Err(io::Error::other(format!("{} exited with {status}", self.text)));
        }
        Ok("Signed in; refreshing".into())
    }
}

/// Finds `name` on `PATH`, also trying the extensions Windows runs commands with.
fn program(name: &str) -> Option<PathBuf> {
    let extensions = env::var("PATHEXT").unwrap_or_default();
    let names: Vec<String> = std::iter::once(name.to_owned())
        .chain(
            extensions
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(|extension| format!("{name}{extension}")),
        )
        .collect();
    env::split_paths(&env::var_os("PATH")?)
        .flat_map(|directory| names.iter().map(move |name| directory.join(name)))
        .find(|path| path.is_file())
}

struct MouseTracking {
    mode: MouseMode,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MouseMode {
    Disabled,
    Buttons,
    Motion,
}

impl MouseTracking {
    /// Asks the terminal for clicks and scrolls, adding pointer motion only for the history chart.
    #[cfg(unix)]
    fn set(&mut self, mode: MouseMode) -> io::Result<()> {
        use std::io::Write;
        self.mode = mode;
        let request: &[u8] = match mode {
            MouseMode::Disabled => b"\x1b[?1000l\x1b[?1003l\x1b[?1006l",
            MouseMode::Buttons => b"\x1b[?1003l\x1b[?1006h\x1b[?1000h",
            MouseMode::Motion => b"\x1b[?1000l\x1b[?1006h\x1b[?1003h",
        };
        let mut output = io::stdout();
        output.write_all(request)?;
        output.flush()
    }

    /// The Windows console reports all mouse input once enabled, and `encode` drops motion the page does not use.
    #[cfg(windows)]
    fn set(&mut self, mode: MouseMode) -> io::Result<()> {
        use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
        let was = std::mem::replace(&mut self.mode, mode);
        match (was == MouseMode::Disabled, mode == MouseMode::Disabled) {
            (true, false) => execute!(io::stdout(), EnableMouseCapture),
            (false, true) => execute!(io::stdout(), DisableMouseCapture),
            _ => Ok(()),
        }
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

/// Where the worker reads from: the engine, or a snapshot that has no actions.
enum Feed {
    Live(Result<Box<Backend>, String>),
    Local(LocalSnapshot),
}

fn worker(source: Source) -> (mpsc::Sender<Job>, mpsc::Receiver<Response>) {
    let (send_job, jobs) = mpsc::channel();
    let (send_response, responses) = mpsc::channel();
    thread::spawn(move || {
        let mut feed = match source {
            Source::Live => Feed::Live(Backend::start().map(Box::new).map_err(|error| error.to_string())),
            Source::Local(local) => Feed::Local(local),
        };
        while let Ok(job) = jobs.recv() {
            let response = match (job, &mut feed) {
                (Job::Snapshot, Feed::Live(Ok(backend))) => Response::Snapshot(backend.usage()),
                (Job::Snapshot, Feed::Local(local)) => Response::Snapshot(local.read()),
                (Job::Command(page, command), Feed::Live(Ok(backend))) => {
                    Response::Command(page, backend.execute(command))
                }
                (Job::Snapshot, Feed::Live(Err(error))) => {
                    Response::Snapshot(Err(io::Error::other(format!("Quotaline could not start: {error}"))))
                }
                (Job::Command(page, _), Feed::Live(Err(error))) => Response::Command(
                    page,
                    Err(io::Error::other(format!("Quotaline could not start: {error}"))),
                ),
                (Job::Command(page, _), Feed::Local(_)) => Response::Command(
                    page,
                    Err(io::Error::other(
                        "This action needs live data; run quotaline-tui without --demo or --file",
                    )),
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
