//! Phase 0 inspection commands (C# `FetchCommands`).

use std::collections::{BTreeMap, HashMap};

use boors_market::domain::{DayStatusClassifier, Instrument, MarketCalendar, detect_flat_index_days};
use boors_market::sources::CachePolicy;
use boors_market::sources::codal::CodalSearchQuery;
use boors_market::sources::tsetmc::{MarketWatchQuery, TEDPIX_INS_CODE};
use boors_market::{Date, Decimal, Error, MarketContext};
use clap::Subcommand;

use crate::output::*;
use crate::{Global, open_context};

#[derive(Subcommand, Debug)]
pub enum FetchCommand {
    /// Instrument master from the market watch; --details adds GetInstrumentInfo per instrument.
    Instruments {
        #[arg(long)]
        details: bool,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Daily history, flows, adjustments and share changes, with day-status classification.
    History { target: String },
    /// Market-watch snapshot with queue counts.
    Marketwatch,
    /// Index history (default TEDPIX) and detected market closures.
    Index { ins_code: Option<i64> },
    /// TSETMC static data (sector and paper-type codes).
    Static,
    /// Codal letter taxonomy (categories and LetterType ids).
    #[command(name = "codal-categories")]
    CodalCategories,
    /// Codal letters for a symbol.
    #[command(name = "codal-letters")]
    CodalLetters {
        symbol: String,
        #[arg(long = "type", default_value_t = -1)]
        letter_type: i32,
        #[arg(long, default_value_t = 1)]
        page: i32,
    },
}

pub async fn run(global: &Global, command: FetchCommand) -> anyhow::Result<u8> {
    let ctx = open_context(global)?;
    let policy = global.policy();
    let parsed = ctx.options.data_root.join("parsed");
    match command {
        FetchCommand::Instruments { details, limit } => instruments(&ctx, policy, details, limit, &parsed).await,
        FetchCommand::History { target } => history(&ctx, policy, &target, &parsed).await,
        FetchCommand::Marketwatch => market_watch(&ctx, policy).await,
        FetchCommand::Index { ins_code } => index(&ctx, policy, ins_code.unwrap_or(TEDPIX_INS_CODE), &parsed).await,
        FetchCommand::Static => {
            let mut items = ctx.tsetmc.get_static_data(policy).await?;
            items.sort_by(|a, b| boors_market::raw_cache::ordinal_cmp(&a.kind, &b.kind).then(a.code.cmp(&b.code)));
            table(&["type", "code", "name"], items.iter().map(|i| vec![i.kind.as_str().into(), i.code.into(), i.name.as_str().into()]).collect());
            Ok(0)
        }
        FetchCommand::CodalCategories => {
            let taxonomy = ctx.codal.get_taxonomy(policy).await?;
            for category in &taxonomy.categories {
                println!("[{}] {}", category.code, category.name);
                for letter in &category.letter_types {
                    println!("    {}  {}", pad_left(&letter.id.to_string(), 5), letter.name);
                }
            }
            let json: Vec<serde_json::Value> = taxonomy
                .categories
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "Code": c.code,
                        "Name": c.name,
                        "LetterTypes": c.letter_types.iter().map(|l| serde_json::json!({"Id": l.id, "Name": l.name})).collect::<Vec<_>>(),
                    })
                })
                .collect();
            std::fs::create_dir_all(&parsed)?;
            let path = parsed.join("codal-taxonomy.json");
            std::fs::write(&path, serde_json::to_string_pretty(&json)?)?;
            println!();
            println!(
                "{} letter types in {} categories. Wrote {}",
                taxonomy.letter_types_by_id().len(),
                taxonomy.categories.len(),
                std::path::absolute(&path)?.display()
            );
            Ok(0)
        }
        FetchCommand::CodalLetters { symbol, letter_type, page } => {
            let result =
                ctx.codal.search(&CodalSearchQuery { symbol: Some(symbol), letter_type, page_number: page, ..CodalSearchQuery::default() }, policy).await?;
            println!("{} letters ({} pages).", result.total, result.page_count);
            table(
                &["tracing", "published", "type", "code", "group", "excel", "title"],
                result
                    .letters
                    .iter()
                    .map(|l| {
                        vec![
                            l.tracing_no.into(),
                            l.published_at.map(|p| p.format("%Y-%m-%d %H:%M:%S%:z").to_string()).into(),
                            l.letter_type_id.into(),
                            l.letter_code.clone().into(),
                            l.disclosure_group.clone().into(),
                            (if l.has_excel { "y" } else { "" }).into(),
                            short(Some(&l.title), 70).into(),
                        ]
                    })
                    .collect(),
            );
            Ok(0)
        }
    }
}

