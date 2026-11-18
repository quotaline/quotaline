//! Checks GNOME's path and lifecycle binding to the shared reader.

use super::*;
use std::io::Write;

fn line(id: Option<&str>) -> String {
    let mut message = serde_json::json!({
        "model": "claude-sonnet-4-6",
        "usage": {"input_tokens": 2, "output_tokens": 3}
    });
    if let Some(id) = id {
        message["id"] = id.into();
    }
    format!(
        "{}\n",
        serde_json::json!({
            "type": "assistant", "timestamp": Timestamp::now().to_string(), "message": message
        })
    )
}

#[test]
fn scan_counts_distinct_files_without_provider_message_ids() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("projects");
    fs::create_dir(&root).unwrap();
    for name in ["a.jsonl", "b.jsonl"] {
        fs::write(root.join(name), line(None)).unwrap();
    }
    let mut scanner = Scanner::new(root, directory.path().join("checkpoint.json"));
    let scan = scanner.scan(60, Timestamp::now().as_second() - 18_000, true);
    assert!(!scan.pending);
    assert_eq!(scan.usage.unwrap().today_messages, 2);
}

#[test]
fn scan_follows_appended_hot_files_while_discovery_is_throttled() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("projects");
    fs::create_dir(&root).unwrap();
    let file = root.join("session.jsonl");
    fs::write(&file, line(Some("one"))).unwrap();
    let mut scanner = Scanner::new(root, directory.path().join("checkpoint.json"));
    assert_eq!(
        scanner
            .scan(60, Timestamp::now().as_second() - 18_000, true)
            .usage
            .unwrap()
            .today_messages,
        1
    );
    fs::OpenOptions::new()
        .append(true)
        .open(file)
        .unwrap()
        .write_all(line(Some("two")).as_bytes())
        .unwrap();
    assert_eq!(
        scanner
            .scan(60, Timestamp::now().as_second() - 18_000, false)
            .usage
            .unwrap()
            .today_messages,
        2
    );
}

#[test]
fn clear_removes_checkpoint_and_restarts_the_reader() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("projects");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("session.jsonl"), line(Some("one"))).unwrap();
    let checkpoint = directory.path().join("checkpoint.json");
    let mut scanner = Scanner::new(root, checkpoint.clone());
    scanner.scan(60, Timestamp::now().as_second() - 18_000, true);
    assert!(checkpoint.exists());
    scanner.clear().unwrap();
    assert!(!checkpoint.exists());
    assert_eq!(
        scanner
            .scan(60, Timestamp::now().as_second() - 18_000, true)
            .usage
            .unwrap()
            .today_messages,
        1
    );
}

#[test]
fn missing_project_root_has_no_usage_or_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let mut scanner = Scanner::new(
        directory.path().join("missing"),
        directory.path().join("checkpoint.json"),
    );
    let scan = scanner.scan(60, Timestamp::now().as_second() - 18_000, false);
    assert!(!scan.pending);
    assert!(scan.usage.is_none());
    scanner.clear().unwrap();
}

#[test]
fn discovery_removes_deleted_file_contributions() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("projects");
    fs::create_dir(&root).unwrap();
    let removed = root.join("removed.jsonl");
    fs::write(&removed, line(Some("removed"))).unwrap();
    fs::write(root.join("kept.jsonl"), line(Some("kept"))).unwrap();
    let mut scanner = Scanner::new(root, directory.path().join("checkpoint.json"));
    let window = Timestamp::now().as_second() - 18_000;
    assert_eq!(scanner.scan(60, window, true).usage.unwrap().today_messages, 2);
    fs::remove_file(removed).unwrap();
    assert!(scanner.scan(60, window, true).pending);
    assert_eq!(scanner.scan(60, window, false).usage.unwrap().today_messages, 1);
}

#[test]
fn clear_reports_unremovable_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let mut scanner = Scanner::new(directory.path().join("missing"), directory.path().to_path_buf());
    assert!(scanner.clear().is_err());
}

#[test]
fn large_oversized_file_does_not_block_a_later_transcript() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("projects");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.jsonl"), format!("{}\n", "x".repeat(1_048_577)).repeat(9)).unwrap();
    fs::write(root.join("b.jsonl"), line(Some("after-budget"))).unwrap();
    let mut scanner = Scanner::new(root, directory.path().join("checkpoint.json"));
    let scan = (0..100)
        .map(|_| scanner.scan(60, Timestamp::now().as_second() - 18_000, true))
        .find(|scan| scan.usage.is_some());
    assert_eq!(
        scan.and_then(|scan| scan.usage.map(|usage| usage.today_messages)),
        Some(1)
    );
}
