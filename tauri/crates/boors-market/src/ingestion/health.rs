//! Freshness of every dataset against the session it should already contain, latest job runs and the last
//! reachability probe (C# `DataHealthService`).

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration, FixedOffset, Utc, Weekday};

use crate::context::MarketContext;
use crate::domain::InstrumentClass;
use crate::storage::CollectionRun;
use crate::text::{iran_now, is_official_holiday};
use crate::{Date, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetStatus {
    Ok,
    Stale,
    Empty,
    Info,
}

impl DatasetStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DatasetStatus::Ok => "Ok",
            DatasetStatus::Stale => "Stale",
            DatasetStatus::Empty => "Empty",
            DatasetStatus::Info => "Info",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatasetHealth {
    pub dataset: String,
    pub rows: i64,
    pub latest_date: Option<Date>,
    pub expected_date: Option<Date>,
    pub status: DatasetStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceReachability {
    pub name: String,
    pub verdict: String,
    pub status: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DataHealthReport {
    pub generated_at: DateTime<Utc>,
    pub node: String,
    pub expected_session: Date,
    pub datasets: Vec<DatasetHealth>,
    pub latest_runs: Vec<CollectionRun>,
    pub last_probe_at: Option<DateTime<FixedOffset>>,
    pub sources: Vec<SourceReachability>,
}

impl DataHealthReport {
    pub fn healthy(&self) -> bool {
        self.datasets.iter().all(|d| matches!(d.status, DatasetStatus::Ok | DatasetStatus::Info))
            && self.latest_runs.iter().all(|r| matches!(r.status.as_str(), "ok" | "skipped" | "running"))
            && self.sources.iter().all(|s| matches!(s.verdict.as_str(), "ok" | "reachable"))
    }
}

/// Calendar days a dataset may lag behind the expected session before it is stale.
fn allowed_lag_days(dataset: &str) -> Option<i64> {
    match dataset {
        "daily_bar" | "client_type_daily" | "index_daily" => Some(0),
        "instrument" | "supervisor_message" | "codal_letter" => Some(3),
        "macro_daily" => Some(4),
        "fund_daily" => Some(6),
        _ => None,
    }
}

pub struct DataHealthService<'a> {
    ctx: &'a MarketContext,
}

impl<'a> DataHealthService<'a> {
    pub fn new(ctx: &'a MarketContext) -> Self {
        Self { ctx }
    }

    pub fn get_report(&self) -> Result<DataHealthReport> {
        let now = iran_now();
        let expected = self.expected_session(now)?;
        let datasets = self
            .ctx
            .store
            .get_freshness()?
            .into_iter()
            .map(|f| {
                let lag = allowed_lag_days(&f.dataset);
                let expected_date = lag.map(|days| expected - Duration::days(days));
                let status = if f.rows == 0 {
                    if lag.is_none() { DatasetStatus::Info } else { DatasetStatus::Empty }
                } else {
                    match expected_date {
                        None => DatasetStatus::Info,
                        Some(e) if f.latest_date.is_some_and(|l| l >= e) => DatasetStatus::Ok,
                        Some(_) => DatasetStatus::Stale,
                    }
                };
                DatasetHealth { dataset: f.dataset, rows: f.rows, latest_date: f.latest_date, expected_date, status }
            })
            .collect();

        let (last_probe_at, sources) = self.read_latest_probe();
        Ok(DataHealthReport {
            generated_at: Utc::now(),
            node: self.ctx.options.node.clone(),
            expected_session: expected,
            datasets,
            latest_runs: self.ctx.store.get_latest_runs()?,
            last_probe_at,
            sources,
        })
    }

    /// The latest session whose end-of-day data should be in the store: today after the close (+30 min), otherwise
    /// the previous trading day.
    pub fn expected_session(&self, iran_now: DateTime<FixedOffset>) -> Result<Date> {
        let closures: HashSet<Date> = self.ctx.store.get_closure_days()?.into_iter().collect();
        let today = iran_now.date_naive();
        let close = self.ctx.rules.get_session(InstrumentClass::TseStock, today)?.value.close().overflowing_add_signed(Duration::minutes(30)).0;
        let mut day = if iran_now.time() >= close { today } else { today - Duration::days(1) };
        for _ in 0..60 {
            if is_trading_day(day, &closures) {
                return Ok(day);
            }
            day -= Duration::days(1);
        }
        Ok(day)
    }

    fn read_latest_probe(&self) -> (Option<DateTime<FixedOffset>>, Vec<SourceReachability>) {
        let directory = self.ctx.options.data_root.join("reports");
        let prefix = format!("probe-{}-", self.ctx.options.node);
        let mut files: Vec<String> = std::fs::read_dir(&directory)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.starts_with(&prefix) && n.ends_with(".json"))
                    .collect()
            })
            .unwrap_or_default();
        files.sort_by(|a, b| crate::raw_cache::ordinal_cmp(a, b));
        let Some(latest) = files.last() else { return (None, Vec::new()) };
        let Ok(bytes) = std::fs::read(directory.join(latest)) else { return (None, Vec::new()) };
        let Ok(root) = serde_json::from_slice::<serde_json::Value>(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)) else {
            return (None, Vec::new());
        };
        let sources = root
            .get("results")
            .and_then(|r| r.as_array())
            .map(|results| {
                results
                    .iter()
                    .map(|r| SourceReachability {
                        name: r.get("Name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        verdict: r.get("Verdict").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                        status: r.get("Status").and_then(|v| v.as_i64()).and_then(|v| i32::try_from(v).ok()),
                        error: r.get("Error").and_then(|v| v.as_str()).map(str::to_string),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let probed_at = root.get("probedAtUtc").and_then(|v| v.as_str()).and_then(|s| DateTime::parse_from_rfc3339(s).ok());
        (probed_at, sources)
    }
}

/// Trading days are Saturday–Wednesday, excluding official holidays and recorded closures.
pub fn is_trading_day(day: Date, closures: &HashSet<Date>) -> bool {
    if matches!(day.weekday(), Weekday::Thu | Weekday::Fri) || closures.contains(&day) {
        return false;
    }
    !is_official_holiday(day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn trading_days_exclude_weekend_holidays_and_closures() {
        let closures: HashSet<Date> = [NaiveDate::from_ymd_opt(2026, 4, 1).unwrap()].into_iter().collect();
        let cases = [("2026-10-17", true), ("2026-10-15", false), ("2026-10-16", false), ("2026-03-21", false), ("2026-04-01", false)];
        for (date, expected) in cases {
            assert_eq!(expected, is_trading_day(NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(), &closures), "{date}");
        }
    }
}
