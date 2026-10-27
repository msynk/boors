//! Collection jobs, rebuild, health, ad-hoc queries, cache sync and imports (C# `CollectCommands`, `ProbeCommand`).

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use boors_market::domain::InstrumentClass;
use boors_market::ingestion::CollectionResult;
use boors_market::raw_cache::{self, RawCache};
use boors_market::sources::CachePolicy;
use boors_market::{Date, MarketContext, probe as probing};
use clap::{Args, Subcommand};

use crate::output::{self, V, pad_right, table};
use crate::{Global, open_context};

#[derive(Args, Debug, Clone, Default)]
pub struct CollectArgs {
    /// instruments | indices | calendar | backfill | eod | funds | macro | codal | messages | halts | all
    pub job: String,
    /// instruments: also fetch GetInstrumentInfo for instruments never detailed.
    #[arg(long)]
    pub details: bool,
    #[arg(long = "max-details", default_value_t = 300)]
    pub max_details: usize,
    /// indices: also fetch the main indices' history.
    #[arg(long)]
    pub history: bool,
    /// backfill: at most N instruments; halts: at most N state queries.
    #[arg(long)]
    pub max: Option<usize>,
    #[arg(long)]
    pub force: bool,
    #[arg(long = "with-messages")]
    pub with_messages: bool,
    /// Rules-class keys, e.g. tse_stock,ifb_stock.
    #[arg(long)]
    pub classes: Option<String>,
    #[arg(long, default_value_t = 5)]
    pub pages: i32,
    #[arg(long, default_value_t = 200)]
    pub top: i32,
    #[arg(long = "max-halts", default_value_t = 500)]
    pub max_halts: usize,
}

#[derive(Subcommand, Debug)]
pub enum CacheCommand {
    /// Export raw-cache entries into a zip bundle.
    Export {
        bundle: PathBuf,
        #[arg(long)]
        since: Option<String>,
    },
    /// Import a bundle (a union; latest-pointers only move forward).
    Import { bundle: PathBuf },
}

#[derive(Subcommand, Debug)]
pub enum ImportCommand {
    /// Monthly macro values (CPI, Exchange Centre rate): series,period,period_end,value,source[,note].
    Macro { file: PathBuf },
}

fn parse_scope(classes: Option<&str>) -> Option<HashSet<InstrumentClass>> {
    let classes = classes.filter(|c| !c.trim().is_empty())?;
    Some(classes.split(',').map(str::trim).filter(|c| !c.is_empty()).map(InstrumentClass::from_rule_key).collect())
}

fn print_result(result: &CollectionResult) {
    println!(
        "{} {} items={} failures={} {}",
        pad_right(&result.status, 9),
        pad_right(&result.job, 12),
        pad_right(&result.items.to_string(), 8),
        pad_right(&result.failures.to_string(), 4),
        result.message.as_deref().unwrap_or("")
    );
}

pub async fn collect(ctx: &Arc<MarketContext>, args: &CollectArgs, policy: CachePolicy) -> anyhow::Result<u8> {
    let collector = ctx.collector();
    let scope = parse_scope(args.classes.as_deref());
    let max = args.max.unwrap_or(usize::MAX);
    let mut results = Vec::new();
    let mut step = |result: CollectionResult| {
        print_result(&result);
        results.push(result);
    };

    match args.job.as_str() {
        "instruments" => step(collector.sync_instruments(args.details, args.max_details, policy).await?),
        "indices" => step(collector.sync_indices(args.history, policy).await?),
        "calendar" => {
            let calendar = collector.refresh_calendar()?;
            let count = calendar.detected_closed_days().len() as i32;
            step(CollectionResult {
                job: "calendar".into(),
                status: "ok".into(),
                items: count,
                failures: 0,
                message: Some(format!("{count} closure days recorded")),
            });
        }
        "backfill" => step(collector.backfill_history(max, args.force, args.with_messages, scope.as_ref(), policy).await?),
        "eod" => step(collector.end_of_day(args.force, policy).await?),
        "funds" => step(collector.sync_funds(policy).await?),
        "macro" => step(collector.sync_macro(None, policy).await?),
        "codal" => step(collector.sync_codal(args.pages, policy).await?),
        "messages" => step(collector.sync_messages(args.top, policy).await?),
        "halts" => step(collector.resolve_halts(args.max.unwrap_or(500), scope.as_ref(), policy).await?),
        "all" => {
            step(collector.sync_indices(true, policy).await?);
            step(collector.sync_instruments(args.details, args.max_details, policy).await?);
            step(collector.backfill_history(max, false, args.with_messages, scope.as_ref(), policy).await?);
            step(collector.end_of_day(args.force, policy).await?);
            step(collector.sync_messages(200, policy).await?);
            step(collector.resolve_halts(args.max_halts, scope.as_ref(), policy).await?);
            step(collector.sync_codal(args.pages, policy).await?);
            step(collector.sync_macro(None, policy).await?);
        }
        other => {
            return Err(boors_market::Error::Argument(format!(
                "Unknown job '{other}'. Jobs: instruments, indices, calendar, backfill, eod, funds, macro, codal, messages, halts, all."
            ))
            .into());
        }
    }

    Ok(if results.iter().all(CollectionResult::succeeded) { 0 } else { 6 })
}

