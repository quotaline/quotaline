use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn loading_missing_files_returns_an_empty_history() {
    let directory = tempfile::tempdir().unwrap();
    let loaded = load(&directory.path().join("missing.log"), 2, 100, 10, 0.0, None);
    assert!(loaded.entries.is_empty());
    assert!(!loaded.dropped_entries);
}

#[test]
fn append_rotates_private_files_and_loads_recent_entries() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("logs/app.log");
    let lines = (0..8)
        .map(|index| format!("[2026-08-29 19:00:0{index}.000] [info] entry-{index}"))
        .collect::<Vec<_>>();
    append(&path, &lines, 90, 3).unwrap();
    let loaded = load(&path, 3, 90, 100, 0.0, None);
    assert_eq!(loaded.entries.last().unwrap().message, "entry-7");
    assert!(!path.with_extension("log.3").exists());
    for file in [
        path.clone(),
        PathBuf::from(format!("{}.1", path.display())),
        PathBuf::from(format!("{}.2", path.display())),
    ] {
        assert!(file.exists());
        assert_eq!(fs::metadata(file).unwrap().permissions().mode() & 0o777, 0o600);
    }
}

#[test]
fn loading_bounds_file_tails_and_discards_an_incomplete_first_line() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    fs::write(
        &path,
        format!("{}\n[2026-08-29 19:00:00.000] [info] recent\n", "x".repeat(200)),
    )
    .unwrap();
    let loaded = load(&path, 1, 80, 100, 0.0, None);
    assert_eq!(
        loaded
            .entries
            .iter()
            .map(|entry| entry.message.as_str())
            .collect::<Vec<_>>(),
        ["recent"]
    );
    assert!(loaded.dropped_entries);
}

#[test]
fn loading_reports_a_truncated_tail_without_a_complete_line() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    fs::write(&path, "x".repeat(200)).unwrap();
    let loaded = load(&path, 1, 10, 100, 0.0, None);
    assert!(loaded.entries.is_empty());
    assert!(loaded.dropped_entries);
}

#[test]
fn loading_limits_entry_count_and_retention() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    fs::write(
        &path,
        "[2026-08-29 19:00:00.000] [info] old\n[2026-08-29 19:00:02.000] [info] kept\ncontinuation\n",
    )
    .unwrap();
    let loaded = load(&path, 1, 1_000, 2, 1_788_030_001.0, None);
    assert_eq!(
        loaded
            .entries
            .iter()
            .map(|entry| entry.message.as_str())
            .collect::<Vec<_>>(),
        ["kept", "continuation"]
    );
    assert!(loaded.dropped_entries);
}

#[test]
fn append_truncates_utf8_on_a_character_boundary() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &["éééé".into()], 4, 1).unwrap();
    assert_eq!(fs::read(path).unwrap(), "é\n".as_bytes());
}

#[test]
fn single_file_rotation_discards_the_old_entry() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &["first".into()], 7, 1).unwrap();
    append(&path, &["second".into()], 7, 1).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), "second\n");
}

#[test]
fn empty_append_does_not_create_a_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &[], 100, 1).unwrap();
    assert!(!path.exists());
}

#[test]
fn rewrite_replaces_rotated_files_and_clear_is_idempotent() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &["old-one".into(), "old-two".into()], 9, 2).unwrap();
    rewrite(&path, &["fresh".into()], 9, 2).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "fresh\n");
    assert!(!PathBuf::from(format!("{}.1", path.display())).exists());
    clear(&path, 2).unwrap();
    clear(&path, 2).unwrap();
    assert!(!path.exists());
}

#[test]
fn storage_reports_path_conflicts() {
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("blocked");
    fs::write(&parent, "not a directory").unwrap();
    assert!(append(&parent.join("app.log"), &["entry".into()], 100, 1).is_err());
    let path = directory.path().join("directory.log");
    fs::create_dir(&path).unwrap();
    assert!(clear(&path, 1).is_err());
}

#[test]
fn rotation_reports_an_existing_archive_directory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &["first".into()], 7, 2).unwrap();
    fs::create_dir(format!("{}.1", path.display())).unwrap();
    assert!(append(&path, &["second".into()], 7, 2).is_err());
}

#[test]
fn rotation_reports_a_rename_failure() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &["first".into()], 7, 2).unwrap();
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o500)).unwrap();
    let result = append(&path, &["second".into()], 7, 2);
    fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(result.is_err());
}

#[test]
fn one_byte_files_hold_only_a_newline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("app.log");
    append(&path, &["entry".into()], 1, 1).unwrap();
    assert_eq!(fs::read(path).unwrap(), b"\n");
}
