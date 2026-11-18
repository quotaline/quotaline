//! Prepares window usage samples for platform history charts.

use std::collections::{BTreeMap, HashSet};

use jiff::Timestamp;
use jiff::tz::TimeZone;

use super::{downsample, insert_reset_zeros, point_limits};

mod pipeline;
pub use pipeline::window_chart;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum WindowRollup {
    Minute,
    Hour,
    Day,
}

impl WindowRollup {
    pub fn seconds(self) -> f64 {
        match self {
            Self::Minute => 60.0,
            Self::Hour => 3_600.0,
            Self::Day => 86_400.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WindowSample {
    pub key: String,
    pub at: f64,
    pub used: f64,
    pub reset_at: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WindowChartPoint {
    pub at: f64,
    pub value: f64,
    pub reset_at: Option<f64>,
    pub is_reset: bool,
    /// A copy of a reading moved to the period's edge so the line spans it; not a reading of its own.
    pub carried: bool,
    pub segment: u32,
    pub stack_base: f64,
}

impl WindowChartPoint {
    fn sample(sample: &WindowSample) -> Self {
        Self {
            at: sample.at,
            value: sample.used,
            reset_at: sample.reset_at,
            is_reset: false,
            carried: false,
            segment: 0,
            stack_base: 0.0,
        }
    }

    fn carried(&self, at: f64) -> Self {
        Self {
            at,
            is_reset: false,
            carried: true,
            segment: 0,
            stack_base: 0.0,
            ..self.clone()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WindowChartSeries {
    pub key: String,
    pub points: Vec<WindowChartPoint>,
    pub visible: bool,
    pub summary_value: f64,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WindowResetEvent {
    pub key: String,
    pub at: f64,
    pub reset_at: f64,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WindowChart {
    pub series: Vec<WindowChartSeries>,
    pub timeline: Vec<f64>,
    pub resets: Vec<WindowResetEvent>,
    pub y_max: f64,
    pub data_point_count: u64,
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct WindowChartRequest {
    pub keys: Vec<String>,
    pub visible: Vec<String>,
    pub start: f64,
    pub end: f64,
    pub now: f64,
    pub rollup: WindowRollup,
    pub stacked: bool,
    pub time_zone: String,
    pub fallback_offset: i32,
    pub includes_end: bool,
}

pub fn offset_at(at: f64, zone: Option<&TimeZone>, fallback: i32) -> i32 {
    zone.zip(Timestamp::from_second(at as i64).ok())
        .map_or(fallback, |(zone, date)| zone.to_offset(date).seconds())
}

/// Keeps the UTC offset in sub-day keys so a repeated local hour after a DST change stays a separate bucket.
pub fn bucket_key(at: f64, seconds: f64, zone: Option<&TimeZone>, fallback: i32) -> (i64, i32) {
    let offset = offset_at(at, zone, fallback);
    let bucket = ((at + f64::from(offset)) / seconds).floor() as i64;
    (bucket, if seconds == 86_400.0 { 0 } else { offset })
}

fn bucket_from<'a>(
    samples: impl IntoIterator<Item = &'a WindowSample>,
    rollup: WindowRollup,
    time_zone: &str,
    fallback_offset: i32,
) -> Vec<WindowChartPoint> {
    let zone = TimeZone::get(time_zone).ok();
    let mut latest = BTreeMap::<(i64, i32), WindowChartPoint>::new();
    for sample in samples {
        let key = bucket_key(sample.at, rollup.seconds(), zone.as_ref(), fallback_offset);
        let point = WindowChartPoint::sample(sample);
        if latest.get(&key).is_none_or(|existing| existing.at <= point.at) {
            latest.insert(key, point);
        }
    }
    let mut points = latest.into_values().collect::<Vec<_>>();
    points.sort_by(|left, right| left.at.total_cmp(&right.at));
    points
}

pub fn window_clip(
    points: &[WindowChartPoint],
    start: f64,
    end: f64,
    cadence: f64,
    includes_end: bool,
) -> Vec<WindowChartPoint> {
    let mut inside = points
        .iter()
        .filter(|point| point.at >= start && (if includes_end { point.at <= end } else { point.at < end }))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(last) = points.iter().rev().find(|point| point.at < start)
        && start - last.at <= cadence * 1.5
        && inside.first().is_none_or(|point| point.at > start)
    {
        inside.insert(0, last.carried(start));
    }
    inside
}

pub fn window_change_points(points: &[WindowChartPoint], cadence: Option<f64>) -> Vec<WindowChartPoint> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    points
        .iter()
        .enumerate()
        .filter_map(|(index, point)| {
            let previous = index.checked_sub(1).map(|index| &points[index]);
            let next = points.get(index + 1);
            let value_changes = previous.is_none_or(|previous| previous.value != point.value)
                || next.is_none_or(|next| next.value != point.value);
            let reset_boundary =
                point.is_reset || (point.value == 0.0 && previous.is_some_and(|previous| previous.value > 0.0));
            let gap_before =
                cadence.is_some_and(|cadence| previous.is_some_and(|previous| point.at - previous.at > cadence * 1.5));
            let gap_after = cadence.is_some_and(|cadence| next.is_some_and(|next| next.at - point.at > cadence * 1.5));
            (index == 0 || next.is_none() || value_changes || reset_boundary || gap_before || gap_after)
                .then(|| point.clone())
        })
        .collect()
}

pub fn window_extend_to(points: &[WindowChartPoint], end: f64, cadence: Option<f64>) -> Vec<WindowChartPoint> {
    let mut extended = points.to_vec();
    if let Some(last) = points.last()
        && last.at < end
        && cadence.is_none_or(|cadence| end - last.at <= cadence * 1.5)
    {
        extended.push(last.carried(end));
    }
    extended
}

pub fn window_nearest(dates: &[f64], at: f64) -> Option<f64> {
    let index = dates.partition_point(|date| *date < at);
    let before = index.checked_sub(1).and_then(|index| dates.get(index));
    let after = dates.get(index);
    match (before, after) {
        (None, None) => None,
        (Some(before), None) => Some(*before),
        (None, Some(after)) => Some(*after),
        (Some(before), Some(after)) => Some(if at - before <= after - at { *before } else { *after }),
    }
}

pub fn window_stack(series: &[WindowChartSeries]) -> Vec<WindowChartSeries> {
    let mut dates = series
        .iter()
        .filter(|series| series.visible)
        .flat_map(|series| series.points.iter().map(|point| point.at))
        .collect::<Vec<_>>();
    dates.sort_by(f64::total_cmp);
    dates.dedup();
    let mut bases = vec![0.0; dates.len()];
    series
        .iter()
        .map(|line| {
            if !line.visible {
                return line.clone();
            }
            let mut cursor = 0;
            let mut carried = 0.0;
            let points = dates
                .iter()
                .enumerate()
                .map(|(index, at)| {
                    while cursor < line.points.len() && line.points[cursor].at <= *at {
                        carried = line.points[cursor].value;
                        cursor += 1;
                    }
                    let point = WindowChartPoint {
                        at: *at,
                        value: carried,
                        reset_at: None,
                        is_reset: false,
                        carried: false,
                        segment: 0,
                        stack_base: bases[index],
                    };
                    bases[index] += carried;
                    point
                })
                .collect();
            WindowChartSeries { points, ..line.clone() }
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/unit/window/tests.rs"]
mod tests;
