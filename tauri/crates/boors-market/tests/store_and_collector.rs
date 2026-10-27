//! Store, collector and analytics tests ported from Boors.Market.Tests (MarketStoreTests, CollectorTests,
//! MarketAnalyticsTests). Everything runs offline on in-memory stores and temporary raw caches.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;

use boors_market::analytics::{MarketAnalytics, MarketWatchRequest, MarketWatchService, MoverKind};
use boors_market::domain::*;
use boors_market::raw_cache::RawRequest;
use boors_market::sources::CachePolicy;
use boors_market::sources::fipiran::FundRecord;
use boors_market::sources::tgju::{MacroBar, series};
use boors_market::sources::tsetmc::{MarketWatchQuery, TEDPIX_INS_CODE};
use boors_market::storage::{CollectionRun, MarketStore};
use boors_market::{Date, Decimal, MarketContext, MarketDataOptions};
use chrono::{TimeZone, Utc};

const FOLAD: i64 = 46348559193224090;

fn golden(path: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(path)).expect("golden file")
}

fn d(y: i32, m: u32, day: u32) -> Date {
    Date::from_ymd_opt(y, m, day).unwrap()
}

fn n(v: i64) -> Decimal {
    Decimal::from(v)
}

fn day() -> Date {
    d(2026, 10, 17)
}

fn stock(ins_code: i64, symbol: &str, isin: &str, class: InstrumentClass, name: Option<&str>) -> Instrument {
    Instrument { ins_code, symbol: symbol.into(), isin: Some(isin.into()), name: name.map(str::to_string), class, ..Instrument::default() }
}

fn bar(ins_code: i64, close: i64, trades: i64) -> DailyBar {
    DailyBar {
        ins_code,
        date: day(),
        close: n(close),
        yesterday: n(close),
        low: n(close),
        high: n(close),
        open: n(close),
        last: n(close),
        trade_count: trades,
        volume: trades * 100,
        value: n(trades * 100 * close),
        last_trade_time: None,
    }
}

// ------------------------------------------------------------------------------------------------ store

#[test]
fn instrument_merge_keeps_known_fields_and_respects_class_source_rank() {
    let store = MarketStore::open_in_memory().unwrap();
    store.upsert_instruments(&[stock(1, "الف", "IRT1ALEF0001", InstrumentClass::EtfEquity, Some("صندوق الف"))], day(), "isin", None).unwrap();
    store
        .apply_fund_types(&[FundRecord {
            reg_no: "1".into(),
            ins_code: Some(1),
            name: "صندوق الف".into(),
            fund_type: 4,
            type_of_invest: Some("Negotiable".into()),
            ..FundRecord::default()
        }])
        .unwrap();
    store.upsert_instruments(&[stock(1, "الف", "IRT1ALEF0001", InstrumentClass::EtfEquity, None)], day().succ_opt().unwrap(), "isin", None).unwrap();
    let instrument = store.get_instrument(1).unwrap().unwrap();
    assert_eq!(Some("صندوق الف".to_string()), instrument.name);
    assert_eq!(InstrumentClass::EtfFixedIncome, instrument.class);
}

#[test]
fn secondary_listings_are_linked_to_the_primary_isin() {
    let store = MarketStore::open_in_memory().unwrap();
    store
        .upsert_instruments(
            &[
                stock(10, "آباد", "IRO1ABAD0001", InstrumentClass::TseStock, None),
                stock(11, "آباد3", "IRO1ABAD0003", InstrumentClass::TseStock, None),
                stock(12, "تنها", "IRO1TANH0003", InstrumentClass::TseStock, None),
            ],
            day(),
            "isin",
            None,
        )
        .unwrap();
    assert_eq!(Some(10), store.get_instrument(11).unwrap().unwrap().parent_ins_code);
    assert_eq!(None, store.get_instrument(10).unwrap().unwrap().parent_ins_code);
    assert_eq!(None, store.get_instrument(12).unwrap().unwrap().parent_ins_code);
}

