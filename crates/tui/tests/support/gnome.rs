use super::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    sync::mpsc,
    thread::{self, JoinHandle},
};
use zbus::connection::Builder;

const SERVICE: &str = "tech.bernat.Quotaline.Gnome";
const OBJECT: &str = "/tech/bernat/Quotaline/Gnome";

#[test]
fn missing_service_reports_an_error_without_using_an_export() {
    let bus = Bus::start();
    let mut cli = bus.cli();
    cli.read_until("Read failed");
    cli.quit(b"q");
}

#[test]
fn displays_desktop_state_history_settings_and_logs_without_an_export() {
    let bus = Bus::start();
    let service = Fixture::serve(&bus.address, false);
    let mut cli = bus.cli();
    cli.read_until("108.5%");
    cli.click(2, 2);
    cli.read_until("Example workspace");
    cli.click(22, 23);
    cli.read_until("Stack compatible history series");
    cli.click(2, 3);
    service.wait_for("SetSetting", &mut cli);
    cli.read_until("Settings saved");
    cli.write(b"j\x1b[D\x1b[C");
    cli.read_until("Settings saved");
    cli.write(b"2");
    cli.read_until("Latest 78.00");
    cli.write(b"\x1b[D");
    cli.read_until("2026-");
    cli.write(b"4");
    cli.read_until("Collector ready");
    cli.quit(b"q");
}

#[test]
fn old_service_quota_reports_usage_without_the_epoch_reset() {
    let bus = Bus::start();
    let _service = Fixture::serve(&bus.address, false);
    let mut cli = bus.cli();
    cli.resize(130, 30);
    cli.read_until("Remaining: 300 credits");
    let screen = cli.screen.screen().contents();
    let premium = screen.lines().find(|line| line.contains("Premium requests")).unwrap();
    assert!(premium.contains("40.0%"));
    assert!(premium.contains("Reset unavailable"));
    assert!(screen.contains("Included: 500 credits"));
    assert!(screen.contains("Spend credit balance · 0.125 EUR"));
    assert!(screen.contains("Credits · Unlimited"));
    assert!(screen.contains("Reset credit expired"));
    assert!(screen.contains("Credits used: 200 credits"));
    assert!(!screen.contains("Chat"));
    assert!(screen.contains("Completions · Unlimited"));
    assert!(!screen.contains("Private account"));
    assert!(!screen.contains("Example workspace"));
    cli.quit(b"q");
}

#[test]
fn sends_desktop_controls_to_the_service() {
    let bus = Bus::start();
    let service = Fixture::serve(&bus.address, false);
    let mut cli = bus.cli();
    cli.read_until("108.5%");
    for (keys, method) in [
        (&b"R"[..], "Refresh"),
        (&b"s"[..], "StartSignIn"),
        (&b":set hide_identity false\r"[..], "SetSetting"),
        (&b":toggle-model codex:weekly\r"[..], "ToggleModel"),
        (&b":toggle-provider codex\r"[..], "ToggleProviderModels"),
        (&b":interval codex 1\r"[..], "StepProviderInterval"),
        (&b":threshold 75\r"[..], "ToggleNotificationThreshold"),
        (&b":reorder provider cursor codex before\r"[..], "Reorder"),
        (&b":visibility {}\r"[..], "UpdateHistoryVisibility"),
        (&b":models search phrase\r"[..], "GetModelGroups"),
        (&b":diagnostics\r"[..], "GetDiagnostics"),
        (&b":issue\r"[..], "GetIssueUrl"),
        (&b":spend\r"[..], "GetSpendSummary"),
        (&b":reviews\r"[..], "GetReviewSummary"),
        (&b":logs full\r"[..], "GetLogs"),
        (&b":clear logs confirm\r"[..], "ClearLogs"),
        (&b":clear history confirm\r"[..], "ClearHistory"),
        (&b":reset settings confirm\r"[..], "ResetSettings"),
        (&b"["[..], "GetHistoryView"),
        (&b"]"[..], "GetHistoryView"),
        (&b"n"[..], "GetHistoryView"),
        (&b"r"[..], "GetHistoryView"),
    ] {
        cli.write(keys);
        service.wait_for(method, &mut cli);
    }
    cli.read_until("Latest 78.00");
    let directory = tempdir().unwrap();
    let metric_path = directory.path().join("metric file.csv");
    let all_path = directory.path().join("all file.csv");
    cli.write(format!(":export {}\r", metric_path.display()).as_bytes());
    service.wait_for("ExportMetricHistory", &mut cli);
    cli.write(format!(":export-all {}\r", all_path.display()).as_bytes());
    service.wait_for("ExportAllHistory", &mut cli);
    cli.read_until("History exported");
    cli.write(b":export-all /\r");
    cli.read_until("Choose a filename for the export");
    cli.write(b":refresh invalid\r");
    cli.read_until("was declined by Quotaline");
    cli.write(b"1");
    cli.read_until("Quota used");
    cli.quit(b"q");
}

