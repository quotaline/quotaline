#![cfg(any(target_os = "linux", target_os = "macos"))]

use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    fs::{Mode, OFlags, open},
    io::{FdFlags, fcntl_setfd},
    process::{Pid, Signal, kill_process},
    pty::{OpenptFlags, grantpt, openpt, ptsname, unlockpt},
    termios::{LocalModes, Winsize, tcgetattr, tcsetwinsize},
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::tempdir;

#[test]
fn help_lists_source_modes() {
    for flag in ["-h", "--help"] {
        let output = command().arg(flag).output().unwrap();
        assert!(output.status.success());
        assert!(
            String::from_utf8(output.stdout)
                .unwrap()
                .contains("quotaline-tui [--file PATH | --demo | --help]")
        );
    }
}

#[test]
fn rejects_conflicting_or_incomplete_sources() {
    for arguments in [
        &["--demo", "--file", "usage.json"][..],
        &["--file"],
        &["--unknown"],
        &["--demo", "--help"],
    ] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("expected --file PATH, --demo, or --help")
        );
    }
}

#[test]
fn rejects_redirected_input_or_output_before_opening_an_alternate_screen() {
    let (_master, slave) = terminal_pair();
    for (input, output) in [
        (Stdio::piped(), Stdio::from(slave.try_clone().unwrap())),
        (Stdio::from(slave), Stdio::piped()),
    ] {
        let result = command().arg("--demo").stdin(input).stdout(output).output().unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8(result.stderr)
                .unwrap()
                .contains("standard input and output must be terminals")
        );
    }
}

#[test]
fn reports_how_to_select_an_export_without_a_home_directory() {
    let output = command()
        .env_remove("HOME")
        .env_remove("XDG_DATA_HOME")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("could not locate the desktop usage.json export")
    );
}

#[test]
fn reads_the_default_export_without_an_explicit_path() {
    let home = tempdir().unwrap();
    let export = home_export(home.path());
    fs::create_dir_all(export.parent().unwrap()).unwrap();
    fs::write(export, r#"{"providers":[]}"#).unwrap();

    let mut cli = Interactive::start(command().env("HOME", home.path()).env_remove("XDG_DATA_HOME"));
    cli.read_until("No providers in snapshot");
    cli.quit(b"q");
}

#[cfg(target_os = "macos")]
fn home_export(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Quotaline/usage.json")
}

#[cfg(target_os = "linux")]
fn home_export(home: &Path) -> PathBuf {
    home.join(".local/share/quotaline/usage.json")
}

#[cfg(target_os = "linux")]
#[test]
fn finds_the_export_in_the_xdg_data_directory() {
    let directory = tempdir().unwrap();
    let export = directory.path().join("quotaline/usage.json");
    fs::create_dir_all(export.parent().unwrap()).unwrap();
    fs::write(export, r#"{"providers":[]}"#).unwrap();

    let mut cli = Interactive::start(command().env("XDG_DATA_HOME", directory.path()).env_remove("HOME"));
    cli.read_until("No providers in snapshot");
    cli.quit(b"\x03");
}

#[test]
fn redraws_and_reloads_an_explicit_snapshot_in_a_terminal() {
    let directory = tempdir().unwrap();
    let export = directory.path().join("usage practice.json");
    let providers = quotaline_demo::demo();
    fs::write(&export, quotaline_export::UsageExport::from(&providers).json()).unwrap();

    let mut cli = Interactive::start(command().arg("--file").arg(export.as_os_str()));
    cli.read_until("Antigravity");
    cli.write(b"\x1b[B");
    cli.read_until("Max 20x");
    cli.resize(30, 20);
    cli.read_until("Resize to at least 34");
    cli.resize(60, 20);
    cli.read_until("Quota windows");

    fs::write(&export, "{").unwrap();
    cli.read_until("Stale");
    fs::write(export, r#"{"providers":[]}"#).unwrap();
    cli.write(b"r");
    cli.read_until("No providers in snapshot");
    cli.quit(b"q");
}

#[test]
fn restores_terminal_after_termination_signal() {
    let mut cli = Interactive::start(command().arg("--demo"));
    cli.read_until("Demo data");
    kill_process(Pid::from_raw(cli.child.id() as i32).unwrap(), Signal::TERM).unwrap();
    cli.assert_restored();
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_quotaline-tui"))
}

struct Interactive {
    child: Child,
    master: File,
    slave: File,
    original_modes: LocalModes,
}

impl Interactive {
    fn start(command: &mut Command) -> Self {
        let (master, slave) = terminal_pair();
        tcsetwinsize(
            &slave,
            Winsize {
                ws_row: 24,
                ws_col: 80,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .unwrap();
        let original_modes = tcgetattr(&slave).unwrap().local_modes;
        let child = command
            .stdin(slave.try_clone().unwrap())
            .stdout(slave.try_clone().unwrap())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        Self {
            child,
            master,
            slave,
            original_modes,
        }
    }

    fn read_until(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bytes = Vec::new();
        while !String::from_utf8_lossy(&bytes).contains(text) {
            assert!(
                Instant::now() < deadline,
                "did not read {text:?} in {:?}",
                String::from_utf8_lossy(&bytes)
            );
            if poll(
                &mut [PollFd::new(&self.master, PollFlags::IN)],
                Some(&Timespec {
                    tv_sec: 0,
                    tv_nsec: 100_000_000,
                }),
            )
            .unwrap()
                != 0
            {
                let mut chunk = [0; 2048];
                let read = self.master.read(&mut chunk).unwrap();
                bytes.extend_from_slice(&chunk[..read]);
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }

    fn resize(&self, width: u16, height: u16) {
        tcsetwinsize(
            &self.slave,
            Winsize {
                ws_row: height,
                ws_col: width,
                ws_xpixel: 0,
                ws_ypixel: 0,
            },
        )
        .unwrap();
    }

    fn quit(&mut self, key: &[u8]) {
        self.write(key);
        self.assert_restored();
    }

    fn assert_restored(&mut self) {
        self.read_until("\x1b[?1049l");
        assert!(self.child.wait().unwrap().success());
        assert_eq!(tcgetattr(&self.slave).unwrap().local_modes, self.original_modes);
    }
}

impl Drop for Interactive {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn terminal_pair() -> (File, File) {
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
    fcntl_setfd(&master, FdFlags::CLOEXEC).unwrap();
    grantpt(&master).unwrap();
    unlockpt(&master).unwrap();
    let slave = open(
        &*ptsname(&master, Vec::new()).unwrap(),
        OFlags::RDWR | OFlags::NOCTTY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .unwrap();
    (master.into(), slave.into())
}
