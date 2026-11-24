use super::*;

#[test]
fn hidden_non_json_and_directory_entries_do_not_add_messages() {
    let directory = tempfile::tempdir().unwrap();
    fs::create_dir(directory.path().join("fake.jsonl")).unwrap();
    fs::create_dir(directory.path().join(".hidden")).unwrap();
    fs::write(directory.path().join(".hidden/session.jsonl"), line("hidden")).unwrap();
    fs::write(directory.path().join("ignored.txt"), line("text")).unwrap();
    fs::write(directory.path().join("session.jsonl"), line("visible")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 1);
    assert_eq!(reader.workload().stat_checks, 1);
    assert_eq!(reader.workload().files_opened, 1);
}

#[test]
fn nested_project_transcript_is_discovered() {
    let directory = tempfile::tempdir().unwrap();
    let project = directory.path().join("project");
    fs::create_dir(&project).unwrap();
    fs::write(project.join("session.jsonl"), line("one")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 1);
    assert_eq!(reader.workload().files_opened, 1);
}

#[cfg(unix)]
#[test]
fn unreadable_nested_directory_does_not_stop_other_transcripts() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    fs::create_dir(&blocked).unwrap();
    fs::write(blocked.join("session.jsonl"), line("blocked")).unwrap();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o0)).unwrap();
    fs::write(directory.path().join("visible.jsonl"), line("visible")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    let count = reader.refresh(now(), 60, false).state.message_count();
    fs::set_permissions(&blocked, fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn old_files_are_filtered_before_opening() {
    let directory = tempfile::tempdir().unwrap();
    let old = directory.path().join("old.jsonl");
    fs::write(&old, line("old")).unwrap();
    fs::File::open(&old)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(100)))
        .unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 0);
    assert_eq!(reader.workload().files_opened, 0);
    assert_eq!(reader.workload().stat_checks, 0);
}

#[test]
fn throttling_keeps_tail_reads_but_delays_tree_walks() {
    let directory = tempfile::tempdir().unwrap();
    let first = directory.path().join("first.jsonl");
    fs::write(&first, line("one")).unwrap();
    let mut reader = reader(
        directory.path(),
        None,
        Config {
            file_scan_interval: 1.0,
            ..Config::default()
        },
    );
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 1);
    fs::OpenOptions::new()
        .append(true)
        .open(&first)
        .unwrap()
        .write_all(line("two").as_bytes())
        .unwrap();
    fs::write(directory.path().join("second.jsonl"), line("three")).unwrap();
    assert_eq!(reader.refresh(now() + 2.0, 60, true).state.message_count(), 2);
    assert_eq!(reader.workload().scans_started, 1);
    assert_eq!(reader.refresh(now() + 3.0, 60, false).state.message_count(), 3);
}

#[cfg(unix)]
#[test]
fn symlinked_root_uses_canonical_paths_for_checkpoint_offsets() {
    let directory = tempfile::tempdir().unwrap();
    let actual = directory.path().join("actual");
    fs::create_dir(&actual).unwrap();
    let file = actual.join("session.jsonl");
    fs::write(&file, line("one")).unwrap();
    let linked = directory.path().join("linked");
    std::os::unix::fs::symlink(&actual, &linked).unwrap();
    let mut reader = reader(&linked, None, Config::default());
    let snapshot = reader.refresh(now(), 60, false);
    assert_eq!(
        snapshot.state.offset(file.to_str().unwrap()).unwrap().bytes,
        line("one").len() as u64
    );
}

#[test]
fn retention_changes_rescan_and_prune_old_messages() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("session.jsonl"), line("one")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 1);
    assert_eq!(reader.refresh(now(), 90, false).state.message_count(), 1);
    assert_eq!(reader.workload().scans_started, 2);
    assert_eq!(
        reader.refresh(now() + 100.0 * 86_400.0, 7, false).state.message_count(),
        0
    );
}

