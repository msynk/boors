//! Verification commands (C# `VerifyCommands` and the verify parts of `CollectCommands`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;

use boors_market::analytics::point_in_time::{adjust_from_bars, implied_adjustments_default};
use boors_market::contracts::{self, Shape};
use boors_market::domain::{BestLimitLevel, ClosingPriceSnapshot, DailyBar, Instrument, MarketWatchRow};
use boors_market::sources::CachePolicy;
use boors_market::sources::tsetmc::{MarketWatchQuery, parser};
use boors_market::{Decimal, Error, MarketContext};
use chrono::{Datelike, Weekday};
use clap::Subcommand;

use crate::output::*;
use crate::{Global, open_context};

#[derive(Subcommand, Debug)]
pub enum VerifyCommand {
    /// Check the inferred market-watch compact keys against GetClosingPriceInfo/GetInstrumentInfo.
    #[command(name = "mw-keys")]
    MwKeys {
        #[arg(long, default_value_t = 10)]
        count: usize,
    },
    /// Compare TSETMC's published adjustments with those implied by the bars.
    Adjustments { target: String },
    /// Compare live JSON shapes of every endpoint with the committed baseline.
    Contracts {
        #[arg(long)]
        update: bool,
        #[arg(long, default_value_t = 4)]
        samples: usize,
        /// Baseline file (default: crates/boors-market/data/baseline.json, else the embedded copy).
        #[arg(long)]
        baseline: Option<PathBuf>,
    },
    /// Compare stored bars of the top traded equities with TSETMC now.
    Reconcile {
        #[arg(long, default_value_t = 20)]
        count: usize,
        #[arg(long, default_value_t = 5)]
        days: usize,
    },
}

pub async fn run(global: &Global, command: VerifyCommand) -> anyhow::Result<u8> {
    let ctx = open_context(global)?;
    match command {
        VerifyCommand::MwKeys { count } => mw_keys(&ctx, global.policy(), count).await,
        VerifyCommand::Adjustments { target } => adjustments(&ctx, global.policy(), &target).await,
        VerifyCommand::Contracts { update, samples, baseline } => contracts_check(&ctx, update, samples, baseline).await,
        VerifyCommand::Reconcile { count, days } => reconcile(&ctx, count, days).await,
    }
}

#[derive(Clone, PartialEq, Debug)]
enum Field {
    Num(Decimal),
    Text(Option<String>),
    Missing,
}

type Compare = fn(&MarketWatchRow, &Instrument, &ClosingPriceSnapshot, Option<(Decimal, Decimal)>, Option<&BestLimitLevel>) -> (Field, Field);

fn num(v: Decimal) -> Field {
    Field::Num(v)
}

fn int(v: i64) -> Field {
    Field::Num(Decimal::from(v))
}

fn text(v: Option<&str>) -> Field {
    Field::Text(v.map(|s| s.trim().to_string()))
}

fn opt(v: Option<Decimal>) -> Field {
    v.map_or(Field::Missing, Field::Num)
}

fn show(field: &Field) -> String {
    match field {
        Field::Num(d) => format(&V::Dec(*d)),
        Field::Text(t) => t.clone().unwrap_or_default(),
        Field::Missing => String::new(),
    }
}

