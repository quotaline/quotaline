//! Displays redacted Quotaline usage snapshots in a terminal.

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod dashboard;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod snapshot;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod terminal;

use std::{env, ffi::OsString, io, process::ExitCode};

#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::path::PathBuf;

fn main() -> ExitCode {
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
             Displays the desktop usage.json export by default."
        );
        return Ok(());
    }
    run_dashboard(arguments)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn run_dashboard(arguments: &[OsString]) -> io::Result<()> {
    let source = match arguments {
        [] => Source::File(
            default_directory()
                .ok_or_else(|| {
                    io::Error::other("could not locate the desktop usage.json export; pass --file PATH or --demo")
                })?
                .join("usage.json"),
        ),
        [flag] if flag == "--demo" => Source::Demo,
        [flag, path] if flag == "--file" => Source::File(path.into()),
        _ => return Err(io::Error::other("expected --file PATH, --demo, or --help")),
    };
    terminal::run(source)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn run_dashboard(_arguments: &[OsString]) -> io::Result<()> {
    Err(io::Error::other("the terminal dashboard runs on Linux and macOS"))
}

#[cfg(target_os = "macos")]
fn default_directory() -> Option<PathBuf> {
    env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support/Quotaline"))
}

#[cfg(target_os = "linux")]
fn default_directory() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|path| path.join("quotaline"))
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
enum Source {
    File(PathBuf),
    Demo,
}
