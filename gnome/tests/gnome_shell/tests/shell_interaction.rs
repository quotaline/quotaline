//! Exercises shell interaction in an isolated GNOME session.

use super::super::*;

#[cfg(feature = "fixture-api")]
#[test]
fn settings_exports_complete_history_in_shell() {
    portal_shell_fixture("extension/tests/integration/full-export.test.js", 0, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn save_dialog_accepts_an_early_response_in_shell() {
    portal_shell_fixture("extension/tests/integration/full-export.test.js", 6, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn save_dialog_ignores_an_unrelated_response_in_shell() {
    portal_shell_fixture("extension/tests/integration/full-export.test.js", 8, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn pending_save_dialog_method_does_not_block_extension_shutdown() {
    portal_shell_fixture("extension/tests/integration/portal-disable.test.js", 7, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_exports_selected_period_through_save_dialog_in_shell() {
    portal_shell_fixture("extension/tests/integration/selected-export.test.js", 0, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn save_dialog_rejects_a_non_file_uri_in_shell() {
    portal_shell_fixture(
        "extension/tests/integration/portal-error.test.js",
        0,
        Some("https://example.invalid/history.csv"),
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn save_dialog_reports_a_portal_failure_in_shell() {
    portal_shell_fixture("extension/tests/integration/portal-error.test.js", 2, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_export_write_failure_is_explained_in_shell() {
    portal_shell_fixture("extension/tests/integration/portal-error.test.js", 5, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn cancelled_save_dialog_keeps_history_unchanged_in_shell() {
    portal_shell_fixture("extension/tests/integration/portal-cancel.test.js", 1, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn failed_save_dialog_call_is_explained_in_shell() {
    portal_shell_fixture("extension/tests/integration/portal-error.test.js", 3, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn disabling_shell_cancels_pending_save_dialog() {
    portal_shell_fixture("extension/tests/integration/portal-disable.test.js", 4, None);
}

#[cfg(feature = "fixture-api")]
#[test]
fn remaining_mode_changes_usage_card_values_in_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true,"show_as":"remaining"}));
    run_shell_script(&fixture, "extension/tests/integration/remaining-usage.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn quota_help_runs_in_reused_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases(
        &fixture,
        &[
            ("tooltip.test.js", &[]),
            ("tooltip.test.js", &[("TMB_TOOLTIP_INPUT", "hover")]),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn settings_help_runs_in_reused_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases(
        &fixture,
        &[
            ("settings-help.test.js", &[]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "demo")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "notifications")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "log")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "format")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "footer")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "filter")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "group")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "selection")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "meter")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "label")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "template")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "save")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "provider")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "threshold")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "export")]),
            ("settings-help.test.js", &[("TMB_HELP_TARGET", "reset")]),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_and_panel_help_run_in_reused_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases(
        &fixture,
        &[
            ("history-help.test.js", &[]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "chart")]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "utc")]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "legend")]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "from")]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "to")]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "apply")]),
            ("history-help.test.js", &[("TMB_HISTORY_HELP", "stack")]),
            ("panel-help.test.js", &[]),
            ("panel-help.test.js", &[("TMB_PANEL_TRANSFER", "1")]),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn empty_panel_item_help_explains_missing_usage() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"providers":{
        "claude":false,"codex":false,"gemini":false,"antigravity":false,"cursor":false,"copilot":false
    }}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-help.test.js",
        &[("TMB_PANEL_EMPTY", "1")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn compact_shell_adapts_to_760_pixel_monitor() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/compact.test.js",
        &[("TMB_VIRTUAL_MONITOR", "760x720")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn very_narrow_shell_adapts_to_420_pixel_monitor() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/compact.test.js",
        &[("TMB_VIRTUAL_MONITOR", "420x720"), ("TMB_CAPTURE_VARIANT", "narrow")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn narrow_shell_adapts_to_560_pixel_monitor() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/compact.test.js",
        &[("TMB_VIRTUAL_MONITOR", "560x720"), ("TMB_CAPTURE_VARIANT", "middle")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn small_shell_adapts_to_320_pixel_monitor() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/compact.test.js",
        &[("TMB_VIRTUAL_MONITOR", "320x720"), ("TMB_CAPTURE_VARIANT", "small")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn demo_layout_history_and_diagnostics_share_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_cases(
        &fixture,
        &[
            ("panel-alignment.test.js", &[]),
            ("log-diagnostics.test.js", &[]),
            ("history-calendar.test.js", &[]),
        ],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn history_touch_isolation_reaches_shell() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(&fixture, "extension/tests/integration/history-last-series.test.js", &[]);
}

#[cfg(feature = "fixture-api")]
#[test]
fn centered_medium_history_stacks_two_legend_columns() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-alignment.test.js",
        &[("TMB_VIRTUAL_MONITOR", "760x720"), ("TMB_CENTERED_COMPACT", "1")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn provider_status_yields_to_other_panel_items() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-alignment.test.js",
        &[("TMB_PANEL_SPACE", "1")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn provider_status_yields_to_another_panel_group() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-alignment.test.js",
        &[("TMB_PANEL_SPACE", "1"), ("TMB_PANEL_PEER", "1")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn stacked_status_survives_panel_layout_reset() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-restriction.test.js",
        &[("TMB_PANEL_ORIENTATION", "1")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn mini_bars_survive_panel_layout_reset() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true}));
    run_shell_script(
        &fixture,
        "extension/tests/integration/panel-restriction.test.js",
        &[("TMB_PANEL_ORIENTATION", "1"), ("TMB_PANEL_MINI", "1")],
    );
}

#[cfg(feature = "fixture-api")]
#[test]
fn disabled_provider_with_cached_usage_stays_in_settings() {
    let _shell = shell_lock();
    let fixture = isolated_service(&serde_json::json!({"demo":true,"providers":{
        "claude":true,"codex":true,"gemini":true,"antigravity":false,"cursor":true,"copilot":true
    }}));
    run_shell_script(&fixture, "extension/tests/integration/disabled-provider.test.js", &[]);
}
