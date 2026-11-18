use super::*;
use jiff::Timestamp;
use quotaline_history::{AnalyticsPoint, Sample, SampleWrite, Store};
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
    cli.read_until("Credits · $0.00 remaining");
    cli.click(2, 2);
    cli.read_until("workspace");
    assert!(!cli.screen.screen().contents().contains("Example Org"));

    cli.click(22, 23);
    cli.read_until("Settings · read only");
    cli.read_until("Codex collection");
    assert!(!cli.screen.screen().contents().contains('‹'));
    cli.write(b":set decimals 2\r");
    cli.read_until("Change decimals in the Quotaline menu bar app");

    cli.write(b"2");
    cli.read_until("Latest 78.00");
    cli.write(b"[");
    cli.read_until("No history in this range");
    cli.write(b"n");
    cli.read_until("Latest 78.00");
    cli.write(
        concat!(
            ":visibility {\"kind\":\"toggle\",\"metric\":\"usage\",",
            "\"id\":\"codex:session\",\"series\":[\"codex:session\",\"codex:weekly\"]}\r"
        )
        .as_bytes(),
    );
    cli.read_until("Series hidden · v to show it");
    cli.read_until("● Codex · Native week");
    cli.read_until("44.00%");
    cli.click(55, 4);
    cli.read_until("Latest 78.00%");
    cli.write(b":clear history confirm\r");
    cli.read_until("Clear history in the Quotaline menu bar app");
    let csv = fixture.home.path().join("usage history.csv");
    cli.write(format!(":export {}\r", csv.display()).as_bytes());
    cli.read_until("History exported");
    assert!(
        fs::read_to_string(csv)
            .unwrap()
            .contains("codex:session,Native session,78.00")
    );

    cli.click(34, 23);
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

#[test]
fn reads_native_support_data_and_leaves_configuration_with_the_app() {
    let fixture = MacFixture::new();
    fixture.cache(62.5);
    let mut cli = Interactive::start(&mut fixture.command());
    cli.read_until("62.5%");
    cli.write(b"s");
    cli.read_until("Set up codex in the Quotaline menu bar app");
    for (input, text) in [
        (":set decimals\r", "Use set KEY JSON"),
        (":export /history.csv\r", "Open History before exporting a metric"),
        (":models Native\r", "Selected · codex:session · Native session"),
        (":spend\r", "Last 30 days $6.25"),
        (":reviews\r", "No review history yet"),
        (":diagnostics\r", "Quotaline macOS diagnostics"),
        (":issue\r", "github.com/quotaline/quotaline/issues/new"),
        (":toggle-model codex:session\r", "Change selection for codex:session"),
        (":toggle-provider codex\r", "Change model selection for codex"),
        (":interval codex 1\r", "Change the refresh interval for codex"),
        (":threshold 75\r", "Change the 75% threshold"),
        (
            ":reorder provider codex cursor before\r",
            "Move codex in the Quotaline menu bar app",
        ),
        (
            ":reset settings confirm\r",
            "Reset settings in the Quotaline menu bar app",
        ),
        (":period 30\r", "Latest 78.00"),
        (":export history.csv\r", "Use an absolute export path"),
        (":metric costUSD\r", "API-equivalent cost · Total $6.25"),
    ] {
        cli.write(input.as_bytes());
        cli.read_until(text);
    }
    cli.write(b"\x1b[D");
    cli.read_until("T00:00:00Z · $6.25");
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
        let history = Store::open(Some(&support.join("usage.sqlite")), 60).unwrap();
        history
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
        history
            .record_analytics(
                "codex",
                None,
                now as f64,
                &[AnalyticsPoint {
                    provider: "codex".into(),
                    day: quotaline_analytics::utc_day::day((now - 86_400) as f64),
                    metric: "costUSD".into(),
                    series: "Native model".into(),
                    value: 6.25,
                }],
                &[],
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
        provider.credit_balance = Some(0.0);
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
