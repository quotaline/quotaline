//! Provides isolated Shell test fixtures for history.

use super::*;

pub(crate) fn run_shell_cases(fixture: &IsolatedService, cases: &[(&str, &[(&str, &str)])]) {
    run_shell_cases_with_reset(fixture, cases, -1, Some("CC 5h"), &[]);
}

#[cfg(feature = "fixture-api")]
pub(crate) fn run_shell_cases_with_reset(
    fixture: &IsolatedService,
    cases: &[(&str, &[(&str, &str)])],
    reset_before: isize,
    ready_text: Option<&str>,
    shell_env: &[(&str, &str)],
) {
    let cases = cases
        .iter()
        .map(|(script, variables)| {
            serde_json::json!({
                "script": script,
                "variables": variables.iter().copied().collect::<std::collections::BTreeMap<_, _>>()
            })
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_string(&cases).unwrap();
    let reset_before = reset_before.to_string();
    let mut variables = vec![
        ("TMB_SHELL_CASES", encoded.as_str()),
        ("TMB_SHELL_RESET_BEFORE", reset_before.as_str()),
    ];
    if let Some(ready_text) = ready_text {
        variables.push(("TMB_SHELL_READY_TEXT", ready_text));
    }
    variables.extend_from_slice(shell_env);
    run_shell_script(fixture, "extension/tests/integration/shell-lane.js", &variables);
}

#[cfg(feature = "fixture-api")]
pub(crate) async fn sign_in_response(fixture: &IsolatedService, provider: &str) -> bool {
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
    proxy.call("StartSignIn", &(provider,)).await.unwrap()
}

#[cfg(feature = "fixture-api")]
pub(crate) fn project_history_shell(private: bool, projects: &[(&str, f64)], expected: &[&str], capture: &str) {
    let _shell = shell_lock();
    let providers = serde_json::json!({"claude":true,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false});
    let fixture = isolated_service_configured(
        &serde_json::json!({"providers":providers,"history_metric":"projectCost","last_tab":"History","hide_identity":private}),
        |root| {
            let path = root.join("home/.claude/projects/demo/session.jsonl");
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            let records = projects
                .iter()
                .enumerate()
                .map(|(index, (project, value))| {
                    serde_json::json!({"type":"assistant","timestamp":chrono::Utc::now().to_rfc3339(),
                        "cwd":project,"sessionId":"session","costUSD":value,
                        "message":{"id":format!("message-{index}"),"model":"claude-sonnet-5-5",
                            "usage":{"input_tokens":1,"output_tokens":1}}})
                    .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(path, format!("{records}\n")).unwrap();
        },
        |_| {},
    );
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
        wait_for_reply(
            &proxy,
            "GetHistoryView",
            &(serde_json::json!({"kind":"load"}).to_string(), "UTC", 0_i32),
            |view| {
                view["history"]["series"]
                    .as_array()
                    .is_some_and(|series| series.len() == projects.len())
            },
        )
        .await;
    });
    let paths = projects.iter().map(|(path, _)| *path).collect::<Vec<_>>().join("|");
    let labels = expected.join("|");
    run_shell_script(
        &fixture,
        "extension/tests/integration/history-project-label.test.js",
        &[
            ("TMB_PROJECT_PRIVATE", if private { "1" } else { "0" }),
            ("TMB_PROJECT_PATHS", &paths),
            ("TMB_PROJECT_LABELS", &labels),
            ("TMB_PROJECT_CAPTURE", capture),
        ],
    );
}

#[cfg(feature = "fixture-api")]
pub(crate) fn cloud_credit_shell_case(case: &str, credits: serde_json::Value, status: &str) {
    let _shell = shell_lock();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":true},"last_tab":"Usage","show_all_providers":true}),
        move |root| {
            let cache = root.join("cache/quotaline");
            fs::create_dir_all(&cache).unwrap();
            fs::write(
                cache.join("snapshot.json"),
                serde_json::json!({"claude":{
                    "id":"claude","name":"Claude","status":status,"plan":"Max 20x",
                    "identity":"","source":"Local","notice":"","updated":chrono::Utc::now().timestamp(),
                    "windows":[{"id":"weekly","label":"All models","used":25,"reset":null,"group":"weekly"}],
                    "cloud_credits":credits
                }})
                .to_string(),
            )
            .unwrap();
        },
    );
    run_shell_script(
        &fixture,
        "extension/tests/integration/claude-cloud-credit.test.js",
        &[("TMB_CLOUD_CASE", case)],
    );
}
