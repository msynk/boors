//! In-process job queue: one worker, jobs run one at a time (DuckDB allows one writer), failed jobs retried twice
//! (the C# Hangfire runner's `[AutomaticRetry(Attempts = 2)]`).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::sync::mpsc;

use super::dto::{JobStatusDto, iso};
use crate::context::MarketContext;
use crate::ingestion::CollectionResult;
use crate::sources::CachePolicy;
use crate::{Error, Result};

/// Jobs `MarketData/RunJob` accepts.
pub const JOBS: [&str; 10] = ["instruments", "indices", "backfill", "eod", "funds", "macro", "codal", "messages", "halts", "all"];

/// Internal job queued after a `Sync/Pull`: replays the raw cache into the store (no network).
pub const REPLAY_JOB: &str = "replay";

const MAX_ATTEMPTS: u32 = 3;
const FINISHED_KEPT: usize = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobState {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "Queued",
            JobState::Running => "Running",
            JobState::Succeeded => "Succeeded",
            JobState::Failed => "Failed",
        }
    }
}

#[derive(Debug, Clone)]
struct JobRecord {
    id: String,
    job: String,
    state: JobState,
    queued_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    message: Option<String>,
    attempts: u32,
}

impl JobRecord {
    fn dto(&self) -> JobStatusDto {
        JobStatusDto {
            id: self.id.clone(),
            job: self.job.clone(),
            state: self.state.as_str().to_string(),
            queued_at: iso(&self.queued_at),
            started_at: self.started_at.as_ref().map(iso),
            finished_at: self.finished_at.as_ref().map(iso),
            message: self.message.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct Board {
    active: Vec<JobRecord>,
    finished: VecDeque<JobRecord>,
}

/// Handle to the queue (cheap to clone).
#[derive(Debug, Clone)]
pub struct JobQueue {
    board: Arc<Mutex<Board>>,
    sender: mpsc::UnboundedSender<String>,
}

impl JobQueue {
    /// Starts the worker on the current tokio runtime.
    pub fn start(market: Arc<MarketContext>) -> JobQueue {
        let (sender, receiver) = mpsc::unbounded_channel();
        let queue = JobQueue { board: Arc::new(Mutex::new(Board::default())), sender };
        tokio::spawn(worker(queue.clone(), market, receiver));
        queue
    }

    fn board(&self) -> std::sync::MutexGuard<'_, Board> {
        self.board.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Queues a job and returns its id.
    pub fn enqueue(&self, job: &str) -> Result<String> {
        if !JOBS.contains(&job) && job != REPLAY_JOB {
            return Err(Error::Argument(format!("Unknown market-data job '{job}'.")));
        }
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.board().active.push(JobRecord {
            id: id.clone(),
            job: job.to_string(),
            state: JobState::Queued,
            queued_at: Utc::now(),
            started_at: None,
            finished_at: None,
            message: None,
            attempts: 0,
        });
        self.sender.send(id.clone()).map_err(|_| Error::InvalidOperation("The job worker has stopped.".into()))?;
        Ok(id)
    }

    /// Queued, running and the last 50 finished jobs, newest first.
    pub fn list(&self) -> Vec<JobStatusDto> {
        let board = self.board();
        let mut all: Vec<&JobRecord> = board.active.iter().chain(board.finished.iter()).collect();
        all.sort_by_key(|r| std::cmp::Reverse(r.queued_at));
        all.into_iter().map(JobRecord::dto).collect()
    }

    /// True while any job is queued or running.
    pub fn busy(&self) -> bool {
        !self.board().active.is_empty()
    }

    fn update(&self, id: &str, change: impl FnOnce(&mut JobRecord)) -> Option<JobRecord> {
        let mut board = self.board();
        let record = board.active.iter_mut().find(|r| r.id == id)?;
        change(record);
        Some(record.clone())
    }

    fn finish(&self, id: &str) {
        let mut board = self.board();
        if let Some(position) = board.active.iter().position(|r| r.id == id) {
            let record = board.active.remove(position);
            board.finished.push_front(record);
            board.finished.truncate(FINISHED_KEPT);
        }
    }
}

async fn worker(queue: JobQueue, market: Arc<MarketContext>, mut receiver: mpsc::UnboundedReceiver<String>) {
    while let Some(id) = receiver.recv().await {
        let Some(record) = queue.update(&id, |r| {
            r.state = JobState::Running;
            r.started_at = Some(Utc::now());
            r.attempts += 1;
        }) else {
            continue;
        };

        let job = record.job.clone();
        let ctx = market.clone();
        let handle = tokio::runtime::Handle::current();
        // Jobs mix network I/O with blocking DuckDB writes, so they run on a blocking thread that drives the
        // async parts with the runtime handle.
        let outcome = tokio::task::spawn_blocking(move || handle.block_on(run_job(&ctx, &job)))
            .await
            .map_err(|e| Error::InvalidOperation(format!("The job panicked: {e}")))
            .and_then(|r| r);

        let (succeeded, message) = match outcome {
            Ok(results) => summarize(&results),
            Err(e) => (false, e.to_string()),
        };

        if succeeded {
            queue.update(&id, |r| {
                r.state = JobState::Succeeded;
                r.finished_at = Some(Utc::now());
                r.message = Some(message);
            });
            queue.finish(&id);
        } else if record.attempts < MAX_ATTEMPTS {
            let attempt = record.attempts;
            queue.update(&id, |r| {
                r.state = JobState::Queued;
                r.message = Some(format!("attempt {attempt} of {MAX_ATTEMPTS} failed, retrying: {message}"));
            });
            let sender = queue.sender.clone();
            let delay = Duration::from_secs(30 * u64::from(attempt));
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                let _ = sender.send(id);
            });
        } else {
            queue.update(&id, |r| {
                r.state = JobState::Failed;
                r.finished_at = Some(Utc::now());
                r.message = Some(message);
            });
            queue.finish(&id);
        }
    }
}

/// A job fails when any of its steps fails (C# runner: `if (result.Status == "failed") throw`).
fn summarize(results: &[CollectionResult]) -> (bool, String) {
    let succeeded = results.iter().all(|r| r.status != "failed");
    let message = results
        .iter()
        .map(|r| {
            format!("{} {}: {} items, {} failures{}", r.job, r.status, r.items, r.failures, r.message.as_deref().map(|m| format!(" ({m})")).unwrap_or_default())
        })
        .collect::<Vec<_>>()
        .join("; ");
    (succeeded, message)
}

/// Runs a job by name with the server's parameters (C# `MarketDataJobRunner.Run`).
pub async fn run_job(ctx: &MarketContext, job: &str) -> Result<Vec<CollectionResult>> {
    let collector = ctx.collector();
    let o = &ctx.options;
    let network = CachePolicy::Network;
    Ok(match job {
        "instruments" => vec![collector.sync_instruments(true, o.max_instrument_details_per_run, network).await?],
        "indices" => vec![collector.sync_indices(true, network).await?],
        "backfill" => vec![collector.backfill_history(o.max_backfill_per_run, false, false, None, network).await?],
        "eod" => vec![collector.end_of_day(false, network).await?],
        "funds" => vec![collector.sync_funds(network).await?],
        "macro" => vec![collector.sync_macro(None, network).await?],
        "codal" => vec![collector.sync_codal(3, network).await?],
        "messages" => vec![collector.sync_messages(200, network).await?],
        "halts" => vec![collector.resolve_halts(o.max_halt_queries_per_run, None, network).await?],
        "all" => vec![
            collector.sync_indices(true, network).await?,
            collector.sync_instruments(true, o.max_instrument_details_per_run, network).await?,
            collector.backfill_history(o.max_backfill_per_run, false, false, None, network).await?,
            collector.end_of_day(false, network).await?,
            collector.sync_messages(200, network).await?,
            collector.resolve_halts(o.max_halt_queries_per_run, None, network).await?,
            collector.sync_codal(3, network).await?,
            collector.sync_macro(None, network).await?,
        ],
        REPLAY_JOB => replay(ctx).await?,
        other => return Err(Error::Argument(format!("Unknown market-data job '{other}'."))),
    })
}

/// Cache-only replay of every job into the existing store (merges are idempotent). Used after a sync import.
pub async fn replay(ctx: &MarketContext) -> Result<Vec<CollectionResult>> {
    let collector = ctx.collector();
    let cache = CachePolicy::CacheOnly;
    Ok(vec![
        collector.sync_indices(true, cache).await?,
        collector.sync_instruments(false, 0, cache).await?,
        collector.backfill_history(usize::MAX, true, false, None, cache).await?,
        collector.end_of_day(true, cache).await?,
        collector.sync_messages(200, cache).await?,
        collector.resolve_halts(ctx.options.max_halt_queries_per_run, None, cache).await?,
        collector.sync_codal(5, cache).await?,
        collector.sync_macro(None, cache).await?,
    ])
}
