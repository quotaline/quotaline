//! Aggregates parsed transcript records by day, project, and minute.

use jiff::Timestamp;

use super::*;
use crate::claude_transcripts::{TokenCounts, cost};
use quotaline_analytics::{
    self as analytics, AnalyticsSnapshot, CoverageScope, DayTotal, ModelTotal, Point, ProjectTotal,
};

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptAnalytics {
    pub points: Vec<Point>,
    pub first_day: String,
    pub last_day: String,
}

impl TranscriptAnalytics {
    pub fn snapshot(self, now: f64, projects_complete: bool) -> AnalyticsSnapshot {
        AnalyticsSnapshot {
            provider: "claude".into(),
            points: self.points,
            credit_events: Vec::new(),
            fetched_at: now,
            account_fingerprint: None,
            covered_scopes: if projects_complete {
                vec![CoverageScope {
                    metrics: vec!["projectCost".into(), "projectMessages".into()],
                    start_day: self.first_day,
                    end_day: self.last_day,
                }]
            } else {
                Vec::new()
            },
        }
    }
}

fn day(at: f64) -> String {
    Timestamp::from_second(at as i64)
        .unwrap()
        .strftime("%Y-%m-%d")
        .to_string()
}

impl TranscriptState {
    pub fn ingest(&mut self, record: &Record, fallback_project: &str) -> bool {
        let day = &record.day;
        let project = record.project.as_deref().unwrap_or(fallback_project);
        let aggregate = self.days.entry(day.clone()).or_default();
        let project_new = self
            .project_seen_by_day
            .entry(day.clone())
            .or_default()
            .insert(record.id.clone());
        let record_cost = record.cost.unwrap_or(0.0);
        if project_new {
            let legacy_name = if let Some(directory) = project.strip_prefix("directory:") {
                directory.split('-').rfind(|segment| !segment.is_empty())
            } else {
                project.split('/').rfind(|segment| !segment.is_empty())
            };
            if let Some(legacy_name) = legacy_name {
                let key = format!("legacy:{legacy_name}");
                if let Some(legacy) = aggregate.projects.get_mut(&key) {
                    legacy.messages -= 1;
                    legacy.cost -= record_cost;
                    if legacy.messages <= 0 {
                        aggregate.projects.remove(&key);
                    }
                }
            }
            let project_total = aggregate.projects.entry(project.into()).or_default();
            project_total.messages += 1;
            project_total.cost += record_cost;
            project_total.priced |= record.cost.is_some();
        }
        if !self
            .seen_by_day
            .entry(day.clone())
            .or_default()
            .insert(record.id.clone())
        {
            return project_new;
        }
        let usage = aggregate.models.entry(record.model.clone()).or_default();
        usage.input += record.input;
        usage.output += record.output;
        usage.cache_write += record.cache_write;
        usage.cache_read += record.cache_read;
        if let Some(one_hour) = record.cache_write_one_hour {
            *usage.cache_write_one_hour.get_or_insert(0) += one_hour;
        }
        if record.cost.is_some() {
            *aggregate.costs.entry(record.model.clone()).or_default() += record_cost;
        }
        aggregate.messages += 1;
        aggregate.sessions.insert(record.session.clone());
        aggregate.tool_calls += record.tool_calls;
        if record.cost.is_none() {
            self.unpriced_models.insert(record.model.clone());
        }
        let minute = record.timestamp.div_euclid(60) * 60;
        self.recent
            .entry(minute.to_string())
            .or_insert_with(|| RecentAggregate::new(minute as f64))
            .append(RecentEvent {
                timestamp: SwiftDate(record.timestamp as f64),
                tokens: record.input + record.output + record.cache_write + record.cache_read,
                cost: record_cost,
                messages: 1,
            });
        true
    }

    pub fn prune(&mut self, now: f64, retention_days: u32) -> bool {
        let retained_day = day(now - f64::from(retention_days) * 86_400.0);
        let old_days = self.days.len();
        let old_recent = self.recent.len();
        self.days.retain(|day, _| day >= &retained_day);
        self.seen_by_day.retain(|day, _| day >= &retained_day);
        self.project_seen_by_day.retain(|day, _| day >= &retained_day);
        let mut compacted = BTreeMap::<String, RecentAggregate>::new();
        for aggregate in self
            .recent
            .values()
            .filter(|aggregate| aggregate.last_timestamp.unwrap_or(aggregate.timestamp).0 >= now - 26.0 * 3_600.0)
        {
            let minute = (aggregate.timestamp.0 as i64).div_euclid(60) * 60;
            compacted
                .entry(minute.to_string())
                .or_insert_with(|| RecentAggregate::new(minute as f64))
                .merge(aggregate.clone());
        }
        let recent_changed = compacted != self.recent;
        self.recent = compacted;
        self.days.len() != old_days || self.recent.len() != old_recent || recent_changed
    }

    pub fn analytics(&self) -> Option<TranscriptAnalytics> {
        self.analytics_with_unpriced_zero(true)
    }

    pub fn analytics_without_unpriced_costs(&self) -> Option<TranscriptAnalytics> {
        self.analytics_with_unpriced_zero(false)
    }

    fn analytics_with_unpriced_zero(&self, emit_unpriced_zero: bool) -> Option<TranscriptAnalytics> {
        if self.message_count() == 0 {
            return None;
        }
        let models = self
            .days
            .iter()
            .flat_map(|(day, aggregate)| {
                aggregate.models.iter().map(move |(model, usage)| {
                    let estimated = cost(
                        model,
                        TokenCounts {
                            input: usage.input,
                            output: usage.output,
                            cache_write: usage.cache_write,
                            cache_read: usage.cache_read,
                            cache_write_one_hour: usage.cache_write_one_hour.unwrap_or(0),
                        },
                        None,
                    );
                    ModelTotal {
                        day: day.clone(),
                        model: model.clone(),
                        input: usage.input,
                        output: usage.output,
                        cache_write: usage.cache_write,
                        cache_read: usage.cache_read,
                        cost: aggregate
                            .costs
                            .get(model)
                            .copied()
                            .or(estimated)
                            .or(emit_unpriced_zero.then_some(0.0)),
                    }
                })
            })
            .collect::<Vec<_>>();
        let days = self
            .days
            .iter()
            .map(|(day, aggregate)| DayTotal {
                day: day.clone(),
                messages: aggregate.messages,
                sessions: aggregate.sessions.len() as i64,
                tool_calls: aggregate.tool_calls,
            })
            .collect::<Vec<_>>();
        let projects = self
            .days
            .iter()
            .flat_map(|(day, aggregate)| {
                aggregate.projects.iter().map(move |(project, total)| ProjectTotal {
                    day: day.clone(),
                    project: project.clone(),
                    messages: total.messages,
                    cost: (emit_unpriced_zero || total.priced || total.cost != 0.0).then_some(total.cost),
                })
            })
            .collect::<Vec<_>>();
        Some(TranscriptAnalytics {
            points: analytics::claude_points(&models, &days, &projects),
            first_day: self.days.first_key_value().unwrap().0.clone(),
            last_day: self.days.last_key_value().unwrap().0.clone(),
        })
    }
}
