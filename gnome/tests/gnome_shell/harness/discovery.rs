//! Provides isolated Shell test fixtures for discovery.

use super::*;

pub(crate) fn assert_antigravity_ignores_local_process(
    app_directory: &str,
    same_runtime: bool,
    csrf_token: &str,
    extra_args: &[&str],
    expect_request: bool,
) {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
    let fixture = isolated_service(&serde_json::json!({"providers":providers}));
    let separate = tempfile::tempdir().unwrap();
    let server_root = if same_runtime {
        fixture.scratch.path()
    } else {
        separate.path()
    };
    let app_argument = format!("--app_data_dir={app_directory}");
    let csrf_argument = format!("--csrf_token={csrf_token}");
    let mut args = vec![app_argument.as_str(), csrf_argument.as_str()];
    args.extend_from_slice(extra_args);
    let (mut server, port) = local_antigravity_fixture(server_root, "language_server", &args);
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
        assert!(proxy.call::<_, _, bool>("Refresh", &("antigravity",)).await.unwrap());
        let state = wait_for_state(&proxy, |state| state["providers"]["antigravity"]["status"] == "sign-in").await;
        assert_eq!(state["providers"]["antigravity"]["source"], "");
        if expect_request {
            let requests = fs::read_to_string(server_root.join("antigravity-requests.log")).unwrap();
            assert!(requests.contains("/RetrieveUserQuotaSummary"));
        }
    });
    let mut shutdown = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    shutdown.write_all(b"GET /Shutdown HTTP/1.1\r\n\r\n").unwrap();
    wait_for_exit(&mut server.0, Duration::from_secs(3)).expect("local fixture did not exit after shutdown");
}

#[cfg(feature = "fixture-api")]
pub(crate) fn local_antigravity_fixture(scratch: &Path, program: &str, args: &[&str]) -> (Process, u16) {
    let executable = scratch.join(program);
    std::os::unix::fs::symlink(env!("CARGO_BIN_EXE_antigravity_fixture"), &executable).unwrap();
    let mut server = Process(
        Command::new(&executable)
            .args(args)
            .env("XDG_RUNTIME_DIR", scratch.join("runtime"))
            .env("TMB_FIXTURE_REQUEST_LOG", scratch.join("antigravity-requests.log"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut port = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut port)
        .unwrap();
    (server, port.trim().parse().unwrap())
}

#[cfg(feature = "fixture-api")]
pub(crate) fn assert_antigravity_discovers_local_process(
    program: &str,
    args: &[&str],
    expected_source: &str,
    expected_plan: &str,
) {
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
    let fixture = isolated_service(&serde_json::json!({"providers":providers}));
    let (mut server, port) = local_antigravity_fixture(fixture.scratch.path(), program, args);
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
        let accepted: bool = proxy.call("Refresh", &("antigravity",)).await.unwrap();
        assert!(accepted);
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["antigravity"]["status"] == "connected"
        })
        .await;
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 75.0);
        assert_eq!(state["providers"]["antigravity"]["source"], expected_source);
        assert_eq!(state["providers"]["antigravity"]["plan"], expected_plan);
        let mut shutdown = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        shutdown.write_all(b"GET /Shutdown HTTP/1.1\r\n\r\n").unwrap();
        wait_for_exit(&mut server.0, Duration::from_secs(3)).expect("local fixture did not exit after shutdown");
    });
}

#[cfg(feature = "fixture-api")]
#[derive(Clone, Copy)]
pub(crate) enum GeminiKeyringFixture {
    Missing,
    Empty,
    Conflicting,
}

#[cfg(feature = "fixture-api")]
pub(crate) fn gemini_file_source_state(encrypted: bool, keyring: GeminiKeyringFixture) -> serde_json::Value {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer gemini-file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"standard-tier"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .and(matchers::header("authorization", "Bearer gemini-file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.7}]
            })))
            .expect(1)
            .mount(&server)
            .await;
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{"claude":false,"codex":false,"gemini":false,
                "antigravity":false,"cursor":false,"copilot":false}}),
            |root| {
                let path = root.join("home/.gemini/oauth_creds.json");
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, r#"{"access_token":"gemini-file-fixture"}"#).unwrap();
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri());
                if encrypted {
                    command.env("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "true");
                }
            },
        );
        let _keyring = match keyring {
            GeminiKeyringFixture::Missing => None,
            GeminiKeyringFixture::Empty | GeminiKeyringFixture::Conflicting => Some(start_keyring(&fixture).await),
        };
        if matches!(keyring, GeminiKeyringFixture::Conflicting) {
            store_keyring_item(
                &fixture,
                "Unused Gemini account",
                &[("service", "gemini-cli-oauth"), ("username", "main-account")],
                br#"{"access_token":"another-account"}"#,
            );
        }
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
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,
            "antigravity":false,"cursor":false,"copilot":false});
        let _: String = proxy
            .call("SetSetting", &("providers", providers.to_string()))
            .await
            .unwrap();
        let state = wait_for_state(&proxy, |state| state["providers"]["gemini"]["status"] == "connected").await;
        server.verify().await;
        state
    })
}

#[cfg(feature = "fixture-api")]
pub(crate) fn gemini_keyring_failure_state(file: Option<&str>, connected: bool, locked: bool) -> serde_json::Value {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        let requests = u64::from(connected);
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer gemini-file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"standard-tier"}
            })))
            .expect(requests)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/v1internal:retrieveUserQuota"))
            .and(matchers::header("authorization", "Bearer gemini-file-fixture"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "buckets":[{"modelId":"gemini-2.5-pro","remainingFraction":0.7}]
            })))
            .expect(requests)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":true,"antigravity":false,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":{
                "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
            },"notifications":false}),
            |root| {
                if let Some(file) = file {
                    let credentials = root.join("home/.gemini/oauth_creds.json");
                    fs::create_dir_all(credentials.parent().unwrap()).unwrap();
                    fs::write(credentials, file).unwrap();
                }
            },
            |command| {
                command.env("TMB_FIXTURE_API_URL", server.uri())
                    .env("GEMINI_FORCE_ENCRYPTED_FILE_STORAGE", "true");
            },
        );
        let _keyring = start_keyring(&fixture).await;
        store_keyring_item(
            &fixture,
            "Gemini invalid fixture",
            &[("service", "gemini-cli-oauth"), ("username", "main-account")],
            if locked { br#"{"access_token":"locked"}"# } else { b"not JSON" },
        );
        let connection = zbus::connection::Builder::address(fixture.address.as_str())
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
        if locked {
            let secrets = secret_service::SecretService::connect_with_existing(
                secret_service::EncryptionType::Dh,
                connection.clone(),
            )
            .await
            .unwrap();
            let collection = secrets.get_default_collection().await.unwrap();
            collection.lock().await.unwrap();
            assert!(collection.is_locked().await.unwrap());
        }
        let proxy = zbus::Proxy::new(
            &connection,
            "tech.bernat.Quotaline.Gnome",
            "/tech/bernat/Quotaline/Gnome",
            "tech.bernat.Quotaline.Gnome",
        )
        .await
        .unwrap();
        let _: String = proxy.call("SetSetting", &("providers", providers.to_string().as_str())).await.unwrap();
        let state = provider_with_status(&fixture, "gemini", if connected { "connected" } else { "unreadable" }).await;
        server.verify().await;
        state
    })
}
