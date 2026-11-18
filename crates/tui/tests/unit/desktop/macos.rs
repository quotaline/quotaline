use super::*;
use quotaline_history::{AnalyticsPoint, Sample, SampleWrite};
use quotaline_macos_ffi::{
    logging::log_make_entry,
    provider_record::{ProviderReadingRecord, ReadingSource},
    reading_cache::encode_reading_cache,
};
use quotaline_quota::{
    CloudCredits, Detail, Notice, NoticeKind, Provider as QuotaProvider, ResetCredits, ResetPrecision, UsageCredits,
    Window as QuotaWindow,
};
use serde_json::json;
use std::fs;

struct Fixture {
    home: tempfile::TempDir,
    library: PathBuf,
    support: PathBuf,
}

impl Fixture {
    fn new(sandboxed: bool) -> Self {
        let home = tempfile::tempdir().unwrap();
        let library = home.path().join(if sandboxed {
            "Library/Containers/tech.bernat.quotaline/Data/Library"
        } else {
            "Library"
        });
        let support = library.join("Application Support/Quotaline");
        fs::create_dir_all(&support).unwrap();
        fs::create_dir_all(library.join("Preferences")).unwrap();
        Self { home, library, support }
    }

    fn backend(&self) -> Backend {
        Backend {
            home: Some(self.home.path().to_owned()),
            ..Backend::default()
        }
    }

    fn write_preferences(&self, values: Vec<(&str, plist::Value)>, binary: bool) {
        let preferences =
            plist::Value::Dictionary(values.into_iter().map(|(key, value)| (key.to_owned(), value)).collect());
        let path = self.library.join("Preferences/tech.bernat.quotaline.plist");
        if binary {
            preferences.to_file_binary(path).unwrap();
        } else {
            preferences.to_file_xml(path).unwrap();
        }
    }

    fn settings(&self, value: &Settings, binary: bool) {
        self.write_preferences(
            vec![("settings", plist::Value::Data(serde_json::to_vec(value).unwrap()))],
            binary,
        );
    }

    fn readings(&self, demo: bool) {
        let mut codex = QuotaProvider::empty("codex", "connected");
        codex.identity = "someone@example.com".into();
        codex.organization = "Private Org".into();
        codex.plan = "Plus".into();
        codex.credit_balance = Some(12.5);
        codex.cloud_credits = Some(CloudCredits {
            limit: 20.0,
            remaining: 10.0,
            expires_at: None,
            locked_reason: None,
        });
        codex
            .notices
            .push(Notice::new(NoticeKind::Info, "Private Org limit reached"));
        codex.details.push(Detail {
            id: "feature".into(),
            title: "Feature".into(),
            value: "Access".into(),
            explanation: String::new(),
        });
        codex.details.push(Detail {
            id: "oauth-availability".into(),
            title: "Excluded".into(),
            value: "Internal".into(),
            explanation: String::new(),
        });
        let reset = Timestamp::from_second(Timestamp::now().as_second() + 3600)
            .unwrap()
            .to_string();
        codex.windows = [
            (
                "session",
                "Current session",
                24.0,
                Some(reset.as_str()),
                ResetPrecision::Day,
            ),
            ("alternate", "Alternate window", 12.0, None, ResetPrecision::Instant),
            ("ahead", "Ahead", 80.0, Some(reset.as_str()), ResetPrecision::Instant),
            (
                "on-track",
                "On track",
                50.0,
                Some(reset.as_str()),
                ResetPrecision::Instant,
            ),
            (
                "exhausted",
                "Exhausted",
                100.0,
                Some(reset.as_str()),
                ResetPrecision::Instant,
            ),
        ]
        .into_iter()
        .map(|(id, label, used, reset, precision)| {
            let mut window = QuotaWindow::new(id, label, used, reset.map(str::to_owned), "");
            window.duration_seconds = reset.map(|_| 7200);
            window.reset_precision = precision;
            window
        })
        .collect();
        self.write_readings(demo, vec![codex, QuotaProvider::empty("cursor", "connected")]);
    }

    fn write_readings(&self, demo: bool, providers: Vec<QuotaProvider>) {
        let readings = providers
            .into_iter()
            .map(|provider| ProviderReadingRecord {
                provider: provider.into(),
                fetched_at: 1_780_000_000.0,
                source: ReadingSource::Network,
            })
            .collect();
        fs::write(
            self.support
                .join(if demo { "snapshots-demo.json" } else { "snapshots.json" }),
            encode_reading_cache(readings),
        )
        .unwrap();
    }

