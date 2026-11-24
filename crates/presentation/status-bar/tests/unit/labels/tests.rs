//! Provider and model label cases.

use super::*;
use crate::{demo, short_label_conflicts};

fn windows() -> Vec<LabelWindow> {
    vec![
        LabelWindow {
            provider: "claude".into(),
            id: "session".into(),
            label: "Session".into(),
            scope: None,
        },
        LabelWindow {
            provider: "codex".into(),
            id: "session".into(),
            label: "Session".into(),
            scope: None,
        },
    ]
}

#[test]
fn short_label_helpers_preserve_graphemes_and_semantic_tags() {
    assert_eq!(short_label_draft("A👨‍👩‍👧‍👦BCDEF"), "A👨‍👩‍👧‍👦BCDE");
    assert_eq!(label_override("  ", "CX"), None);
    assert_eq!(label_override("CX", "CX"), None);
    assert_eq!(label_override("SPARKLES", "CX").as_deref(), Some("SPARKL"));
    assert_eq!(
        default_short_label("claude", "session", "Session", None).as_deref(),
        Some("CC 5h")
    );
    assert_eq!(
        default_short_label("claude", "monthly", "Monthly", None).as_deref(),
        Some("CC 1mo")
    );
    assert_eq!(
        default_short_label("claude", "opus", "Opus", None).as_deref(),
        Some("OP")
    );
    assert_eq!(
        default_short_label("claude", "opus4", "Opus 4", None).as_deref(),
        Some("OP4")
    );
    assert_eq!(default_short_label("missing", "session", "Session", None), None);
}

#[test]
fn provider_short_labels_cover_uncategorized_copilot_and_gemini_models() {
    for (provider, label, expected) in [
        ("copilot", "Completion", "GHX"),
        ("copilot", "Chat", "GHC"),
        ("copilot", "Other", "OTH"),
        ("gemini", "Pro", "PRO"),
    ] {
        assert_eq!(
            default_short_label(provider, "model", label, None).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn default_labels_cover_flash_pro_initials_and_generic_names() {
    for (provider, label, expected) in [
        ("gemini", "Flash", "GFL"),
        ("claude", "Flash", "FLA"),
        ("gemini", "Pro 2", "G2"),
        ("gemini", "Pro", "PRO"),
        ("gemini", "Other", "OTH"),
        ("claude", "Alpha Beta", "AB"),
        ("claude", "Model", "MOD"),
    ] {
        assert_eq!(
            default_short_label(provider, "model", label, None).as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn derived_labels_disambiguate_matching_model_names() {
    let windows = ["first", "second"].map(|id| LabelWindow {
        provider: "claude".into(),
        id: id.into(),
        label: "Spark".into(),
        scope: None,
    });
    let labels = derived_labels_for_windows(&windows);
    assert_eq!(labels["claude:first"], "SPK");
    assert_eq!(labels["claude:second"], "SPK2");
}

#[test]
fn resolved_labels_reject_matching_overrides() {
    let overrides = BTreeMap::from([
        ("claude:session".into(), "SHARED".into()),
        ("codex:session".into(), "SHARED".into()),
    ]);
    let labels = resolved_labels_for_windows(&windows(), &overrides);
    assert_eq!(labels["claude:session"], "SHARED");
    assert_eq!(labels["codex:session"], "CX 5h");
}

#[test]
fn changed_short_label_reports_a_default_collision() {
    let overrides = BTreeMap::from([("claude:session".into(), "CX 5h".into())]);
    assert!(short_label_conflicts(&demo(), &BTreeMap::new(), &overrides));
}

#[test]
fn changed_short_labels_report_each_other_as_conflicts() {
    let overrides = BTreeMap::from([
        ("claude:session".into(), "SHARED".into()),
        ("codex:session".into(), "SHARED".into()),
    ]);
    assert!(short_label_conflicts(&demo(), &BTreeMap::new(), &overrides));
}

#[test]
fn unchanged_or_unique_short_labels_do_not_conflict() {
    let existing = BTreeMap::from([("claude:session".into(), "SHARED".into())]);
    for (previous, overrides) in [
        (existing.clone(), existing),
        (
            BTreeMap::new(),
            BTreeMap::from([("claude:session".into(), "UNIQUE".into())]),
        ),
        (
            BTreeMap::new(),
            BTreeMap::from([("unknown:key".into(), "UNKNOWN".into())]),
        ),
    ] {
        assert!(!short_label_conflicts(&demo(), &previous, &overrides));
    }
}

#[test]
fn label_maps_apply_valid_overrides_without_losing_defaults() {
    let overrides = BTreeMap::from([("claude:session".into(), "CUSTOM".into())]);
    let defaults = derived_labels_for_windows(&windows());
    let resolved = resolved_labels_for_windows(&windows(), &overrides);
    assert_eq!(defaults.get("claude:session").map(String::as_str), Some("CC 5h"));
    assert_eq!(resolved.get("claude:session").map(String::as_str), Some("CUSTOM"));
    assert_eq!(resolved.get("codex:session"), defaults.get("codex:session"));
    assert!(derived_labels_for_windows(&[]).is_empty());
    let mut unknown = windows();
    unknown.push(LabelWindow {
        provider: "missing".into(),
        id: "session".into(),
        label: "Session".into(),
        scope: None,
    });
    assert_eq!(derived_labels_for_windows(&unknown), defaults);
}

#[test]
fn label_preview_and_draft_merge_reject_collisions() {
    let preview = preview_label_for_windows(&windows(), &BTreeMap::new(), "claude:session", "CX 5h").unwrap();
    assert_eq!(preview.conflict_key.as_deref(), Some("codex:session"));
    assert_eq!(
        preview_label_for_windows(&windows(), &BTreeMap::new(), "missing", "X"),
        None
    );
    let drafts = BTreeMap::from([
        ("claude:session".into(), "CX 5h".into()),
        ("codex:session".into(), "NEW".into()),
        ("missing".into(), "X".into()),
    ]);
    assert_eq!(
        valid_overrides_for_windows(&windows(), &BTreeMap::new(), &drafts),
        BTreeMap::from([("codex:session".into(), "NEW".into())])
    );
    let persisted = BTreeMap::from([("claude:session".into(), "CUSTOM".into())]);
    let cleared = BTreeMap::from([("claude:session".into(), "".into())]);
    assert!(valid_overrides_for_windows(&windows(), &persisted, &cleared).is_empty());
}
