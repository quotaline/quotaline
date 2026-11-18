//! Adds private project labels to the GNOME chart payload.

use std::collections::BTreeMap;

use crate::history::History;
use serde::Serialize;

#[derive(Serialize)]
pub struct MetricHistoryView<'a> {
    #[serde(flatten)]
    history: &'a History,
    project_labels: BTreeMap<String, String>,
}

pub fn history_view<'a>(history: &'a History, project_labels: &BTreeMap<String, String>) -> MetricHistoryView<'a> {
    MetricHistoryView {
        history,
        project_labels: project_labels.clone(),
    }
}

#[cfg(test)]
#[path = "../tests/unit/history_view/tests.rs"]
mod tests;