async fn resolve(ctx: &MarketContext, target: &str, policy: CachePolicy) -> anyhow::Result<Option<i64>> {
    if let Ok(ins) = target.trim().parse::<i64>() {
        return Ok(Some(ins));
    }
    Ok(ctx.tsetmc.resolve_symbol(target, policy).await?.map(|h| h.ins_code))
}

async fn instruments(ctx: &MarketContext, policy: CachePolicy, details: bool, limit: Option<usize>, parsed: &std::path::Path) -> anyhow::Result<u8> {
    let snapshot = ctx.tsetmc.get_market_watch(&MarketWatchQuery::default(), policy).await?;
    let mut rows = snapshot.rows.clone();
    rows.sort_by(|a, b| boors_market::raw_cache::ordinal_cmp(&a.symbol, &b.symbol));
    println!("Market watch: {} instruments.", rows.len());

    let mut by_class: BTreeMap<&str, usize> = BTreeMap::new();
    for row in &rows {
        *by_class.entry(row.class().rule_key()).or_default() += 1;
    }
    let mut counts: Vec<(&str, usize)> = by_class.into_iter().collect();
    counts.sort_by_key(|c| std::cmp::Reverse(c.1));
    table(&["class (by ISIN)", "count"], counts.iter().map(|(k, c)| vec![(*k).into(), (*c).into()]).collect());

    let path = parsed.join("instruments.csv");
    write_csv(
        &path,
        &["ins_code", "symbol", "name", "isin", "sector", "class_by_isin", "close", "value"],
        rows.iter()
            .map(|r| {
                vec![
                    r.ins_code.into(),
                    r.symbol.as_str().into(),
                    r.name.clone().into(),
                    r.isin.clone().into(),
                    r.sector_code.clone().into(),
                    r.class().rule_key().into(),
                    r.close.into(),
                    r.value.into(),
                ]
            })
            .collect(),
    )?;
    println!("Wrote {}", std::path::absolute(&path)?.display());
    if !details {
        return Ok(0);
    }

    let limit = limit.unwrap_or(rows.len());
    let mut detailed: Vec<Instrument> = Vec::new();
    let mut failures = 0;
    for (processed, row) in rows.iter().take(limit).enumerate() {
        if (processed + 1) % 100 == 0 {
            println!("  {}/{limit} instruments...", processed + 1);
        }
        match ctx.tsetmc.get_instrument_info(row.ins_code, policy.prefer()).await {
            Ok(instrument) => detailed.push(instrument),
            Err(e @ (Error::SourceRequest(_) | Error::Rejected(_) | Error::Http(_))) => {
                failures += 1;
                eprintln!("  {} ({}): {e}", row.symbol, row.ins_code);
            }
            Err(e) => return Err(e.into()),
        }
    }

    let detailed_path = parsed.join("instruments-detailed.csv");
    write_csv(
        &detailed_path,
        &[
            "ins_code",
            "symbol",
            "name",
            "isin",
            "company_isin",
            "y_val",
            "flow",
            "board",
            "board_title",
            "sector",
            "sector_name",
            "base_volume",
            "total_shares",
            "free_float",
            "class",
        ],
        detailed
            .iter()
            .map(|i| {
                vec![
                    i.ins_code.into(),
                    i.symbol.as_str().into(),
                    i.name.clone().into(),
                    i.isin.clone().into(),
                    i.company_isin.clone().into(),
                    i.y_val.into(),
                    i.flow.into(),
                    i.board.clone().into(),
                    i.board_title.clone().into(),
                    i.sector_code.clone().into(),
                    i.sector_name.clone().into(),
                    i.base_volume.into(),
                    i.total_shares.into(),
                    i.free_float_percent.into(),
                    i.class.rule_key().into(),
                ]
            })
            .collect(),
    )?;
    println!();
    let mut groups: BTreeMap<(Option<i32>, Option<i32>, &str), usize> = BTreeMap::new();
    for i in &detailed {
        *groups.entry((i.y_val, i.flow, i.class.rule_key())).or_default() += 1;
    }
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by_key(|g| std::cmp::Reverse(g.1));
    table(&["y_val", "flow", "class", "count"], groups.iter().map(|((y, f, c), n)| vec![(*y).into(), (*f).into(), (*c).into(), (*n).into()]).collect());
    println!("Wrote {} ({} rows, {failures} failures)", std::path::absolute(&detailed_path)?.display(), detailed.len());
    Ok(if failures == 0 { 0 } else { 4 })
}