const CHECKS: [(&str, &str, &str, Compare); 19] = [
    ("last price", "pdv", "GetClosingPriceInfo.pDrCotVal", |m, _, c, _, _| (num(m.last), num(c.last))),
    ("closing price", "pcl", "GetClosingPriceInfo.pClosing", |m, _, c, _, _| (num(m.close), num(c.close))),
    ("first price", "pf", "GetClosingPriceInfo.priceFirst", |m, _, c, _, _| (num(m.open), num(c.open))),
    ("day low", "pmn", "GetClosingPriceInfo.priceMin", |m, _, c, _, _| (num(m.low), num(c.low))),
    ("day high", "pmx", "GetClosingPriceInfo.priceMax", |m, _, c, _, _| (num(m.high), num(c.high))),
    ("yesterday", "py", "GetClosingPriceInfo.priceYesterday", |m, _, c, _, _| (num(m.yesterday), num(c.yesterday))),
    ("trade count", "ztt", "GetClosingPriceInfo.zTotTran", |m, _, c, _, _| (int(m.trade_count), int(c.trade_count))),
    ("volume", "qtj", "GetClosingPriceInfo.qTotTran5J", |m, _, c, _, _| (int(m.volume), int(c.volume))),
    ("value", "qtc", "GetClosingPriceInfo.qTotCap", |m, _, c, _, _| (num(m.value), num(c.value))),
    ("allowed high", "pMax", "GetInstrumentInfo.staticThreshold.psGelStaMax", |m, _, _, t, _| (num(m.allowed_high), opt(t.map(|t| t.1)))),
    ("allowed low", "pMin", "GetInstrumentInfo.staticThreshold.psGelStaMin", |m, _, _, t, _| (num(m.allowed_low), opt(t.map(|t| t.0)))),
    ("base volume", "bv", "GetInstrumentInfo.baseVol", |m, i, _, _, _| (int(m.base_volume), int(i.base_volume.unwrap_or(0)))),
    ("total shares", "ztd", "GetInstrumentInfo.zTitad", |m, i, _, _, _| (num(m.total_shares), num(i.total_shares.unwrap_or_default()))),
    ("sector code", "csv", "GetInstrumentInfo.sector.cSecVal", |m, i, _, _, _| (text(m.sector_code.as_deref()), text(i.sector_code.as_deref()))),
    ("ISIN", "insID", "GetInstrumentInfo.instrumentID", |m, i, _, _, _| (text(m.isin.as_deref()), text(i.isin.as_deref()))),
    ("best bid price", "blDs[0].pmd", "BestLimits[0].pMeDem", |m, _, _, _, b| (opt(m.best_limits.first().map(|l| l.bid_price)), opt(b.map(|l| l.bid_price)))),
    ("best bid volume", "blDs[0].qmd", "BestLimits[0].qTitMeDem", |m, _, _, _, b| {
        (opt(m.best_limits.first().map(|l| Decimal::from(l.bid_volume))), opt(b.map(|l| Decimal::from(l.bid_volume))))
    }),
    ("best ask price", "blDs[0].pmo", "BestLimits[0].pMeOf", |m, _, _, _, b| (opt(m.best_limits.first().map(|l| l.ask_price)), opt(b.map(|l| l.ask_price)))),
    ("best ask volume", "blDs[0].qmo", "BestLimits[0].qTitMeOf", |m, _, _, _, b| {
        (opt(m.best_limits.first().map(|l| Decimal::from(l.ask_volume))), opt(b.map(|l| Decimal::from(l.ask_volume))))
    }),
];

