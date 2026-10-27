//! Collection schedule in Iran time — the C# Hangfire registrations of `Program.Middlewares.cs`
//! (`ScheduleMarketDataJobs`), evaluated once per minute.

use std::sync::Arc;
use std::time::Duration;

use super::ApiContext;
use chrono::{Datelike, Timelike};

/// (job, cron "minute hour day-of-month month day-of-week"); trading days are Saturday–Wednesday (6,0-3).
pub const SCHEDULE: [(&str, &str); 10] = [
    ("instruments", "30 7 * * *"),
    ("eod", "35 13 * * 6,0-3"),
    ("eod", "35 18 * * 6,0-3"),
    ("messages", "*/30 8-18 * * 6,0-3"),
    ("codal", "*/15 7-23 * * *"),
    ("macro", "0 9,13,17,21 * * *"),
    ("funds", "0 20 * * *"),
    ("indices", "15 1 * * *"),
    ("backfill", "0 2 * * *"),
    ("halts", "0 3 * * *"),
];

/// One cron field: `*`, `*/n`, `a`, `a-b`, `a-b/n` and comma lists.
fn field_matches(field: &str, value: u32) -> bool {
    field.split(',').any(|part| {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().unwrap_or(1).max(1)),
            None => (part, 1),
        };
        let (from, to) = if range == "*" {
            (0, u32::MAX)
        } else if let Some((a, b)) = range.split_once('-') {
            (a.parse().unwrap_or(u32::MAX), b.parse().unwrap_or(0))
        } else {
            let v = range.parse().unwrap_or(u32::MAX);
            (v, if step > 1 { u32::MAX } else { v })
        };
        value >= from && value <= to && (value - from).is_multiple_of(step)
    })
}

/// True when the cron expression fires at this minute (Sunday = 0).
pub fn cron_matches(expression: &str, minute: u32, hour: u32, day: u32, month: u32, weekday_from_sunday: u32) -> bool {
    let fields: Vec<&str> = expression.split_whitespace().collect();
    fields.len() == 5
        && field_matches(fields[0], minute)
        && field_matches(fields[1], hour)
        && field_matches(fields[2], day)
        && field_matches(fields[3], month)
        && field_matches(fields[4], weekday_from_sunday)
}

/// Enqueues scheduled jobs forever, checking at the start of every Iran-time minute.
pub async fn run(api: Arc<ApiContext>) {
    tracing::info!("collection scheduler started (Asia/Tehran)");
    let mut last_fired: Option<(i32, u32, u32, u32, u32)> = None;
    loop {
        let now = crate::text::iran_now();
        let wait = Duration::from_secs(u64::from(60 - now.second())).saturating_sub(Duration::from_nanos(u64::from(now.nanosecond() % 1_000_000_000)));
        tokio::time::sleep(wait.max(Duration::from_millis(200))).await;

        let now = crate::text::iran_now();
        let key = (now.year(), now.ordinal(), now.hour(), now.minute(), 0);
        if last_fired == Some(key) {
            continue;
        }
        last_fired = Some(key);
        for (job, cron) in SCHEDULE {
            if cron_matches(cron, now.minute(), now.hour(), now.day(), now.month(), now.weekday().num_days_from_sunday()) {
                match api.jobs.enqueue(job) {
                    Ok(id) => tracing::info!(job, id, "scheduled job queued"),
                    Err(e) => tracing::error!(job, error = %e, "could not queue scheduled job"),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_matches_the_hangfire_registrations() {
        // Saturday 13:35 → eod; Thursday 13:35 → nothing.
        assert!(cron_matches("35 13 * * 6,0-3", 35, 13, 17, 10, 6));
        assert!(!cron_matches("35 13 * * 6,0-3", 35, 13, 15, 10, 4));
        assert!(cron_matches("*/30 8-18 * * 6,0-3", 30, 18, 1, 1, 3));
        assert!(!cron_matches("*/30 8-18 * * 6,0-3", 15, 9, 1, 1, 3));
        assert!(cron_matches("*/15 7-23 * * *", 45, 23, 1, 1, 5));
        assert!(cron_matches("0 9,13,17,21 * * *", 0, 17, 1, 1, 5));
        assert!(!cron_matches("0 9,13,17,21 * * *", 0, 18, 1, 1, 5));
    }
}