#[test]
fn unknown_model_warning_is_returned_once() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("session.jsonl"),
        line_with_model("one", "claude-unknown-7"),
    )
    .unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).unpriced_models, ["claude-unknown-7"]);
    assert!(reader.refresh(now(), 60, false).unpriced_models.is_empty());
}

#[test]
fn missing_and_non_file_indexed_paths_are_skipped() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("session.jsonl");
    fs::write(&file, line("one")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    reader.refresh(now(), 60, false);
    fs::remove_file(&file).unwrap();
    reader.refresh(now() + 1.0, 60, false);
    fs::create_dir(&file).unwrap();
    reader.refresh(now() + 2.0, 60, false);
    assert_eq!(reader.workload().files_opened, 1);
}

#[test]
fn hot_index_keeps_the_newest_files() {
    let directory = tempfile::tempdir().unwrap();
    for index in 0..=super::super::MAX_INDEXED_FILES {
        let file = directory.path().join(format!("session-{index:03}.jsonl"));
        fs::write(&file, line(&format!("before-{index}"))).unwrap();
        fs::File::open(&file)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(now() as u64 - index as u64)),
            )
            .unwrap();
    }
    let mut reader = reader(
        directory.path(),
        None,
        Config {
            file_scan_interval: 300.0,
            ..Config::default()
        },
    );
    reader.refresh(now(), 60, false);
    finish(&mut reader);
    for (index, id) in [(0, "hot"), (super::super::MAX_INDEXED_FILES, "cold")] {
        fs::OpenOptions::new()
            .append(true)
            .open(directory.path().join(format!("session-{index:03}.jsonl")))
            .unwrap()
            .write_all(line(id).as_bytes())
            .unwrap();
    }
    assert_eq!(reader.refresh(now() + 1.0, 60, false).state.message_count(), 66);
    assert_eq!(reader.refresh(now() + 301.0, 60, false).state.message_count(), 67);
}

#[test]
fn same_size_replacement_restarts_a_hot_file() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("session.jsonl");
    fs::write(&file, line("one")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 1);
    fs::write(&file, line("two")).unwrap();
    fs::File::open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(now() as u64 + 1)))
        .unwrap();
    assert_eq!(reader.refresh(now() + 1.0, 60, false).state.message_count(), 2);
}

#[test]
fn pending_hot_tail_reports_incomplete_until_the_byte_budget_finishes() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("session.jsonl");
    fs::write(&file, line("one")).unwrap();
    let mut reader = reader(
        directory.path(),
        None,
        Config {
            byte_budget: 1,
            ..Config::default()
        },
    );
    reader.refresh(now(), 60, false);
    finish(&mut reader);
    fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap()
        .write_all(line("two").as_bytes())
        .unwrap();
    reader.request_discovery();
    let pending = reader.refresh(now() + 1.0, 60, false);
    assert!(!pending.projects_complete);
    assert!(reader.pending());
    assert!(!reader.refresh(now() + 1.0, 60, false).projects_complete);
    finish(&mut reader);
    assert_eq!(reader.refresh(now() + 2.0, 60, false).state.message_count(), 2);
}

#[test]
fn pre_epoch_modified_file_is_skipped() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("session.jsonl");
    fs::write(&file, line("one")).unwrap();
    fs::File::open(&file)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH - Duration::from_secs(1)))
        .unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 0);
    assert_eq!(reader.workload().files_opened, 0);
}

#[test]
fn rescanning_an_unchanged_file_does_not_open_it_again() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("session.jsonl"), line("one")).unwrap();
    let mut reader = reader(
        directory.path(),
        None,
        Config {
            file_scan_interval: 1.0,
            entry_budget: 1,
            ..Config::default()
        },
    );
    reader.refresh(now(), 60, false);
    finish(&mut reader);
    let pending = reader.refresh(now() + 2.0, 60, false);
    assert!(!pending.projects_complete);
    assert!(reader.pending());
    finish(&mut reader);
    assert_eq!(reader.workload().files_opened, 1);
    assert_eq!(reader.workload().scans_completed, 2);
}

