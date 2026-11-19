use super::*;

#[test]
fn terminal_ignores_signals_and_reports_a_24_hour_clock() {
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    runtime.block_on(async {
        TerminalHost.changed("{}".into()).await;
        TerminalHost.log_changed().await;
        TerminalHost.notification_planned().await;
        assert!(!TerminalHost.twelve_hour_clock().await);
    });
}