#[test]
fn a_marketwatch_bar_never_replaces_a_history_bar_but_history_replaces_marketwatch() {
    let store = MarketStore::open_in_memory().unwrap();
    store.upsert_daily_bars(&[ClassifiedBar::new(bar(1, 100, 10), DayStatus::Traded)], "history", None).unwrap();
    store.upsert_daily_bars(&[ClassifiedBar::new(bar(1, 999, 10), DayStatus::Traded)], "marketwatch", None).unwrap();
    assert_eq!(n(100), store.get_daily_bars(1, None).unwrap()[0].bar.close);

    store.upsert_daily_bars(&[ClassifiedBar::new(bar(2, 50, 10), DayStatus::Traded)], "marketwatch", None).unwrap();
    store.upsert_daily_bars(&[ClassifiedBar::new(bar(2, 51, 10), DayStatus::QueueLockedUp)], "history", None).unwrap();
    let replaced = &store.get_daily_bars(2, None).unwrap()[0];
    assert_eq!(n(51), replaced.bar.close);
    assert_eq!(Some(DayStatus::QueueLockedUp), replaced.status);
    assert_eq!("history", replaced.source);
}

#[test]
fn estimated_flows_never_overwrite_official_flows() {
    let store = MarketStore::open_in_memory().unwrap();
    let official = ClientTypeDay { ins_code: 1, date: day(), buy_individual_value: n(1000), buy_individual_count: 5, ..ClientTypeDay::default() };
    store.upsert_client_types(std::slice::from_ref(&official)).unwrap();
    store.upsert_client_types(&[ClientTypeDay { buy_individual_value: n(7777), values_estimated: true, ..official }]).unwrap();
    let stored = &store.get_client_types(1).unwrap()[0];
    assert_eq!(n(1000), stored.buy_individual_value);
    assert!(!stored.values_estimated);
}

#[test]
fn messages_link_numbered_sibling_symbols_to_the_parent() {
    let store = MarketStore::open_in_memory().unwrap();
    let message = SupervisorMessage {
        id: 1,
        date: day(),
        title: "توقف".into(),
        kind: MessageKind::Halt,
        reason: HaltReason::Disclosure,
        mentioned_symbols: vec!["فولاد1".into()],
        ..SupervisorMessage::default()
    };
    store.upsert_messages(&[message], &HashMap::from([("فولاد".to_string(), FOLAD)])).unwrap();
    assert_eq!(1, store.get_messages(FOLAD).unwrap()[0].id);
}

#[test]
fn freshness_and_runs_report_latest_dates_and_statuses() {
    let store = MarketStore::open_in_memory().unwrap();
    store.upsert_daily_bars(&[ClassifiedBar::new(bar(1, 100, 10), DayStatus::Traded)], "history", None).unwrap();
    let epoch = Utc.timestamp_opt(0, 0).unwrap();
    let run = CollectionRun {
        run_id: "r1".into(),
        job: "eod".into(),
        node: "n".into(),
        started_at: epoch,
        finished_at: None,
        status: "running".into(),
        items: 0,
        failures: 0,
        message: None,
    };
    store.record_run(&run).unwrap();
    store.record_run(&CollectionRun { finished_at: Some(epoch + chrono::Duration::minutes(1)), status: "ok".into(), items: 5, ..run }).unwrap();
    assert_eq!(Some(day()), store.get_freshness().unwrap().iter().find(|f| f.dataset == "daily_bar").unwrap().latest_date);
    let runs = store.get_latest_runs().unwrap();
    assert_eq!(1, runs.len());
    assert_eq!("ok", runs[0].status);
}

#[test]
fn a_store_with_another_schema_version_refuses_to_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("store.duckdb").to_string_lossy().into_owned();
    {
        let created = MarketStore::open(&path).unwrap();
        created.execute_batch("UPDATE schema_info SET version = 0").unwrap();
    }
    assert!(matches!(MarketStore::open(&path), Err(boors_market::Error::InvalidOperation(_))));
}

// ------------------------------------------------------------------------------------------------ collector (golden raw cache, cache-only)

