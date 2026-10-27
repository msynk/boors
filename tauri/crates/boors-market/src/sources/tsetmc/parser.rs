//! Pure functions from TSETMC JSON bodies to domain records (C# `TsetmcParser`). No I/O; safe to replay over the
//! raw cache. Wire names are camelCase as System.Text.Json's camelCase policy produces them (case-sensitive).

use std::sync::LazyLock;

use regex::Regex;
use rust_decimal::prelude::ToPrimitive;
use serde::Deserialize;

use crate::domain::deven;
use crate::domain::*;
use crate::sources::json::{self as j, parse};
use crate::text::normalize;
use crate::{Date, Decimal, Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentSearchHit {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    pub flow: i32,
    pub flow_title: Option<String>,
    pub board: Option<String>,
    /// TSETMC marks the currently active instrument with lastDate = 1.
    pub is_active: bool,
    pub alternate_ins_codes: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketWatchSnapshot {
    pub rows: Vec<MarketWatchRow>,
}

impl MarketWatchSnapshot {
    /// Largest hEven in the snapshot; pass back as `hEven` to poll incrementally.
    pub fn max_h_even(&self) -> i32 {
        self.rows.iter().map(|r| r.h_even).max().unwrap_or(0)
    }
}

fn trunc_i64(value: Decimal) -> i64 {
    value.trunc().to_i64().unwrap_or(0)
}

fn trunc_i32(value: Decimal) -> i32 {
    value.trunc().to_i32().unwrap_or(0)
}

fn trim(value: Option<String>) -> Option<String> {
    value.map(|v| v.trim().to_string())
}

fn positive(value: Option<i64>) -> Option<i64> {
    value.filter(|v| *v > 0)
}

// ------------------------------------------------------------------------------------------------ closing prices

#[derive(Deserialize, Default)]
struct ClosingPriceDailyEnvelope {
    #[serde(rename = "closingPriceDaily", default, deserialize_with = "j::list")]
    closing_price_daily: Vec<ClosingPriceDto>,
}

#[derive(Deserialize, Default)]
struct ClosingPriceInfoEnvelope {
    #[serde(rename = "closingPriceInfo", default)]
    closing_price_info: Option<ClosingPriceDto>,
}

#[derive(Deserialize, Default)]
struct ClosingPriceDto {
    #[serde(rename = "priceMin", default, deserialize_with = "j::decimal")]
    price_min: Decimal,
    #[serde(rename = "priceMax", default, deserialize_with = "j::decimal")]
    price_max: Decimal,
    #[serde(rename = "priceYesterday", default, deserialize_with = "j::decimal")]
    price_yesterday: Decimal,
    #[serde(rename = "priceFirst", default, deserialize_with = "j::decimal")]
    price_first: Decimal,
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "dEven", default, deserialize_with = "j::int")]
    d_even: i32,
    #[serde(rename = "hEven", default, deserialize_with = "j::int")]
    h_even: i32,
    #[serde(rename = "pClosing", default, deserialize_with = "j::decimal")]
    p_closing: Decimal,
    #[serde(rename = "pDrCotVal", default, deserialize_with = "j::decimal")]
    p_dr_cot_val: Decimal,
    #[serde(rename = "zTotTran", default, deserialize_with = "j::decimal")]
    z_tot_tran: Decimal,
    #[serde(rename = "qTotTran5J", default, deserialize_with = "j::decimal")]
    q_tot_tran5j: Decimal,
    #[serde(rename = "qTotCap", default, deserialize_with = "j::decimal")]
    q_tot_cap: Decimal,
    #[serde(rename = "instrumentState", default, deserialize_with = "state_object")]
    instrument_state: Option<InstrumentStateDto>,
}

#[derive(Deserialize, Default)]
struct InstrumentStateDto {
    #[serde(rename = "cEtaval", default, deserialize_with = "j::opt_string")]
    c_etaval: Option<String>,
    #[serde(rename = "cEtavalTitle", default, deserialize_with = "j::opt_string")]
    c_etaval_title: Option<String>,
}

/// `instrumentState` is an object, null, or (on some endpoints) an array.
fn state_object<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<InstrumentStateDto>, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Object(_) => serde_json::from_value(value).ok(),
        _ => None,
    })
}

pub fn parse_daily_history(json: &[u8], ins_code: i64) -> Result<Vec<DailyBar>> {
    let envelope: ClosingPriceDailyEnvelope = parse(json)?.unwrap_or_default();
    let mut bars = envelope
        .closing_price_daily
        .into_iter()
        .filter(|d| d.d_even > 0)
        .map(|d| {
            let ins = positive(d.ins_code).unwrap_or(ins_code);
            to_daily_bar(d, ins)
        })
        .collect::<Result<Vec<_>>>()?;
    bars.sort_by_key(|b| b.date);
    Ok(bars)
}

fn to_daily_bar(d: ClosingPriceDto, ins_code: i64) -> Result<DailyBar> {
    Ok(DailyBar {
        ins_code,
        date: deven::to_date(d.d_even)?,
        open: d.price_first,
        high: d.price_max,
        low: d.price_min,
        close: d.p_closing,
        last: d.p_dr_cot_val,
        yesterday: d.price_yesterday,
        trade_count: trunc_i64(d.z_tot_tran),
        volume: trunc_i64(d.q_tot_tran5j),
        value: d.q_tot_cap,
        last_trade_time: deven::to_time(d.h_even),
    })
}

