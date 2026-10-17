//! Exercises gemini cli and usage json in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn installed_oauth_clients_work_without_complete_environment_credentials() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token":"fresh","expires_in":0
            })))
            .expect(6)
            .mount(&server)
            .await;
        for (id, secret) in [(None, None), (Some(""), Some("partial")), (Some("partial"), Some(""))] {
        let fixture = isolated_service_configured(
            &serde_json::json!({
                "providers":{"claude":false,"codex":false,"gemini":true,"antigravity":true,"cursor":false,"copilot":false},
                "allow_token_refresh":true
            }),
            |root| {
                let home = root.join("home");
                let gemini = home.join(".gemini/oauth_creds.json");
                fs::create_dir_all(gemini.parent().unwrap()).unwrap();
                fs::write(gemini, r#"{"access_token":"old","refresh_token":"shared","expiry_date":1}"#).unwrap();
                let gemini_client = home.join(".npm-global/lib/node_modules/@google/gemini-cli-core/dist/src/code_assist/oauth2.js");
                fs::create_dir_all(gemini_client.parent().unwrap()).unwrap();
                fs::write(gemini_client, "const OAUTH_CLIENT_ID = 'gemini-id'; const OAUTH_CLIENT_SECRET = 'gemini-secret';").unwrap();
                let antigravity = root.join("config/Antigravity/auth.json");
                fs::create_dir_all(antigravity.parent().unwrap()).unwrap();
                fs::write(antigravity, r#"{"token":{"access_token":"old","refresh_token":"shared","expiry":1}}"#).unwrap();
                let antigravity_client = home.join(".local/bin/agy");
                fs::create_dir_all(antigravity_client.parent().unwrap()).unwrap();
                fs::write(antigravity_client, b"\x0012345-fixture.apps.googleusercontent.com\0GOCSPX-AAAAAAAAAA-BBBBBBBBBB_CCCCCC\0").unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri())
                    .env_remove("PATH")
                    .env_remove("GEMINI_OAUTH_CLIENT_ID")
                    .env_remove("GEMINI_OAUTH_CLIENT_SECRET")
                    .env_remove("ANTIGRAVITY_OAUTH_CLIENT_ID")
                    .env_remove("ANTIGRAVITY_OAUTH_CLIENT_SECRET");
                if let (Some(id), Some(secret)) = (id, secret) {
                    command.env("GEMINI_OAUTH_CLIENT_ID", id)
                        .env("GEMINI_OAUTH_CLIENT_SECRET", secret)
                        .env("ANTIGRAVITY_OAUTH_CLIENT_ID", id)
                        .env("ANTIGRAVITY_OAUTH_CLIENT_SECRET", secret);
                }
            },
        );
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
        let _ = wait_for_state(&proxy, |state| {
            state["providers"]["gemini"]["notice"] == "Gemini refresh returned an invalid expiry"
        })
        .await;
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["antigravity"]["notice"] == "Antigravity refresh returned an invalid expiry"
        })
        .await;
        assert_eq!(state["providers"]["gemini"]["status"], "unreadable");
        assert_eq!(state["providers"]["antigravity"]["status"], "unreadable");
        }
        server.verify().await;
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_expired_token_without_an_installed_oauth_client_reports_unreadable_credentials() {
    let fixture = isolated_service_configured(
        &serde_json::json!({
            "providers":{"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false},
            "allow_token_refresh":true
        }),
        |root| {
            let path = root.join("home/.gemini/oauth_creds.json");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                r#"{"access_token":"expired","refresh_token":"shared","expiry_date":1}"#,
            )
            .unwrap();
        },
        |command| {
            command
                .env_remove("PATH")
                .env_remove("NPM_CONFIG_PREFIX")
                .env_remove("GEMINI_OAUTH_CLIENT_ID")
                .env_remove("GEMINI_OAUTH_CLIENT_SECRET");
        },
    );
    let state =
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(provider_with_status(&fixture, "gemini", "unreadable"));
    assert_eq!(
        state["providers"]["gemini"]["notice"],
        "Cannot find the installed Gemini CLI OAuth client"
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_invalid_keyring_falls_back_to_cli_file() {
    let state = gemini_keyring_failure_state(Some(r#"{"access_token":"gemini-file-fixture"}"#), true, false);
    let used = state["providers"]["gemini"]["windows"][0]["used"].as_f64().unwrap();
    assert!((used - 30.0).abs() < 0.000_001);
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_invalid_keyring_falls_back_to_nested_cli_token() {
    let state = gemini_keyring_failure_state(Some(r#"{"token":{"accessToken":"gemini-file-fixture"}}"#), true, false);
    assert!(
        state["providers"]["gemini"]["source"]
            .as_str()
            .unwrap()
            .ends_with("/.gemini/oauth_creds.json")
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_invalid_keyring_without_file_reports_unreadable_credentials() {
    let state = gemini_keyring_failure_state(None, false, false);
    assert_eq!(
        state["providers"]["gemini"]["notice"],
        "Gemini keyring item is not JSON"
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_invalid_keyring_and_file_reports_keyring_error() {
    let state = gemini_keyring_failure_state(Some("{"), false, false);
    assert_eq!(
        state["providers"]["gemini"]["notice"],
        "Gemini keyring item is not JSON"
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn gemini_locked_keyring_without_file_reports_unreadable_credentials() {
    let state = gemini_keyring_failure_state(None, false, true);
    assert_eq!(state["providers"]["gemini"]["notice"], "Gemini keyring is locked");
}

#[test]
fn usage_json_cli_reads_only_the_cache() {
    let scratch = tempfile::tempdir().unwrap();
    let executable = env!("CARGO_BIN_EXE_quotaline-gnome");
    let invoke = || {
        Command::new(executable)
            .arg("--usage-json")
            .env("XDG_CACHE_HOME", scratch.path())
            .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
            .output()
            .unwrap()
    };
    let missing = invoke();
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("no cached usage yet"));
    let cache = scratch.path().join("quotaline");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("snapshot.json"), r#"{"codex":{"id":"codex","name":"Codex","status":"connected","plan":"Plus","identity":"private@example.com","source":"/private/auth.json","windows":[{"id":"session","label":"5 hours","used":21,"reset":"2026-10-10T12:00:00Z","group":"session"}],"notice":"","updated":1780000000}}"#).unwrap();
    let output = invoke();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["providers"][0]["windows"][0]["usedPercent"], 21.0);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private@example.com"));
}

#[test]
fn usage_json_cli_finds_home_cache_without_xdg_variables() {
    let scratch = tempfile::tempdir().unwrap();
    let cache = scratch.path().join(".cache/quotaline");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("snapshot.json"), r#"{"codex":{"id":"codex","name":"Codex","status":"connected","plan":"Plus","identity":"private@example.com","source":"/private/auth.json","windows":[{"id":"session","label":"5 hours","used":21,"reset":"2026-10-10T12:00:00Z","group":"session"}],"notice":"","updated":1780000000}}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_quotaline-gnome"))
        .arg("--usage-json")
        .env("HOME", scratch.path())
        .env_remove("XDG_CACHE_HOME")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["providers"][0]["windows"][0]["usedPercent"], 21.0);
}

#[test]
fn usage_json_cli_without_home_reads_working_directory_cache() {
    let scratch = tempfile::tempdir().unwrap();
    let cache = scratch.path().join(".cache/quotaline");
    fs::create_dir_all(&cache).unwrap();
    fs::write(cache.join("snapshot.json"), r#"{"codex":{"id":"codex","name":"Codex","status":"connected","plan":"Plus","identity":"","source":"","windows":[{"id":"session","label":"5 hours","used":21,"reset":null,"group":"session"}],"notice":"","updated":1780000000}}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_quotaline-gnome"))
        .arg("--usage-json")
        .current_dir(scratch.path())
        .env_remove("HOME")
        .env_remove("XDG_CACHE_HOME")
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent")
        .env("DBUS_SYSTEM_BUS_ADDRESS", "unix:path=/nonexistent")
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["providers"][0]["windows"][0]["usedPercent"], 21.0);
}