    fn seed_history(&self, demo: bool) {
        let now = Timestamp::now();
        let store = Store::open(
            Some(
                &self
                    .support
                    .join(if demo { "usage-demo.sqlite" } else { "usage.sqlite" }),
            ),
            60,
        )
        .unwrap();
        store
            .seed(&[
                SampleWrite {
                    sample: Sample {
                        at: (now.as_second() - 120) as f64,
                        key: "codex:session".into(),
                        used: 0.0,
                        reset_at: None,
                    },
                    label: "Current session".into(),
                },
                SampleWrite {
                    sample: Sample {
                        at: (now.as_second() - 60) as f64,
                        key: "codex:session".into(),
                        used: 24.0,
                        reset_at: None,
                    },
                    label: "Current session".into(),
                },
                SampleWrite {
                    sample: Sample {
                        at: (now.as_second() - 60) as f64,
                        key: "codex:alternate".into(),
                        used: 12.0,
                        reset_at: None,
                    },
                    label: "Alternate window".into(),
                },
            ])
            .unwrap();
        let days = analytics::spend_days(now);
        let rows = [
            (days.yesterday.as_str(), "projectCost", "/private/secret-project", 9.0),
            (days.today.as_str(), "codeReviews", "total", 4.0),
            (days.today.as_str(), "costUSD", "model", 10.0),
        ]
        .map(|(day, metric, series, value)| AnalyticsPoint {
            provider: "codex".into(),
            day: day.into(),
            metric: metric.into(),
            series: series.into(),
            value,
        });
        store
            .record_analytics("codex", None, now.as_second() as f64, &rows, &[])
            .unwrap();
    }
}

fn text(output: Output) -> Vec<String> {
    payload(output, "Text")
}

fn history(output: Output) -> History {
    payload(output, "History")
}

fn payload<Value: serde::de::DeserializeOwned>(output: Output, variant: &str) -> Value {
    serde_json::from_value(serde_json::to_value(output).unwrap()[variant].take()).unwrap()
}

#[test]
fn loads_native_cache_with_preferences() {
    for (sandboxed, binary) in [(false, true), (true, false)] {
        let fixture = Fixture::new(sandboxed);
        fixture.readings(false);
        fixture.settings(
            &Settings {
                providers: [("codex".into(), true), ("cursor".into(), false)].into(),
                configured_providers: ["cursor".into()].into(),
                has_custom_selection: Some(true),
                selected_windows: vec!["codex:alternate".into()],
                hide_identity: true,
                demo_choice: Some(false),
                ..Settings::default()
            },
            binary,
        );
        let state = fixture.backend().usage().unwrap();
        let provider = &state.providers[0];
        assert_eq!(
            (
                state.providers.len(),
                provider.id.as_str(),
                provider.plan.as_deref(),
                provider.status.as_str(),
                provider.fetched_at.map(|stamp| stamp.as_second()),
                provider.windows.len(),
                provider.windows[0].label.as_str(),
                provider.windows[0].used_percent,
                provider.windows[0].resets_at,
                state.settings["hide_identity"].as_bool()
            ),
            (
                1,
                "codex",
                Some("Plus"),
                "Cached",
                Some(1_780_000_000),
                1,
                "Alternate window",
                Some(12.0),
                None,
                Some(true)
            ),
        );
        assert_eq!(
            provider.details,
            [
                "account",
                "workspace",
                "Feature · Access",
                "workspace limit reached",
                "Credits · 12.50",
                "Cloud session credits · $10.00 of $20.00 left"
            ]
        );
        assert_eq!(
            provider.overview,
            [
                "Credits · $12.50 remaining",
                "Cloud session credits · $10.00 of $20.00 remaining"
            ]
        );
    }
}