pub fn parse_closing_price_info(json: &[u8], ins_code: i64) -> Result<ClosingPriceSnapshot> {
    let envelope: ClosingPriceInfoEnvelope = parse(json)?.unwrap_or_default();
    let dto = envelope.closing_price_info.ok_or_else(|| Error::format("closingPriceInfo is missing."))?;
    let (state, state_title) = match dto.instrument_state {
        Some(s) => (trim(s.c_etaval), normalize(s.c_etaval_title.as_deref())),
        None => (None, None),
    };
    Ok(ClosingPriceSnapshot {
        ins_code,
        date: deven::to_date_or_none(dto.d_even)?,
        time: deven::to_time(dto.h_even),
        last: dto.p_dr_cot_val,
        close: dto.p_closing,
        open: dto.price_first,
        low: dto.price_min,
        high: dto.price_max,
        yesterday: dto.price_yesterday,
        trade_count: trunc_i64(dto.z_tot_tran),
        volume: trunc_i64(dto.q_tot_tran5j),
        value: dto.q_tot_cap,
        state,
        state_title,
    })
}

// ------------------------------------------------------------------------------------------------ flows

#[derive(Deserialize, Default)]
struct ClientTypeHistoryEnvelope {
    #[serde(rename = "clientType", default, deserialize_with = "j::list")]
    client_type: Vec<ClientTypeDto>,
}

#[derive(Deserialize, Default)]
struct ClientTypeDto {
    #[serde(rename = "recDate", default, deserialize_with = "j::int")]
    rec_date: i32,
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "buy_I_Volume", default, deserialize_with = "j::decimal")]
    buy_i_volume: Decimal,
    #[serde(rename = "buy_N_Volume", default, deserialize_with = "j::decimal")]
    buy_n_volume: Decimal,
    #[serde(rename = "buy_I_Value", default, deserialize_with = "j::decimal")]
    buy_i_value: Decimal,
    #[serde(rename = "buy_N_Value", default, deserialize_with = "j::decimal")]
    buy_n_value: Decimal,
    #[serde(rename = "buy_I_Count", default, deserialize_with = "j::decimal")]
    buy_i_count: Decimal,
    #[serde(rename = "buy_N_Count", default, deserialize_with = "j::decimal")]
    buy_n_count: Decimal,
    #[serde(rename = "sell_I_Volume", default, deserialize_with = "j::decimal")]
    sell_i_volume: Decimal,
    #[serde(rename = "sell_N_Volume", default, deserialize_with = "j::decimal")]
    sell_n_volume: Decimal,
    #[serde(rename = "sell_I_Value", default, deserialize_with = "j::decimal")]
    sell_i_value: Decimal,
    #[serde(rename = "sell_N_Value", default, deserialize_with = "j::decimal")]
    sell_n_value: Decimal,
    #[serde(rename = "sell_I_Count", default, deserialize_with = "j::decimal")]
    sell_i_count: Decimal,
    #[serde(rename = "sell_N_Count", default, deserialize_with = "j::decimal")]
    sell_n_count: Decimal,
}

pub fn parse_client_type_history(json: &[u8], ins_code: i64) -> Result<Vec<ClientTypeDay>> {
    let envelope: ClientTypeHistoryEnvelope = parse(json)?.unwrap_or_default();
    let mut days = envelope
        .client_type
        .into_iter()
        .filter(|d| d.rec_date > 0)
        .map(|d| {
            Ok(ClientTypeDay {
                ins_code: positive(d.ins_code).unwrap_or(ins_code),
                date: deven::to_date(d.rec_date)?,
                buy_individual_volume: trunc_i64(d.buy_i_volume),
                buy_institutional_volume: trunc_i64(d.buy_n_volume),
                buy_individual_value: d.buy_i_value,
                buy_institutional_value: d.buy_n_value,
                buy_individual_count: trunc_i32(d.buy_i_count),
                buy_institutional_count: trunc_i32(d.buy_n_count),
                sell_individual_volume: trunc_i64(d.sell_i_volume),
                sell_institutional_volume: trunc_i64(d.sell_n_volume),
                sell_individual_value: d.sell_i_value,
                sell_institutional_value: d.sell_n_value,
                sell_individual_count: trunc_i32(d.sell_i_count),
                sell_institutional_count: trunc_i32(d.sell_n_count),
                values_estimated: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    days.sort_by_key(|d| d.date);
    Ok(days)
}

#[derive(Deserialize, Default)]
struct ClientTypeAllEnvelope {
    #[serde(rename = "clientTypeAllDto", default, deserialize_with = "j::list")]
    items: Vec<ClientTypeAllDto>,
}

#[derive(Deserialize, Default)]
struct ClientTypeAllDto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "buy_I_Volume", default, deserialize_with = "j::decimal")]
    buy_i_volume: Decimal,
    #[serde(rename = "buy_N_Volume", default, deserialize_with = "j::decimal")]
    buy_n_volume: Decimal,
    #[serde(rename = "buy_CountI", default, deserialize_with = "j::decimal")]
    buy_count_i: Decimal,
    #[serde(rename = "buy_CountN", default, deserialize_with = "j::decimal")]
    buy_count_n: Decimal,
    #[serde(rename = "sell_I_Volume", default, deserialize_with = "j::decimal")]
    sell_i_volume: Decimal,
    #[serde(rename = "sell_N_Volume", default, deserialize_with = "j::decimal")]
    sell_n_volume: Decimal,
    #[serde(rename = "sell_CountI", default, deserialize_with = "j::decimal")]
    sell_count_i: Decimal,
    #[serde(rename = "sell_CountN", default, deserialize_with = "j::decimal")]
    sell_count_n: Decimal,
}

pub fn parse_client_type_all(json: &[u8]) -> Result<Vec<ClientTypeSnapshot>> {
    let envelope: ClientTypeAllEnvelope = parse(json)?.unwrap_or_default();
    Ok(envelope
        .items
        .into_iter()
        .filter_map(|d| {
            let ins_code = positive(d.ins_code)?;
            Some(ClientTypeSnapshot {
                ins_code,
                buy_individual_volume: trunc_i64(d.buy_i_volume),
                buy_institutional_volume: trunc_i64(d.buy_n_volume),
                buy_individual_count: trunc_i32(d.buy_count_i),
                buy_institutional_count: trunc_i32(d.buy_count_n),
                sell_individual_volume: trunc_i64(d.sell_i_volume),
                sell_institutional_volume: trunc_i64(d.sell_n_volume),
                sell_individual_count: trunc_i32(d.sell_count_i),
                sell_institutional_count: trunc_i32(d.sell_count_n),
            })
        })
        .collect())
}

// ------------------------------------------------------------------------------------------------ corporate events

#[derive(Deserialize, Default)]
struct PriceAdjustEnvelope {
    #[serde(rename = "priceAdjust", default, deserialize_with = "j::list")]
    items: Vec<PriceAdjustDto>,
}

#[derive(Deserialize, Default)]
struct PriceAdjustDto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "dEven", default, deserialize_with = "j::int")]
    d_even: i32,
    #[serde(rename = "pClosing", default, deserialize_with = "j::decimal")]
    p_closing: Decimal,
    #[serde(rename = "pClosingNotAdjusted", default, deserialize_with = "j::decimal")]
    p_closing_not_adjusted: Decimal,
}

