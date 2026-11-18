//! Checks the public behavior of the adjacent module.

use super::*;

use std::{fs, os::unix::fs::PermissionsExt};

fn archive(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{index}", path.display()))
}

#[test]
fn records_reloads_and_clears_owner_only_logs() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("data/log.txt");
    let logs = LogStore::new(path.clone());
    logs.record("info", "app", "started\nnow").unwrap();
    assert_eq!(logs.tail()[0].message, "started\\nnow");
    assert_eq!(LogStore::new(path.clone()).retained(), logs.tail());
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    logs.clear().unwrap();
    assert!(logs.tail().is_empty());
    assert!(LogStore::new(path).retained().is_empty());
}

#[test]
fn writes_the_shared_text_line_format() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log.txt");
    LogStore::new(path.clone())
        .record("warning", "refresh", "codex refresh: offline")
        .unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(
        text.ends_with("] [warning] [refresh] codex refresh: offline\n"),
        "{text}"
    );
}

#[test]
fn drops_entries_older_than_the_retention_window() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log.txt");
    fs::write(&path, "[2000-01-01 00:00:00.000] [info] old\n").unwrap();
    assert!(LogStore::new(path).tail().is_empty());
}

#[test]
fn rotates_full_files_and_clears_every_archive() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log.txt");
    let stamp = Timestamp::now().strftime("%Y-%m-%d %H:%M:%S%.3f");
    fs::write(
        &path,
        format!("[{stamp}] [info] {}\n", "x".repeat(FILE_BYTE_LIMIT - 40)),
    )
    .unwrap();
    let logs = LogStore::new(path.clone());
    logs.record("warning", "refresh", "retry").unwrap();
    assert!(archive(&path, 1).exists());
    assert_eq!(logs.retained(), logs.tail());
    logs.clear().unwrap();
    assert!(!archive(&path, 1).exists());
}

#[test]
fn bounds_memory_without_dropping_retained_file_lines() {
    let root = tempfile::tempdir().unwrap();
    let logs = LogStore::new(root.path().join("log.txt"));
    for index in 0..=buffer::CAPACITY {
        logs.record("debug", "app", &index.to_string()).unwrap();
    }
    assert_eq!(logs.tail().len(), buffer::CAPACITY);
    assert_eq!(logs.retained().len(), buffer::CAPACITY + 1);
}

#[test]
fn clear_reports_a_non_file_log_path() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log.txt");
    fs::create_dir(&path).unwrap();
    assert_eq!(
        LogStore::new(path).clear().unwrap_err().kind(),
        io::ErrorKind::IsADirectory
    );
}

#[test]
fn rotate_reports_a_conflicting_backup_directory() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log.txt");
    fs::write(&path, vec![b' '; FILE_BYTE_LIMIT]).unwrap();
    fs::create_dir(archive(&path, 2)).unwrap();
    let error = LogStore::new(path).record("info", "app", "new").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::IsADirectory);
}

#[test]
fn redacts_sensitive_log_fields_before_retaining_them() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("log.txt");
    let logs = LogStore::new(path.clone());
    let home = std::env::var("HOME").unwrap();
    logs.record(
        "warning",
        "network",
        &format!(
            "Bearer abc access_token=xyz user@example.com {home}/private https://example.com/path?key=value\nnext"
        ),
    )
    .unwrap();
    let message = &logs.tail()[0].message;
    assert!(message.contains("Bearer <redacted>"));
    assert!(message.contains("access_token=<redacted>"));
    assert!(message.contains("<redacted-email>"));
    assert!(message.contains("~/private"));
    assert!(message.contains("https://example.com/path?<redacted>"));
    assert!(message.contains("\\nnext"));
    assert!(!message.contains("abc"));
    assert!(!message.contains("xyz"));
    assert!(!message.contains(&home));
    assert_eq!(LogStore::new(path).retained(), logs.tail());
}

#[test]
fn unavailable_home_does_not_change_log_paths() {
    let root = tempfile::tempdir().unwrap();
    let logs = LogStore::new(root.path().join("log.txt"));
    for home in [None, Some("/")] {
        unsafe {
            if let Some(home) = home {
                std::env::set_var("HOME", home);
            } else {
                std::env::remove_var("HOME");
            }
        }
        logs.record("info", "app", "/home/example/private").unwrap();
        assert_eq!(logs.tail().last().unwrap().message, "/home/example/private");
    }
}

#[test]
fn redacts_structured_and_multiline_secrets() {
    let root = tempfile::tempdir().unwrap();
    let logs = LogStore::new(root.path().join("log.txt"));
    for (index, message, secret) in [
        (0, "warning HTTP 500: {\"access_token\":\"sk-secret\"}", "sk-secret"),
        (1, "request body={\n  \"access_token\": \"sk-secret\"\n}", "sk-secret"),
        (2, "Authorization: Basic dXNlcjpwYXNz", "dXNlcjpwYXNz"),
        (3, "{\\\"client_secret\\\":\\\"private-value\\\"}", "private-value"),
    ] {
        logs.record("warning", "network", message).unwrap();
        let retained = &logs.tail()[index].message;
        assert!(retained.contains("<redacted>"), "{retained}");
        assert!(!retained.contains(secret), "{retained}");
    }
}

#[test]
fn bounds_utf8_log_messages_by_bytes() {
    let root = tempfile::tempdir().unwrap();
    let logs = LogStore::new(root.path().join("log.txt"));
    logs.record("info", "app", &"é".repeat(2_000)).unwrap();
    let message = &logs.tail()[0].message;
    assert!(message.len() <= quotaline_logging::MAX_MESSAGE_BYTES);
    assert!(message.ends_with('…'));
}
