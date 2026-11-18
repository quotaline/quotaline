//! Queries retained samples and analytics.

use std::collections::HashMap;

use jiff::tz::TimeZone;
use quotaline_analytics::{bucket_key, offset_at, utc_day::midnight};
use rusqlite::{Connection, Row, named_params};

use super::{AnalyticsPoint, Result, Sample, SampleQuery, Stats, Store, WindowSummary, array};

fn sample(row: &rusqlite::Row<'_>) -> rusqlite::Result<Sample> {
    Ok(Sample {
        at: row.get(0)?,
        key: row.get(1)?,
        used: row.get(2)?,
        reset_at: row.get(3)?,
    })
}

fn segments(start: f64, end: f64, zone: Option<&TimeZone>, fallback: i32) -> Vec<(f64, f64, i32)> {
    let mut segments = Vec::new();
    let mut segment_start = start;
    let mut cursor = start;
    let mut offset = offset_at(start, zone, fallback);
    while cursor < end {
        let next = (cursor + 86_400.0).min(end);
        let next_offset = offset_at(next, zone, fallback);
        if next_offset != offset {
            let mut left = cursor as i64;
            let mut right = next as i64;
            while left + 1 < right {
                let middle = left + (right - left) / 2;
                if offset_at(middle as f64, zone, fallback) == offset {
                    left = middle;
                } else {
                    right = middle;
                }
            }
            segments.push((segment_start, right as f64, offset));
            segment_start = right as f64;
            cursor = right as f64;
            offset = next_offset;
        } else {
            cursor = next;
        }
    }
    if segment_start < end {
        segments.push((segment_start, end, offset));
    }
    segments
}

// An `OR` that skips the key filter would stop SQLite from using the (key, ts) primary key.
fn key_filter(keys: Option<&[String]>) -> &'static str {
    if keys.is_some() {
        "key IN rarray(:keys)"
    } else {
        ":keys IS NULL"
    }
}

fn end_bound(includes_end: bool) -> &'static str {
    if includes_end { "<=" } else { "<" }
}