pub fn parse_price_adjustments(json: &[u8], ins_code: i64) -> Result<Vec<PriceAdjustment>> {
    let envelope: PriceAdjustEnvelope = parse(json)?.unwrap_or_default();
    let mut items = envelope
        .items
        .into_iter()
        .filter(|d| d.d_even > 0)
        .map(|d| {
            Ok(PriceAdjustment {
                ins_code: positive(d.ins_code).unwrap_or(ins_code),
                date: deven::to_date(d.d_even)?,
                adjusted_close: d.p_closing,
                unadjusted_close: d.p_closing_not_adjusted,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    items.sort_by_key(|a| a.date);
    Ok(items)
}

#[derive(Deserialize, Default)]
struct ShareChangeEnvelope {
    #[serde(rename = "instrumentShareChange", default, deserialize_with = "j::list")]
    items: Vec<ShareChangeDto>,
}

#[derive(Deserialize, Default)]
struct ShareChangeDto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "dEven", default, deserialize_with = "j::int")]
    d_even: i32,
    #[serde(rename = "numberOfShareNew", default, deserialize_with = "j::decimal")]
    number_of_share_new: Decimal,
    #[serde(rename = "numberOfShareOld", default, deserialize_with = "j::decimal")]
    number_of_share_old: Decimal,
}

pub fn parse_share_changes(json: &[u8], ins_code: i64) -> Result<Vec<ShareChange>> {
    let envelope: ShareChangeEnvelope = parse(json)?.unwrap_or_default();
    let mut items = envelope
        .items
        .into_iter()
        .filter(|d| d.d_even > 0)
        .map(|d| {
            Ok(ShareChange {
                ins_code: positive(d.ins_code).unwrap_or(ins_code),
                date: deven::to_date(d.d_even)?,
                shares_before: d.number_of_share_old,
                shares_after: d.number_of_share_new,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    items.sort_by_key(|c| c.date);
    Ok(items)
}

// ------------------------------------------------------------------------------------------------ indices

#[derive(Deserialize, Default)]
struct IndexB2Envelope {
    #[serde(rename = "indexB2", default, deserialize_with = "j::list")]
    items: Vec<IndexB2Dto>,
}

#[derive(Deserialize, Default)]
struct IndexB2Dto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "dEven", default, deserialize_with = "j::int")]
    d_even: i32,
    #[serde(rename = "xNivInuClMresIbs", default, deserialize_with = "j::decimal")]
    close: Decimal,
    #[serde(rename = "xNivInuPbMresIbs", default, deserialize_with = "j::decimal")]
    low: Decimal,
    #[serde(rename = "xNivInuPhMresIbs", default, deserialize_with = "j::decimal")]
    high: Decimal,
}

pub fn parse_index_history(json: &[u8], ins_code: i64) -> Result<Vec<IndexBar>> {
    let envelope: IndexB2Envelope = parse(json)?.unwrap_or_default();
    let mut items = envelope
        .items
        .into_iter()
        .filter(|d| d.d_even > 0)
        .map(|d| Ok(IndexBar { ins_code: positive(d.ins_code).unwrap_or(ins_code), date: deven::to_date(d.d_even)?, close: d.close, low: d.low, high: d.high }))
        .collect::<Result<Vec<_>>>()?;
    items.sort_by_key(|b| b.date);
    Ok(items)
}

#[derive(Deserialize, Default)]
struct IndexB1Envelope {
    #[serde(rename = "indexB1", default, deserialize_with = "j::list")]
    items: Vec<IndexB1Dto>,
}

#[derive(Deserialize, Default)]
struct IndexB1Dto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "hEven", default, deserialize_with = "j::int")]
    h_even: i32,
    #[serde(rename = "lVal30", default, deserialize_with = "j::opt_string")]
    l_val30: Option<String>,
    #[serde(rename = "xDrNivJIdx004", default, deserialize_with = "j::decimal")]
    value: Decimal,
    #[serde(rename = "xPhNivJIdx004", default, deserialize_with = "j::decimal")]
    high: Decimal,
    #[serde(rename = "xPbNivJIdx004", default, deserialize_with = "j::decimal")]
    low: Decimal,
    #[serde(rename = "xVarIdxJRfV", default, deserialize_with = "j::decimal")]
    change_percent: Decimal,
    #[serde(rename = "indexChange", default, deserialize_with = "j::decimal")]
    change: Decimal,
}