fn seeded_context() -> (tempfile::TempDir, Arc<MarketContext>) {
    let dir = tempfile::tempdir().unwrap();
    let options =
        MarketDataOptions { data_root: dir.path().to_path_buf(), store_path: Some(":memory:".into()), node: "test".into(), ..MarketDataOptions::default() };
    let ctx = MarketContext::with_store(options, Arc::new(MarketStore::open_in_memory().unwrap())).unwrap();
    let e = &ctx.tsetmc.endpoints;
    let fetched_at = Utc.with_ymd_and_hms(2026, 10, 17, 12, 0, 0).unwrap();
    let seed = |request: RawRequest, file: &str| {
        ctx.cache.store(&request, &golden(file), 200, Some("application/json"), fetched_at).unwrap();
    };
    seed(e.closing_price_daily_list(FOLAD, 0), "tsetmc/closing-price-daily-folad.json");
    seed(e.client_type_history(FOLAD), "tsetmc/client-type-history-folad.json");
    seed(e.price_adjust_list(FOLAD), "tsetmc/price-adjust-folad.json");
    seed(e.instrument_share_change(FOLAD), "tsetmc/share-change-folad.json");
    seed(e.index_history(TEDPIX_INS_CODE), "tsetmc/index-b2-tedpix.json");
    seed(e.market_watch(&MarketWatchQuery::default()), "tsetmc/market-watch-sample.json");
    seed(e.closing_price_info(FOLAD), "tsetmc/closing-price-info-folad.json");
    seed(e.client_type_all(), "tsetmc/client-type-all-sample.json");
    seed(e.index_snapshots(1), "tsetmc/index-b1-last-all.json");
    ctx.store
        .upsert_instruments(
            &[Instrument {
                ins_code: FOLAD,
                symbol: "فولاد".into(),
                isin: Some("IRO1FOLD0001".into()),
                class: InstrumentClass::TseStock,
                ..Instrument::default()
            }],
            day(),
            "yval",
            None,
        )
        .unwrap();
    (dir, ctx)
}

#[tokio::test(flavor = "multi_thread")]
async fn backfill_from_cache_stores_classified_history_flows_and_both_adjustment_sources() {
    let (_dir, ctx) = seeded_context();
    let result = ctx.collector().backfill_history(10, false, false, None, CachePolicy::CacheOnly).await.unwrap();
    assert_eq!("ok", result.status, "{:?}", result.message);
    assert_eq!(1, result.items);
    let bars = ctx.store.get_daily_bars(FOLAD, None).unwrap();
    assert_eq!(97, bars.len());
    assert!(bars.iter().filter(|b| b.bar.date >= d(2026, 3, 2) && b.bar.date <= d(2026, 5, 18)).all(|b| b.status == Some(DayStatus::MarketClosed)));
    assert_eq!(Some(DayStatus::QueueLockedUp), bars.last().unwrap().status);
    assert_eq!(30, ctx.store.get_client_types(FOLAD).unwrap().len());
    assert_eq!(23, ctx.store.get_price_adjustments(FOLAD, "published").unwrap().len());
    assert!(ctx.store.get_price_adjustments(FOLAD, "implied").unwrap().iter().any(|a| a.date == d(2026, 3, 30)));
    assert_eq!("ok", ctx.store.get_latest_runs().unwrap().iter().find(|r| r.job == "backfill").unwrap().status);
}

#[tokio::test(flavor = "multi_thread")]
async fn end_of_day_appends_the_session_from_bulk_calls_and_estimates_flows() {
    let (_dir, ctx) = seeded_context();
    ctx.collector().backfill_history(10, false, false, None, CachePolicy::CacheOnly).await.unwrap();
    let result = ctx.collector().end_of_day(true, CachePolicy::CacheOnly).await.unwrap();
    assert_eq!("ok", result.status, "{:?}", result.message);
    let today = ctx.store.get_daily_bars(FOLAD, Some(day())).unwrap();
    assert_eq!(1, today.len());
    assert_eq!("marketwatch", today[0].source);
    assert_eq!(n(4410), today[0].bar.close);
    assert_eq!(Some(DayStatus::QueueLockedDown), today[0].status);
    assert!(ctx.store.get_client_types(FOLAD).unwrap().iter().find(|c| c.date == day()).unwrap().values_estimated);
    assert_eq!(4, ctx.store.get_freshness().unwrap().iter().find(|f| f.dataset == "instrument").unwrap().rows);
}