async fn mw_keys(ctx: &MarketContext, policy: CachePolicy, count: usize) -> anyhow::Result<u8> {
    let started = boors_market::text::iran_now();
    let snapshot = ctx.tsetmc.get_market_watch(&MarketWatchQuery { paper_types: vec![1], ..MarketWatchQuery::default() }, policy).await?;
    let mut sample: Vec<&MarketWatchRow> = snapshot.rows.iter().filter(|r| r.trade_count > 0).collect();
    sample.sort_by_key(|r| std::cmp::Reverse(r.value));
    sample.truncate(count);

    let mut tallies: Vec<(usize, usize, Option<String>)> = vec![(0, 0, None); CHECKS.len()];
    for row in &sample {
        let info_body = ctx.tsetmc.get_body(&ctx.tsetmc.endpoints.instrument_info(row.ins_code), policy).await?;
        let instrument = parser::parse_instrument_info(&info_body, row.ins_code)?;
        let threshold = parser::parse_static_threshold(&info_body)?;
        let closing = ctx.tsetmc.get_closing_price_info(row.ins_code, policy).await?;
        let best = ctx.tsetmc.get_best_limits(row.ins_code, policy).await?;
        let level1 = best.iter().find(|l| l.level == 1);
        for (i, (_, _, _, compare)) in CHECKS.iter().enumerate() {
            let (actual, expected) = compare(row, &instrument, &closing, threshold, level1);
            let same = actual == expected;
            let tally = &mut tallies[i];
            tally.0 += usize::from(same);
            tally.1 += 1;
            if !same && tally.2.is_none() {
                tally.2 = Some(format!("{}: mw={} ref={}", row.symbol, show(&actual), show(&expected)));
            }
        }
    }

    let during = !matches!(started.weekday(), Weekday::Thu | Weekday::Fri)
        && started.time() >= chrono::NaiveTime::from_hms_opt(8, 45, 0).unwrap_or_default()
        && started.time() <= chrono::NaiveTime::from_hms_opt(13, 15, 0).unwrap_or_default();
    table(
        &["field", "mw key", "reference", "match"],
        CHECKS
            .iter()
            .zip(&tallies)
            .map(|((field, key, reference, _), t)| vec![(*field).into(), (*key).into(), (*reference).into(), format!("{}/{}", t.0, t.1).into()])
            .collect(),
    );

    let mut report = String::from("# Market-watch compact-key verification\n\n");
    report.push_str(&format!("- Run: {} Iran time, node `{}`\n", started.format("%Y-%m-%d %H:%M"), ctx.options.node));
    report.push_str(&format!(
        "- Sample: top {} traded shares by value: {}\n",
        sample.len(),
        sample.iter().map(|s| s.symbol.as_str()).collect::<Vec<_>>().join("، ")
    ));
    report.push_str(&format!(
        "- Market session in progress during the run: {}\n\n",
        if during { "yes — live values move between calls, so small mismatches are expected" } else { "no — values are static" }
    ));
    report.push_str("| Field | Market-watch key | Reference | Match | First mismatch |\n|---|---|---|---|---|\n");
    for ((field, key, reference, _), t) in CHECKS.iter().zip(&tallies) {
        report.push_str(&format!("| {field} | `{key}` | `{reference}` | {}/{} | {} |\n", t.0, t.1, t.2.as_deref().unwrap_or("")));
    }
    let directory = ctx.options.data_root.join("reports");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("mw-keys-{}.md", started.format("%Y%m%d-%H%M")));
    std::fs::write(&path, report)?;
    println!();
    println!("Report: {}", std::path::absolute(&path)?.display());
    Ok(if tallies.iter().all(|t| t.0 == t.1) { 0 } else { 5 })
}

async fn adjustments(ctx: &MarketContext, policy: CachePolicy, target: &str) -> anyhow::Result<u8> {
    let ins_code = match target.trim().parse::<i64>() {
        Ok(ins) => ins,
        Err(_) => ctx.tsetmc.resolve_symbol(target, policy).await?.map(|h| h.ins_code).ok_or_else(|| Error::Argument(format!("Unknown symbol '{target}'.")))?,
    };
    let bars = ctx.tsetmc.get_daily_history(ins_code, policy).await?;
    let published = ctx.tsetmc.get_price_adjustments(ins_code, policy).await?;
    let implied = implied_adjustments_default(&bars);
    let round4 = |d: Decimal| d.round_dp_with_strategy(4, rust_decimal::RoundingStrategy::MidpointNearestEven);
    let published_factors: HashSet<Decimal> = published.iter().map(|p| round4(p.factor())).collect();

    table(
        &["implied date", "factor", "in published list?"],
        implied
            .iter()
            .map(|i| {
                let status = if published.iter().any(|p| p.date == i.date) {
                    "same date"
                } else if published_factors.contains(&round4(i.factor())) {
                    "different date"
                } else {
                    "MISSING"
                };
                vec![i.date.into(), round4(i.factor()).into(), status.into()]
            })
            .collect(),
    );

    let Some(last) = bars.last() else {
        println!();
        println!("{} implied events vs {} published. No bars.", implied.len(), published.len());
        return Ok(0);
    };
    let pairs: Vec<(DailyBar, Option<boors_market::domain::DayStatus>)> = bars.iter().map(|b| (b.clone(), None)).collect();
    let adjusted = adjust_from_bars(&pairs, last.date);
    let mut compounded = Decimal::ONE;
    let mut worst = Decimal::ZERO;
    for i in 1..adjusted.len() {
        if let Some(r) = adjusted[i].daily_return {
            compounded = compounded.checked_mul(Decimal::ONE + r).unwrap_or(compounded);
        }
        let expected = div(adjusted[i].adjusted_close, adjusted[0].adjusted_close);
        if adjusted[i].daily_return.is_some() && !expected.is_zero() {
            worst = worst.max((div(compounded, expected) - Decimal::ONE).abs());
        }
    }
    println!();
    println!(
        "{} implied events vs {} published. Worst gap between compounded returns and the adjusted path: {}%.",
        implied.len(),
        published.len(),
        n(worst * Decimal::ONE_HUNDRED, 4)
    );
    Ok(0)
}