pub fn parse_index_snapshots(json: &[u8]) -> Result<Vec<IndexSnapshot>> {
    let envelope: IndexB1Envelope = parse(json)?.unwrap_or_default();
    Ok(envelope
        .items
        .into_iter()
        .filter_map(|d| {
            let ins_code = positive(d.ins_code)?;
            Some(IndexSnapshot {
                ins_code,
                name: normalize(d.l_val30.as_deref()).unwrap_or_else(|| ins_code.to_string()),
                time: deven::to_time(d.h_even),
                value: d.value,
                high: d.high,
                low: d.low,
                change: d.change,
                change_percent: d.change_percent,
            })
        })
        .collect())
}

// ------------------------------------------------------------------------------------------------ order book

#[derive(Deserialize, Default)]
struct BestLimitsEnvelope {
    #[serde(rename = "bestLimits", default, deserialize_with = "j::list")]
    items: Vec<BestLimitDto>,
}

#[derive(Deserialize, Default)]
struct BestLimitDto {
    #[serde(rename = "number", default, deserialize_with = "j::int")]
    number: i32,
    #[serde(rename = "qTitMeDem", default, deserialize_with = "j::decimal")]
    q_tit_me_dem: Decimal,
    #[serde(rename = "zOrdMeDem", default, deserialize_with = "j::decimal")]
    z_ord_me_dem: Decimal,
    #[serde(rename = "pMeDem", default, deserialize_with = "j::decimal")]
    p_me_dem: Decimal,
    #[serde(rename = "pMeOf", default, deserialize_with = "j::decimal")]
    p_me_of: Decimal,
    #[serde(rename = "zOrdMeOf", default, deserialize_with = "j::decimal")]
    z_ord_me_of: Decimal,
    #[serde(rename = "qTitMeOf", default, deserialize_with = "j::decimal")]
    q_tit_me_of: Decimal,
}

pub fn parse_best_limits(json: &[u8]) -> Result<Vec<BestLimitLevel>> {
    let envelope: BestLimitsEnvelope = parse(json)?.unwrap_or_default();
    let mut levels: Vec<BestLimitLevel> = envelope
        .items
        .into_iter()
        .map(|d| BestLimitLevel {
            level: d.number,
            bid_price: d.p_me_dem,
            bid_volume: trunc_i64(d.q_tit_me_dem),
            bid_orders: trunc_i32(d.z_ord_me_dem),
            ask_price: d.p_me_of,
            ask_volume: trunc_i64(d.q_tit_me_of),
            ask_orders: trunc_i32(d.z_ord_me_of),
        })
        .collect();
    levels.sort_by_key(|l| l.level);
    Ok(levels)
}

// ------------------------------------------------------------------------------------------------ instrument info

#[derive(Deserialize, Default)]
struct InstrumentInfoEnvelope {
    #[serde(rename = "instrumentInfo", default)]
    instrument_info: Option<InstrumentInfoDto>,
    #[serde(rename = "instrumentIdentity", default)]
    instrument_identity: Option<InstrumentInfoDto>,
}

#[derive(Deserialize, Default)]
struct InstrumentInfoDto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "lVal18", default, deserialize_with = "j::opt_string")]
    l_val18: Option<String>,
    #[serde(rename = "lVal18AFC", default, deserialize_with = "j::opt_string")]
    l_val18_afc: Option<String>,
    #[serde(rename = "lVal30", default, deserialize_with = "j::opt_string")]
    l_val30: Option<String>,
    #[serde(rename = "yVal", default, deserialize_with = "j::opt_string")]
    y_val: Option<String>,
    #[serde(rename = "flow", default, deserialize_with = "j::int")]
    flow: i32,
    #[serde(rename = "cIsin", default, deserialize_with = "j::opt_string")]
    c_isin: Option<String>,
    #[serde(rename = "instrumentID", default, deserialize_with = "j::opt_string")]
    instrument_id: Option<String>,
    #[serde(rename = "cgrValCot", default, deserialize_with = "j::opt_string")]
    cgr_val_cot: Option<String>,
    #[serde(rename = "cgrValCotTitle", default, deserialize_with = "j::opt_string")]
    cgr_val_cot_title: Option<String>,
    #[serde(rename = "zTitad", default, deserialize_with = "j::decimal")]
    z_titad: Decimal,
    #[serde(rename = "baseVol", default, deserialize_with = "j::decimal")]
    base_vol: Decimal,
    #[serde(rename = "kAjCapValCpsIdx", default, deserialize_with = "j::flex_decimal")]
    k_aj_cap_val_cps_idx: Option<Decimal>,
    #[serde(rename = "sector", default)]
    sector: Option<SectorDto>,
    #[serde(rename = "subSector", default)]
    sub_sector: Option<SubSectorDto>,
    #[serde(rename = "staticThreshold", default)]
    static_threshold: Option<StaticThresholdDto>,
}