// ------------------------------------------------------------------------------------------------ analytics (hand-built store)

fn analytics_store() -> Arc<MarketStore> {
    let store = Arc::new(MarketStore::open_in_memory().unwrap());
    let d1 = d(2026, 10, 14);
    let d2 = day();
    let make = |ins: i64, symbol: &str, isin: &str, class: InstrumentClass, sector: &str, shares: i64| Instrument {
        ins_code: ins,
        symbol: symbol.into(),
        isin: Some(isin.into()),
        class,
        sector_code: Some(sector.into()),
        total_shares: Some(n(shares)),
        name: Some(format!("{symbol} Co")),
        ..Instrument::default()
    };
    store
        .upsert_instruments(
            &[
                make(1, "الف", "IRO1AAAA0001", InstrumentClass::TseStock, "27", 1000),
                make(2, "ب", "IRO1BBBB0001", InstrumentClass::TseStock, "27", 3000),
                make(3, "پ", "IRO3CCCC0001", InstrumentClass::IfbStock, "44", 1000),
                make(4, "صندوق", "IRT1DDDD0001", InstrumentClass::EtfFixedIncome, "68", 1000),
                make(5, "الف3", "IRO1AAAA0003", InstrumentClass::TseStock, "27", 1000),
            ],
            d2,
            "yval",
            None,
        )
        .unwrap();
    let b = |ins: i64, date: Date, close: i64, yesterday: i64, value: i64, status: DayStatus| {
        ClassifiedBar::new(
            DailyBar {
                ins_code: ins,
                date,
                close: n(close),
                last: n(close),
                yesterday: n(yesterday),
                open: n(yesterday),
                high: n(close.max(yesterday)),
                low: n(close.min(yesterday)),
                trade_count: 10,
                volume: 100,
                value: n(value),
                last_trade_time: None,
            },
            status,
        )
    };
    let t = DayStatus::Traded;
    store
        .upsert_daily_bars(
            &[
                b(1, d1, 100, 100, 1_000, t),
                b(2, d1, 50, 50, 2_000, t),
                b(3, d1, 20, 20, 500, t),
                b(4, d1, 10, 10, 9_000, t),
                b(5, d1, 100, 100, 0, t),
                b(1, d2, 103, 100, 3_000, DayStatus::QueueLockedUp),
                b(2, d2, 49, 50, 2_000, t),
                b(3, d2, 19, 20, 1_000, t),
                b(4, d2, 10, 10, 8_000, t),
                b(5, d2, 103, 103, 0, t),
            ],
            "history",
            None,
        )
        .unwrap();
    store
        .update_session_snapshots(&[SessionSnapshot {
            ins_code: 1,
            date: d2,
            eps: Some(n(5)),
            price_earnings: Some(n(20)),
            allowed_low: n(97),
            allowed_high: n(103),
            top: Some(BestLimitLevel { level: 1, bid_price: n(103), bid_volume: 1_000, ask_price: Decimal::ZERO, ask_volume: 0, ..BestLimitLevel::default() }),
        }])
        .unwrap();
    store
        .upsert_client_types(&[
            ClientTypeDay {
                ins_code: 1,
                date: d2,
                buy_individual_value: n(3_000),
                buy_individual_count: 3,
                sell_individual_value: n(1_000),
                sell_individual_count: 10,
                ..ClientTypeDay::default()
            },
            ClientTypeDay {
                ins_code: 2,
                date: d2,
                buy_individual_value: n(500),
                buy_individual_count: 5,
                sell_individual_value: n(2_000),
                sell_individual_count: 4,
                ..ClientTypeDay::default()
            },
        ])
        .unwrap();
    store
        .upsert_index_bars(
            &[
                IndexBar { ins_code: TEDPIX_INS_CODE, date: d1, close: n(1_000), low: n(1_000), high: n(1_000) },
                IndexBar { ins_code: TEDPIX_INS_CODE, date: d2, close: n(1_020), low: n(1_000), high: n(1_020) },
            ],
            "history",
        )
        .unwrap();
    let mb =
        |date: Date, v: i64| MacroBar { series: series::USD_FREE_MARKET.into(), date, open: n(v), low: n(v), high: n(v), close: n(v), source: "tgju".into() };
    store.upsert_macro_daily(&[mb(d1, 100), mb(d2, 102)]).unwrap();
    store
}

