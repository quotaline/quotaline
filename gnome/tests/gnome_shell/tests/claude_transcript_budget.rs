//! Exercises claude transcript budget in an isolated GNOME session.

use super::super::*;

#[test]
fn claude_transcript_scan_resumes_after_its_byte_budget() {
    let now = chrono::Utc::now();
    let fixture = isolated_service_with(
        &serde_json::json!({"providers":{"claude":true,"codex":false,"gemini":false,
            "antigravity":false,"cursor":false,"copilot":false}}),
        |root| {
            let folder = root.join("home/.claude/projects/app");
            fs::create_dir_all(&folder).unwrap();
            fs::write(folder.join("a.jsonl"), format!("{}\n", "x".repeat(1_048_577)).repeat(9)).unwrap();
            fs::write(
                folder.join("b.jsonl"),
                format!(
                    "{}\n",
                    serde_json::json!({
                        "type":"assistant","timestamp":now.to_rfc3339(),
                        "message":{"id":"after-budget","model":"claude-sonnet-5-5","usage":{"input_tokens":10}}
                    }),
                ),
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
        let view = wait_for_reply(
            &proxy,
            "GetHistoryView",
            &(
                serde_json::json!({"kind":"metric","metric":"messages"}).to_string(),
                "UTC",
                0_i32,
            ),
            |view| {
                view["history"]["series"]
                    .as_array()
                    .is_some_and(|series| !series.is_empty())
            },
        )
        .await;
        let points = view["history"]["series"][0]["points"].as_array().unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0][1], 1.0);
    });
}
