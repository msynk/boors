//! Golden-file parser tests ported from Boors.Market.Tests (TsetmcParserTests, Phase1ParserTests, CodalParserTests,
//! DayStatusClassifierTests, MarketCalendarTests, PointInTimeTests). The files are real responses captured on
//! 2026-10-17; see tests/golden/README.md.

use std::str::FromStr;

use boors_market::analytics::point_in_time::implied_adjustments_default;
use boors_market::domain::*;
use boors_market::rules::MarketRules;
use boors_market::sources::codal::{self, letter_types};
use boors_market::sources::tsetmc::{TEDPIX_INS_CODE, parser};
use boors_market::sources::{fipiran, tgju};
use boors_market::{Date, Decimal};
use chrono::{FixedOffset, NaiveTime, TimeZone};

const FOLAD: i64 = 46348559193224090;

fn golden(path: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(path)).expect("golden file")
}

fn d(y: i32, m: u32, day: u32) -> Date {
    Date::from_ymd_opt(y, m, day).unwrap()
}

fn dec(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

// ------------------------------------------------------------------------------------------------ TSETMC

#[test]
fn daily_history_is_sorted_oldest_first_and_maps_closing_price_fields() {
    let bars = parser::parse_daily_history(&golden("tsetmc/closing-price-daily-folad.json"), FOLAD).unwrap();
    assert_eq!(97, bars.len());
    assert_eq!(d(2007, 3, 11), bars[0].date);
    assert!(bars.windows(2).all(|w| w[0].date <= w[1].date));
    let last = bars.last().unwrap();
    assert_eq!(d(2026, 10, 14), last.date);
    assert_eq!(FOLAD, last.ins_code);
    assert_eq!(dec("4540"), last.close);
    assert_eq!(dec("4410"), last.yesterday);
    assert_eq!(dec("4540"), last.low);
    assert_eq!(dec("4540"), last.high);
    assert_eq!(13401, last.trade_count);
    assert_eq!(1563050927, last.volume);
    assert_eq!(dec("7096251208580"), last.value);
    assert_eq!(NaiveTime::from_hms_opt(12, 29, 54), last.last_trade_time);
}

#[test]
fn daily_history_contains_flat_zero_trade_rows_during_the_2026_closure() {
    let bars = parser::parse_daily_history(&golden("tsetmc/closing-price-daily-folad.json"), FOLAD).unwrap();
    let closure: Vec<_> = bars.iter().filter(|b| b.date >= d(2026, 3, 2) && b.date <= d(2026, 5, 18)).collect();
    assert!(!closure.is_empty());
    assert!(closure.iter().all(|b| b.trade_count == 0 && b.volume == 0));
}

#[test]
fn client_type_history_maps_individual_and_institutional_flows() {
    let days = parser::parse_client_type_history(&golden("tsetmc/client-type-history-folad.json"), FOLAD).unwrap();
    let day = days.iter().find(|x| x.date == d(2026, 10, 14)).unwrap();
    assert_eq!(1091991421, day.buy_individual_volume);
    assert_eq!(1517, day.buy_individual_count);
    assert_eq!(2887, day.sell_individual_count);
    assert_eq!(91, day.buy_institutional_count);
    assert_eq!(dec("88499622860"), day.individual_net_value());
    assert_eq!(Some(dec("4957641051340") / Decimal::from(1517)), day.per_capita_buy());
    assert!(day.buyer_power().is_some());
}

#[test]
fn price_adjustments_read_numeric_inscode_beyond_double_precision() {
    let adjustments = parser::parse_price_adjustments(&golden("tsetmc/price-adjust-folad.json"), 0).unwrap();
    let latest = adjustments.last().unwrap();
    assert_eq!(FOLAD, latest.ins_code);
    assert_eq!(d(2026, 8, 3), latest.date);
    assert_eq!(dec("2549"), latest.adjusted_close);
    assert_eq!(dec("2604"), latest.unadjusted_close);
    assert_eq!(dec("2549") / dec("2604"), latest.factor());
}

#[test]
fn share_changes_map_old_and_new_share_counts() {
    let changes = parser::parse_share_changes(&golden("tsetmc/share-change-folad.json"), FOLAD).unwrap();
    let latest = changes.last().unwrap();
    assert_eq!(d(2026, 3, 30), latest.date);
    assert_eq!(dec("1500000000000"), latest.shares_before);
    assert_eq!(dec("1935000000000"), latest.shares_after);
}

#[test]
fn closing_price_info_uses_the_requested_inscode_and_trims_state() {
    let snapshot = parser::parse_closing_price_info(&golden("tsetmc/closing-price-info-folad.json"), FOLAD).unwrap();
    assert_eq!(FOLAD, snapshot.ins_code);
    assert_eq!(Some(d(2026, 10, 17)), snapshot.date);
    assert_eq!(dec("4410"), snapshot.close);
    assert_eq!(dec("4540"), snapshot.yesterday);
    assert_eq!(17246, snapshot.trade_count);
    assert_eq!(Some("A".to_string()), snapshot.state);
    assert!(!snapshot.is_halted());
}

#[test]
fn instrument_info_maps_codes_normalizes_persian_and_classifies() {
    let bytes = golden("tsetmc/instrument-info-folad.json");
    let instrument = parser::parse_instrument_info(&bytes, FOLAD).unwrap();
    assert_eq!("فولاد", instrument.symbol);
    assert_eq!(Some("فولاد مبارکه اصفهان".to_string()), instrument.name);
    assert_eq!(Some(300), instrument.y_val);
    assert_eq!(Some(1), instrument.flow);
    assert_eq!(Some("N1".to_string()), instrument.board);
    assert_eq!(Some("27".to_string()), instrument.sector_code);
    assert_eq!(Some("IRO1FOLD0001".to_string()), instrument.isin);
    assert_eq!(Some("IRO1FOLD0009".to_string()), instrument.company_isin);
    assert_eq!(Some(dec("1935000000000")), instrument.total_shares);
    assert_eq!(Some(dec("36")), instrument.free_float_percent);
    assert_eq!(InstrumentClass::TseStock, instrument.class);
    assert_eq!(Some((dec("4410"), dec("4670"))), parser::parse_static_threshold(&bytes).unwrap());
}

#[test]
fn instrument_info_classifies_every_verified_instrument_type() {
    let cases = [
        ("zob", 71483646978964608_i64, 300, 1, "N2", InstrumentClass::TseStock),
        ("kerman", 50792786683910016, 303, 2, "Z1", InstrumentClass::IfbStock),
        ("balout-etf", 4769396408895066, 305, 1, "H1", InstrumentClass::EtfEquity),
        ("option", 33107199324373968, 320, 3, "13", InstrumentClass::Option),
        ("sukuk", 6503819370387370, 208, 3, "N4", InstrumentClass::Debt),
        ("rights", 21533807502850906, 400, 1, "N2", InstrumentClass::Rights),
    ];
    for (file, ins_code, y_val, flow, board, expected) in cases {
        let instrument = parser::parse_instrument_info(&golden(&format!("tsetmc/instrument-info-{file}.json")), ins_code).unwrap();
        assert_eq!(ins_code, instrument.ins_code, "{file}");
        assert_eq!(Some(y_val), instrument.y_val, "{file}");
        assert_eq!(Some(flow), instrument.flow, "{file}");
        assert_eq!(Some(board.to_string()), instrument.board, "{file}");
        assert_eq!(expected, instrument.class, "{file}");
    }
}

#[test]
fn instrument_identity_is_accepted_and_falls_back_to_requested_inscode() {
    let instrument = parser::parse_instrument_info(&golden("tsetmc/instrument-identity-folad.json"), FOLAD).unwrap();
    assert_eq!(FOLAD, instrument.ins_code);
    assert_eq!(Some(2710), instrument.sub_sector_code);
    assert_eq!(None, instrument.flow);
    assert_eq!(InstrumentClass::TseStock, instrument.class);
}

#[test]
fn instrument_search_marks_active_rows_and_collects_alternate_codes() {
    let hits = parser::parse_instrument_search(&golden("tsetmc/instrument-search-folad.json")).unwrap();
    let folad = hits.iter().find(|h| h.symbol == "فولاد").unwrap();
    assert_eq!(FOLAD, folad.ins_code);
    assert!(folad.is_active);
    assert_eq!(3, folad.alternate_ins_codes.len());
    assert!(folad.alternate_ins_codes.contains(&61934586397353104));
}

#[test]
fn best_limits_map_bid_and_ask_sides() {
    let levels = parser::parse_best_limits(&golden("tsetmc/best-limits-folad.json")).unwrap();
    assert_eq!(5, levels.len());
    assert_eq!(1, levels[0].level);
    assert_eq!(dec("4400"), levels[0].bid_price);
    assert_eq!(dec("4410"), levels[0].ask_price);
    assert_eq!(2232275616, levels[0].ask_volume);
    assert_eq!(9999, levels[0].ask_orders);
}

#[test]
fn index_history_maps_close_low_high() {
    let bars = parser::parse_index_history(&golden("tsetmc/index-b2-tedpix.json"), TEDPIX_INS_CODE).unwrap();
    let day = bars.iter().find(|b| b.date == d(2026, 3, 2)).unwrap();
    assert_eq!(dec("3713955.9"), day.close);
    assert_eq!(dec("3713950"), day.low);
    assert_eq!(dec("3713950"), day.high);
}

#[test]
fn static_data_trims_fixed_width_padding() {
    let items = parser::parse_static_data(&golden("tsetmc/static-data.json")).unwrap();
    let shares = items.iter().find(|i| i.kind == "PaperType" && i.code == 1).unwrap();
    assert_eq!("سهام", shares.name);
    assert_eq!(9, items.iter().filter(|i| i.kind == "PaperType").count());
}

#[test]
fn market_watch_maps_verified_compact_keys_and_best_limits() {
    let snapshot = parser::parse_market_watch(&golden("tsetmc/market-watch-sample.json")).unwrap();
    let folad = snapshot.rows.iter().find(|r| r.ins_code == FOLAD).unwrap();
    assert_eq!(4, snapshot.rows.len());
    assert_eq!("فولاد", folad.symbol);
    assert_eq!(dec("4410"), folad.close);
    assert_eq!(dec("4540"), folad.yesterday);
    assert_eq!(dec("4670"), folad.allowed_high);
    assert_eq!(dec("4410"), folad.allowed_low);
    assert_eq!(Some(dec("8.5")), folad.price_earnings);
    assert_eq!(Some(dec("518")), folad.eps);
    assert_eq!(5, folad.best_limits.len());
    assert_eq!(InstrumentClass::TseStock, folad.class());
    assert_eq!(snapshot.rows.iter().map(|r| r.h_even).max().unwrap(), snapshot.max_h_even());
}

#[test]
fn market_watch_reads_placeholder_pe_as_null() {
    let json = br#"{"marketwatch":[{"insCode":"1","lva":"x","pe":"-","eps":null,"pcl":10.0,"blDs":[]}]}"#;
    let snapshot = parser::parse_market_watch(json).unwrap();
    let row = &snapshot.rows[0];
    assert_eq!(None, row.price_earnings);
    assert_eq!(None, row.eps);
    assert_eq!(dec("10"), row.close);
}

// ------------------------------------------------------------------------------------------------ Phase 1 parsers

#[test]
fn client_type_all_maps_volumes_and_counts_and_estimates_values_from_average_price() {
    let snapshots = parser::parse_client_type_all(&golden("tsetmc/client-type-all-sample.json")).unwrap();
    let folad = snapshots.iter().find(|s| s.ins_code == FOLAD).unwrap();
    assert_eq!(31, snapshots.len());
    assert!(folad.buy_individual_count > 0);
    let day = folad.to_estimated_day(d(2026, 10, 17), dec("4410"));
    assert!(day.values_estimated);
    assert_eq!(Decimal::from(folad.buy_individual_volume) * dec("4410"), day.buy_individual_value);
}

#[test]
fn index_snapshots_map_value_high_low_and_names() {
    let indices = parser::parse_index_snapshots(&golden("tsetmc/index-b1-last-all.json")).unwrap();
    let steel = indices.iter().find(|i| i.ins_code == 32453344048876642).unwrap();
    assert_eq!(57, indices.len());
    assert_eq!("27-فلزات اساسی", steel.name);
    assert!(steel.value >= steel.low);
    assert!(steel.value <= steel.high);
}

#[test]
fn messages_are_classified_by_kind_reason_and_limit() {
    let messages = parser::parse_messages(&golden("tsetmc/messages-folad-sample.json")).unwrap();
    let by_id = |id: i64| messages.iter().find(|m| m.id == id).unwrap();
    let disclosure_halt = by_id(260551);
    assert_eq!(MessageKind::Halt, disclosure_halt.kind);
    assert_eq!(HaltReason::Disclosure, disclosure_halt.reason);
    assert!(disclosure_halt.mentioned_symbols.contains(&"فولاد1".to_string()));
    assert_eq!(HaltReason::Underlying, by_id(251499).reason);
    assert_eq!(MessageKind::Halt, by_id(255362).kind);
    assert_eq!(HaltReason::Supervisory, by_id(255362).reason);
    assert_eq!(MessageKind::Reopen, by_id(259792).kind);
    assert!(by_id(259792).reopens_without_limit);
    assert_eq!(MessageKind::Reopen, by_id(225554).kind);
    assert_eq!(MessageKind::Other, by_id(262429).kind);
    assert_eq!(6, by_id(260679).mentioned_symbols.len());
}

#[test]
fn shareholders_use_the_snapshot_date_when_the_source_sends_zero() {
    let holders = parser::parse_shareholders(&golden("tsetmc/shareholders-folad.json"), FOLAD, d(2026, 10, 17)).unwrap();
    assert_eq!(12, holders.len());
    assert!(holders.iter().all(|h| h.snapshot_date == d(2026, 10, 17)));
    assert_eq!(dec("16.709"), holders.iter().map(|h| h.percent).max().unwrap());
}

#[test]
fn instrument_state_reads_an_object_and_treats_an_empty_array_as_unknown() {
    let halted = parser::parse_instrument_state(&golden("tsetmc/instrument-state-halted.json"), FOLAD, d(2026, 5, 19)).unwrap();
    let unknown = parser::parse_instrument_state(br#"{"instrumentState":[]}"#, 1, d(2008, 12, 6)).unwrap();
    assert_eq!(Some("IS".to_string()), halted.state);
    assert!(halted.is_halted());
    assert_eq!(None, unknown.state);
    assert!(!unknown.is_halted());
}

#[test]
fn tgju_history_reads_open_low_high_close_and_strips_html() {
    let bars = tgju::parse_history(&golden("tgju/price-dollar-rl-sample.json"), tgju::series::USD_FREE_MARKET).unwrap();
    let last = bars.last().unwrap();
    assert_eq!(43, bars.len());
    assert_eq!(d(2011, 11, 26), bars[0].date);
    assert_eq!(d(2026, 10, 15), last.date);
    assert_eq!(dec("2674050"), last.open);
    assert_eq!(dec("2669000"), last.low);
    assert_eq!(dec("2679000"), last.high);
    assert_eq!(dec("2679000"), last.close);
}

#[test]
fn fipiran_funds_map_types_classes_and_tolerate_placeholder_dates() {
    let funds = fipiran::parse_fund_compare(&golden("fipiran/fund-compare-sample.json")).unwrap();
    let leveraged = funds.iter().find(|f| f.reg_no == "12288").unwrap();
    let unknown_start = funds.iter().find(|f| f.reg_no == "11276").unwrap();
    assert_eq!(Some(35831329532779850), leveraged.ins_code);
    assert_eq!(22, leveraged.fund_type);
    assert!(leveraged.is_exchange_traded());
    assert_eq!(Some(InstrumentClass::FundLeveraged), leveraged.implied_class());
    assert_eq!(Some(d(2026, 10, 12)), leveraged.date);
    assert_eq!(None, unknown_start.initiation_date);
    assert_eq!(Some(InstrumentClass::EtfFixedIncome), funds.iter().find(|f| f.fund_type == 4).unwrap().implied_class());
}

// ------------------------------------------------------------------------------------------------ Codal

#[test]
fn codal_categories_build_letter_type_lookup_without_the_all_pseudo_category() {
    let taxonomy = codal::parse_categories(&golden("codal/categories.json")).unwrap();
    assert!(taxonomy.categories.iter().all(|c| c.code >= 0));
    let by_id = taxonomy.letter_types_by_id();
    assert_eq!("گزارش فعالیت ماهانه", by_id[&letter_types::MONTHLY_ACTIVITY].name);
    assert_eq!(vec![3], taxonomy.categories_of(letter_types::MONTHLY_ACTIVITY));
    assert!(by_id.contains_key(&letter_types::FINANCIAL_STATEMENTS));
    assert!(by_id.contains_key(&letter_types::MATERIAL_DISCLOSURE));
}

#[test]
fn codal_search_parses_letter_type_jalali_timestamps_and_absolute_urls() {
    let page = codal::parse_search(&golden("codal/search-folad.json"), "https://codal.ir/").unwrap();
    let monthly = &page.letters[0];
    assert_eq!(1471, page.total);
    assert_eq!(1605314, monthly.tracing_no);
    assert_eq!(Some(letter_types::MONTHLY_ACTIVITY), monthly.letter_type_id);
    assert_eq!(Some("ن-30".to_string()), monthly.letter_code);
    let tehran = FixedOffset::east_opt(12600).unwrap();
    assert_eq!(Some(tehran.with_ymd_and_hms(2026, 9, 29, 18, 55, 34).unwrap()), monthly.published_at);
    assert!(monthly.report_url.as_deref().unwrap().starts_with("https://codal.ir/Reports/Decision.aspx?"));
    assert!(monthly.pdf_url.as_deref().unwrap().starts_with("https://codal.ir/Reports/DownloadFile.aspx?"));
    assert!(monthly.excel_url.as_deref().unwrap().starts_with("https://excel.codal.ir/service/Excel/GetAll/"));
    assert!(monthly.has_excel);
    assert!(!monthly.has_xbrl);
    let statements = page.letters.iter().find(|l| l.letter_code.as_deref() == Some("ن-10")).unwrap();
    assert_eq!(Some(letter_types::FINANCIAL_STATEMENTS), statements.letter_type_id);
}

// ------------------------------------------------------------------------------------------------ classifier, calendar, point in time

#[test]
fn real_history_flags_the_2026_closure_but_not_the_symbols_own_suspension_afterwards() {
    let rules = MarketRules::load_default().unwrap();
    let index = parser::parse_index_history(&golden("tsetmc/index-b2-tedpix.json"), TEDPIX_INS_CODE).unwrap();
    let bars = parser::parse_daily_history(&golden("tsetmc/closing-price-daily-folad.json"), FOLAD).unwrap();
    let calendar = MarketCalendar::from_rules(&rules, Some(&index));
    let classifier = DayStatusClassifier::new(&calendar, |day| rules.price_limit_or_zero(InstrumentClass::TseStock, day));
    let by_date: std::collections::HashMap<Date, DayStatus> = classifier.classify(&bars, None, None).into_iter().map(|c| (c.bar.date, c.status)).collect();

    assert!(by_date.iter().filter(|(k, _)| **k >= d(2026, 2, 28) && **k <= d(2026, 5, 18)).all(|(_, v)| *v == DayStatus::MarketClosed));
    assert_eq!(DayStatus::NoTrade, by_date[&d(2026, 5, 19)]);
    assert_eq!(DayStatus::QueueLockedUp, by_date[&d(2026, 10, 13)]);
    assert_eq!(DayStatus::QueueLockedUp, by_date[&d(2026, 10, 14)]);
}

#[test]
fn flat_index_rows_identify_both_war_closures() {
    let index = parser::parse_index_history(&golden("tsetmc/index-b2-tedpix.json"), TEDPIX_INS_CODE).unwrap();
    let flat = detect_flat_index_days(&index);
    assert!(flat.contains(&d(2025, 6, 15)));
    assert!(flat.contains(&d(2025, 6, 25)));
    assert!(flat.contains(&d(2026, 2, 28)));
    assert!(flat.contains(&d(2026, 5, 18)));
    assert!(!flat.contains(&d(2025, 6, 28)));
    assert!(!flat.contains(&d(2026, 5, 19)));
}

#[test]
fn real_history_finds_the_capital_increase_during_the_2026_closure() {
    let bars = parser::parse_daily_history(&golden("tsetmc/closing-price-daily-folad.json"), FOLAD).unwrap();
    let implied = implied_adjustments_default(&bars);
    let event = implied.iter().find(|a| a.date == d(2026, 3, 30)).unwrap();
    assert_eq!(dec("2604"), event.adjusted_close);
    assert_eq!(dec("3359"), event.unadjusted_close);
}

#[test]
fn jalali_conversion_matches_dotnet_persian_calendar_for_1300_to_1500() {
    // persian-years.tsv was exported from System.Globalization.PersianCalendar: year, Gregorian 1 Farvardin, leap.
    let text = String::from_utf8(golden("persian-years.tsv")).unwrap();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let year: i32 = fields[0].parse().unwrap();
        let start = Date::parse_from_str(fields[1].trim(), "%Y-%m-%d").unwrap();
        let leap = fields[2].trim() == "1";
        assert_eq!(Some(start), boors_market::text::jalali_to_gregorian(year, 1, 1), "year {year}");
        assert_eq!(leap, boors_market::text::is_jalali_leap_year(year), "leap {year}");
    }
}