#[derive(Deserialize, Default)]
struct SectorDto {
    #[serde(rename = "cSecVal", default, deserialize_with = "j::opt_string")]
    c_sec_val: Option<String>,
    #[serde(rename = "lSecVal", default, deserialize_with = "j::opt_string")]
    l_sec_val: Option<String>,
}

#[derive(Deserialize, Default)]
struct SubSectorDto {
    #[serde(rename = "cSoSecVal", default, deserialize_with = "j::opt_int")]
    c_so_sec_val: Option<i32>,
    #[serde(rename = "lSoSecVal", default, deserialize_with = "j::opt_string")]
    l_so_sec_val: Option<String>,
}

#[derive(Deserialize, Default)]
struct StaticThresholdDto {
    #[serde(rename = "psGelStaMax", default, deserialize_with = "j::decimal")]
    ps_gel_sta_max: Decimal,
    #[serde(rename = "psGelStaMin", default, deserialize_with = "j::decimal")]
    ps_gel_sta_min: Decimal,
}

/// Parses GetInstrumentInfo (or GetInstrumentIdentity) into an [`Instrument`].
pub fn parse_instrument_info(json: &[u8], ins_code: i64) -> Result<Instrument> {
    let envelope: InstrumentInfoEnvelope = parse(json)?.unwrap_or_default();
    let info = envelope.instrument_info.or(envelope.instrument_identity).ok_or_else(|| Error::format("instrumentInfo / instrumentIdentity is missing."))?;

    let y_val = info.y_val.as_deref().and_then(|y| y.trim().parse::<i32>().ok());
    let flow = Some(info.flow).filter(|f| *f > 0);
    let isin = trim(info.instrument_id);
    let class = classify(y_val, flow, isin.as_deref());
    let (sector_code, sector_name) = match info.sector {
        Some(s) => (trim(s.c_sec_val), normalize(s.l_sec_val.as_deref())),
        None => (None, None),
    };
    let (sub_sector_code, sub_sector_name) = match info.sub_sector {
        Some(s) => (s.c_so_sec_val, normalize(s.l_so_sec_val.as_deref())),
        None => (None, None),
    };

    Ok(Instrument {
        ins_code: positive(info.ins_code).unwrap_or(ins_code),
        symbol: normalize(info.l_val18_afc.as_deref()).unwrap_or_else(|| ins_code.to_string()),
        name: normalize(info.l_val30.as_deref()),
        latin_name: trim(info.l_val18),
        isin,
        company_isin: trim(info.c_isin),
        y_val,
        flow,
        board: trim(info.cgr_val_cot),
        board_title: normalize(info.cgr_val_cot_title.as_deref()),
        sector_code,
        sector_name,
        sub_sector_code,
        sub_sector_name,
        base_volume: if info.base_vol > Decimal::ZERO { Some(trunc_i64(info.base_vol)) } else { None },
        total_shares: if info.z_titad > Decimal::ZERO { Some(info.z_titad) } else { None },
        free_float_percent: info.k_aj_cap_val_cps_idx,
        class,
        parent_ins_code: None,
    })
}

/// Allowed price band for the current day from GetInstrumentInfo's staticThreshold, if present.
pub fn parse_static_threshold(json: &[u8]) -> Result<Option<(Decimal, Decimal)>> {
    let envelope: InstrumentInfoEnvelope = parse(json)?.unwrap_or_default();
    Ok(envelope.instrument_info.and_then(|i| i.static_threshold).filter(|t| t.ps_gel_sta_max > Decimal::ZERO).map(|t| (t.ps_gel_sta_min, t.ps_gel_sta_max)))
}

#[derive(Deserialize, Default)]
struct InstrumentSearchEnvelope {
    #[serde(rename = "instrumentSearch", default, deserialize_with = "j::list")]
    items: Vec<InstrumentSearchDto>,
}

#[derive(Deserialize, Default)]
struct InstrumentSearchDto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "insCode2", default, deserialize_with = "j::opt_long")]
    ins_code2: Option<i64>,
    #[serde(rename = "insCode3", default, deserialize_with = "j::opt_long")]
    ins_code3: Option<i64>,
    #[serde(rename = "insCode4", default, deserialize_with = "j::opt_long")]
    ins_code4: Option<i64>,
    #[serde(rename = "lVal18AFC", default, deserialize_with = "j::opt_string")]
    l_val18_afc: Option<String>,
    #[serde(rename = "lVal30", default, deserialize_with = "j::opt_string")]
    l_val30: Option<String>,
    #[serde(rename = "flow", default, deserialize_with = "j::int")]
    flow: i32,
    #[serde(rename = "flowTitle", default, deserialize_with = "j::opt_string")]
    flow_title: Option<String>,
    #[serde(rename = "cgrValCot", default, deserialize_with = "j::opt_string")]
    cgr_val_cot: Option<String>,
    #[serde(rename = "lastDate", default, deserialize_with = "j::int")]
    last_date: i32,
}

