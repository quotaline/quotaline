//! Exercises transcript refresh through the reader's public API.

use super::{Config, Reader};
use std::{fs, io::Write, path::Path, time::Duration};

#[path = "tests/checkpoint.rs"]
mod checkpoint;
#[path = "tests/scan.rs"]
mod scan;
#[path = "tests/tails.rs"]
mod tails;

fn now() -> f64 {
    "2026-10-10T12:00:00Z".parse::<jiff::Timestamp>().unwrap().as_second() as f64
}

fn line(id: &str) -> String {
    line_with_model(id, "claude-sonnet-4-6")
}

fn line_with_model(id: &str, model: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({
            "type":"assistant",
            "timestamp":"2026-10-10T11:00:00Z",
            "message":{"id":id,"model":model,"usage":{"input_tokens":2,"output_tokens":3}}
        })
    )
}

fn reader(root: &Path, state: Option<&Path>, config: Config) -> Reader {
    Reader::new(root.to_path_buf(), state.map(Path::to_path_buf), config)
}

fn finish(reader: &mut Reader) {
    for _ in 0..500 {
        if !reader.pending() {
            break;
        }
        reader.run_background_slice();
    }
    assert!(!reader.pending(), "transcript scan did not finish");
}

#[test]
fn missing_root_finishes_an_empty_scan() {
    let directory = tempfile::tempdir().unwrap();
    let mut reader = reader(&directory.path().join("missing"), None, Config::default());
    let snapshot = reader.refresh(now(), 60, false);
    assert_eq!(snapshot.state.message_count(), 0);
    assert!(snapshot.projects_complete);
    assert_eq!(reader.workload().scans_completed, 1);
}

#[test]
fn first_refresh_reads_and_deduplicates_lines() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("session.jsonl"),
        format!("{}{}", line("one"), line("one")),
    )
    .unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    let snapshot = reader.refresh(now(), 60, false);
    assert_eq!(snapshot.state.message_count(), 1);
    assert!(snapshot.projects_complete);
    assert_eq!(reader.workload().files_opened, 1);
}

#[test]
fn entry_and_byte_budgets_continue_in_background() {
    let directory = tempfile::tempdir().unwrap();
    for id in ["one", "two", "three"] {
        fs::write(directory.path().join(format!("{id}.jsonl")), line(id)).unwrap();
    }
    let config = Config {
        byte_budget: 1,
        entry_budget: 1,
        time_budget: Duration::from_secs(1),
        ..Config::default()
    };
    let mut reader = reader(directory.path(), None, config);
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 0);
    assert!(reader.pending());
    assert!(!reader.refresh(now() + 1.0, 60, false).projects_complete);
    assert_eq!(reader.workload().scans_started, 1);
    finish(&mut reader);
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 3);
    assert_eq!(reader.workload().scans_completed, 1);
    assert!(reader.workload().largest_slice_bytes_read <= 1);
    assert!(reader.workload().largest_slice_entries_examined <= 1);
}

#[test]
fn time_budget_resumes_a_large_transcript_without_rereading_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let content = line("one").repeat(80_000);
    fs::write(directory.path().join("session.jsonl"), &content).unwrap();
    let mut reader = reader(
        directory.path(),
        None,
        Config {
            byte_budget: content.len(),
            time_budget: Duration::from_millis(1),
            ..Config::default()
        },
    );
    reader.refresh(now(), 60, false);
    assert!(reader.pending());
    finish(&mut reader);
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 1);
    assert_eq!(reader.workload().bytes_read, content.len() as u64);
    assert!(reader.workload().largest_slice_bytes_read < content.len() as u64);
}
