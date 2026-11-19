//! Runs the CLI and owns the terminal session.

use std::{env, ffi::OsString, io, process::ExitCode};

use crate::snapshot::LocalSnapshot;

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
             Collects quota from your signed-in tools by default."
        );
        return Ok(());
    }
    run_dashboard(arguments)
}

fn run_dashboard(arguments: &[OsString]) -> io::Result<()> {
    let source = match arguments {
        [] => Source::Live,
        [flag] if flag == "--demo" => Source::Local(LocalSnapshot::Demo),
        [flag, path] if flag == "--file" => Source::Local(LocalSnapshot::File(path.into())),
        _ => return Err(io::Error::other("expected --file PATH, --demo, or --help")),
    };
    terminal::run(source)
}

enum Source {
    Live,
    Local(LocalSnapshot),
}