async fn history(ctx: &MarketContext, policy: CachePolicy, target: &str, parsed: &std::path::Path) -> anyhow::Result<u8> {
    let Some(ins_code) = resolve(ctx, target, policy).await? else {
        eprintln!("No active instrument matches '{target}'.");
        return Ok(1);
    };

    let t = &ctx.tsetmc;
    let instrument = t.get_instrument_info(ins_code, policy).await?;
    let bars = t.get_daily_history(ins_code, policy).await?;
    let flows = t.get_client_type_history(ins_code, policy).await?;
    let adjustments = t.get_price_adjustments(ins_code, policy).await?;
    let share_changes = t.get_share_changes(ins_code, policy).await?;
    let index = t.get_index_history(TEDPIX_INS_CODE, policy.prefer()).await?;

    let calendar = MarketCalendar::from_rules(&ctx.rules, Some(&index));
    let class = instrument.class;
    let classifier = DayStatusClassifier::new(&calendar, |d| ctx.rules.price_limit_or_zero(class, d));
    let classified = classifier.classify(&bars, None, None);
    let flows_by_date: HashMap<Date, &boors_market::domain::ClientTypeDay> = flows.iter().map(|f| (f.date, f)).collect();

    println!(
        "{} — {} ({}, {}, yVal {}, flow {}, board {}, class {})",
        instrument.symbol,
        instrument.name.as_deref().unwrap_or(""),
        instrument.ins_code,
        instrument.isin.as_deref().unwrap_or(""),
        instrument.y_val.map(|y| y.to_string()).unwrap_or_default(),
        instrument.flow.map(|f| f.to_string()).unwrap_or_default(),
        instrument.board.as_deref().unwrap_or(""),
        instrument.class.rule_key()
    );
    println!(
        "Daily rows: {} ({} .. {}); client-type rows: {}; adjustments: {}; share changes: {}",
        bars.len(),
        bars.first().map(|b| b.date.to_string()).unwrap_or_default(),
        bars.last().map(|b| b.date.to_string()).unwrap_or_default(),
        flows.len(),
        adjustments.len(),
        share_changes.len()
    );
    println!();
    let mut status_counts: BTreeMap<boors_market::domain::DayStatus, usize> = BTreeMap::new();
    for c in &classified {
        *status_counts.entry(c.status).or_default() += 1;
    }
    table(&["day status", "rows"], status_counts.iter().map(|(s, c)| vec![s.as_str().into(), (*c).into()]).collect());
    println!();
    table(
        &["date", "status", "close", "yesterday", "low", "high", "trades", "value", "indiv. net value"],
        classified[classified.len().saturating_sub(8)..]
            .iter()
            .map(|c| {
                vec![
                    c.bar.date.into(),
                    c.status.as_str().into(),
                    c.bar.close.into(),
                    c.bar.yesterday.into(),
                    c.bar.low.into(),
                    c.bar.high.into(),
                    c.bar.trade_count.into(),
                    c.bar.value.into(),
                    flows_by_date.get(&c.bar.date).map(|f| f.individual_net_value()).into(),
                ]
            })
            .collect(),
    );

    let path = parsed.join("history").join(format!("{ins_code}.csv"));
    write_csv(
        &path,
        &[
            "date",
            "status",
            "open",
            "high",
            "low",
            "close",
            "last",
            "yesterday",
            "trades",
            "volume",
            "value",
            "buy_i_value",
            "sell_i_value",
            "buy_i_count",
            "sell_i_count",
            "buy_n_value",
            "sell_n_value",
            "buyer_power",
        ],
        classified
            .iter()
            .map(|c| {
                let f = flows_by_date.get(&c.bar.date);
                vec![
                    c.bar.date.into(),
                    c.status.as_str().into(),
                    c.bar.open.into(),
                    c.bar.high.into(),
                    c.bar.low.into(),
                    c.bar.close.into(),
                    c.bar.last.into(),
                    c.bar.yesterday.into(),
                    c.bar.trade_count.into(),
                    c.bar.volume.into(),
                    c.bar.value.into(),
                    f.map(|f| f.buy_individual_value).into(),
                    f.map(|f| f.sell_individual_value).into(),
                    f.map(|f| f.buy_individual_count).into(),
                    f.map(|f| f.sell_individual_count).into(),
                    f.map(|f| f.buy_institutional_value).into(),
                    f.map(|f| f.sell_institutional_value).into(),
                    f.and_then(|f| f.buyer_power()).into(),
                ]
            })
            .collect(),
    )?;

    let events_path = parsed.join("history").join(format!("{ins_code}-events.csv"));
    let mut events: Vec<(Date, Vec<V>)> = adjustments
        .iter()
        .map(|a| {
            (a.date, vec![a.date.into(), "price_adjustment".into(), a.adjusted_close.into(), a.unadjusted_close.into(), a.factor().into(), V::Null, V::Null])
        })
        .collect();
    events.extend(
        share_changes
            .iter()
            .map(|s| (s.date, vec![s.date.into(), "share_change".into(), V::Null, V::Null, V::Null, s.shares_before.into(), s.shares_after.into()])),
    );
    events.sort_by_key(|e| e.0);
    write_csv(
        &events_path,
        &["date", "kind", "adjusted_close", "unadjusted_close", "factor", "shares_before", "shares_after"],
        events.into_iter().map(|e| e.1).collect(),
    )?;

    println!();
    println!("Wrote {}", std::path::absolute(&path)?.display());
    println!("Wrote {}", std::path::absolute(&events_path)?.display());
    Ok(0)
}