#[test]
fn deleted_file_is_removed_from_aggregates_after_discovery() {
    let directory = tempfile::tempdir().unwrap();
    let removed = directory.path().join("removed.jsonl");
    fs::write(&removed, line("removed")).unwrap();
    fs::write(directory.path().join("kept.jsonl"), line("kept")).unwrap();
    let mut reader = reader(directory.path(), None, Config::default());
    assert_eq!(reader.refresh(now(), 60, false).state.message_count(), 2);
    fs::remove_file(removed).unwrap();
    reader.request_discovery();
    assert!(!reader.refresh(now() + 1.0, 60, false).projects_complete);
    finish(&mut reader);
    assert_eq!(reader.refresh(now() + 2.0, 60, false).state.message_count(), 1);
}

#[cfg(target_os = "linux")]
#[test]
fn hot_index_breaks_equal_time_ties_by_path() {
    let directory = tempfile::tempdir().unwrap();
    for name in (0..super::super::MAX_INDEXED_FILES)
        .map(|index| format!("m-{index:03}.jsonl"))
        .chain(["a-new.jsonl".into(), "z-old.jsonl".into()])
    {
        let file = directory.path().join(name);
        fs::write(&file, line(file.file_stem().unwrap().to_str().unwrap())).unwrap();
        fs::File::open(&file)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(std::time::UNIX_EPOCH + Duration::from_secs(now() as u64)))
            .unwrap();
    }
    let mut reader = reader(directory.path(), None, Config::default());
    reader.refresh(now(), 60, false);
    finish(&mut reader);
    for name in ["a-new.jsonl", "z-old.jsonl"] {
        fs::OpenOptions::new()
            .append(true)
            .open(directory.path().join(name))
            .unwrap()
            .write_all(line(&format!("after-{name}")).as_bytes())
            .unwrap();
    }
    assert_eq!(reader.refresh(now() + 1.0, 60, false).state.message_count(), 67);
    assert_eq!(reader.refresh(now() + 901.0, 60, false).state.message_count(), 68);
}

fn two_hot_files(config: Config) -> (tempfile::TempDir, Reader) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("a.jsonl"), line("one")).unwrap();
    fs::write(directory.path().join("b.jsonl"), line("two")).unwrap();
    let mut reader = reader(directory.path(), None, config);
    reader.refresh(now(), 60, false);
    finish(&mut reader);
    (directory, reader)
}

#[test]
fn exhausted_byte_budget_defers_the_second_hot_tail() {
    let appended = line("three");
    let (directory, mut reader) = two_hot_files(Config {
        byte_budget: appended.len(),
        ..Config::default()
    });
    for (name, id) in [("a.jsonl", "three"), ("b.jsonl", "four")] {
        fs::OpenOptions::new()
            .append(true)
            .open(directory.path().join(name))
            .unwrap()
            .write_all(line(id).as_bytes())
            .unwrap();
    }
    assert_eq!(reader.refresh(now() + 1.0, 60, false).state.message_count(), 3);
    assert!(reader.pending());
    finish(&mut reader);
    assert_eq!(reader.refresh(now() + 2.0, 60, false).state.message_count(), 4);
}

#[test]
fn exhausted_time_budget_defers_the_second_hot_tail() {
    let (directory, mut reader) = two_hot_files(Config {
        byte_budget: 1_000_000,
        time_budget: Duration::from_millis(1),
        ..Config::default()
    });
    let first = line("one").repeat(1_000);
    fs::OpenOptions::new()
        .append(true)
        .open(directory.path().join("a.jsonl"))
        .unwrap()
        .write_all(first.as_bytes())
        .unwrap();
    fs::OpenOptions::new()
        .append(true)
        .open(directory.path().join("b.jsonl"))
        .unwrap()
        .write_all(line("three").as_bytes())
        .unwrap();
    assert_eq!(reader.refresh(now() + 1.0, 60, false).state.message_count(), 2);
    assert!(reader.pending());
    finish(&mut reader);
    assert_eq!(reader.refresh(now() + 2.0, 60, false).state.message_count(), 3);
}
