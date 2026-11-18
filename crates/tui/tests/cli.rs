//! Exercises terminal behavior through a real pseudoterminal on Linux and macOS.

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
    num::NonZeroU16,
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

#[cfg(target_os = "linux")]
#[path = "support/gnome.rs"]
mod gnome;

#[cfg(target_os = "macos")]
#[path = "support/macos.rs"]
mod macos;

#[test]
fn command_editor_in_demo_mode_keeps_live_actions_disabled() {
    let mut cli = Interactive::start(command().arg("--demo"));
    cli.read_until("Antigravity");
    cli.write(b"?");
    cli.read_until("Enter a command");
    cli.write(b"3");
    cli.read_until("Open the desktop source");
    cli.write(b"\r\x1b[C");
    cli.write(b":set invalid\x1b:unknown\r");
    cli.read_until("Unknown command");
    cli.write(b":metrics\r");
    cli.read_until("Quota usage");
    cli.write(b":logs full\r");
    cli.read_until("This action requires the desktop source");
    cli.quit(b"\x03");
}

#[test]
fn provider_commands_handle_a_snapshot_with_no_visible_providers() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("usage.json");
    fs::write(&path, r#"{"providers":[]}"#).unwrap();
    let mut cli = Interactive::start(command().arg("--file").arg(path));
    cli.read_until("No providers in snapshot");
    cli.write(b"1\rsviR");
    cli.read_until("This action requires the desktop source");
    cli.quit(b"q");
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
    cli.read_until("› Gemini 5-hour");
    cli.resize(30, 20);
    cli.read_until("Resize to at least 34");
    cli.resize(60, 20);
    cli.read_until("Quota used");

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
    screen: vt100::Parser,
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
            screen: vt100::Parser::new(NonZeroU16::new(24).unwrap(), NonZeroU16::new(80).unwrap(), 0),
        }
    }

    fn read_until(&mut self, text: &str) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut bytes = Vec::new();
        loop {
            let output = self.screen.screen().contents();
            if output.contains(text) || String::from_utf8_lossy(&bytes).contains(text) {
                break;
            }
            assert!(Instant::now() < deadline, "did not read {text:?} in {output:?}");
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
                self.screen.process(&chunk[..read]);
                bytes.extend_from_slice(&chunk[..read]);
            }
        }
    }

    fn write(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }

    fn resize(&mut self, width: u16, height: u16) {
        self.screen
            .set_size(NonZeroU16::new(height).unwrap(), NonZeroU16::new(width).unwrap());
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
