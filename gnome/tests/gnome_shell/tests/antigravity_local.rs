//! Exercises antigravity local in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_discovers_hyphenated_server_and_ide_directory() {
    assert_antigravity_discovers_local_process(
        "language-server",
        &["--app_data_dir=/tmp/antigravity-ide", "--csrf_token=fixture-csrf"],
        "Antigravity language server",
        "Fixture Pro",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_discovers_separate_arguments_and_uppercase_app_directory() {
    assert_antigravity_discovers_local_process(
        "language_server",
        &["--app_data_dir", "/tmp/ANTIGRAVITY", "--csrf_token", "fixture-csrf"],
        "Antigravity language server",
        "Fixture Pro",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_skips_unrelated_and_damaged_proc_entries() {
    let proc = tempfile::tempdir().unwrap();
    fs::create_dir_all(proc.path().join("self")).unwrap();
    fs::create_dir_all(proc.path().join("net")).unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("TMB_FIXTURE_PROC_ROOT", proc.path());
        },
    );
    let (_server, port) = local_antigravity_fixture(
        fixture.scratch.path(),
        "language_server",
        &["--app_data_dir=/tmp/antigravity", "--csrf_token=fixture-csrf"],
    );
    fs::write(
        proc.path().join("net/tcp"),
        format!(
            "sl local_address rem_address st tx_queue tr retrnsmt uid timeout inode\n\
         0: 0100007F:{port:04X} 00000000:0000 01 0:0 00:0 0 1000 0 424242\n\
         1: 0100007F:{port:04X} 00000000:0000 0A 0:0 00:0 0 1000 0 424242\n"
        ),
    )
    .unwrap();
    fs::write(
        proc.path().join("net/tcp6"),
        format!(
            "sl local_address rem_address st tx_queue tr retrnsmt uid timeout inode\n\
         0: 0100007F:{port:04X} 00000000:0000 0A 0:0 00:0 0 1000 0 invalid\n"
        ),
    )
    .unwrap();
    fs::write(proc.path().join("note"), "not a process").unwrap();
    std::os::unix::fs::symlink("missing", proc.path().join("1")).unwrap();
    fs::create_dir(proc.path().join("2")).unwrap();
    let runtime = format!("XDG_RUNTIME_DIR={}", fixture.scratch.path().join("runtime").display());
    let write_process = |id: u32, args: &[&str], environment: Option<&str>, socket: Option<&str>| {
        let directory = proc.path().join(id.to_string());
        fs::create_dir_all(directory.join("fd")).unwrap();
        fs::write(directory.join("cmdline"), args.join("\0")).unwrap();
        if let Some(environment) = environment {
            fs::write(directory.join("environ"), environment).unwrap();
        }
        if let Some(socket) = socket {
            std::os::unix::fs::symlink(socket, directory.join("fd/3")).unwrap();
        }
        directory
    };
    let invalid = write_process(3, &["/fake/other"], Some(&runtime), None);
    fs::write(invalid.join("cmdline"), b"\xff\0").unwrap();
    write_process(4, &["/fake/other"], Some(&runtime), None);
    write_process(5, &["/fake/language_server"], Some(&runtime), None);
    write_process(
        6,
        &["/fake/language_server", "--app_data_dir=/tmp/other"],
        Some(&runtime),
        None,
    );
    write_process(
        7,
        &["/fake/language_server", "--app_data_dir=/tmp/antigravity"],
        Some(&runtime),
        None,
    );
    let valid_args = [
        "/fake/language_server",
        "--app_data_dir=/tmp/antigravity",
        "--csrf_token=fixture-csrf",
    ];
    write_process(8, &valid_args, None, None);
    write_process(9, &valid_args, Some("XDG_RUNTIME_DIR=/other"), None);
    let no_fds = write_process(10, &valid_args, Some(&runtime), None);
    fs::remove_dir(no_fds.join("fd")).unwrap();
    let bad_fds = write_process(11, &valid_args, Some(&runtime), Some("socket:[bad]"));
    fs::write(bad_fds.join("fd/4"), "not a link").unwrap();
    write_process(12, &valid_args, Some(&runtime), Some("socket:[123]"));
    write_process(13, &valid_args, Some(&runtime), Some("socket:[424242]"));
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
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["antigravity"]["status"] == "connected"
        })
        .await;
        assert_eq!(
            state["providers"]["antigravity"]["source"],
            "Antigravity language server"
        );
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 75.0);
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_discovers_local_process_without_runtime_directory() {
    let proc = tempfile::tempdir().unwrap();
    fs::create_dir_all(proc.path().join("self")).unwrap();
    fs::create_dir_all(proc.path().join("net")).unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command
                .env("TMB_FIXTURE_PROC_ROOT", proc.path())
                .env_remove("XDG_RUNTIME_DIR");
        },
    );
    let (_server, port) = local_antigravity_fixture(
        fixture.scratch.path(),
        "language_server",
        &["--app_data_dir=/tmp/antigravity", "--csrf_token=fixture-csrf"],
    );
    fs::write(
        proc.path().join("net/tcp"),
        format!(
            "sl local_address rem_address st tx_queue tr retrnsmt uid timeout inode\n\
         0: 0100007F:{port:04X} 00000000:0000 0A 0:0 00:0 0 1000 0 424242\n"
        ),
    )
    .unwrap();
    let process = proc.path().join("13");
    fs::create_dir_all(process.join("fd")).unwrap();
    fs::write(
        process.join("cmdline"),
        b"/fake/language_server\0--app_data_dir=/tmp/antigravity\0--csrf_token=fixture-csrf\0",
    )
    .unwrap();
    std::os::unix::fs::symlink("socket:[424242]", process.join("fd/3")).unwrap();
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
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["antigravity"]["status"] == "connected"
        })
        .await;
        assert_eq!(
            state["providers"]["antigravity"]["source"],
            "Antigravity language server"
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_handles_unavailable_proc_directory() {
    let proc = tempfile::tempdir().unwrap();
    let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers}),
        |_| {},
        |command| {
            command.env("TMB_FIXTURE_PROC_ROOT", proc.path().join("missing"));
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
        assert!(proxy.call::<_, _, bool>("Refresh", &("antigravity",)).await.unwrap());
        let state = wait_for_state(&proxy, |state| {
            state["providers"]["antigravity"]["notice"] == "No Antigravity sign-in found"
        })
        .await;
        assert_eq!(state["providers"]["antigravity"]["status"], "sign-in");
        assert!(
            state["providers"]["antigravity"]["windows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    });
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_discovers_same_user_cli_without_csrf() {
    assert_antigravity_discovers_local_process("antigravity-cli", &[], "Antigravity CLI", "Fixture Pro");
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_discovers_short_cli_name_without_csrf() {
    assert_antigravity_discovers_local_process("agy", &[], "Antigravity CLI", "Fixture Pro");
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_cli_quota_survives_status_failure() {
    assert_antigravity_discovers_local_process(
        "antigravity-cli",
        &["--status-unavailable"],
        "Antigravity CLI",
        "Antigravity",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_accepts_case_insensitive_local_success_code() {
    assert_antigravity_discovers_local_process(
        "language_server",
        &[
            "--app_data_dir=/tmp/antigravity",
            "--csrf_token=fixture-csrf",
            "--quota-code-ok",
        ],
        "Antigravity language server",
        "Fixture Pro",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_accepts_nested_local_summary_groups() {
    assert_antigravity_discovers_local_process(
        "language_server",
        &[
            "--app_data_dir=/tmp/antigravity",
            "--csrf_token=fixture-csrf",
            "--quota-summary-groups",
        ],
        "Antigravity language server",
        "Fixture Pro",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_accepts_top_level_local_groups() {
    assert_antigravity_discovers_local_process(
        "language_server",
        &[
            "--app_data_dir=/tmp/antigravity",
            "--csrf_token=fixture-csrf",
            "--quota-root-groups",
        ],
        "Antigravity language server",
        "Fixture Pro",
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_ignores_language_server_for_another_app() {
    assert_antigravity_ignores_local_process("/tmp/not-antigravity", true, "fixture-csrf", &[], false);
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_ignores_language_server_from_another_runtime() {
    assert_antigravity_ignores_local_process("/tmp/antigravity", false, "fixture-csrf", &[], false);
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_rejects_invalid_local_csrf() {
    assert_antigravity_ignores_local_process("/tmp/antigravity", true, "invalid-csrf", &[], true);
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_rejects_nonzero_local_quota_code() {
    assert_antigravity_ignores_local_process("/tmp/antigravity", true, "fixture-csrf", &["--quota-code-number"], true);
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_rejects_failed_local_quota_code() {
    assert_antigravity_ignores_local_process("/tmp/antigravity", true, "fixture-csrf", &["--quota-code-string"], true);
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_rejects_local_quota_without_groups() {
    assert_antigravity_ignores_local_process("/tmp/antigravity", true, "fixture-csrf", &["--quota-empty"], true);
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_rejects_malformed_local_quota_json() {
    assert_antigravity_ignores_local_process(
        "/tmp/antigravity",
        true,
        "fixture-csrf",
        &["--quota-invalid-json"],
        true,
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn antigravity_cli_file_reaches_cloud_quota_without_a_local_server() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let server = MockServer::start().await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:retrieveUserQuotaSummary"))
            .and(matchers::header("authorization", "Bearer cli-file-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "response":{"groups":[{"buckets":[{"bucketId":"gemini-5h","remainingFraction":0.3}]}]}
            })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(matchers::method("POST"))
            .and(matchers::path("/antigravity-primary/v1internal:loadCodeAssist"))
            .and(matchers::header("authorization", "Bearer cli-file-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "currentTier":{"id":"pro-tier","name":"Google AI Pro"}
            })))
            .expect(1)
            .mount(&server)
            .await;
        let providers = serde_json::json!({"claude":false,"codex":false,"gemini":false,"antigravity":true,"cursor":false,"copilot":false});
        let fixture = isolated_service_configured(
            &serde_json::json!({"providers":providers}),
            |root| {
                let credential = root.join("home/.gemini/antigravity-cli/antigravity-oauth-token");
                fs::create_dir_all(credential.parent().unwrap()).unwrap();
                fs::write(credential, r#"{"auth_method":"consumer","token":{"access_token":"cli-file-token"}}"#)
                    .unwrap();
            },
            |command| {
                command
                    .env("TMB_FIXTURE_API_URL", server.uri())
                    .env_remove("ANTIGRAVITY_AUTH_FILE");
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
        let state = wait_for_state(&proxy, |state| state["providers"]["antigravity"]["status"] == "connected").await;
        assert_eq!(state["providers"]["antigravity"]["windows"][0]["used"], 70.0);
        assert_eq!(state["providers"]["antigravity"]["plan"], "Google AI Pro");
        assert!(
            state["providers"]["antigravity"]["source"]
                .as_str()
                .unwrap()
                .ends_with("/.gemini/antigravity-cli/antigravity-oauth-token")
        );
    });
}
