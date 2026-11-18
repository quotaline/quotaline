use super::*;
use jiff::Timestamp;
use quotaline_history::{Sample, SampleWrite, Store};
use quotaline_macos_ffi::{
    logging::log_make_entry,
    provider_record::{ProviderReadingRecord, ReadingSource},
    reading_cache::encode_reading_cache,
};
use quotaline_quota::{Provider, Window};
use quotaline_settings::Settings;

#[test]
fn reads_mutates_session_and_recovers_using_the_native_mac_data() {
    let fixture = MacFixture::new();
    fixture.cache(62.5);
    let mut cli = Interactive::start(&mut fixture.command());
    cli.read_until("62.5%");
    cli.write(b"d");
    cli.read_until("workspace");
    assert!(!cli.screen.screen().contents().contains("Example Org"));

    cli.write(b"3");
    cli.read_until("Settings · read only");
    cli.read_until("Codex collection");
    cli.write(b":set decimals 2\r");
    cli.read_until("Change decimals in the Quotaline menu bar app");

    cli.write(b"2");
    cli.read_until("Latest 78.00");
    cli.write(b"v");
    cli.read_until("○ Codex · Native session");
    cli.write(b"v");
    cli.read_until("● Codex · Native session");
    let csv = fixture.home.path().join("usage history.csv");
    cli.write(format!(":export {}\r", csv.display()).as_bytes());
    cli.read_until("History exported");
    assert!(
        fs::read_to_string(csv)
            .unwrap()
            .contains("codex:session,Native session,78.00")
    );

    cli.write(b"4");
    cli.read_until("Collector ready");
    cli.write(b":clear logs confirm\r");
    cli.read_until("Clear logs in the Quotaline menu bar app");
    assert!(
        fs::read_to_string(fixture.support.join("log.txt"))
            .unwrap()
            .contains("Collector ready")
    );

    cli.write(b"1");
    cli.read_until("62.5%");
    cli.write(b"R");
    cli.read_until("Refresh codex in the Quotaline menu bar app");
    fs::write(fixture.support.join("snapshots.json"), "{").unwrap();
    cli.read_until("Stale");
    assert!(cli.screen.screen().contents().contains("62.5%"));
    fixture.cache(39.5);
    cli.write(b"r");
    cli.read_until("39.5%");
    assert!(!cli.screen.screen().contents().contains("Stale"));
    cli.quit(b"q");
}

#[test]
fn reports_missing_home_at_default_startup() {
    let mut cli = Interactive::start(command().env_remove("HOME"));
    cli.read_until("Read failed");
    cli.read_until("HOME is unset");
    cli.quit(b"q");
}

struct MacFixture {
    home: tempfile::TempDir,
    support: std::path::PathBuf,
}

impl MacFixture {
    fn new() -> Self {
        let home = tempdir().unwrap();
        let library = home.path().join("Library");
        let support = library.join("Application Support/Quotaline");
        fs::create_dir_all(&support).unwrap();
        fs::create_dir_all(library.join("Preferences")).unwrap();
        let settings = Settings {
            demo_choice: Some(false),
            decimals: 1,
            hide_identity: true,
            history_days: 7,
            ..Settings::default()
        };
        let mut plist = plist::Dictionary::new();
        plist.insert(
            "settings".into(),
            plist::Value::Data(serde_json::to_vec(&settings).unwrap()),
        );
        plist::Value::Dictionary(plist)
            .to_file_binary(library.join("Preferences/tech.bernat.quotaline.plist"))
            .unwrap();

        let now = Timestamp::now().as_second();
        Store::open(Some(&support.join("usage.sqlite")), 60)
            .unwrap()
            .seed(
                &[("session", "Native session", 78.0), ("weekly", "Native week", 44.0)].map(|(id, label, used)| {
                    SampleWrite {
                        sample: Sample {
                            at: (now - 120) as f64,
                            key: format!("codex:{id}"),
                            used,
                            reset_at: None,
                        },
                        label: label.into(),
                    }
                }),
            )
            .unwrap();
        let log = log_make_entry(
            now as f64,
            "info".into(),
            "app".into(),
            "Collector ready".into(),
            home.path().to_string_lossy().into_owned(),
        );
        fs::write(support.join("log.txt"), log.line).unwrap();
        Self { home, support }
    }

    fn command(&self) -> Command {
        let mut command = command();
        command.env("HOME", self.home.path());
        command
    }

    fn cache(&self, used: f64) {
        let mut provider = Provider::empty("codex", "connected");
        provider.organization = "Example Org".into();
        provider.windows = vec![
            Window::new("session", "Native session", used, None, ""),
            Window::new("weekly", "Native week", 44.0, None, ""),
        ];
        let bytes = encode_reading_cache(vec![ProviderReadingRecord {
            provider: provider.into(),
            fetched_at: Timestamp::now().as_second() as f64,
            source: ReadingSource::Network,
        }]);
        fs::write(self.support.join("snapshots.json"), bytes).unwrap();
    }
}