async fn market_watch(ctx: &MarketContext, policy: CachePolicy) -> anyhow::Result<u8> {
    let snapshot = ctx.tsetmc.get_market_watch(&MarketWatchQuery::default(), policy).await?;
    let traded: Vec<_> = snapshot.rows.iter().filter(|r| r.trade_count > 0).collect();
    let hemat = Decimal::from(10_000_000_000_000_i64);

    // Queue = best bid pinned at the upper band with no offers (buy queue), or the mirror image (sell queue).
    let buy: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|r| r.best_limits.first().is_some_and(|l| r.allowed_high > Decimal::ZERO && l.bid_price == r.allowed_high && l.ask_volume == 0))
        .collect();
    let sell: Vec<_> = snapshot
        .rows
        .iter()
        .filter(|r| r.best_limits.first().is_some_and(|l| r.allowed_low > Decimal::ZERO && l.ask_price == r.allowed_low && l.bid_volume == 0))
        .collect();
    let buy_value: Decimal = buy.iter().map(|r| Decimal::from(r.best_limits[0].bid_volume) * r.best_limits[0].bid_price).sum();
    let sell_value: Decimal = sell.iter().map(|r| Decimal::from(r.best_limits[0].ask_volume) * r.best_limits[0].ask_price).sum();
    let two = |d: Decimal| d.round_dp_with_strategy(2, rust_decimal::RoundingStrategy::MidpointAwayFromZero).normalize().to_string();

    println!("Instruments: {}, traded today: {}", snapshot.rows.len(), traded.len());
    println!("Buy queues: {} ({} hemat), sell queues: {} ({} hemat)", buy.len(), two(div(buy_value, hemat)), sell.len(), two(div(sell_value, hemat)));
    println!("Max hEven (for incremental polling): {}", snapshot.max_h_even());
    println!();

    let mut by_class: BTreeMap<&str, (usize, Decimal)> = BTreeMap::new();
    for r in &traded {
        let entry = by_class.entry(r.class().rule_key()).or_default();
        entry.0 += 1;
        entry.1 += r.value;
    }
    let mut classes: Vec<_> = by_class.into_iter().collect();
    classes.sort_by_key(|c| std::cmp::Reverse(c.1.1));
    table(
        &["class (by ISIN)", "traded", "value (hemat = 10^13 rial)"],
        classes.iter().map(|(k, (c, v))| vec![(*k).into(), (*c).into(), round_dec(div(*v, hemat), 2).into()]).collect(),
    );
    println!();

    let mut top: Vec<_> = traded
        .iter()
        .filter(|r| {
            matches!(
                r.class(),
                boors_market::domain::InstrumentClass::TseStock
                    | boors_market::domain::InstrumentClass::IfbStock
                    | boors_market::domain::InstrumentClass::Rights
            )
        })
        .collect();
    top.sort_by_key(|r| std::cmp::Reverse(r.value));
    table(
        &["symbol", "class", "close", "change %", "value (B rial)", "trades"],
        top.iter()
            .take(15)
            .map(|r| {
                vec![
                    r.symbol.as_str().into(),
                    r.class().rule_key().into(),
                    r.close.into(),
                    (if r.yesterday.is_zero() { None } else { Some(round_dec((div(r.close, r.yesterday) - Decimal::ONE) * Decimal::ONE_HUNDRED, 2)) }).into(),
                    round_dec(div(r.value, Decimal::from(1_000_000_000_i64)), 1).into(),
                    r.trade_count.into(),
                ]
            })
            .collect(),
    );
    Ok(0)
}

