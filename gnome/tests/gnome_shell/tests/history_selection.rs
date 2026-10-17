//! Exercises history selection in an isolated GNOME session.

use super::super::*;

#[test]
fn history_view_rejects_unknown_metric_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let action = serde_json::json!({"kind":"metric","metric":"unknown"}).to_string();
        let result: zbus::Result<String> = proxy.call("GetHistoryView", &(action, "UTC", 0_i32)).await;
        assert!(result.unwrap_err().to_string().contains("Unknown metric"));
    });
}

#[test]
fn metric_export_rejects_unknown_metric_over_dbus() {
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let path = fixture.scratch.path().join("unknown.csv");
        let result: zbus::Result<bool> = proxy
            .call(
                "ExportMetricHistory",
                &("unknown", 0_i64, 1_i64, path.to_str().unwrap()),
            )
            .await;
        assert!(result.unwrap_err().to_string().contains("Unknown metric"));
        assert!(!path.exists());
    });
}

#[test]
fn local_metric_export_creates_an_owner_only_csv() {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service(&serde_json::json!({"providers":providers}));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let path = fixture.scratch.path().join("turns.csv");
        let now = chrono::Utc::now().timestamp();
        let exported: bool = proxy
            .call(
                "ExportMetricHistory",
                &("turns", now - 86_400, now, path.to_str().unwrap()),
            )
            .await
            .unwrap();
        assert!(exported);
        assert_eq!(fs::read_to_string(&path).unwrap(), "provider,day,metric,series,value\n");
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    });
}

#[test]
fn full_history_export_streams_samples_and_analytics_with_private_projects() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":false,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id)); CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
            db.execute(
                "INSERT INTO samples VALUES(?1,'claude','session','Current session',42.5,NULL)",
                [now.timestamp()],
            )
            .unwrap();
            db.execute(
                "INSERT INTO daily_analytics VALUES(?1,'claude','projectCost','/work/secret-project',1.25)",
                [now.format("%Y-%m-%d").to_string()],
            )
            .unwrap();
            db.execute(
                "INSERT INTO daily_analytics VALUES(?1,'claude','projectMessages','/work/secret-project',3)",
                [now.format("%Y-%m-%d").to_string()],
            )
            .unwrap();
            db.execute(
                "INSERT INTO daily_analytics VALUES(?1,'claude','messages','total',3)",
                [now.format("%Y-%m-%d").to_string()],
            )
            .unwrap();
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let public = fixture.scratch.path().join("public.csv");
        let exported: bool = proxy
            .call("ExportAllHistory", &(public.to_str().unwrap(),))
            .await
            .unwrap();
        assert!(exported);
        let csv = fs::read_to_string(&public).unwrap();
        assert!(csv.starts_with("kind,timestamp,key,label,used_percent,resets_at,provider,day,metric,series,value\n"));
        assert!(csv.contains("\nsample,"));
        assert!(csv.contains(",claude:session,"));
        assert!(csv.contains(",42.50,"));
        assert!(csv.contains("\nanalytics,"));
        assert!(csv.contains(",/work/secret-project,"));
        assert_eq!(fs::metadata(&public).unwrap().permissions().mode() & 0o777, 0o600);
        let _: String = proxy.call("SetSetting", &("hide_identity", "true")).await.unwrap();
        let private = fixture.scratch.path().join("private.csv");
        fs::write(&private, "old export").unwrap();
        fs::set_permissions(&private, fs::Permissions::from_mode(0o644)).unwrap();
        let exported: bool = proxy
            .call("ExportAllHistory", &(private.to_str().unwrap(),))
            .await
            .unwrap();
        assert!(exported);
        let csv = fs::read_to_string(&private).unwrap();
        assert_eq!(csv.matches(",Project 1f57fcdf0259,").count(), 2);
        assert!(csv.contains(",messages,total,3.0000"));
        assert!(!csv.contains("/work/secret-project"));
        assert!(!csv.contains("old export"));
        assert_eq!(fs::metadata(&private).unwrap().permissions().mode() & 0o777, 0o600);
        let error: zbus::Result<bool> = proxy
            .call(
                "ExportAllHistory",
                &(fixture.scratch.path().join("missing/history.csv").to_str().unwrap(),),
            )
            .await;
        assert!(
            error
                .unwrap_err()
                .to_string()
                .contains("Export directory does not exist")
        );
        let error: zbus::Result<bool> = proxy
            .call("ExportAllHistory", &(fixture.scratch.path().to_str().unwrap(),))
            .await;
        assert!(error.unwrap_err().to_string().contains("Is a directory"));
    });
}