#[test]
fn native_overview_preserves_selected_quota_and_credit_facts() {
    let fixture = Fixture::new(false);
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            hide_identity: false,
            has_custom_selection: Some(true),
            selected_windows: vec!["copilot:premium_interactions".into()],
            ..Settings::default()
        },
        true,
    );
    let mut copilot = QuotaProvider::empty("copilot", "connected");
    copilot.identity = "example@example.com".into();
    copilot.warnings.push("Quota reporting delayed".into());
    copilot.windows.push(QuotaWindow::new(
        "premium_interactions",
        "Premium requests",
        60.0,
        None,
        "",
    ));
    for (id, title, value) in [
        (
            "quota:premium_interactions",
            "Premium requests",
            "Remaining: 120 · Included: 300 · Credits used: 180",
        ),
        ("quota:chat", "Chat", "Unlimited"),
    ] {
        copilot.details.push(Detail {
            id: id.into(),
            title: title.into(),
            value: value.into(),
            explanation: String::new(),
        });
    }
    let mut codex = QuotaProvider::empty("codex", "connected");
    codex.usage_credits = Some(UsageCredits {
        unlimited: true,
        has_credits: Some(true),
        overage_limit_reached: false,
        local_messages: None,
        cloud_messages: None,
    });
    codex.reset_credits = Some(ResetCredits {
        available: 2,
        applicable: 1,
        total_earned: None,
        purchase_eligible: false,
        details: Vec::new(),
    });
    fixture.write_readings(false, vec![codex, copilot]);
    let providers = fixture.backend().usage().unwrap().providers;
    assert_eq!(
        providers[0].overview,
        ["Credits · Unlimited", "Limit resets · 2 available (1 applicable now)",]
    );
    assert_eq!(
        providers[1].overview,
        [
            "Premium requests · Remaining: 120 · Included: 300 · Credits used: 180",
            "Provider warnings · See details"
        ]
    );
    assert_eq!(providers[1].details[0], "example@example.com");
    assert!(
        providers[1]
            .details
            .iter()
            .any(|detail| detail == "Quota reporting delayed")
    );
}