#[test]
fn history_uses_service_supplied_private_project_labels() {
    let bus = Bus::start();
    let _service = Fixture::serve(&bus.address, true);
    let mut cli = bus.cli();
    cli.read_until("108.5%");
    cli.write(b":metric projectCost\r");
    cli.read_until("Project 1");
    cli.read_until("$1.9k");
    assert!(!cli.screen.screen().contents().contains("Weekly"));
    cli.write(b":metric unavailable\r");
    cli.read_until("No samples yet");
    cli.write(b":metric no_history\r");
    cli.read_until("History unavailable");
    cli.quit(b"q");
}

#[test]
fn retains_the_last_snapshot_when_the_connection_closes() {
    let bus = Bus::start();
    let _service = Fixture::serve(&bus.address, false);
    let mut cli = bus.cli();
    cli.read_until("108.5%");
    drop(bus);
    cli.read_until("Stale");
    assert!(cli.screen.screen().contents().contains("108.5%"));
    cli.quit(b"q");
}

#[test]
fn quitting_does_not_wait_for_a_blocked_desktop_request() {
    let bus = Bus::start();
    let service = Fixture::serve(&bus.address, false);
    let mut cli = bus.cli();
    cli.read_until("108.5%");
    cli.write(b":models stall\r");
    service.wait_for("GetModelGroups", &mut cli);
    cli.quit(b"q");
}

#[cfg(feature = "gnome-smoke")]
#[test]
fn reads_the_real_service_with_the_usage_export_disabled() {
    let bus = Bus::start();
    let directory = tempdir().unwrap();
    let root = directory.path();
    let data = root.join("data");
    let _service = Process(
        Command::new(std::env::var_os("QUOTALINE_TUI_TEST_GNOME_BIN").unwrap())
            .arg("--demo")
            .env_clear()
            .env("HOME", root)
            .env("XDG_DATA_HOME", &data)
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("DBUS_SESSION_BUS_ADDRESS", &bus.address)
            .env("DBUS_SYSTEM_BUS_ADDRESS", &bus.address)
            .spawn()
            .unwrap(),
    );
    let mut cli = bus.cli();
    cli.read_until("Codex");
    cli.write(b"3");
    cli.read_until("Codex collection");
    cli.write(b"\tj\x1b[Cj");
    cli.read_until("› Account identity");
    cli.write(b"\r");
    cli.read_until("Settings saved");
    cli.write(b"2");
    cli.read_until("Latest");
    let preferences: Value =
        serde_json::from_slice(&fs::read(root.join("config/quotaline/settings.json")).unwrap()).unwrap();
    assert_eq!(preferences["hide_identity"], true);
    assert_eq!(preferences["decimals"], 1);
    cli.click(51, 23);
    cli.write(b"set hide_identity false");
    cli.read_until(":set hide_identity false");
    cli.click(75, 23);
    cli.read_until("\x1b[?1006h\x1b[?1003h");
    cli.click(51, 23);
    cli.write(b"set decimals 2");
    cli.read_until(":set decimals 2");
    cli.click(69, 23);
    cli.read_until("Settings saved");
    let preferences: Value =
        serde_json::from_slice(&fs::read(root.join("config/quotaline/settings.json")).unwrap()).unwrap();
    assert_eq!(preferences["hide_identity"], true);
    assert_eq!(preferences["decimals"], 2);
    cli.write(b"v");
    cli.read_until("Series hidden");
    cli.write(b"i");
    cli.read_until("Latest");
    cli.write(b"4");
    cli.read_until("Service started");
    cli.quit(b"q");
    assert!(!data.join("quotaline/usage.json").exists());
}

struct Bus {
    _process: Process,
    address: String,
}

