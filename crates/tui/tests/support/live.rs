use super::*;
use serde_json::Value;
use std::{os::unix::fs::PermissionsExt, path::Path};
use tempfile::TempDir;

/// A home of its own for one live run, so the engine never reads the developer's sign-ins or settings.
struct Home(TempDir);

impl Home {
    fn new(settings: &str) -> Self {
        let home = Self(tempdir().unwrap());
        fs::create_dir_all(home.config()).unwrap();
        fs::write(home.config().join("settings.json"), settings).unwrap();
        home
    }

    /// Selects the engine's demo providers, which need no sign-in or network.
    fn demo() -> Self {
        Self::new(r#"{"demo":true,"first_run_card_dismissed":true}"#)
    }

    /// Turns every provider off, so a live run reads no credential store; the Keychain and Secret Service ignore
    /// `HOME` and would hand over the developer's own sign-ins.
    fn offline() -> Self {
        Self::new(
            r#"{"first_run_card_dismissed":true,"providers":{"claude":false,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}"#,
        )
    }

    #[cfg(target_os = "macos")]
    fn config(&self) -> std::path::PathBuf {
        self.data()
    }

    #[cfg(target_os = "macos")]
    fn data(&self) -> std::path::PathBuf {
        self.path("Library/Application Support/Quotaline Terminal")
    }

    #[cfg(not(target_os = "macos"))]
    fn config(&self) -> std::path::PathBuf {
        self.path("config/quotaline")
    }

    #[cfg(not(target_os = "macos"))]
    fn data(&self) -> std::path::PathBuf {
        self.path("data/quotaline")
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.0.path().join(relative)
    }

    fn command(&self) -> Command {
        let mut command = command();
        command
            .env("HOME", self.0.path())
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CACHE_HOME", self.path("cache"));
        command
    }

    fn settings(&self) -> Value {
        serde_json::from_slice(&fs::read(self.config().join("settings.json")).unwrap()).unwrap()
    }

    /// Puts a stand-in for a provider's command-line tool in `bin`, exiting with `status`.
    fn tool(&self, name: &str, status: i32) -> std::path::PathBuf {
        let directory = self.path("bin");
        fs::create_dir_all(&directory).unwrap();
        let tool = directory.join(name);
        fs::write(
            &tool,
            format!("#!/bin/sh\necho signing in with \"$@\"\nexit {status}\n"),
        )
        .unwrap();
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        directory
    }
}

#[test]
fn live_engine_serves_usage_history_settings_and_logs() {
    let home = Home::demo();
    let mut cli = Interactive::start(&mut home.command());
    cli.read_until("Max 20x");
    assert!(!cli.screen.screen().contents().contains("Demo data"));
    cli.write(b"d");
    cli.read_until("Weekly limits are boosted 2x until the next reset.");
    cli.write(b"2");
    cli.read_until("Latest");
    cli.write(b"v");
    cli.read_until("Series hidden");
    cli.write(b"i");
    cli.read_until("Latest");
    cli.write(b"3");
    cli.read_until("Claude collection");
    cli.click(5, 3);
    cli.read_until("Settings saved");
    assert_eq!(home.settings()["providers"]["claude"], false);
    cli.write(b"4");
    cli.read_until("Service started");
    cli.quit(b"q");
    assert!(!home.data().join("usage.json").exists());
}

#[test]
fn live_engine_runs_every_command() {
    let home = Home::demo();
    let mut cli = Interactive::start(&mut home.command());
    cli.read_until("Max 20x");
    for (keys, reply) in [
        (&b"R"[..], "Refresh requested"),
        (b":set hide_identity false\r", "Settings saved"),
        (b":toggle-model codex:weekly\r", "Settings saved"),
        (b":toggle-provider codex\r", "Settings saved"),
        (b":interval codex 1\r", "Settings saved"),
        (b":threshold 75\r", "Settings saved"),
        (b":reorder provider cursor codex before\r", "Settings saved"),
        (b":models\r", "can_toggle_all"),
        (b":diagnostics\r", "history_path"),
        (b":issue\r", "https://github.com/"),
        (b":spend\r", "by_provider"),
        (b":reviews\r", "review"),
        (b":logs full\r", "Service started"),
        (b"2", "Latest"),
        (b"[", "Latest"),
        (b"]", "Latest"),
        (b"n", "Latest"),
        (b"r", "Latest"),
        (b":refresh invalid\r", "Refresh was declined by Quotaline"),
        (b":visibility nonsense\r", "expected"),
        (b":visibility {}\r", "missing field"),
        (b":metric projectCost\r", "Latest"),
        (b":metric unknown\r", "unknown"),
    ] {
        cli.write(keys);
        cli.read_until(reply);
    }
    let directory = tempdir().unwrap();
    let metric = directory.path().join("metric file.csv");
    let all = directory.path().join("all file.csv");
    for (command, path, header) in [
        (":export", &metric, "provider,day,metric,"),
        (":export-all", &all, "kind,timestamp,"),
    ] {
        cli.write(format!("{command} {}\r", path.display()).as_bytes());
        // Both exports report the same status line, so the file shows when this one finished.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !path.exists() {
            assert!(Instant::now() < deadline, "{command} wrote nothing");
            cli.read_output();
        }
        assert!(fs::read_to_string(path).unwrap().starts_with(header));
    }
    for command in [":export", ":export-all"] {
        cli.write(format!("{command} /\r").as_bytes());
        cli.read_until("Choose a filename for the export");
        cli.write(b"1");
        cli.read_until("Quota used");
    }
    for (keys, reply) in [
        (&b":clear logs confirm\r"[..], "Logs cleared"),
        (b":clear history confirm\r", "History cleared"),
        (b":reset settings confirm\r", "Settings saved"),
    ] {
        cli.write(keys);
        cli.read_until(reply);
    }
    cli.write(b"2");
    cli.read_until("No history in this range");
    cli.quit(b"q");
}