#[test]
fn overview_counts_primary_equities_queues_flows_and_usd_terms() {
    let analytics = MarketAnalytics::new(analytics_store());
    let o = analytics.get_overview(None).unwrap().unwrap();
    assert_eq!(day(), o.session);
    assert_eq!(n(6_000), o.equity_value);
    assert_eq!(n(8_000), o.fund_value);
    assert_eq!(1, o.advancers);
    assert_eq!(2, o.decliners);
    assert_eq!(1, o.buy_queues);
    assert_eq!(Some(n(103_000)), o.buy_queue_value);
    assert_eq!(n(500), o.individual_net_flow);
    assert_eq!(1, o.indices.len());
    assert!((o.indices[0].change_percent.unwrap() - 2.0).abs() < 1e-9);
    assert!((o.tedpix_usd_change_percent.unwrap() - (1.02 / 1.02 - 1.0) * 100.0).abs() < 1e-9);
}

#[test]
fn sectors_weight_returns_by_market_cap() {
    let analytics = MarketAnalytics::new(analytics_store());
    let sectors = analytics.get_sectors(None).unwrap();
    let metals = sectors.iter().find(|s| s.sector_code == "27").unwrap();
    assert_eq!(2, metals.instruments);
    assert!(metals.weighted_return.unwrap().abs() < 1e-9);
    assert_eq!(n(500), metals.individual_net_flow);
}

#[test]
fn movers_rank_by_the_requested_measure() {
    let analytics = MarketAnalytics::new(analytics_store());
    assert_eq!("الف", analytics.get_movers(MoverKind::Gainers, 1, None).unwrap()[0].symbol);
    assert_eq!("پ", analytics.get_movers(MoverKind::Losers, 1, None).unwrap()[0].symbol);
    assert_eq!("ب", analytics.get_movers(MoverKind::Outflow, 1, None).unwrap()[0].symbol);
}

#[test]
fn instrument_metrics_report_returns_flows_and_buyer_power() {
    let analytics = MarketAnalytics::new(analytics_store());
    let m = analytics.get_instrument_metrics(1).unwrap().unwrap();
    assert_eq!(Some(day()), m.as_of);
    assert!((m.change_percent.unwrap() - 3.0).abs() < 1e-9);
    assert_eq!(Some(n(103) * n(1000)), m.market_cap);
    assert_eq!(None, m.returns.iter().find(|r| r.0 == "1w").unwrap().1);
    assert_eq!(vec!["1w", "1m", "3m", "6m", "1y"], m.returns.iter().map(|r| r.0.as_str()).collect::<Vec<_>>());
    assert_eq!(Some(n(2_000)), m.individual_net_flow_20);
    assert_eq!(Some((n(3_000) / n(3)) / (n(1_000) / n(10))), m.buyer_power);
}

#[test]
fn search_puts_exact_symbol_first_and_primary_before_secondary() {
    let analytics = MarketAnalytics::new(analytics_store());
    let results = analytics.search("الف", 20).unwrap();
    assert_eq!(1, results[0].ins_code);
    assert_eq!(5, results[1].ins_code);
}

#[test]
fn market_watch_applies_filters_and_reports_queues() {
    let analytics = MarketAnalytics::new(analytics_store());
    let service = MarketWatchService::new(analytics);
    let all = service.query(&MarketWatchRequest::default(), None).unwrap();
    let filtered = service.query(&MarketWatchRequest { filter: Some("(pc)>(py)".into()), ..MarketWatchRequest::default() }, None).unwrap();
    assert_eq!(4, all.entries.len());
    assert_eq!(Some("buy".to_string()), all.entries.iter().find(|e| e.ins_code == 1).unwrap().queue);
    assert_eq!(vec![1], filtered.entries.iter().map(|e| e.ins_code).collect::<Vec<_>>());
    let _ = Decimal::from_str("0");
}