impl Bus {
    fn start() -> Self {
        let directory = tempdir().unwrap();
        let config = directory.path().join("bus.conf");
        fs::write(
            &config,
            r#"<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen>
            <policy context="default"><allow send_destination="*"/><allow receive_sender="*"/>
            <allow own="*"/></policy></busconfig>"#,
        )
        .unwrap();
        let mut child = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            _process: Process(child),
            address: address.trim().into(),
        }
    }

    fn cli(&self) -> Interactive {
        Interactive::start(command().env("DBUS_SESSION_BUS_ADDRESS", &self.address))
    }
}

struct Process(Child);

impl Drop for Process {
    fn drop(&mut self) {
        self.0.kill().unwrap();
        self.0.wait().unwrap();
    }
}

struct Fixture {
    calls: mpsc::Receiver<&'static str>,
    stop: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Fixture {
    fn serve(address: &str, stacked: bool) -> Self {
        let (ready, online) = mpsc::channel();
        let (calls, received) = mpsc::channel();
        let (stop, shutdown) = mpsc::channel();
        let address = address.to_owned();
        let thread = thread::spawn(move || {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .unwrap()
                .block_on(async {
                    let _connection = Builder::address(address.as_str())
                        .name(SERVICE)
                        .serve_at(OBJECT, Service { calls, stacked })
                        .build()
                        .await
                        .unwrap();
                    ready.send(()).unwrap();
                    shutdown.recv().unwrap();
                });
        });
        online.recv_timeout(Duration::from_secs(5)).unwrap();
        Self {
            calls: received,
            stop,
            thread: Some(thread),
        }
    }

    fn wait_for(&self, expected: &str, cli: &mut Interactive) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.calls.try_iter().any(|method| method == expected) {
            assert!(Instant::now() < deadline, "desktop service did not receive {expected}");
            cli.read_output();
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.send(()).unwrap();
        self.thread.take().unwrap().join().unwrap();
    }
}

struct Service {
    calls: mpsc::Sender<&'static str>,
    stacked: bool,
}

#[zbus::interface(name = "tech.bernat.Quotaline.Gnome")]
impl Service {
    fn get_state(&self) -> String {
        json!({
            "providers": {
                "codex": {
                    "plan":"Example plan","updated":1780000000,"status":"ready",
                    "show_in_usage":true,
                    "identity":"Private account","organization":"Example workspace","show_organization":true,
                    "details":[{"title":"Seats","value":"5"}],
                    "notices":[{"kind":"promotion","text":"Plan renewal"}],"warnings":["Backoff"],
                    "credit_balance":25.0,
                    "cloud_credits":{"remaining":15.0,"limit":50.0},
                    "usage_credits":{"unlimited":true,"has_credits":true,"overage_limit_reached":false},
                    "reset_credits":{"available":3,"applicable":2,"purchase_eligible":false,"details":[]},
                    "reset_credit_expiries":[{"id":"pending-refresh","at":1780003600}],
                    "spend":{"percent":40.0,"limit_reached":false,"balance_money":{"currency":"EUR","amount_minor":125,"exponent":3}},
                    "windows":[
                        {"id":"weekly","label":"Weekly","used":108.5,"reset":"2026-11-20T15:00:00Z",
                         "reset_precision":"day","pace":{"status":"on_track","expected_percent":80}},
                        {"id":"hidden","label":"Unselected","used":999.0}
                    ]
                },
                "cursor":{
                    "plan":"","status":"sign-in","show_in_usage":false,
                    "identity":"","show_organization":false,"details":[],"notices":[],"warnings":[],"windows":[]
                },
                "gemini":{
                    "plan":"","status":"sign-in","show_in_usage":true,
                    "identity":"","show_organization":false,"details":[],"notices":[],"warnings":[],"windows":[]
                },
                "copilot":{
                    "plan":"Pro","updated":1780000000,"status":"ready","show_in_usage":true,
                    "identity":"","show_organization":false,"details":[
                        {"id":"quota:premium_interactions","title":"Premium credits",
                         "value":"Included: 500 credits · Remaining: 300 credits · Credits used: 200 credits · Overage used: 0 credits"},
                        {"id":"quota:chat","title":"Chat","value":"Unlimited · Included: 0 credits"},
                        {"id":"quota:completions","title":"Completions","value":"Unlimited"}
                    ],"notices":[],"warnings":[],
                    "windows":[{"id":"premium_interactions","label":"Premium requests","used":40.0,
                                "reset":"1970-01-01T00:00:00+00:00"},
                               {"id":"completions","label":"Completions","used":null}]
                }
            },
            "ordering":{"providers":["notCached","codex","cursor","gemini","copilot"]},
            "selected_window_keys":["codex:weekly","copilot:premium_interactions","copilot:completions"],
            "settings":{"write_usage_json":false,"hide_identity":true,"history_stacked":self.stacked}
        })
        .to_string()
    }