async fn index(ctx: &MarketContext, policy: CachePolicy, ins_code: i64, parsed: &std::path::Path) -> anyhow::Result<u8> {
    let bars = ctx.tsetmc.get_index_history(ins_code, policy).await?;
    let flat = detect_flat_index_days(&bars);
    println!(
        "Index {ins_code}: {} rows ({} .. {}), last close {}",
        bars.len(),
        bars.first().map(|b| b.date.to_string()).unwrap_or_default(),
        bars.last().map(|b| b.date.to_string()).unwrap_or_default(),
        bars.last().map(|b| n(b.close, 0)).unwrap_or_default()
    );
    println!("Rows identical to the previous close (closure candidates): {}", flat.len());
    println!();

    // Group flat days that are consecutive rows of the index (not calendar days: weekends have no rows).
    let position: HashMap<Date, usize> = bars.iter().enumerate().map(|(i, b)| (b.date, i)).collect();
    let mut runs: Vec<(Date, Date, usize)> = Vec::new();
    let mut sorted = flat.clone();
    sorted.sort();
    for day in sorted {
        if let Some(last) = runs.last_mut()
            && position.get(&day).copied() == position.get(&last.1).map(|p| p + 1)
        {
            last.1 = day;
            last.2 += 1;
            continue;
        }
        runs.push((day, day, 1));
    }
    table(
        &["from", "to", "rows", "known closure in rules.yaml"],
        runs.iter()
            .filter(|r| r.2 >= 2)
            .map(|r| vec![r.0.into(), r.1.into(), r.2.into(), (if ctx.rules.is_known_closure(r.0) { "yes" } else { "NO" }).into()])
            .collect(),
    );

    let path = parsed.join("index").join(format!("{ins_code}.csv"));
    write_csv(
        &path,
        &["date", "close", "low", "high", "flat"],
        bars.iter().map(|b| vec![b.date.into(), b.close.into(), b.low.into(), b.high.into(), (if flat.contains(&b.date) { 1 } else { 0 }).into()]).collect(),
    )?;
    println!();
    println!("Wrote {}", std::path::absolute(&path)?.display());
    Ok(0)
}
