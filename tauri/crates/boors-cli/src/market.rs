//! Read-side commands over the market store (C# `MarketCommands`).

use std::sync::Arc;

use boors_market::analytics::{MarketWatchRequest, MoverKind, ScreenKind};
use boors_market::{Date, Decimal, Error, MarketContext};
use clap::{Args, Subcommand};

use crate::output::*;

#[derive(Args, Debug, Clone)]
pub struct Common {
    /// Session (yyyy-MM-dd); default: the latest stored session.
    #[arg(long)]
    pub date: Option<String>,
    #[arg(long, default_value_t = 15)]
    pub take: i64,
}

#[derive(Subcommand, Debug)]
pub enum MarketCommand {
    Overview {
        #[command(flatten)]
        common: Common,
    },
    Breadth {
        #[arg(long, default_value_t = 20)]
        days: i64,
        #[command(flatten)]
        common: Common,
    },
    Sectors {
        #[command(flatten)]
        common: Common,
    },
    Movers {
        /// gainers | losers | value | inflow | outflow
        kind: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    Screen {
        /// Momentum6To1 | High52WeekProximity | IndividualFlowIntensity20 | BuyerPower5
        kind: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    Metrics {
        symbol: String,
    },
    Search {
        text: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    Watch {
        /// A TSETMC filter, e.g. "(pc)>(py) && (ct).Buy_I_Volume>(ct).Sell_I_Volume".
        #[arg(long)]
        filter: Option<String>,
        #[arg(long)]
        traded: bool,
        #[command(flatten)]
        common: Common,
    },
}

/// همت = 1,000 billion toman = 10^13 rial.
fn hemat() -> Decimal {
    Decimal::from(10_000_000_000_000_i64)
}

fn billion() -> Decimal {
    Decimal::from(1_000_000_000_i64)
}

fn ten_million() -> Decimal {
    Decimal::from(10_000_000_i64)
}

fn session(common: &Common) -> Option<Date> {
    common.date.as_deref().and_then(|d| Date::parse_from_str(d, "%Y-%m-%d").ok())
}

pub fn run(ctx: &Arc<MarketContext>, command: MarketCommand) -> anyhow::Result<u8> {
    let analytics = &ctx.analytics;
    match command {
        MarketCommand::Overview { common } => overview(ctx, session(&common)),
        MarketCommand::Breadth { days, .. } => {
            table(
                &["date", "adv", "dec", "unch", "no trade", "halted", "locked up", "locked down", "value (hemat)"],
                analytics
                    .get_breadth(days)?
                    .iter()
                    .map(|b| {
                        vec![
                            b.date.into(),
                            b.advancers.into(),
                            b.decliners.into(),
                            b.unchanged.into(),
                            b.not_traded.into(),
                            b.halted.into(),
                            b.locked_up.into(),
                            b.locked_down.into(),
                            round_dec(div(b.equity_value, hemat()), 1).into(),
                        ]
                    })
                    .collect(),
            );
            Ok(0)
        }
        MarketCommand::Sectors { common } => {
            table(
                &["sector", "name", "members", "traded", "value (hemat)", "weighted %", "median %", "indiv. net (hemat)", "adv", "dec"],
                analytics
                    .get_sectors(session(&common))?
                    .iter()
                    .take(common.take.max(0) as usize)
                    .map(|s| {
                        vec![
                            s.sector_code.as_str().into(),
                            short(s.sector_name.as_deref(), 28).into(),
                            s.instruments.into(),
                            s.traded.into(),
                            round_dec(div(s.value, hemat()), 2).into(),
                            pct(s.weighted_return).into(),
                            pct(s.median_return).into(),
                            round_dec(div(s.individual_net_flow, hemat()), 2).into(),
                            s.advancers.into(),
                            s.decliners.into(),
                        ]
                    })
                    .collect(),
            );
            Ok(0)
        }
        MarketCommand::Movers { kind, common } => {
            let kind = kind.as_deref().and_then(MoverKind::parse).unwrap_or(MoverKind::Value);
            table(
                &["symbol", "status", "close", "change %", "value (B rial)", "indiv. net (B rial)"],
                analytics
                    .get_movers(kind, common.take, session(&common))?
                    .iter()
                    .map(|m| {
                        vec![
                            m.symbol.as_str().into(),
                            m.day_status.clone().into(),
                            m.close.into(),
                            round2(m.change_percent).into(),
                            round_dec(div(m.value, billion()), 1).into(),
                            m.individual_net_flow.map(|n| round_dec(div(n, billion()), 1)).into(),
                        ]
                    })
                    .collect(),
            );
            Ok(0)
        }
        MarketCommand::Screen { kind, common } => {
            let kind = kind.as_deref().and_then(ScreenKind::parse).unwrap_or(ScreenKind::Momentum6To1);
            table(
                &["rank", "symbol", "class", "sector", "score", "days"],
                analytics
                    .screen(kind, common.take.max(0) as usize, session(&common))?
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        vec![
                            (i + 1).into(),
                            r.symbol.as_str().into(),
                            r.instrument_class.as_str().into(),
                            r.sector_code.clone().into(),
                            round_f64(r.score, 4).into(),
                            r.observations.into(),
                        ]
                    })
                    .collect(),
            );
            Ok(0)
        }
        MarketCommand::Metrics { symbol } => metrics(ctx, &symbol),
        MarketCommand::Search { text, common } => {
            table(
                &["ins_code", "symbol", "name", "class", "parent"],
                analytics
                    .search(text.as_deref().unwrap_or(""), common.take.max(0) as usize)?
                    .iter()
                    .map(|i| {
                        vec![
                            i.ins_code.into(),
                            i.symbol.as_str().into(),
                            short(i.name.as_deref(), 40).into(),
                            i.class.rule_key().into(),
                            i.parent_ins_code.into(),
                        ]
                    })
                    .collect(),
            );
            Ok(0)
        }
        MarketCommand::Watch { filter, traded, common } => watch(ctx, filter, traded, &common),
    }
}

fn overview(ctx: &Arc<MarketContext>, session: Option<Date>) -> anyhow::Result<u8> {
    let Some(o) = ctx.analytics.get_overview(session)? else {
        println!("No sessions in the store yet. Run 'boors collect all'.");
        return Ok(1);
    };

    println!("Session {}", o.session);
    for index in &o.indices {
        println!("  {} {}  {}%", pad_right(&index.name, 28), pad_left(&n(index.value, 0), 16), pad_left(&g(round2(index.change_percent)), 7));
    }
    if let Some(usd_change) = o.tedpix_usd_change_percent {
        println!("  {} {}  {}%", pad_right("TEDPIX in free-market USD", 28), pad_left("", 16), pad_left(&g(Some(round_f64(usd_change, 2))), 7));
    }
    println!();
    println!("  Equity value   {} hemat   trades {}", pad_left(&n(div(o.equity_value, hemat()), 2), 10), n(Decimal::from(o.equity_trades), 0));
    println!("  Fund value     {} hemat", pad_left(&n(div(o.fund_value, hemat()), 2), 10));
    println!("  Breadth        +{} / -{} / ={}   no trade {}, halted {}", o.advancers, o.decliners, o.unchanged, o.not_traded, o.halted);
    let queue_text = |value: Option<Decimal>| match value {
        Some(v) => format!("{} hemat", n(div(v, hemat()), 2)),
        None => "value n/a (no close snapshot)".to_string(),
    };
    println!("  Buy queues     {}   {}", pad_left(&o.buy_queues.to_string(), 5), queue_text(o.buy_queue_value));
    println!("  Sell queues    {}   {}", pad_left(&o.sell_queues.to_string(), 5), queue_text(o.sell_queue_value));
    println!(
        "  Individual net {} hemat{}   per-capita buy {} / sell {} million toman",
        pad_left(&n(div(o.individual_net_flow, hemat()), 2), 10),
        if o.flow_values_estimated { " (includes estimates)" } else { "" },
        n_opt(o.individual_per_capita_buy.map(|v| div(v, ten_million())), 0),
        n_opt(o.individual_per_capita_sell.map(|v| div(v, ten_million())), 0)
    );
    if let Some(usd) = &o.usd {
        println!("  USD (free)     {} rial  {}%  ({})", pad_left(&n(usd.value, 0), 14), pad_left(&g(round2(usd.change_percent)), 6), usd.date);
    }
    if let Some(coin) = &o.coin {
        println!("  Emami coin     {} rial  {}%  ({})", pad_left(&n(coin.value, 0), 14), pad_left(&g(round2(coin.change_percent)), 6), coin.date);
    }
    Ok(0)
}

fn metrics(ctx: &Arc<MarketContext>, symbol: &str) -> anyhow::Result<u8> {
    let instrument = ctx.analytics.search(symbol, 1)?.into_iter().next();
    let metrics = match instrument {
        Some(i) => ctx.analytics.get_instrument_metrics(i.ins_code)?,
        None => None,
    };
    let Some(m) = metrics else {
        eprintln!("Unknown symbol '{symbol}'.");
        return Ok(1);
    };

    println!(
        "{} — {} ({}, sector {} {})",
        m.symbol,
        m.name.as_deref().unwrap_or(""),
        m.instrument_class,
        m.sector_code.as_deref().unwrap_or(""),
        m.sector_name.as_deref().unwrap_or("")
    );
    println!(
        "  As of {} [{}]  close {}  {}%  market cap {} hemat",
        m.as_of.map(|d| d.to_string()).unwrap_or_default(),
        m.day_status.as_deref().unwrap_or(""),
        n_opt(m.close, 0),
        g(round2(m.change_percent)),
        n_opt(m.market_cap.map(|c| div(c, hemat())), 1)
    );
    println!(
        "  52w high {}  low {}  from high {}%  (adjusted closes)",
        n_opt(m.high_52_week, 0),
        n_opt(m.low_52_week, 0),
        g(round2(m.from_high_52_week_percent))
    );
    println!(
        "  Returns  {}   1y in USD {}%",
        m.returns.iter().map(|(k, v)| format!("{k} {}%", g(pct(*v)))).collect::<Vec<_>>().join("  "),
        g(pct(m.usd_return_1_year))
    );
    println!(
        "  Value 20d avg {} B rial   individual net 20d {} B rial",
        n_opt(m.average_value_20.map(|v| div(v, billion())), 1),
        n_opt(m.individual_net_flow_20.map(|v| div(v, billion())), 1)
    );
    println!(
        "  Per-capita buy {} / sell {} million toman   buyer power {} (percentile {})",
        n_opt(m.per_capita_buy.map(|v| div(v, ten_million())), 0),
        n_opt(m.per_capita_sell.map(|v| div(v, ten_million())), 0),
        n_opt(m.buyer_power, 2),
        g(pct(m.buyer_power_percentile))
    );
    Ok(0)
}

fn watch(ctx: &Arc<MarketContext>, filter: Option<String>, traded: bool, common: &Common) -> anyhow::Result<u8> {
    let started = std::time::Instant::now();
    let result = match ctx.market_watch.query(&MarketWatchRequest { session: session(common), filter, classes: None, traded_only: traded }, None) {
        Ok(result) => result,
        Err(Error::FilterSyntax(message)) => {
            eprintln!("Filter error: {message}");
            return Ok(1);
        }
        Err(e) => return Err(e.into()),
    };
    let elapsed = started.elapsed();

    table(
        &["symbol", "class", "close", "change %", "value (B rial)", "queue", "P/E", "buyer power", "indiv. buy share"],
        result
            .entries
            .iter()
            .take(common.take.max(0) as usize)
            .map(|e| {
                vec![
                    e.symbol.as_str().into(),
                    e.instrument_class.as_str().into(),
                    e.close.into(),
                    round2(e.change_percent).into(),
                    round_dec(div(e.value, billion()), 1).into(),
                    e.queue.clone().into(),
                    e.price_earnings.into(),
                    e.buyer_power.map(|p| round_dec(p, 2)).into(),
                    pct(e.individual_buy_share).into(),
                ]
            })
            .collect(),
    );
    println!(
        "{} instrument(s), session {}{}.",
        result.entries.len(),
        result.session.map(|d| d.to_string()).unwrap_or_default(),
        result.filter.as_ref().map(|f| format!(", filter evaluated on {} rows", f.evaluated)).unwrap_or_default()
    );
    for error in result.filter.as_ref().map(|f| f.errors.as_slice()).unwrap_or(&[]) {
        println!("  filter error: {error}");
    }
    tracing::info!(elapsed_ms = elapsed.as_millis() as u64, "market watch");
    if std::env::var_os("BOORS_TIMING").is_some() {
        eprintln!("market watch: {} ms", elapsed.as_millis());
    }
    Ok(0)
}