pub fn parse_instrument_search(json: &[u8]) -> Result<Vec<InstrumentSearchHit>> {
    let envelope: InstrumentSearchEnvelope = parse(json)?.unwrap_or_default();
    Ok(envelope
        .items
        .into_iter()
        .filter_map(|d| {
            let ins_code = positive(d.ins_code)?;
            let mut alternates = Vec::new();
            for code in [d.ins_code2, d.ins_code3, d.ins_code4].into_iter().flatten() {
                if code > 0 && code != ins_code && !alternates.contains(&code) {
                    alternates.push(code);
                }
            }
            Some(InstrumentSearchHit {
                ins_code,
                symbol: normalize(d.l_val18_afc.as_deref()).unwrap_or_default(),
                name: normalize(d.l_val30.as_deref()),
                flow: d.flow,
                flow_title: normalize(d.flow_title.as_deref()),
                board: trim(d.cgr_val_cot),
                is_active: d.last_date == 1,
                alternate_ins_codes: alternates,
            })
        })
        .collect())
}

#[derive(Deserialize, Default)]
struct StaticDataEnvelope {
    #[serde(rename = "staticData", default, deserialize_with = "j::list")]
    items: Vec<StaticDataDto>,
}

#[derive(Deserialize, Default)]
struct StaticDataDto {
    #[serde(rename = "id", default, deserialize_with = "j::int")]
    id: i32,
    #[serde(rename = "code", default, deserialize_with = "j::int")]
    code: i32,
    #[serde(rename = "type", default, deserialize_with = "j::opt_string")]
    kind: Option<String>,
    #[serde(rename = "name", default, deserialize_with = "j::opt_string")]
    name: Option<String>,
    #[serde(rename = "description", default, deserialize_with = "j::opt_string")]
    description: Option<String>,
}

pub fn parse_static_data(json: &[u8]) -> Result<Vec<StaticDataItem>> {
    let envelope: StaticDataEnvelope = parse(json)?.unwrap_or_default();
    Ok(envelope
        .items
        .into_iter()
        .map(|d| StaticDataItem {
            id: d.id,
            code: d.code,
            kind: d.kind.map(|k| k.trim().to_string()).unwrap_or_default(),
            name: normalize(d.name.as_deref()).unwrap_or_default(),
            description: normalize(d.description.as_deref()),
        })
        .collect())
}

// ------------------------------------------------------------------------------------------------ market watch

#[derive(Deserialize, Default)]
struct MarketWatchEnvelope {
    #[serde(rename = "marketwatch", default, deserialize_with = "j::list")]
    items: Vec<MarketWatchDto>,
}

#[derive(Deserialize, Default)]
struct MarketWatchDto {
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "lva", default, deserialize_with = "j::opt_string")]
    lva: Option<String>,
    #[serde(rename = "lvc", default, deserialize_with = "j::opt_string")]
    lvc: Option<String>,
    #[serde(rename = "insID", default, deserialize_with = "j::opt_string")]
    ins_id: Option<String>,
    #[serde(rename = "csv", default, deserialize_with = "j::opt_string")]
    csv: Option<String>,
    #[serde(rename = "eps", default, deserialize_with = "j::flex_decimal")]
    eps: Option<Decimal>,
    #[serde(rename = "pe", default, deserialize_with = "j::flex_decimal")]
    pe: Option<Decimal>,
    #[serde(rename = "qtj", default, deserialize_with = "j::decimal")]
    qtj: Decimal,
    #[serde(rename = "pdv", default, deserialize_with = "j::decimal")]
    pdv: Decimal,
    #[serde(rename = "ztt", default, deserialize_with = "j::decimal")]
    ztt: Decimal,
    #[serde(rename = "qtc", default, deserialize_with = "j::decimal")]
    qtc: Decimal,
    #[serde(rename = "bv", default, deserialize_with = "j::decimal")]
    bv: Decimal,
    #[serde(rename = "pmn", default, deserialize_with = "j::decimal")]
    pmn: Decimal,
    #[serde(rename = "pmx", default, deserialize_with = "j::decimal")]
    pmx: Decimal,
    #[serde(rename = "py", default, deserialize_with = "j::decimal")]
    py: Decimal,
    #[serde(rename = "pf", default, deserialize_with = "j::decimal")]
    pf: Decimal,
    #[serde(rename = "pcl", default, deserialize_with = "j::decimal")]
    pcl: Decimal,
    #[serde(rename = "vc", default, deserialize_with = "j::int")]
    vc: i32,
    #[serde(rename = "pMax", default, deserialize_with = "j::decimal")]
    p_max: Decimal,
    #[serde(rename = "pMin", default, deserialize_with = "j::decimal")]
    p_min: Decimal,
    #[serde(rename = "ztd", default, deserialize_with = "j::decimal")]
    ztd: Decimal,
    #[serde(rename = "hEven", default, deserialize_with = "j::int")]
    h_even: i32,
    #[serde(rename = "blDs", default, deserialize_with = "j::list")]
    bl_ds: Vec<MarketWatchBestLimitDto>,
    #[serde(rename = "yVal", default, deserialize_with = "j::opt_int")]
    y_val: Option<i32>,
    #[serde(rename = "flow", default, deserialize_with = "j::opt_int")]
    flow: Option<i32>,
    #[serde(rename = "cGrValCot", default, deserialize_with = "j::opt_string")]
    c_gr_val_cot: Option<String>,
}