/// Deletes the store and rebuilds it from the raw cache only (no network).
pub async fn rebuild(global: &Global, max: Option<usize>) -> anyhow::Result<u8> {
    let root = global.data_root();
    for file in ["market.duckdb", "market.duckdb.wal"] {
        let path = root.join(file);
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    }
    let started = Instant::now();
    let ctx = open_context(global)?;
    let args = CollectArgs { job: "all".into(), force: true, max, max_details: 300, pages: 5, top: 200, max_halts: 500, ..CollectArgs::default() };
    let code = collect(&ctx, &args, CachePolicy::CacheOnly).await?;
    println!("Rebuilt {} from the raw cache in {:.0} s.", ctx.store.path(), started.elapsed().as_secs_f64());
    Ok(code)
}

pub fn health(ctx: &Arc<MarketContext>) -> anyhow::Result<u8> {
    let report = ctx.health().get_report()?;
    println!("Node {}; expected latest session {}; overall: {}", report.node, report.expected_session, if report.healthy() { "HEALTHY" } else { "ATTENTION" });
    println!();
    table(
        &["dataset", "rows", "latest", "expected", "status"],
        report
            .datasets
            .iter()
            .map(|d| vec![V::from(d.dataset.as_str()), V::Int(d.rows), d.latest_date.into(), d.expected_date.into(), V::from(d.status.as_str())])
            .collect(),
    );
    println!();
    table(
        &["job", "last run (UTC)", "status", "items", "failures", "message"],
        report
            .latest_runs
            .iter()
            .map(|r| {
                vec![V::from(r.job.as_str()), V::Ts(r.started_at), V::from(r.status.as_str()), r.items.into(), r.failures.into(), r.message.clone().into()]
            })
            .collect(),
    );
    println!();
    match report.last_probe_at {
        None => println!("No reachability probe on record for this node. Run 'boors probe'."),
        Some(at) => {
            let problems: Vec<_> = report.sources.iter().filter(|s| !matches!(s.verdict.as_str(), "ok" | "reachable")).collect();
            println!(
                "Last probe {} UTC: {}/{} sources reachable.",
                at.to_utc().format("%Y-%m-%d %H:%M"),
                report.sources.len() - problems.len(),
                report.sources.len()
            );
            for p in problems {
                println!("  {}: {} {} {}", p.verdict, p.name, p.status.map(|s| s.to_string()).unwrap_or_default(), p.error.as_deref().unwrap_or(""));
            }
        }
    }
    Ok(if report.healthy() { 0 } else { 7 })
}

pub fn query(ctx: &Arc<MarketContext>, sql: &str, max: usize) -> anyhow::Result<u8> {
    let (columns, rows) = ctx.store.run_query(sql, max)?;
    let header: Vec<&str> = columns.iter().map(String::as_str).collect();
    let count = rows.len();
    table(&header, rows.iter().map(|r| r.iter().map(V::from).collect()).collect());
    println!("({count} row(s))");
    Ok(0)
}

pub fn cache(global: &Global, command: CacheCommand) -> anyhow::Result<u8> {
    let options = global.options();
    let cache = RawCache::new(options.data_root.clone(), options.node.clone());
    match command {
        CacheCommand::Export { bundle, since } => {
            let since = since.as_deref().and_then(|s| Date::parse_from_str(s, "%Y-%m-%d").ok());
            let exported = raw_cache::export_bundle(&cache, &bundle, since)?;
            let size = std::fs::metadata(&bundle).map(|m| m.len()).unwrap_or(0);
            println!("Exported {exported} file(s) to {} ({} KiB).", std::path::absolute(&bundle)?.display(), size / 1024);
        }
        CacheCommand::Import { bundle } => {
            let (imported, skipped) = raw_cache::import_bundle(&cache, &bundle)?;
            println!(
                "Imported {imported} file(s), skipped {skipped} already present. Run 'boors rebuild' or the collection jobs with --cache to load them into the store."
            );
        }
    }
    Ok(0)
}

pub async fn import(ctx: &Arc<MarketContext>, command: ImportCommand) -> anyhow::Result<u8> {
    let ImportCommand::Macro { file } = command;
    let result = ctx.collector().import_macro_monthly(&file).await?;
    println!("{}: {} value(s) imported. {}", result.status, result.items, result.message.as_deref().unwrap_or(""));
    Ok(if result.succeeded() { 0 } else { 6 })
}

pub async fn probe(global: &Global) -> anyhow::Result<u8> {
    let options = global.options();
    let (results, path) = probing::run(&options, |r| {
        println!(
            "{} {} {} {} {} ms  {}",
            pad_right(&r.verdict, 12),
            pad_right(&r.group, 12),
            pad_right(&r.name, 40),
            output::pad_left(&r.status.map(|s| s.to_string()).unwrap_or_else(|| "-".into()), 4),
            output::pad_left(&r.elapsed_ms.to_string(), 6),
            r.error.as_deref().unwrap_or("")
        );
    })
    .await?;
    println!();
    println!("Report: {}", std::path::absolute(&path)?.display());
    Ok(if results.iter().any(|r| r.group == "market-data" && r.verdict != "ok") { 3 } else { 0 })
}