#[test]
fn disabled_provider_history_is_hidden_from_charts_and_csv() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":false,"codex":true,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let path = root.join("data/quotaline/history.sqlite3");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let db = rusqlite::Connection::open(path).unwrap();
            db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id)); CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
            for (provider, used) in [("claude", 25.0), ("codex", 50.0)] {
                db.execute(
                    "INSERT INTO samples VALUES(?1,?2,'session','Session',?3,NULL)",
                    rusqlite::params![now.timestamp(), provider, used],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO daily_analytics VALUES(?1,?2,'costUSD','total',?3)",
                    rusqlite::params![now.format("%Y-%m-%d").to_string(), provider, used],
                )
                .unwrap();
            }
        },
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let start = now.timestamp() - 86_400;
        let end = now.timestamp() + 86_400;
        let quota = history_view(&proxy, serde_json::json!({"kind":"load"})).await["history"].take();
        assert_eq!(quota["series"].as_array().unwrap().len(), 1);
        assert_eq!(quota["series"][0]["provider"], "codex");
        let analytics =
            history_view(&proxy, serde_json::json!({"kind":"metric","metric":"costUSD"})).await["history"].take();
        assert_eq!(analytics["series"].as_array().unwrap().len(), 1);
        assert_eq!(analytics["series"][0]["provider"], "codex");
        let path = fixture.scratch.path().join("visible.csv");
        let exported: bool = proxy
            .call("ExportMetricHistory", &("usage", start, end, path.to_str().unwrap()))
            .await
            .unwrap();
        assert!(exported);
        let csv = fs::read_to_string(&path).unwrap();
        assert!(csv.contains(",codex:session,"));
        assert!(!csv.contains("claude"));
        let path = fixture.scratch.path().join("visible-analytics.csv");
        let exported: bool = proxy
            .call("ExportMetricHistory", &("costUSD", start, end, path.to_str().unwrap()))
            .await
            .unwrap();
        assert!(exported);
        let csv = fs::read_to_string(&path).unwrap();
        assert!(csv.contains("\ncodex,"));
        assert!(!csv.contains("claude"));
    });
}

#[test]
fn selected_models_scope_quota_history_and_csv_but_not_analytics() {
    let fixture = isolated_service(&serde_json::json!({
        "demo": true,
        "selected_windows": ["claude:session"]
    }));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let end = chrono::Utc::now().timestamp() + 60;
        let start = end - 7 * 86_400;
        for action in [
            serde_json::json!({"kind":"period","days":7}),
            serde_json::json!({"kind":"rollup","rollup":"hour"}),
        ] {
            let history = history_view(&proxy, action).await["history"].take();
            assert_eq!(history["series"].as_array().unwrap().len(), 1);
            assert_eq!(history["series"][0]["id"], "claude:session");
        }
        let path = fixture.scratch.path().join("selected-history.csv");
        let exported: bool = proxy
            .call("ExportMetricHistory", &("usage", start, end, path.to_str().unwrap()))
            .await
            .unwrap();
        assert!(exported);
        let csv = fs::read_to_string(path).unwrap();
        assert!(csv.contains("claude:session"));
        assert!(!csv.contains("claude:weekly"));
        let analytics =
            history_view(&proxy, serde_json::json!({"kind":"metric","metric":"costUSD"})).await["history"].take();
        assert!(!analytics["series"].as_array().unwrap().is_empty());
    });
}

#[test]
fn empty_custom_model_selection_hides_quota_history() {
    let fixture = isolated_service(&serde_json::json!({
        "demo": true,
        "selected_windows": [],
        "has_custom_selection": true
    }));
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let history = history_view(&proxy, serde_json::json!({"kind":"load"})).await;
        assert_eq!(history["history"]["series"], serde_json::json!([]));
    });
}

#[test]
fn default_model_selection_filters_retained_quota_history() {
    let now = chrono::Utc::now().timestamp();
    let fixture = isolated_service_with(&serde_json::json!({}), |root| {
        let cache = root.join("cache/quotaline");
        let data = root.join("data/quotaline");
        fs::create_dir_all(&cache).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::write(
            cache.join("snapshot.json"),
            serde_json::json!({"claude": {
                "id":"claude", "name":"Claude", "status":"connected", "plan":"Pro",
                "identity":"", "source":"fixture", "notice":"", "updated":now,
                "windows":[
                    {"id":"session", "label":"Session", "used":25, "reset":null, "group":"session"},
                    {"id":"weekly", "label":"Weekly", "used":40, "reset":null, "group":"weekly"},
                    {"id":"monthly", "label":"Monthly", "used":60, "reset":null, "group":"monthly"}
                ]
            }})
            .to_string(),
        )
        .unwrap();
        let db = rusqlite::Connection::open(data.join("history.sqlite3")).unwrap();
        db.execute_batch("CREATE TABLE samples(ts INTEGER NOT NULL, provider TEXT NOT NULL, window_id TEXT NOT NULL, label TEXT NOT NULL, used REAL NOT NULL, reset TEXT, PRIMARY KEY(ts,provider,window_id)); CREATE TABLE daily_analytics(day TEXT NOT NULL, provider TEXT NOT NULL, metric TEXT NOT NULL, series TEXT NOT NULL, value REAL NOT NULL, PRIMARY KEY(day,provider,metric,series))").unwrap();
        for key in ["session", "weekly", "monthly"] {
            db.execute(
                "INSERT INTO samples VALUES(?1,'claude',?2,?2,50,NULL)",
                rusqlite::params![now, key],
            )
            .unwrap();
        }
    });
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
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
        let ids = history["series"]
            .as_array()
            .unwrap()
            .iter()
            .map(|series| series["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(ids, ["claude:session", "claude:weekly"]);
    });
}
