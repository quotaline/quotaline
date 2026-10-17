//! Exercises service spend and demo in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn claude_spend_reason_and_purchase_reach_shell() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/usage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "five_hour":{"utilization":42},
                "credit_reset_at":"2026-11-01T00:00:00Z",
                "spend":{"enabled":false,"disabled_reason":"organization_disabled",
                    "can_purchase_credits":true,"can_toggle":true,
                    "balance":{"amount_minor":3760,"currency":"USD"},"auto_reload":true}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/api/oauth/profile"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(1)
            .mount(&server)
            .await;
    });
    let fixture = claude_live_service(&server);
    runtime.block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["claude"]["status"] == "connected").await;
        assert_eq!(
            state["providers"]["claude"]["spend"]["disabled_reason"],
            "Organization Disabled"
        );
    });
    run_shell_script(&fixture, "extension/tests/integration/claude-spend.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_invalid_reset_is_readable_in_shell() {
    claude_reset_shell_fixture("not-a-date", "Reset unknown", "claude-invalid-reset.png");
}

#[cfg(feature = "fixture-api")]
#[test]
fn claude_overdue_reset_matches_macos_deadline() {
    let reset = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    claude_reset_shell_fixture(&reset, "Reset due\nAwaiting refresh", "claude-reset-due.png");
}