async fn contracts_check(ctx: &MarketContext, update: bool, samples: usize, baseline: Option<PathBuf>) -> anyhow::Result<u8> {
    let baseline_path = baseline.unwrap_or_else(|| PathBuf::from("crates/boors-market/data/baseline.json"));
    let baseline: BTreeMap<String, Shape> = contracts::load_baseline(&baseline_path)?;
    let samples = if update { samples.max(1) } else { 1 };
    let mut current: BTreeMap<String, Shape> = BTreeMap::new();

    for (name, client, request) in contracts::all(ctx) {
        let outcome: boors_market::Result<()> = async {
            for _ in 0..samples {
                let shape = contracts::describe(&client.get_body(&request, CachePolicy::Network).await?)?;
                let merged = match current.get(name) {
                    Some(earlier) => contracts::merge(Some(earlier), &shape),
                    None => shape,
                };
                current.insert(name.to_string(), merged);
            }
            // Some endpoints alternate between response variants, so re-sample before calling a contract broken.
            if !update && let Some(expected) = baseline.get(name) {
                for _ in 0..3 {
                    if !contracts::compare(name, expected, &current[name]).is_breaking() {
                        break;
                    }
                    let again = contracts::describe(&client.get_body(&request, CachePolicy::Network).await?)?;
                    if !contracts::compare(name, expected, &again).is_breaking() {
                        current.insert(name.to_string(), again);
                    }
                }
            }
            Ok(())
        }
        .await;
        if let Err(e) = outcome {
            match e {
                Error::SourceRequest(_) | Error::Http(_) | Error::Rejected(_) | Error::Format(_) => println!("FAIL     {name}: {e}"),
                other => return Err(other.into()),
            }
        }
    }

    if update {
        let mut merged: BTreeMap<String, Shape> = BTreeMap::new();
        for name in baseline.keys().chain(current.keys()) {
            let shape = match current.get(name) {
                Some(sample) => contracts::merge(baseline.get(name), sample),
                None => baseline[name].clone(),
            };
            merged.insert(name.clone(), contracts::with_known_optionals(name, shape));
        }
        contracts::save_baseline(&baseline_path, &merged)?;
        println!("Baseline written: {} ({} contracts).", std::path::absolute(&baseline_path)?.display(), merged.len());
        return Ok(0);
    }

    let mut breaking = 0;
    let names: Vec<&String> = {
        let mut set: Vec<&String> = baseline.keys().chain(current.keys()).collect();
        set.sort();
        set.dedup();
        set
    };
    for name in names {
        let Some(now) = current.get(name) else { continue };
        let Some(before) = baseline.get(name) else {
            println!("NEW      {name} (not in baseline)");
            continue;
        };
        let diff = contracts::compare(name, before, now);
        breaking += usize::from(diff.is_breaking());
        let label = if diff.is_breaking() {
            "BREAKING"
        } else if !diff.added.is_empty() {
            "ADDED"
        } else {
            "ok"
        };
        println!("{} {name}", pad_right(label, 8));
        for m in &diff.missing {
            println!("           missing  {m}");
        }
        for k in &diff.kind_changed {
            println!("           changed  {k}");
        }
        for a in diff.added.iter().take(10) {
            println!("           added    {a}");
        }
    }
    Ok(if breaking == 0 && current.len() == baseline.len() { 0 } else { 8 })
}