#[derive(Deserialize, Default)]
struct MarketWatchBestLimitDto {
    #[serde(rename = "n", default, deserialize_with = "j::int")]
    n: i32,
    #[serde(rename = "qmd", default, deserialize_with = "j::decimal")]
    qmd: Decimal,
    #[serde(rename = "zmd", default, deserialize_with = "j::decimal")]
    zmd: Decimal,
    #[serde(rename = "pmd", default, deserialize_with = "j::decimal")]
    pmd: Decimal,
    #[serde(rename = "pmo", default, deserialize_with = "j::decimal")]
    pmo: Decimal,
    #[serde(rename = "zmo", default, deserialize_with = "j::decimal")]
    zmo: Decimal,
    #[serde(rename = "qmo", default, deserialize_with = "j::decimal")]
    qmo: Decimal,
}

pub fn parse_market_watch(json: &[u8]) -> Result<MarketWatchSnapshot> {
    let envelope: MarketWatchEnvelope = parse(json)?.unwrap_or_default();
    let rows = envelope
        .items
        .into_iter()
        .filter_map(|d| {
            let ins_code = positive(d.ins_code)?;
            let mut best_limits: Vec<BestLimitLevel> = d
                .bl_ds
                .into_iter()
                .map(|l| BestLimitLevel {
                    level: l.n,
                    bid_price: l.pmd,
                    bid_volume: trunc_i64(l.qmd),
                    bid_orders: trunc_i32(l.zmd),
                    ask_price: l.pmo,
                    ask_volume: trunc_i64(l.qmo),
                    ask_orders: trunc_i32(l.zmo),
                })
                .collect();
            best_limits.sort_by_key(|l| l.level);
            Some(MarketWatchRow {
                ins_code,
                symbol: normalize(d.lva.as_deref()).unwrap_or_default(),
                name: normalize(d.lvc.as_deref()),
                isin: trim(d.ins_id),
                sector_code: trim(d.csv),
                last: d.pdv,
                close: d.pcl,
                open: d.pf,
                low: d.pmn,
                high: d.pmx,
                yesterday: d.py,
                allowed_high: d.p_max,
                allowed_low: d.p_min,
                trade_count: trunc_i64(d.ztt),
                volume: trunc_i64(d.qtj),
                value: d.qtc,
                base_volume: trunc_i64(d.bv),
                total_shares: d.ztd,
                eps: d.eps,
                price_earnings: d.pe,
                visit_count: d.vc,
                last_trade_time: deven::to_time(d.h_even),
                h_even: d.h_even,
                best_limits,
                y_val: d.y_val,
                flow: d.flow,
                board: trim(d.c_gr_val_cot),
            })
        })
        .collect();
    Ok(MarketWatchSnapshot { rows })
}

// ------------------------------------------------------------------------------------------------ messages

#[derive(Deserialize, Default)]
struct MessageEnvelope {
    #[serde(rename = "msg", default, deserialize_with = "j::list")]
    items: Vec<MessageDto>,
}

#[derive(Deserialize, Default)]
struct MessageDto {
    #[serde(rename = "tseMsgIdn", default, deserialize_with = "j::long")]
    tse_msg_idn: i64,
    #[serde(rename = "dEven", default, deserialize_with = "j::int")]
    d_even: i32,
    #[serde(rename = "hEven", default, deserialize_with = "j::int")]
    h_even: i32,
    #[serde(rename = "tseTitle", default, deserialize_with = "j::opt_string")]
    tse_title: Option<String>,
    #[serde(rename = "tseDesc", default, deserialize_with = "j::opt_string")]
    tse_desc: Option<String>,
    #[serde(rename = "flow", default, deserialize_with = "j::int")]
    flow: i32,
}

pub fn parse_messages(json: &[u8]) -> Result<Vec<SupervisorMessage>> {
    let envelope: MessageEnvelope = parse(json)?.unwrap_or_default();
    let mut messages = envelope
        .items
        .into_iter()
        .filter(|d| d.tse_msg_idn > 0 && d.d_even > 0)
        .map(|d| {
            Ok(classify_message(d.tse_msg_idn, deven::to_date(d.d_even)?, deven::to_time(d.h_even), d.tse_title.as_deref(), d.tse_desc.as_deref(), d.flow))
        })
        .collect::<Result<Vec<_>>>()?;
    // OrderBy(Date).ThenBy(Time); a null time sorts first.
    messages.sort_by_key(|m| (m.date, m.time));
    Ok(messages)
}

static SYMBOL_IN_PARENTHESES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\s*([^()\r\n]{1,24}?)\s*\)").expect("valid regex"));

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| text.contains(n))
}

