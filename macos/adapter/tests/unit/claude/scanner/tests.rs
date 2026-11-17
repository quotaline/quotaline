use super::*;
use std::io::Write;

#[test]
fn bridge_reads_transcripts_and_reports_workload() {
    let directory = tempfile::tempdir().unwrap();
    let line = serde_json::json!({
        "type":"assistant",
        "timestamp":"2026-10-10T11:00:00Z",
        "message":{"id":"one","model":"claude-sonnet-4-6","usage":{"input_tokens":2}}
    });
    std::fs::write(directory.path().join("session.jsonl"), format!("{line}\n")).unwrap();
    let config = claude_scanner_defaults();
    assert_eq!(config.maximum_indexed_files, 64);
    let scanner = ClaudeTranscriptScanner::new(directory.path().display().to_string(), None, config);
    let now = "2026-10-10T12:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second() as f64;
    let snapshot = scanner.refresh(now, 60, false);
    assert_eq!(snapshot.view.message_count(), 1);
    let analytics = snapshot.view.analytics_snapshot(now).unwrap();
    assert_eq!(analytics.provider, "claude");
    assert_eq!(analytics.covered_scopes.len(), 1);
    assert_eq!(snapshot.view.local_usage(None, now, 0.0, now).unwrap().today_tokens, 2);
    assert!(snapshot.pricing_warnings.is_empty());
    assert_eq!(scanner.workload().files_opened, 1);
    assert!(!scanner.pending());
    scanner.run_background_slice();
    scanner.cancel();
    assert!(!scanner.pending());
}

#[test]
fn bridge_bounds_invalid_time_budget() {
    let directory = tempfile::tempdir().unwrap();
    let mut config = claude_scanner_defaults();
    config.time_budget_seconds = f64::NAN;
    let scanner = ClaudeTranscriptScanner::new(directory.path().display().to_string(), None, config);
    let snapshot = scanner.refresh(0.0, 60, false);
    assert!(snapshot.view.analytics_snapshot(0.0).is_none());
    assert!(snapshot.view.local_usage(None, 0.0, 0.0, 0.0).is_none());
}

#[test]
fn bridge_reports_unpriced_model_warnings() {
    let directory = tempfile::tempdir().unwrap();
    let line = serde_json::json!({
        "type":"assistant",
        "timestamp":"2026-10-10T11:00:00Z",
        "message":{"id":"one","model":"claude-unknown-7","usage":{"input_tokens":2}}
    });
    std::fs::write(directory.path().join("session.jsonl"), format!("{line}\n")).unwrap();
    let scanner = ClaudeTranscriptScanner::new(directory.path().display().to_string(), None, claude_scanner_defaults());
    let now = "2026-10-10T12:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second() as f64;
    let snapshot = scanner.refresh(now, 60, false);
    assert_eq!(
        snapshot.pricing_warnings,
        ["Cost for claude-unknown-7 is estimated; the pricing table has no entry"]
    );
}

#[test]
fn bridge_snapshots_remain_immutable_after_later_reads() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("session.jsonl");
    let line = |id| {
        format!(
            "{}\n",
            serde_json::json!({
                "type":"assistant","timestamp":"2026-10-10T11:00:00Z",
                "message":{"id":id,"model":"claude-sonnet-4-6","usage":{"input_tokens":2}}
            })
        )
    };
    std::fs::write(&path, line("one")).unwrap();
    let scanner = ClaudeTranscriptScanner::new(directory.path().display().to_string(), None, claude_scanner_defaults());
    let now = "2026-10-10T12:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second() as f64;
    let before = scanner.refresh(now, 60, false);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(line("two").as_bytes())
        .unwrap();
    let after = scanner.refresh(now + 1.0, 60, false);
    assert_eq!(before.view.message_count(), 1);
    assert_eq!(after.view.message_count(), 2);
}
