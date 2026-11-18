//! Displays Quotaline usage, history, settings, and logs in a terminal.

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod dashboard;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod desktop;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod keys;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod pages;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod preferences;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod snapshot;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod snapshot_schedule;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod usage;

#[cfg(not(test))]
mod runtime;

#[cfg(not(test))]
fn main() -> std::process::ExitCode {
    runtime::main()
}
