//! Runs the CLI and owns the terminal session.

use std::{env, ffi::OsString, io, process::ExitCode};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::snapshot::LocalSnapshot;

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod terminal;

pub(crate) fn main() -> ExitCode {
    match run(&env::args_os().skip(1).collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("quotaline-tui: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[OsString]) -> io::Result<()> {
    if let [flag] = arguments
        && (flag == "--help" || flag == "-h")
    {
        println!(
            "Usage: quotaline-tui [--file PATH | --demo | --help]\n\n\
             Uses Quotaline’s desktop data by default."
        );
        return Ok(());
    }
    run_dashboard(arguments)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_dashboard(arguments: &[OsString]) -> io::Result<()> {
    let source = match arguments {
        [] => Source::Desktop,
        [flag] if flag == "--demo" => Source::Local(LocalSnapshot::Demo),
        [flag, path] if flag == "--file" => Source::Local(LocalSnapshot::File(path.into())),
        _ => return Err(io::Error::other("expected --file PATH, --demo, or --help")),
    };
    terminal::run(source)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn run_dashboard(_arguments: &[OsString]) -> io::Result<()> {
    Err(io::Error::other("the terminal dashboard runs on Linux and macOS"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
enum Source {
    Desktop,
    Local(LocalSnapshot),
}
