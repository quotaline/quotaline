//! Streams history rows to CSV files that only the owner can read.

use std::{
    borrow::Cow,
    fs::{File, OpenOptions},
    io::BufWriter,
    path::Path,
};

use csv::{QuoteStyle, Writer, WriterBuilder};
use jiff::Timestamp;
use quotaline_analytics::project;

use super::query::visit_samples;
use super::{HistoryError, Result, Store, array};
use quotaline_analytics::utc_day::day;

fn stamp(at: f64) -> String {
    Timestamp::from_millisecond((at * 1000.0) as i64)
        .unwrap_or(Timestamp::UNIX_EPOCH)
        .strftime("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string()
}

struct Csv(Writer<BufWriter<File>>);

impl Csv {
    fn create(path: &Path, header: &[&str]) -> Result<Self> {
        if !path.parent().is_some_and(Path::is_dir) {
            return Err(HistoryError::ExportDirectory);
        }
        let file = quotaline_storage::open_private(OpenOptions::new().write(true).create(true).truncate(true), path)?;
        let mut writer = WriterBuilder::new()
            .quote_style(QuoteStyle::Necessary)
            .from_writer(BufWriter::with_capacity(256 * 1024, file));
        writer.write_record(header)?;
        Ok(Self(writer))
    }

    fn row<const N: usize>(&mut self, fields: [String; N]) -> Result<()> {
        self.0.write_record(fields.iter().map(|field| cell(field)))?;
        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        self.0.flush()?;
        Ok(())
    }
}

// Spreadsheets evaluate cells that start with these characters as formulas.
fn cell(field: &str) -> Cow<'_, [u8]> {
    if field.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        Cow::Owned(format!("'{field}").into_bytes())
    } else {
        Cow::Borrowed(field.as_bytes())
    }
}

impl Store {
    pub fn export_all(&self, path: &Path, hide_identity: bool) -> Result<()> {
        let state = self.state();
        let header = [
            "kind",
            "timestamp",
            "key",
            "label",
            "used_percent",
            "resets_at",
            "provider",
            "day",
            "metric",
            "series",
            "value",
        ];
        let mut csv = Csv::create(path, &header)?;
        let mut samples = state
            .db
            .prepare("SELECT ts,key,label,used,resets_at FROM samples ORDER BY ts ASC,key ASC")?;
        let mut rows = samples.query([])?;
        while let Some(row) = rows.next()? {
            let at: f64 = row.get(0)?;
            let used: f64 = row.get(3)?;
            let reset: Option<f64> = row.get(4)?;
            let record = [
                "sample".into(),
                stamp(at),
                row.get(1)?,
                row.get(2)?,
                format!("{used:.2}"),
                reset.map(stamp).unwrap_or_default(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ];
            csv.row(record)?;
        }
        let analytics_sql = "SELECT provider,day,metric,series,value FROM analytics ORDER BY day ASC,provider ASC,metric ASC,series ASC";
        let mut analytics = state.db.prepare(analytics_sql)?;
        let mut rows = analytics.query([])?;
        while let Some(row) = rows.next()? {
            let metric: String = row.get(2)?;
            let series: String = row.get(3)?;
            let value: f64 = row.get(4)?;
            let record = [
                "analytics".into(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                row.get(0)?,
                row.get(1)?,
                metric.clone(),
                project::export_label(&series, &metric, hide_identity),
                format!("{value:.4}"),
            ];
            csv.row(record)?;
        }
        csv.finish()
    }

    pub fn export_samples(
        &self,
        path: &Path,
        start: f64,
        end: f64,
        keys: Option<&[String]>,
        includes_end: bool,
    ) -> Result<()> {
        let mut csv = Csv::create(path, &["timestamp", "key", "label", "used_percent", "resets_at"])?;
        if keys.is_some_and(<[String]>::is_empty) {
            return csv.finish();
        }
        visit_samples(&self.state().db, start, end, keys, includes_end, |row| {
            let used: f64 = row.get(2)?;
            let reset: Option<f64> = row.get(3)?;
            csv.row([
                stamp(row.get(0)?),
                row.get(1)?,
                row.get(4)?,
                format!("{used:.2}"),
                reset.map(stamp).unwrap_or_default(),
            ])
        })?;
        csv.finish()
    }

    pub fn export_analytics(
        &self,
        path: &Path,
        metric: &str,
        providers: &[String],
        start: f64,
        end: f64,
        hide_identity: bool,
    ) -> Result<()> {
        let mut csv = Csv::create(path, &["provider", "day", "metric", "series", "value"])?;
        if providers.is_empty() {
            return csv.finish();
        }
        const SQL: &str = "SELECT provider,day,metric,series,value FROM analytics WHERE metric=?1 AND day>=?2
                           AND day<?3 AND provider IN rarray(?4) ORDER BY day ASC,provider ASC,series ASC";
        let state = self.state();
        let mut query = state.db.prepare(SQL)?;
        let mut rows = query.query((metric, day(start), day(end + 86_400.0), array(providers)))?;
        while let Some(row) = rows.next()? {
            let metric: String = row.get(2)?;
            let series: String = row.get(3)?;
            let value: f64 = row.get(4)?;
            let record = [
                row.get(0)?,
                row.get(1)?,
                metric.clone(),
                project::export_label(&series, &metric, hide_identity),
                format!("{value:.4}"),
            ];
            csv.row(record)?;
        }
        csv.finish()
    }
}
