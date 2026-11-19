//! Displays Quotaline usage, history, settings, and logs in a terminal.

mod dashboard;
mod desktop;
mod keys;
mod pages;
mod preferences;
mod snapshot;
mod snapshot_schedule;
mod usage;

#[cfg(not(test))]
mod runtime;

#[cfg(not(test))]
fn main() -> std::process::ExitCode {
    runtime::main()
}
