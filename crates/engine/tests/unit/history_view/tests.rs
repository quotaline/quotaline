use super::*;
use crate::history::{Point, Series};

#[test]
fn usage_history_labels_only_contributing_providers() {
    let history = History {
        metric: "usage".into(),
        start: 0,
        end: 1,
        series: vec![
            Series {
                id: "claude:session".into(),
                provider: "claude".into(),
                label: "Current session".into(),
                points: vec![Point(0, 12.0, None, false)],
            },
            Series {
                id: "codex:weekly".into(),
                provider: "codex".into(),
                label: "Weekly".into(),
                points: Vec::new(),
            },
        ],
        canonical_total: None,
    };
    let view = serde_json::to_value(history_view(&history, &BTreeMap::new())).unwrap();
    assert_eq!(view["project_labels"], serde_json::json!({}));
}

#[test]
fn project_history_uses_private_display_label() {
    let history = History {
        metric: "projectCost".into(),
        start: 0,
        end: 1,
        series: vec![Series {
            id: "claude:/private/work".into(),
            provider: "claude".into(),
            label: "/private/work".into(),
            points: vec![Point(0, 2.0, None, false)],
        }],
        canonical_total: None,
    };
    let view = serde_json::to_value(history_view(
        &history,
        &BTreeMap::from([("/private/work".into(), "Private project".into())]),
    ))
    .unwrap();
    assert_eq!(view["project_labels"]["/private/work"], "Private project");
}