async fn reconcile(ctx: &MarketContext, count: usize, days: usize) -> anyhow::Result<u8> {
    let (_, rows) = ctx.store.run_query("SELECT max(d) FROM daily_bar", 1)?;
    let latest = rows
        .first()
        .and_then(|r| r.first())
        .and_then(|c| c.as_date())
        .ok_or_else(|| Error::InvalidOperation("The store has no bars. Run 'boors collect all' first.".into()))?;
    let (_, sample) = ctx.store.run_query(
        &format!(
            "SELECT b.ins_code, i.symbol FROM daily_bar b JOIN instrument i USING (ins_code)
             WHERE b.d = DATE '{latest}' AND i.instrument_class IN ('tse_stock', 'ifb_stock') AND i.parent_ins_code IS NULL AND b.trade_count > 0
             ORDER BY b.value DESC LIMIT {count}"
        ),
        count,
    )?;
    let sample: Vec<(i64, String)> = sample.iter().filter_map(|r| Some((r.first()?.as_i64()?, r.get(1)?.as_str()?.to_string()))).collect();

    let mut checks: Vec<Vec<V>> = Vec::new();
    let (mut mismatches, mut compared) = (0, 0);
    for (ins_code, symbol) in &sample {
        let stored: HashMap<_, _> = ctx.store.get_daily_bars(*ins_code, None)?.into_iter().map(|b| (b.bar.date, b)).collect();
        let live = ctx.tsetmc.get_closing_price_info(*ins_code, CachePolicy::Network).await?;
        let history = ctx.tsetmc.get_daily_history(*ins_code, CachePolicy::Network).await?;

        let known: Vec<&DailyBar> = history.iter().filter(|h| stored.contains_key(&h.date)).collect();
        let mut references: Vec<(DailyBar, &str)> = known[known.len().saturating_sub(days)..].iter().map(|b| ((*b).clone(), "history")).collect();
        if let Some(live_date) = live.date
            && stored.contains_key(&live_date)
            && references.iter().all(|r| r.0.date != live_date)
        {
            references.push((
                DailyBar {
                    ins_code: *ins_code,
                    date: live_date,
                    close: live.close,
                    last: live.last,
                    low: live.low,
                    high: live.high,
                    open: live.open,
                    yesterday: live.yesterday,
                    trade_count: live.trade_count,
                    volume: live.volume,
                    value: live.value,
                    last_trade_time: None,
                },
                "closing-price-info",
            ));
        }

        for (reference, source) in references {
            let mine = &stored[&reference.date];
            let mut differences = Vec::new();
            if mine.bar.close != reference.close {
                differences.push(format!("close {} vs {}", mine.bar.close, reference.close));
            }
            if mine.bar.yesterday != reference.yesterday {
                differences.push(format!("yesterday {} vs {}", mine.bar.yesterday, reference.yesterday));
            }
            if mine.bar.volume != reference.volume {
                differences.push(format!("volume {} vs {}", mine.bar.volume, reference.volume));
            }
            if mine.bar.value != reference.value {
                differences.push(format!("value {} vs {}", mine.bar.value, reference.value));
            }
            if mine.bar.trade_count != reference.trade_count {
                differences.push(format!("trades {} vs {}", mine.bar.trade_count, reference.trade_count));
            }
            compared += 1;
            mismatches += usize::from(!differences.is_empty());
            checks.push(vec![
                symbol.as_str().into(),
                reference.date.into(),
                mine.source.as_str().into(),
                source.into(),
                (if differences.is_empty() { "match".to_string() } else { differences.join("; ") }).into(),
            ]);
        }
    }

    table(&["symbol", "date", "stored from", "checked against", "result"], checks);
    println!();
    println!("{}/{compared} symbol-days match ({} symbols, latest session {latest}).", compared - mismatches, sample.len());
    Ok(if mismatches == 0 { 0 } else { 9 })
}