#[test]
fn reads_legacy_provider_and_window_settings() {
    let fixture = Fixture::new(false);
    fixture.readings(true);
    fixture.write_preferences(
        vec![
            ("demoMode", plist::Value::Boolean(true)),
            ("enabledProviders", plist::Value::Data(br#"["codex"]"#.to_vec())),
            ("providerOrder", plist::Value::Data(br#"["codex","cursor"]"#.to_vec())),
            (
                "selectedWindows",
                plist::Value::Data(br#"[{"provider":"codex","windowID":"alternate"}]"#.to_vec()),
            ),
            ("hasCustomSelection", plist::Value::Boolean(true)),
        ],
        true,
    );
    let state = fixture.backend().usage().unwrap();
    assert_eq!(
        (
            state.providers.len(),
            state.providers[0].windows[0].label.as_str(),
            state.settings["demo"].as_bool()
        ),
        (1, "Alternate window", Some(true))
    );
}

#[test]
fn defaults_mask_identity_and_select_standard_windows() {
    let fixture = Fixture::new(false);
    fixture.readings(false);
    let state = fixture.backend().usage().unwrap();
    let codex = state.providers.iter().find(|provider| provider.id == "codex").unwrap();
    assert_eq!(
        (
            codex.windows.len(),
            codex.windows[0].label.as_str(),
            codex.details[0].as_str()
        ),
        (1, "Current session", "account")
    );
}

#[test]
fn surfaces_missing_or_malformed_mac_data() {
    let fixture = Fixture::new(false);
    fs::remove_dir(&fixture.support).unwrap();
    let before_first_reading = fixture.backend().usage().unwrap();
    assert!(before_first_reading.providers.is_empty());
    assert_eq!(before_first_reading.settings["hide_identity"].as_bool(), Some(true));
    fs::create_dir(&fixture.support).unwrap();
    assert!(
        Backend {
            home: None,
            ..Backend::default()
        }
        .usage()
        .err()
        .unwrap()
        .to_string()
        .contains("HOME")
    );
    fixture.write_preferences(vec![("settings", plist::Value::Boolean(true))], true);
    assert_eq!(
        fixture.backend().usage().err().unwrap().to_string(),
        "Invalid macOS settings document"
    );
    fixture.write_preferences(vec![("settings", plist::Value::Data(b"{".to_vec()))], true);
    assert_eq!(
        fixture.backend().usage().err().unwrap().kind(),
        io::ErrorKind::UnexpectedEof
    );
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            ..Settings::default()
        },
        true,
    );
    fs::write(fixture.support.join("snapshots.json"), b"{").unwrap();
    assert!(fixture.backend().usage().is_err());
}

#[test]
fn charts_and_exports_native_history() {
    let fixture = Fixture::new(false);
    fixture.readings(true);
    fixture.settings(
        &Settings {
            demo_choice: Some(true),
            history_days: 7,
            hide_identity: true,
            ..Settings::default()
        },
        true,
    );
    fixture.seed_history(true);
    let mut backend = fixture.backend();
    let usage = history(backend.execute(Command::History(json!({"kind":"load"}))).unwrap());
    assert_eq!(
        usage.series.iter().map(|series| series.id.as_str()).collect::<Vec<_>>(),
        ["codex:session"]
    );
    assert!(usage.series[0].points.iter().any(|point| point.1 == 24.0));
    let project = history(
        backend
            .execute(Command::History(json!({"kind":"metric","metric":"projectCost"})))
            .unwrap(),
    );
    assert_eq!(project.canonical_total, Some(9.0));
    assert_eq!(project.series[0].label, project::anonymous("/private/secret-project"));
    let empty = history(
        backend
            .execute(Command::History(json!({"kind":"metric","metric":"credits"})))
            .unwrap(),
    );
    assert!(empty.series.is_empty());
    let invalid = backend
        .execute(Command::History(json!({"kind":"period","days":3})))
        .err()
        .unwrap();
    assert_eq!(invalid.to_string(), "Unknown history period");
    let export = fixture.home.path().join("all.csv");
    assert_eq!(
        backend
            .execute(Command::ExportAll("/".into()))
            .err()
            .unwrap()
            .to_string(),
        "Choose a file for the history export",
    );
    assert!(matches!(
        backend
            .execute(Command::ExportAll(export.display().to_string()))
            .unwrap(),
        Output::Done(_)
    ));
    let all = fs::read_to_string(export).unwrap();
    assert!(all.contains("costUSD"));
    assert!(!all.contains("secret-project"));
    for chart in [usage, project] {
        let path = fixture.home.path().join(format!("{}.csv", chart.metric));
        let command =
            || Command::ExportMetric(chart.metric.clone(), chart.start, chart.end, path.display().to_string());
        assert!(matches!(backend.execute(command()).unwrap(), Output::Done(_)));
        assert_eq!(
            backend.execute(command()).err().unwrap().kind(),
            io::ErrorKind::AlreadyExists
        );
        let output = fs::read_to_string(path).unwrap();
        assert!(output.contains("codex"));
        assert!(!output.contains("codex:alternate"));
        assert!(!output.contains("secret-project"));
    }
}

#[test]
fn summarizes_costs_and_reviews_from_stored_analytics() {
    let fixture = Fixture::new(true);
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            ..Settings::default()
        },
        true,
    );
    let mut backend = fixture.backend();
    assert!(backend.execute(Command::Spend).is_err());
    let store = Store::open(Some(&fixture.support.join("usage.sqlite")), 60).unwrap();
    assert_eq!(
        text(backend.execute(Command::Reviews).unwrap()),
        ["No review history yet"]
    );
    drop(store);
    fixture.seed_history(false);
    assert_eq!(
        text(backend.execute(Command::Spend).unwrap()),
        [
            "Today $10.00 · Yesterday $0.00 · Last 30 days $10.00",
            "codex · model · $10.00"
        ]
    );
    assert_eq!(
        text(backend.execute(Command::Reviews).unwrap()),
        ["4 today · 4 this week"]
    );
}

#[test]
fn reports_models_and_redacts_persisted_diagnostics() {
    let fixture = Fixture::new(false);
    fixture.readings(false);
    let now = Timestamp::now().as_second() as f64;
    let rows = (0..28)
        .map(|index| {
            log_make_entry(
                now + f64::from(index),
                "info".into(),
                "app".into(),
                format!("Private Org entry {index}"),
                String::new(),
            )
            .line
        })
        .collect::<Vec<_>>();
    fs::write(fixture.support.join("log.txt.1"), rows[..2].join("\n")).unwrap();
    fs::write(fixture.support.join("log.txt"), rows[2..].join("\n")).unwrap();
    let mut backend = fixture.backend();
    assert_eq!(
        text(backend.execute(Command::Models("ALTERNATE".into())).unwrap()),
        ["Hidden · codex:alternate · Alternate window"]
    );
    assert_eq!(
        text(backend.execute(Command::Models("Current".into())).unwrap()),
        ["Selected · codex:session · Current session"]
    );
    let tail = text(backend.execute(Command::Logs(false)).unwrap());
    assert_eq!(tail.len(), 25);
    assert!(tail[0].ends_with("workspace entry 3"));
    let full = text(backend.execute(Command::Logs(true)).unwrap());
    assert_eq!(full.len(), 28);
    assert!(full[0].ends_with("workspace entry 0"));
    let report = text(backend.execute(Command::Diagnostics).unwrap());
    assert!(report.contains(&"Codex · Cached".into()));
    assert!(report.iter().all(|line| !line.contains("Private Org")));
    let url = text(backend.execute(Command::IssueUrl).unwrap());
    assert!(url[0].contains("/issues/new?"));
    assert!(!url[0].contains("Private"));
}

#[test]
fn routes_mutations_to_native_app() {
    let fixture = Fixture::new(false);
    let mut backend = fixture.backend();
    for (command, action) in [
        (Command::Refresh("codex".into()), "Refresh codex"),
        (Command::SignIn("codex".into()), "Set up codex"),
        (Command::Set("retention".into(), "30".into()), "Change retention"),
        (
            Command::ToggleModel("codex:session".into()),
            "Change selection for codex:session",
        ),
        (
            Command::ToggleProviderModels("codex".into()),
            "Change model selection for codex",
        ),
        (
            Command::StepInterval("codex".into(), 1),
            "Change the refresh interval for codex",
        ),
        (Command::ToggleThreshold(20), "Change the 20% threshold"),
        (
            Command::Reorder("provider".into(), "codex".into(), "cursor".into(), true),
            "Move codex",
        ),
        (Command::ClearLogs, "Clear logs"),
        (Command::ClearHistory, "Clear history"),
        (Command::ResetSettings, "Reset settings"),
    ] {
        let error = backend.execute(command).err().unwrap();
        assert_eq!(
            (error.kind(), error.to_string()),
            (
                io::ErrorKind::Unsupported,
                format!("{action} in the Quotaline menu bar app")
            )
        );
    }
}

#[test]
fn reports_pace_and_day_precision_from_cached_windows() {
    let fixture = Fixture::new(false);
    fixture.readings(false);
    fixture.seed_history(false);
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            has_custom_selection: Some(true),
            selected_windows: ["session", "alternate", "ahead", "on-track", "exhausted"]
                .map(|window| format!("codex:{window}"))
                .into(),
            ..Settings::default()
        },
        true,
    );
    let state = fixture.backend().usage().unwrap();
    let provider = &state.providers[0];
    let presentation = provider
        .windows
        .iter()
        .map(|window| {
            (
                window.pace.as_deref().map(|pace| pace.split(" · ").next().unwrap()),
                window.reset_by_day,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        presentation,
        [
            (Some("Behind pace"), true),
            (None, false),
            (Some("Ahead of pace"), false),
            (Some("On track"), false),
            (Some("Exhausted"), false),
        ]
    );
    assert_eq!(
        provider.windows[0].pace.as_deref(),
        Some("Behind pace · hits 100% in 3m")
    );
    assert_eq!(&provider.details[..2], ["someone@example.com", "Private Org"]);
}

#[test]
fn history_visibility_changes_only_the_terminal_session() {
    let fixture = Fixture::new(false);
    fixture.readings(false);
    fixture.seed_history(false);
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            history_utc: true,
            history_days: 7,
            has_custom_selection: Some(true),
            selected_windows: vec!["codex:session".into(), "codex:alternate".into()],
            history_hidden_keys: vec!["codex:session".into()],
            ..Settings::default()
        },
        true,
    );
    let mut backend = fixture.backend();
    let load = || Command::History(json!({"kind":"load"}));
    let toggle = || {
        Command::Visibility(
            json!({
                "kind": "toggle",
                "metric": "usage",
                "id": "codex:session",
                "series": ["codex:session", "codex:alternate"]
            })
            .to_string(),
        )
    };
    let initial = history(backend.execute(load()).unwrap());
    assert_eq!(initial.hidden, ["codex:session".into()].into());
    assert!(initial.series.iter().any(|series| series.id == "codex:session"));
    assert!(history(backend.execute(toggle()).unwrap()).hidden.is_empty());
    assert!(history(backend.execute(load()).unwrap()).hidden.is_empty());
    assert_eq!(history(backend.execute(toggle()).unwrap()).hidden, initial.hidden);
    assert_eq!(history(backend.execute(load()).unwrap()).hidden, initial.hidden);
    assert_eq!(
        history(fixture.backend().execute(load()).unwrap()).hidden,
        initial.hidden
    );
}

#[test]
fn logs_allow_workspace_names_when_identity_is_visible() {
    let fixture = Fixture::new(false);
    fixture.readings(false);
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            ..Settings::default()
        },
        true,
    );
    let line = log_make_entry(
        Timestamp::now().as_second() as f64,
        "info".into(),
        "app".into(),
        "Private Org".into(),
        String::new(),
    )
    .line;
    fs::write(fixture.support.join("log.txt"), &line).unwrap();
    assert_eq!(text(fixture.backend().execute(Command::Logs(false)).unwrap()), [line]);
}

#[test]
fn history_loads_unknown_saved_metrics_as_empty() {
    let fixture = Fixture::new(false);
    fixture.seed_history(false);
    fixture.settings(
        &Settings {
            demo_choice: Some(false),
            history_metric: "removed".into(),
            ..Settings::default()
        },
        true,
    );
    let data = history(
        fixture
            .backend()
            .execute(Command::History(json!({"kind":"load"})))
            .unwrap(),
    );
    assert_eq!(
        (data.metric.as_str(), data.series.len(), data.canonical_total),
        ("removed", 0, None)
    );
}