    fn get_history_view(&self, action: &str, _zone: &str, _offset: i32) -> String {
        self.record("GetHistoryView");
        let action = serde_json::from_str::<Value>(action).unwrap();
        let metric = action["metric"].as_str().unwrap_or("usage");
        let (value, second) = if metric == "projectCost" {
            (1250.0, 500.0)
        } else {
            (78.0, 53.0)
        };
        let (history, error) = match metric {
            "unavailable" => (Value::Null, Some("No samples yet")),
            "no_history" => (Value::Null, None),
            _ => (
                json!({
                    "metric":metric, "start":1780000000, "end":1780123456,
                    "series":[
                        {"id":"codex:weekly","provider":"Codex","label":"Weekly","points":[[1780040000,value,null,false]]},
                        {"id":"second","provider":"Codex","label":"Second","points":[[1780040000,second,null,false]]}
                    ],
                    "canonicalTotal":value, "project_labels":{"Weekly":"Project 1"}
                }),
                None,
            ),
        };
        json!({"history":history,"error":error,"view":{"utc":true}}).to_string()
    }

    fn get_logs(&self, full: bool) -> String {
        self.record("GetLogs");
        json!([{"line": if full { "Retained messages" } else { "Collector ready" }}]).to_string()
    }

    fn get_diagnostics(&self) -> String {
        self.record("GetDiagnostics");
        json!({"version":"TUI test service"}).to_string()
    }

    async fn get_model_groups(&self, query: &str, _revealed: &str) -> String {
        self.record("GetModelGroups");
        if query == "stall" {
            std::future::pending::<()>().await;
        }
        json!({"query":query}).to_string()
    }

    fn refresh(&self, provider: &str) -> bool {
        self.record("Refresh");
        provider != "invalid"
    }

    fn start_sign_in(&self, _provider: &str) -> bool {
        self.record("StartSignIn");
        true
    }

    fn set_setting(&self, _key: &str, _value: &str) -> String {
        self.record("SetSetting");
        self.get_state()
    }

    fn toggle_model(&self, _key: &str) -> String {
        self.record("ToggleModel");
        self.get_state()
    }

    fn toggle_provider_models(&self, _provider: &str) -> String {
        self.record("ToggleProviderModels");
        self.get_state()
    }

    fn step_provider_interval(&self, _provider: &str, _delta: i32) -> String {
        self.record("StepProviderInterval");
        self.get_state()
    }

    fn toggle_notification_threshold(&self, _threshold: i32) -> String {
        self.record("ToggleNotificationThreshold");
        self.get_state()
    }

    fn reorder(&self, _kind: &str, _source: &str, _target: &str, _after: bool) -> String {
        self.record("Reorder");
        self.get_state()
    }

    fn update_history_visibility(&self, _action: &str) -> String {
        self.record("UpdateHistoryVisibility");
        json!({"hidden":["codex:weekly"],"shown":[]}).to_string()
    }

    fn get_spend_summary(&self) -> String {
        self.record("GetSpendSummary");
        "{}".into()
    }

    fn get_review_summary(&self) -> String {
        self.record("GetReviewSummary");
        String::new()
    }

    fn export_all_history(&self, _path: &str) -> bool {
        self.record("ExportAllHistory");
        true
    }

    fn export_metric_history(&self, _metric: &str, _start: i64, _end: i64, _path: &str) -> bool {
        self.record("ExportMetricHistory");
        true
    }

    fn get_issue_url(&self) -> String {
        self.record("GetIssueUrl");
        "https://example.invalid".into()
    }

    fn clear_logs(&self) -> bool {
        self.record("ClearLogs");
        true
    }

    fn clear_history(&self) -> bool {
        self.record("ClearHistory");
        true
    }

    fn reset_settings(&self) -> String {
        self.record("ResetSettings");
        self.get_state()
    }
}

impl Service {
    fn record(&self, method: &'static str) {
        self.calls.send(method).unwrap();
    }
}