/// Classifies a supervisor message from its wording (keyword rules on normalized text).
pub fn classify_message(id: i64, date: Date, time: Option<crate::Time>, title: Option<&str>, body: Option<&str>, flow: i32) -> SupervisorMessage {
    let normalized_title = normalize(title).unwrap_or_default();
    let normalized_body = normalize(body);
    let text = format!("{} {}", normalized_title, normalized_body.as_deref().unwrap_or(""));

    let kind = if contains_any(&normalized_title, &["بازگشایی", "رفع تعلیق", "حراج"]) {
        MessageKind::Reopen
    } else if contains_any(&normalized_title, &["توقف", "تعلیق"]) {
        MessageKind::Halt
    } else {
        MessageKind::Other
    };

    let reason = if kind == MessageKind::Other {
        HaltReason::Unknown
    } else if contains_any(&text, &["نماد اصلی", "قرارداد آتی", "اختیار معامله", "اوراق تبعی"]) {
        HaltReason::Underlying
    } else if contains_any(&text, &["تعلیق", "شفاف", "بررسی", "دستکاری", "ابهام", "رسیدگی"]) {
        HaltReason::Supervisory
    } else if contains_any(&text, &["مجمع"]) {
        HaltReason::GeneralMeeting
    } else if contains_any(&text, &["افشای اطلاعات", "اطلاعات با اهمیت", "اطلاعات بااهمیت"]) {
        HaltReason::Disclosure
    } else {
        HaltReason::Unknown
    };

    let mut mentioned: Vec<String> = Vec::new();
    for captures in SYMBOL_IN_PARENTHESES.captures_iter(&text) {
        let symbol = captures.get(1).map(|m| m.as_str().trim().to_string()).unwrap_or_default();
        let length = symbol.encode_utf16().count();
        // Symbols may contain a space ("پی پاد"); other parentheticals are harmless because unmatched symbols are not linked.
        if length > 0 && length <= 20 && symbol.matches(' ').count() <= 1 && !mentioned.contains(&symbol) {
            mentioned.push(symbol);
        }
    }

    SupervisorMessage {
        id,
        date,
        time,
        title: normalized_title,
        body: normalized_body,
        flow,
        kind,
        reason,
        reopens_without_limit: contains_any(&text, &["بدون محدودیت"]),
        mentioned_symbols: mentioned,
    }
}

// ------------------------------------------------------------------------------------------------ shareholders, state, statistics

#[derive(Deserialize, Default)]
struct ShareholderEnvelope {
    #[serde(rename = "shareHolder", default, deserialize_with = "j::list")]
    items: Vec<ShareholderDto>,
}

#[derive(Deserialize, Default)]
struct ShareholderDto {
    #[serde(rename = "shareHolderName", default, deserialize_with = "j::opt_string")]
    share_holder_name: Option<String>,
    #[serde(rename = "dEven", default, deserialize_with = "j::int")]
    d_even: i32,
    #[serde(rename = "numberOfShares", default, deserialize_with = "j::decimal")]
    number_of_shares: Decimal,
    #[serde(rename = "perOfShares", default, deserialize_with = "j::decimal")]
    per_of_shares: Decimal,
    #[serde(rename = "change", default, deserialize_with = "j::int")]
    change: i32,
    #[serde(rename = "changeAmount", default, deserialize_with = "j::decimal")]
    change_amount: Decimal,
    #[serde(rename = "shareHolderShareID", default, deserialize_with = "j::long")]
    share_holder_share_id: i64,
}

pub fn parse_shareholders(json: &[u8], ins_code: i64, snapshot_date: Date) -> Result<Vec<ShareholderPosition>> {
    let envelope: ShareholderEnvelope = parse(json)?.unwrap_or_default();
    envelope
        .items
        .into_iter()
        .map(|d| {
            Ok(ShareholderPosition {
                ins_code,
                snapshot_date: deven::to_date_or_none(d.d_even)?.unwrap_or(snapshot_date),
                holder_share_id: d.share_holder_share_id,
                holder_name: normalize(d.share_holder_name.as_deref()).unwrap_or_default(),
                shares: d.number_of_shares,
                percent: d.per_of_shares,
                change: d.change,
                change_amount: d.change_amount,
            })
        })
        .collect()
}

/// GetInstrumentState returns an object for dates with a state record and `[]` otherwise; the latter yields a
/// null state (unknown), which callers must not read as "not halted".
pub fn parse_instrument_state(json: &[u8], ins_code: i64, date: Date) -> Result<InstrumentStateOnDate> {
    let root: serde_json::Value = serde_json::from_slice(json)?;
    let unknown = InstrumentStateOnDate { ins_code, date, state: None, time: None };
    let Some(mut element) = root.get("instrumentState") else {
        return Ok(unknown);
    };
    if let serde_json::Value::Array(items) = element {
        match items.first() {
            Some(first) => element = first,
            None => return Ok(unknown),
        }
    }
    let serde_json::Value::Object(object) = element else {
        return Ok(unknown);
    };

    let state = object.get("cEtaval").and_then(|c| c.as_str()).map(|s| s.trim().to_string());
    let time = object.get("realHeven").and_then(serde_json::Value::as_i64).and_then(|h| i32::try_from(h).ok()).and_then(deven::to_time);
    Ok(InstrumentStateOnDate { ins_code, date, state, time })
}

pub fn parse_instrument_statistics(json: &[u8]) -> Result<Vec<InstrumentStatistic>> {
    let root: serde_json::Value = serde_json::from_slice(json)?;
    let Some(items) = root.get("instValueAllInstAllParam").and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };

    let mut result = Vec::with_capacity(items.len());
    for item in items {
        let ins_code = match item.get("insCode") {
            Some(serde_json::Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
            Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
            _ => 0,
        };
        let value = match item.get("dataValue") {
            Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().unwrap_or(f64::NAN),
            Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
            _ => f64::NAN,
        };
        let data_type = item.get("dataType").and_then(serde_json::Value::as_i64).and_then(|t| i32::try_from(t).ok());
        let (Some(data_type), true, false) = (data_type, ins_code > 0, value.is_nan()) else {
            continue;
        };
        let d_even = item.get("dEven").and_then(serde_json::Value::as_i64).and_then(|d| i32::try_from(d).ok()).unwrap_or(0);
        result.push(InstrumentStatistic { ins_code, data_type, date: deven::to_date_or_none(d_even).ok().flatten(), value });
    }
    Ok(result)
}