pub(super) fn visit_samples(
    db: &Connection,
    start: f64,
    end: f64,
    keys: Option<&[String]>,
    includes_end: bool,
    mut visit: impl FnMut(&Row<'_>) -> Result<()>,
) -> Result<()> {
    let sql = format!(
        "SELECT ts,key,used,resets_at,label FROM samples WHERE ts>=:start AND ts{}:end AND {} ORDER BY ts ASC",
        end_bound(includes_end),
        key_filter(keys)
    );
    let mut statement = db.prepare(&sql)?;
    let mut rows = statement.query(named_params! {":start": start, ":end": end, ":keys": keys.map(array)})?;
    while let Some(row) = rows.next()? {
        visit(row)?;
    }
    Ok(())
}

impl Store {
    pub fn samples(&self, request: SampleQuery<'_>) -> Result<Vec<Sample>> {
        let SampleQuery {
            keys,
            start,
            end,
            rollup,
            time_zone,
            fallback_offset,
            includes_end,
        } = request;
        if keys.is_some_and(<[String]>::is_empty) {
            return Ok(Vec::new());
        }
        let state = self.state();
        let Some(rollup) = rollup.filter(|rollup| *rollup > 0.0) else {
            let mut rows = Vec::new();
            visit_samples(&state.db, start, end, keys, includes_end, |row| {
                rows.push(sample(row)?);
                Ok(())
            })?;
            return Ok(rows);
        };
        let read_bounds = |row: &rusqlite::Row<'_>| Ok((row.get(0)?, row.get(1)?));
        let bounds_sql = "SELECT MIN(ts),MAX(ts) FROM samples WHERE ts>=?1 AND ts<=?2";
        let bounds: (Option<f64>, Option<f64>) = state.db.query_row(bounds_sql, [start, end], read_bounds)?;
        let (Some(first), Some(last)) = bounds else {
            return Ok(Vec::new());
        };
        let zone = TimeZone::get(time_zone).ok();
        let end_limit = last.min(end) + 1.0;
        let mut latest = HashMap::<(String, (i64, i32)), Sample>::new();
        for (segment_start, segment_end, offset) in
            segments(first.max(start), end_limit, zone.as_ref(), fallback_offset)
        {
            let sql = format!(
                "SELECT ts,key,used,resets_at FROM (SELECT ts,key,used,resets_at,ROW_NUMBER() OVER \
                 (PARTITION BY key,CAST((ts + :offset) / :rollup AS INTEGER) ORDER BY ts DESC) AS rank \
                 FROM samples WHERE ts>=:start AND ts{}:end AND {}) WHERE rank=1 ORDER BY ts ASC",
                end_bound(segment_end >= end_limit && includes_end),
                key_filter(keys)
            );
            let parameters = named_params! {
                ":offset": f64::from(offset),
                ":rollup": rollup,
                ":start": segment_start,
                ":end": segment_end.min(end),
                ":keys": keys.map(array),
            };
            for row in state.db.prepare(&sql)?.query_map(parameters, sample)? {
                let row = row?;
                latest.insert(
                    (
                        row.key.clone(),
                        bucket_key(row.at, rollup, zone.as_ref(), fallback_offset),
                    ),
                    row,
                );
            }
        }
        let mut rows: Vec<_> = latest.into_values().collect();
        rows.sort_by(|left, right| left.at.total_cmp(&right.at).then_with(|| left.key.cmp(&right.key)));
        Ok(rows)
    }

    pub fn last_usage_dates(&self, keys: Option<&[String]>, start: f64, end: f64) -> Result<Vec<(String, f64)>> {
        if keys.is_some_and(<[String]>::is_empty) {
            return Ok(Vec::new());
        }
        let sql = format!(
            "WITH chronological AS (
               SELECT ts,key,used,resets_at,
                 LAG(ts) OVER (PARTITION BY key ORDER BY ts) AS previous_ts,
                 LAG(used) OVER (PARTITION BY key ORDER BY ts) AS previous_used,
                 LAG(resets_at) OVER (PARTITION BY key ORDER BY ts) AS previous_resets_at
               FROM samples WHERE ts>=:start AND ts<=:end AND {})
             SELECT key,MAX(ts) FROM chronological WHERE used>0 AND
               (previous_ts IS NULL OR resets_at IS NOT previous_resets_at OR used>previous_used)
             GROUP BY key",
            key_filter(keys)
        );
        let parameters = named_params! {":start": start, ":end": end, ":keys": keys.map(array)};
        self.state()
            .db
            .prepare(&sql)?
            .query_map(parameters, |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn analytics(&self, provider: &str, start: &str, end: &str) -> Result<Vec<AnalyticsPoint>> {
        self.state()
            .db
            .prepare("SELECT day,metric,series,value FROM analytics WHERE provider=?1 AND day>=?2 AND day<=?3 ORDER BY day ASC")?
            .query_map([provider, start, end], |row| {
                Ok(AnalyticsPoint {
                    provider: provider.into(),
                    day: row.get(0)?,
                    metric: row.get(1)?,
                    series: row.get(2)?,
                    value: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn analytics_rows(
        &self,
        metric: &str,
        providers: &[String],
        start: &str,
        end: &str,
        includes_end: bool,
    ) -> Result<Vec<AnalyticsPoint>> {
        if providers.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT provider,day,series,value FROM analytics WHERE metric=?1 AND day>=?2 AND day{}?3
             AND provider IN rarray(?4) ORDER BY day ASC,provider ASC,series ASC",
            end_bound(includes_end)
        );
        self.state()
            .db
            .prepare(&sql)?
            .query_map((metric, start, end, array(providers)), |row| {
                Ok(AnalyticsPoint {
                    provider: row.get(0)?,
                    day: row.get(1)?,
                    metric: metric.into(),
                    series: row.get(2)?,
                    value: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn summaries(&self) -> Result<Vec<WindowSummary>> {
        const SQL: &str = "SELECT s.key,s.label,s.ts,s.used FROM samples s \
                           JOIN (SELECT key,MAX(ts) AS last_ts FROM samples GROUP BY key) latest \
                           ON s.key=latest.key AND s.ts=latest.last_ts ORDER BY s.key";
        self.state()
            .db
            .prepare(SQL)?
            .query_map([], |row| {
                Ok(WindowSummary {
                    key: row.get(0)?,
                    label: row.get(1)?,
                    last_seen: row.get(2)?,
                    last_percent: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn earliest_sample(&self, keys: Option<&[String]>) -> Result<Option<f64>> {
        if keys.is_some_and(<[String]>::is_empty) {
            return Ok(None);
        }
        let sql = format!("SELECT MIN(ts) FROM samples WHERE {}", key_filter(keys));
        let parameters = named_params! {":keys": keys.map(array)};
        Ok(self.state().db.query_row(&sql, parameters, |row| row.get(0))?)
    }

    pub fn earliest_analytics(&self, metric: &str, providers: &[String]) -> Result<Option<f64>> {
        if providers.is_empty() {
            return Ok(None);
        }
        let sql = "SELECT MIN(day) FROM analytics WHERE metric=?1 AND provider IN rarray(?2)";
        let value: Option<String> = self
            .state()
            .db
            .query_row(sql, (metric, array(providers)), |row| row.get(0))?;
        Ok(value.as_deref().and_then(midnight))
    }

    pub fn stats(&self) -> Result<Stats> {
        let state = self.state();
        let (sample_count, oldest, newest): (i64, Option<f64>, Option<f64>) =
            state
                .db
                .query_row("SELECT COUNT(*),MIN(ts),MAX(ts) FROM samples", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        let analytics_count: i64 = state
            .db
            .query_row("SELECT COUNT(*) FROM analytics", [], |row| row.get(0))?;
        Ok(Stats {
            sample_count: sample_count as u64,
            analytics_count: analytics_count as u64,
            oldest,
            newest,
        })
    }
}