#[cfg(feature = "fixture-api")]
#[test]
fn copilot_day_reset_uses_the_provider_calendar_date_in_shell() {
    let _shell = shell_lock();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let server = runtime.block_on(MockServer::start());
    let day = (chrono::Utc::now().date_naive() + chrono::Duration::days(3))
        .format("%Y-%m-%d")
        .to_string();
    runtime.block_on(async {
        Mock::given(matchers::method("GET"))
            .and(matchers::path("/copilot_internal/user"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "quota_reset_date":day,
                "quota_snapshots":{"chat":{"entitlement":10.125,"quota_remaining":4.05}}
            })))
            .expect(1)
            .mount(&server)
            .await;
    });
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":true});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |root| {
            let credentials = root.join("home/.copilot/config.json");
            fs::create_dir_all(credentials.parent().unwrap()).unwrap();
            fs::write(
                credentials,
                r#"{"loggedInUsers":[{"token":"copilot-fixture","host":"github.com","user":"octocat"}]}"#,
            )
            .unwrap();
        },
        |command| {
            command.env("TMB_FIXTURE_API_URL", server.uri());
        },
    );
    runtime.block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        wait_for_state(&proxy, |state| {
            state["providers"]["copilot"]["windows"][0]["reset_precision"] == "day"
        })
        .await;
    });
    run_shell_cases_with_reset(
        &fixture,
        &[
            ("copilot-day.test.js", &[("TMB_RESET_DAY", day.as_str())]),
            ("copilot-detail-help.test.js", &[]),
        ],
        -1,
        None,
        &[],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn disabled_cursor_spend_remains_visible_in_shell() {
    cursor_shell_fixture(
        serde_json::json!({"individualUsage":{"onDemand":{"enabled":false,"used":100,"limit":100,"remaining":0}}}),
        "extension/tests/integration/cursor-spend.test.js",
        &[],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_unlimited_notice_remains_visible_without_quota_rows() {
    cursor_shell_fixture(
        serde_json::json!({"isUnlimited":true}),
        "extension/tests/integration/cursor-unlimited.test.js",
        &[],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_spend_without_reported_limit_stays_readable_in_shell() {
    cursor_shell_fixture(
        serde_json::json!({"individualUsage":{"onDemand":{"used":200}}}),
        "extension/tests/integration/cursor-spend-variant.test.js",
        &[
            (
                "TMB_EXPECT_SPEND",
                "Enablement not reported · $2.00 of unreported limit",
            ),
            ("TMB_SPEND_SCREENSHOT", "cursor-spend-no-limit.png"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_spend_without_reported_usage_stays_readable_in_shell() {
    cursor_shell_fixture(
        serde_json::json!({"individualUsage":{"onDemand":{"limit":1000}}}),
        "extension/tests/integration/cursor-spend-variant.test.js",
        &[
            ("TMB_EXPECT_SPEND", "Enablement not reported · Unavailable of $10.00"),
            ("TMB_SPEND_SCREENSHOT", "cursor-spend-no-usage.png"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_spend_without_amounts_shows_unavailable_summary() {
    cursor_shell_fixture(
        serde_json::json!({"individualUsage":{"onDemand":{"enabled":true}}}),
        "extension/tests/integration/cursor-spend-variant.test.js",
        &[
            ("TMB_EXPECT_SPEND", "Usage unavailable"),
            ("TMB_SPEND_SCREENSHOT", "cursor-spend-no-amounts.png"),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_exhausted_limit_uses_the_warning_color_in_shell() {
    cursor_shell_fixture(
        serde_json::json!({"individualUsage":{"plan":{"enabled":true,"used":100,"limit":100,"remaining":0}}}),
        "extension/tests/integration/cursor-high-use.test.js",
        &[],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn cursor_at_limit_format_shows_reset_in_warning_color() {
    let reset = (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
    cursor_shell_fixture(
        serde_json::json!({"billingCycleEnd":reset,"individualUsage":{"plan":{"enabled":true,"used":100,"limit":100,"remaining":0}}}),
        "extension/tests/integration/cursor-at-limit.test.js",
        &[],
    );
}

#[test]
fn rolled_history_uses_local_calendar_buckets_and_rejects_unknown_rollups() {
    let _shell = shell_lock();
    let scratch = tempfile::tempdir().unwrap();
    let data = scratch.path().join("data/quotaline");
    let config = scratch.path().join("config/quotaline");
    fs::create_dir_all(&data).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(scratch.path().join("home")).unwrap();
    fs::create_dir_all(scratch.path().join("cache")).unwrap();
    fs::write(
        config.join("settings.json"),
        b"{\"providers\":{\"claude\":true,\"codex\":false,\"gemini\":false,\"antigravity\":false,\"cursor\":false,\"copilot\":false}}",
    )
    .unwrap();
    let db = rusqlite::Connection::open(data.join("history.sqlite3")).unwrap();
    db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id))").unwrap();
    let stamp = |date: &str| chrono::DateTime::parse_from_rfc3339(date).unwrap().timestamp();
    // A recent day keeps the samples inside retention; the zone below has no daylight saving.
    let day = (chrono::Utc::now() - chrono::Duration::days(3)).date_naive();
    let next = day.succ_opt().unwrap();
    for (time, used) in [("06:50:00Z", 10.0), ("07:10:00Z", 20.0)] {
        db.execute(
            "INSERT INTO samples VALUES(?1,'claude','session','Session',?2,NULL)",
            rusqlite::params![stamp(&format!("{day}T{time}")), used],
        )
        .unwrap();
    }
    drop(db);
    let (_bus, address) = private_bus(scratch.path(), "session-bus");
    let _service = Process(
        environment(
            &mut Command::new(env!("CARGO_BIN_EXE_quotaline-gnome")),
            scratch.path(),
            address.trim(),
        )
        .env("TZ", "Etc/GMT+7")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap(),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let period = (stamp(&format!("{day}T06:00:00Z")), stamp(&format!("{day}T08:00:00Z")));
        let end = stamp(&format!("{next}T00:00:00Z"));
        let custom = serde_json::json!({"kind":"custom","from":day.to_string(),"to":day.to_string()});
        history_view(&proxy, serde_json::json!({"kind":"rollup","rollup":"day"})).await;
        let local = history_view(&proxy, custom.clone()).await;
        // UTC-7 splits 06:50 and 07:10 UTC across midnight; each line ends at the period's end.
        assert_eq!(
            local["history"]["series"][0]["points"],
            serde_json::json!([
                [stamp(&format!("{day}T06:50:00Z")), 10.0, null, false],
                [stamp(&format!("{day}T07:10:00Z")), 20.0, null, false],
                [end, 20.0, null, true]
            ])
        );
        let utc = history_view(&proxy, serde_json::json!({"kind":"utc","utc":true})).await;
        assert_eq!(
            utc["history"]["series"][0]["points"],
            serde_json::json!([
                [stamp(&format!("{day}T07:10:00Z")), 20.0, null, false],
                [end, 20.0, null, true]
            ])
        );
        let action = serde_json::json!({"kind":"rollup","rollup":"week"}).to_string();
        let invalid: zbus::Result<String> = proxy.call("GetHistoryView", &(action, "UTC", 0_i32)).await;
        assert!(invalid.unwrap_err().to_string().contains("Unknown history rollup"));
        let db = rusqlite::Connection::open(data.join("history.sqlite3")).unwrap();
        for (date, value) in [(day, 10), (next, 20)] {
            db.execute(
                "INSERT INTO analytics VALUES('claude',?1,'costUSD','total',?2)",
                rusqlite::params![date.to_string(), value],
            )
            .unwrap();
        }
        let daily = history_view(&proxy, serde_json::json!({"kind":"metric","metric":"costUSD"})).await;
        assert_eq!(daily["history"]["series"][0]["points"].as_array().unwrap().len(), 1);
        db.execute(
            "INSERT INTO analytics VALUES('claude',?1,'costUSD','opus',4)",
            [chrono::Utc::now().date_naive().to_string()],
        )
        .unwrap();
        let spend: String = proxy.call("GetSpendSummary", &()).await.unwrap();
        let spend: serde_json::Value = serde_json::from_str(&spend).unwrap();
        assert_eq!(spend["by_provider"]["claude"]["today"], 4.0);
        let reviews: String = proxy.call("GetReviewSummary", &()).await.unwrap();
        assert!(reviews.is_empty());
        let csv_path = scratch.path().join("quota.csv");
        let exported: bool = proxy
            .call(
                "ExportMetricHistory",
                &("usage", period.0, period.1, csv_path.to_str().unwrap()),
            )
            .await
            .unwrap();
        assert!(exported);
        assert_eq!(fs::read_to_string(&csv_path).unwrap().lines().count(), 3);
        assert_eq!(fs::metadata(&csv_path).unwrap().permissions().mode() & 0o777, 0o600);
        let cleared: bool = proxy.call("ClearHistory", &()).await.unwrap();
        assert!(cleared);
        let empty = history_view(&proxy, serde_json::json!({"kind":"metric","metric":"usage"})).await;
        assert_eq!(empty["history"]["series"], serde_json::json!([]));
    });
}

#[test]
fn demo_usage_history_reaches_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await["history"].take();
        assert!(
            history["series"]
                .as_array()
                .unwrap()
                .iter()
                .any(|series| { series["id"] == "claude:session" && !series["points"].as_array().unwrap().is_empty() })
        );
    });
}