#[test]
fn fresh_home_has_no_reviews_to_report() {
    let home = Home::offline();
    let mut cli = Interactive::start(&mut home.command());
    cli.read_until("No signed-in providers yet");
    cli.write(b":reviews\r");
    cli.read_until("No readings reported");
    cli.quit(b"q");
}

#[test]
fn reports_history_the_store_cannot_open() {
    let home = Home::offline();
    // A directory where the database belongs fails every history read, without stopping the engine.
    fs::create_dir_all(home.data().join("history.sqlite3")).unwrap();
    let mut cli = Interactive::start(&mut home.command());
    cli.read_until("Quota used");
    cli.write(b"2");
    cli.read_until("Is a directory");
    cli.quit(b"q");
}

#[test]
fn reports_an_engine_that_cannot_start() {
    let home = Home(tempdir().unwrap());
    // A file where the settings directory belongs stops the first run from saving settings, so the engine never
    // starts or reads a credential.
    fs::create_dir_all(home.config().parent().unwrap()).unwrap();
    fs::write(home.config(), "").unwrap();
    let mut cli = Interactive::start(&mut home.command());
    cli.read_until("Quotaline could not start");
    cli.write(b":diagnostics\r");
    cli.read_until("Quotaline could not start");
    cli.quit(b"q");
}

#[test]
fn signs_in_with_the_provider_tool_in_this_terminal() {
    let home = Home::demo();
    let bin = home.tool("codex", 0);
    home.tool("claude", 1);
    home.tool("gemini.sh", 0);
    let mut cli = Interactive::start(home.command().env("PATH", &bin).env("PATHEXT", ".sh"));
    cli.read_until("Max 20x");
    cli.write(b":sign-in codex\r");
    cli.read_until("signing in with login");
    cli.read_until("Refresh requested");
    cli.write(b":sign-in gemini\r");
    cli.read_until("signing in with");
    cli.read_until("Refresh requested");
    cli.write(b":sign-in claude\r");
    cli.read_until("claude auth login exited with");
    cli.write(b":sign-in cursor\r");
    cli.read_until("cursor-agent login, then press R");
    cli.write(b"s");
    cli.read_until("claude auth login exited with");
    cli.quit(b"q");
}

#[test]
fn sign_in_without_a_search_path_names_the_command() {
    let home = Home::demo();
    let mut cli = Interactive::start(home.command().env_remove("PATH"));
    cli.read_until("Max 20x");
    cli.write(b":sign-in antigravity\r");
    cli.read_until("Sign in through Antigravity, then press R");
    cli.quit(b"q");
}

#[test]
fn snapshot_sources_keep_sign_in_and_settings_read_only() {
    let directory = tempdir().unwrap();
    let path = Path::new(directory.path()).join("usage.json");
    fs::write(
        &path,
        quotaline_export::UsageExport::from(&quotaline_demo::demo()).json(),
    )
    .unwrap();
    let mut cli = Interactive::start(command().arg("--file").arg(&path));
    cli.read_until("Antigravity");
    cli.write(b"s");
    cli.read_until("This action needs live data");
    cli.quit(b"q");
}
