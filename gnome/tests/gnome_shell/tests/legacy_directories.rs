//! Checks that the service adopts directories left by the pre-rename install.

use super::super::*;

#[test]
fn service_moves_pre_rename_directories_and_logs_it() {
    let _shell = shell_lock();
    let scratch = tempfile::tempdir().unwrap();
    let legacy = scratch.path().join("config/token-menu-bar-gnome");
    fs::create_dir_all(&legacy).unwrap();
    fs::write(legacy.join("settings.json"), br#"{"demo":true}"#).unwrap();
    let (_bus, address) = private_bus(scratch.path(), "session-bus");
    let _service = Process(
        environment(
            &mut Command::new(env!("CARGO_BIN_EXE_quotaline-gnome")),
            scratch.path(),
            address.trim(),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap(),
    );
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let connection = zbus::connection::Builder::address(address.trim())
            .unwrap()
            .build()
            .await
            .unwrap();
        wait_for_service(&connection).await;
    });
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(scratch.path().join("config/quotaline/settings.json")).unwrap()).unwrap();
    assert_eq!(settings["demo"], true);
    assert!(!legacy.exists());
    let log = fs::read_to_string(scratch.path().join("data/quotaline/log.txt")).unwrap();
    assert!(log.contains("Moved "), "{log}");
}
