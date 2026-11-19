//! Resolves the home, configuration and application folders each operating system uses.

use std::{
    env,
    ffi::OsString,
    path::{Path, PathBuf},
};

#[cfg(not(target_os = "macos"))]
const APP_DIRECTORY: &str = "quotaline";

/// Only `HOME` counts on Unix, so a process started without one reads paths relative to its working directory.
#[cfg(unix)]
fn home_variable() -> Option<OsString> {
    env::var_os("HOME")
}

/// Windows sets no `HOME`; its profile folder comes from `USERPROFILE`.
#[cfg(windows)]
fn home_variable() -> Option<OsString> {
    env::home_dir().map(PathBuf::into_os_string)
}

pub fn home() -> PathBuf {
    home_variable().map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// The home folder as text, which failure messages shorten to `~`.
pub fn home_text() -> Option<String> {
    home_variable()?.into_string().ok()
}

/// Honours `variable` only for the signed-in user's own home, so a provider read against another home stays there.
fn override_for(home: &Path, variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|_| env::home_dir().as_deref() == Some(home))
        .map(PathBuf::from)
}

#[cfg(not(target_os = "macos"))]
fn base(variable: &str, fallback: &str) -> PathBuf {
    let home = home();
    override_for(&home, variable).unwrap_or_else(|| home.join(fallback))
}

/// Where command-line tools keep settings: `~/.config` on Linux and macOS.
#[cfg(unix)]
pub fn config_home(home: &Path) -> PathBuf {
    override_for(home, "XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"))
}

/// Where command-line tools keep settings: `%LOCALAPPDATA%` on Windows.
#[cfg(windows)]
pub fn config_home(home: &Path) -> PathBuf {
    override_for(home, "LOCALAPPDATA").unwrap_or_else(|| home.join("AppData/Local"))
}

/// Where a desktop application named `name` keeps its data, as Electron apps such as Cursor do.
#[cfg(target_os = "linux")]
pub fn app_data(home: &Path, name: &str) -> PathBuf {
    config_home(home).join(name)
}

#[cfg(target_os = "macos")]
pub fn app_data(home: &Path, name: &str) -> PathBuf {
    home.join("Library/Application Support").join(name)
}

#[cfg(windows)]
pub fn app_data(home: &Path, name: &str) -> PathBuf {
    override_for(home, "APPDATA")
        .unwrap_or_else(|| home.join("AppData/Roaming"))
        .join(name)
}

/// The XDG base directory named by `variable`, before Quotaline's own folder is added.
#[cfg(target_os = "linux")]
pub fn xdg_base(variable: &str, fallback: &str) -> PathBuf {
    base(variable, fallback)
}

/// Quotaline's settings folder.
#[cfg(target_os = "linux")]
pub fn config_dir() -> PathBuf {
    base("XDG_CONFIG_HOME", ".config").join(APP_DIRECTORY)
}

/// Quotaline's history, logs and shared engine state.
#[cfg(target_os = "linux")]
pub fn data_dir() -> PathBuf {
    base("XDG_DATA_HOME", ".local/share").join(APP_DIRECTORY)
}

/// Quotaline's disposable snapshot.
#[cfg(target_os = "linux")]
pub fn cache_dir() -> PathBuf {
    base("XDG_CACHE_HOME", ".cache").join(APP_DIRECTORY)
}

// The menu bar app keeps its own history in Application Support/Quotaline and polls on its own schedule, so the
// terminal keeps a separate folder until both share one lease.
#[cfg(target_os = "macos")]
const MACOS_DIRECTORY: &str = "Quotaline Terminal";

#[cfg(target_os = "macos")]
pub fn config_dir() -> PathBuf {
    data_dir()
}

#[cfg(target_os = "macos")]
pub fn data_dir() -> PathBuf {
    home().join("Library/Application Support").join(MACOS_DIRECTORY)
}

#[cfg(target_os = "macos")]
pub fn cache_dir() -> PathBuf {
    home().join("Library/Caches").join(MACOS_DIRECTORY)
}

#[cfg(windows)]
pub fn config_dir() -> PathBuf {
    data_dir()
}

#[cfg(windows)]
pub fn data_dir() -> PathBuf {
    base("APPDATA", "AppData/Roaming").join(APP_DIRECTORY)
}

#[cfg(windows)]
pub fn cache_dir() -> PathBuf {
    base("LOCALAPPDATA", "AppData/Local").join(APP_DIRECTORY)
}

/// The name the Gemini API expects for this operating system, as Node reports it.
pub fn client_os() -> &'static str {
    #[cfg(target_os = "macos")]
    return "darwin";
    #[cfg(windows)]
    return "win32";
    #[cfg(target_os = "linux")]
    "linux"
}
