//! The DTOs of `tauri/docs/api-contract.md`. Every property is always present (`null` is explicit), names are
//! camelCase, 64-bit identifiers are JSON strings, and rials/decimals are JSON numbers.

use chrono::{DateTime, SecondsFormat, TimeZone};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize, Serializer};

use crate::analytics::*;
use crate::domain::InstrumentClass;
use crate::{Date, Decimal};

fn decimal<S: Serializer>(value: &Decimal, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_f64(value.to_f64().unwrap_or(0.0))
}

fn opt_decimal<S: Serializer>(value: &Option<Decimal>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) => decimal(v, serializer),
        None => serializer.serialize_none(),
    }
}

fn opt_f64<S: Serializer>(value: &Option<f64>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        Some(v) if v.is_finite() => serializer.serialize_f64(*v),
        _ => serializer.serialize_none(),
    }
}

fn f64_finite<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.is_finite() { serializer.serialize_f64(*value) } else { serializer.serialize_none() }
}

/// DateTimeOffset as ISO 8601 with an offset.
pub fn iso<Tz: TimeZone>(value: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    value.to_rfc3339_opts(SecondsFormat::AutoSi, false)
}

/// The contract's `instrumentClass` value (the C# enum member name) for a stored rules key.
pub fn class_name(rule_key: &str) -> String {
    InstrumentClass::from_rule_key(rule_key).name().to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesPointDto {
    pub date: Date,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexMoveDto {
    pub ins_code: String,
    pub name: String,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
    #[serde(serialize_with = "opt_f64")]
    pub change_percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroQuoteDto {
    pub series: String,
    pub date: Date,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
    #[serde(serialize_with = "opt_f64")]
    pub change_percent: Option<f64>,
}

impl From<&MacroQuote> for MacroQuoteDto {
    fn from(q: &MacroQuote) -> Self {
        Self { series: q.series.clone(), date: q.date, value: q.value, change_percent: q.change_percent }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketOverviewDto {
    pub session: Date,
    pub indices: Vec<IndexMoveDto>,
    #[serde(serialize_with = "decimal")]
    pub equity_value: Decimal,
    #[serde(serialize_with = "decimal")]
    pub fund_value: Decimal,
    pub equity_trades: i64,
    pub advancers: i32,
    pub decliners: i32,
    pub unchanged: i32,
    pub not_traded: i32,
    pub halted: i32,
    pub buy_queues: i32,
    #[serde(serialize_with = "opt_decimal")]
    pub buy_queue_value: Option<Decimal>,
    pub sell_queues: i32,
    #[serde(serialize_with = "opt_decimal")]
    pub sell_queue_value: Option<Decimal>,
    #[serde(serialize_with = "decimal")]
    pub individual_net_flow: Decimal,
    #[serde(serialize_with = "opt_decimal")]
    pub individual_per_capita_buy: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub individual_per_capita_sell: Option<Decimal>,
    pub flow_values_estimated: bool,
    pub usd: Option<MacroQuoteDto>,
    pub coin: Option<MacroQuoteDto>,
    #[serde(serialize_with = "opt_f64")]
    pub tedpix_usd_change_percent: Option<f64>,
}

impl From<&MarketOverview> for MarketOverviewDto {
    fn from(o: &MarketOverview) -> Self {
        Self {
            session: o.session,
            indices: o
                .indices
                .iter()
                .map(|i| IndexMoveDto { ins_code: i.ins_code.to_string(), name: i.name.clone(), value: i.value, change_percent: i.change_percent })
                .collect(),
            equity_value: o.equity_value,
            fund_value: o.fund_value,
            equity_trades: o.equity_trades,
            advancers: o.advancers,
            decliners: o.decliners,
            unchanged: o.unchanged,
            not_traded: o.not_traded,
            halted: o.halted,
            buy_queues: o.buy_queues,
            buy_queue_value: o.buy_queue_value,
            sell_queues: o.sell_queues,
            sell_queue_value: o.sell_queue_value,
            individual_net_flow: o.individual_net_flow,
            individual_per_capita_buy: o.individual_per_capita_buy,
            individual_per_capita_sell: o.individual_per_capita_sell,
            flow_values_estimated: o.flow_values_estimated,
            usd: o.usd.as_ref().map(MacroQuoteDto::from),
            coin: o.coin.as_ref().map(MacroQuoteDto::from),
            tedpix_usd_change_percent: o.tedpix_usd_change_percent,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreadthDayDto {
    pub date: Date,
    pub advancers: i32,
    pub decliners: i32,
    pub unchanged: i32,
    pub not_traded: i32,
    pub halted: i32,
    pub locked_up: i32,
    pub locked_down: i32,
    #[serde(serialize_with = "decimal")]
    pub equity_value: Decimal,
}

impl From<&BreadthDay> for BreadthDayDto {
    fn from(b: &BreadthDay) -> Self {
        Self {
            date: b.date,
            advancers: b.advancers,
            decliners: b.decliners,
            unchanged: b.unchanged,
            not_traded: b.not_traded,
            halted: b.halted,
            locked_up: b.locked_up,
            locked_down: b.locked_down,
            equity_value: b.equity_value,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SectorPerformanceDto {
    pub sector_code: String,
    pub sector_name: Option<String>,
    pub instruments: i32,
    pub traded: i32,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
    #[serde(serialize_with = "opt_f64")]
    pub weighted_return: Option<f64>,
    #[serde(serialize_with = "opt_f64")]
    pub median_return: Option<f64>,
    #[serde(serialize_with = "decimal")]
    pub individual_net_flow: Decimal,
    pub advancers: i32,
    pub decliners: i32,
}

impl From<&SectorPerformance> for SectorPerformanceDto {
    fn from(s: &SectorPerformance) -> Self {
        Self {
            sector_code: s.sector_code.clone(),
            sector_name: s.sector_name.clone(),
            instruments: s.instruments,
            traded: s.traded,
            value: s.value,
            weighted_return: s.weighted_return,
            median_return: s.median_return,
            individual_net_flow: s.individual_net_flow,
            advancers: s.advancers,
            decliners: s.decliners,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoverDto {
    pub ins_code: String,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub day_status: Option<String>,
    #[serde(serialize_with = "decimal")]
    pub close: Decimal,
    #[serde(serialize_with = "opt_f64")]
    pub change_percent: Option<f64>,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
    #[serde(serialize_with = "opt_decimal")]
    pub individual_net_flow: Option<Decimal>,
}

impl From<&Mover> for MoverDto {
    fn from(m: &Mover) -> Self {
        Self {
            ins_code: m.ins_code.to_string(),
            symbol: m.symbol.clone(),
            name: m.name.clone(),
            instrument_class: class_name(&m.instrument_class),
            day_status: m.day_status.clone(),
            close: m.close,
            change_percent: m.change_percent,
            value: m.value,
            individual_net_flow: m.individual_net_flow,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenRowDto {
    pub ins_code: String,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub sector_code: Option<String>,
    #[serde(serialize_with = "f64_finite")]
    pub score: f64,
    pub observations: i32,
}

impl From<&ScreenRow> for ScreenRowDto {
    fn from(r: &ScreenRow) -> Self {
        Self {
            ins_code: r.ins_code.to_string(),
            symbol: r.symbol.clone(),
            name: r.name.clone(),
            instrument_class: class_name(&r.instrument_class),
            sector_code: r.sector_code.clone(),
            score: r.score,
            observations: r.observations,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentSearchResultDto {
    pub ins_code: String,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub parent_ins_code: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MarketWatchRequestDto {
    pub date: Option<String>,
    pub filter: Option<String>,
    pub classes: Option<Vec<String>>,
    pub traded_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketWatchEntryDto {
    pub ins_code: String,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub sector_code: Option<String>,
    pub day_status: Option<String>,
    #[serde(serialize_with = "decimal")]
    pub close: Decimal,
    #[serde(serialize_with = "opt_f64")]
    pub change_percent: Option<f64>,
    #[serde(serialize_with = "decimal")]
    pub last: Decimal,
    #[serde(serialize_with = "opt_f64")]
    pub last_change_percent: Option<f64>,
    pub volume: i64,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
    pub trades: i64,
    #[serde(serialize_with = "opt_decimal")]
    pub allowed_low: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub allowed_high: Option<Decimal>,
    pub queue: Option<String>,
    #[serde(serialize_with = "opt_decimal")]
    pub queue_value: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub price_earnings: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub market_cap: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub individual_net_value: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub per_capita_buy: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub per_capita_sell: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub buyer_power: Option<Decimal>,
    #[serde(serialize_with = "opt_f64")]
    pub individual_buy_share: Option<f64>,
}

impl From<&MarketWatchEntry> for MarketWatchEntryDto {
    fn from(e: &MarketWatchEntry) -> Self {
        Self {
            ins_code: e.ins_code.to_string(),
            symbol: e.symbol.clone(),
            name: e.name.clone(),
            instrument_class: class_name(&e.instrument_class),
            sector_code: e.sector_code.clone(),
            day_status: e.day_status.clone(),
            close: e.close,
            change_percent: e.change_percent,
            last: e.last,
            last_change_percent: e.last_change_percent,
            volume: e.volume,
            value: e.value,
            trades: e.trades,
            allowed_low: e.allowed_low,
            allowed_high: e.allowed_high,
            queue: e.queue.clone(),
            queue_value: e.queue_value,
            price_earnings: e.price_earnings,
            market_cap: e.market_cap,
            individual_net_value: e.individual_net_value,
            per_capita_buy: e.per_capita_buy,
            per_capita_sell: e.per_capita_sell,
            buyer_power: e.buyer_power,
            individual_buy_share: e.individual_buy_share,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketWatchResultDto {
    pub session: Option<Date>,
    pub entries: Vec<MarketWatchEntryDto>,
    pub evaluated: usize,
    pub filter_errors: Vec<String>,
}

/// `returns` keeps the insertion order of `MarketAnalytics.GetInstrumentMetrics` ("1w", "1m", "3m", "6m", "1y").
#[derive(Debug, Clone)]
pub struct Returns(pub Vec<(String, Option<f64>)>);

impl Serialize for Returns {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            match value {
                Some(v) if v.is_finite() => map.serialize_entry(key, v)?,
                _ => map.serialize_entry(key, &Option::<f64>::None)?,
            }
        }
        map.end()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentMetricsDto {
    pub ins_code: String,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub sector_code: Option<String>,
    pub sector_name: Option<String>,
    pub as_of: Option<Date>,
    pub day_status: Option<String>,
    #[serde(serialize_with = "opt_decimal")]
    pub close: Option<Decimal>,
    #[serde(serialize_with = "opt_f64")]
    pub change_percent: Option<f64>,
    #[serde(serialize_with = "opt_decimal")]
    pub market_cap: Option<Decimal>,
    #[serde(rename = "high52Week", serialize_with = "opt_decimal")]
    pub high_52_week: Option<Decimal>,
    #[serde(rename = "low52Week", serialize_with = "opt_decimal")]
    pub low_52_week: Option<Decimal>,
    #[serde(rename = "fromHigh52WeekPercent", serialize_with = "opt_f64")]
    pub from_high_52_week_percent: Option<f64>,
    pub returns: Returns,
    #[serde(rename = "usdReturn1Year", serialize_with = "opt_f64")]
    pub usd_return_1_year: Option<f64>,
    #[serde(rename = "averageValue20", serialize_with = "opt_decimal")]
    pub average_value_20: Option<Decimal>,
    #[serde(rename = "individualNetFlow20", serialize_with = "opt_decimal")]
    pub individual_net_flow_20: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub per_capita_buy: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub per_capita_sell: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub buyer_power: Option<Decimal>,
    #[serde(serialize_with = "opt_f64")]
    pub buyer_power_percentile: Option<f64>,
}

impl From<&InstrumentMetrics> for InstrumentMetricsDto {
    fn from(m: &InstrumentMetrics) -> Self {
        Self {
            ins_code: m.ins_code.to_string(),
            symbol: m.symbol.clone(),
            name: m.name.clone(),
            instrument_class: class_name(&m.instrument_class),
            sector_code: m.sector_code.clone(),
            sector_name: m.sector_name.clone(),
            as_of: m.as_of,
            day_status: m.day_status.clone(),
            close: m.close,
            change_percent: m.change_percent,
            market_cap: m.market_cap,
            high_52_week: m.high_52_week,
            low_52_week: m.low_52_week,
            from_high_52_week_percent: m.from_high_52_week_percent,
            returns: Returns(m.returns.clone()),
            usd_return_1_year: m.usd_return_1_year,
            average_value_20: m.average_value_20,
            individual_net_flow_20: m.individual_net_flow_20,
            per_capita_buy: m.per_capita_buy,
            per_capita_sell: m.per_capita_sell,
            buyer_power: m.buyer_power,
            buyer_power_percentile: m.buyer_power_percentile,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarDto {
    pub date: Date,
    #[serde(serialize_with = "decimal")]
    pub open: Decimal,
    #[serde(serialize_with = "decimal")]
    pub high: Decimal,
    #[serde(serialize_with = "decimal")]
    pub low: Decimal,
    #[serde(serialize_with = "decimal")]
    pub close: Decimal,
    #[serde(serialize_with = "decimal")]
    pub adjusted_close: Decimal,
    pub volume: i64,
    #[serde(serialize_with = "decimal")]
    pub value: Decimal,
    pub trades: i64,
    pub status: Option<String>,
    #[serde(rename = "return", serialize_with = "opt_f64")]
    pub daily_return: Option<f64>,
    #[serde(serialize_with = "opt_decimal")]
    pub allowed_low: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub allowed_high: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDayDto {
    pub date: Date,
    #[serde(serialize_with = "decimal")]
    pub buy_individual_value: Decimal,
    #[serde(serialize_with = "decimal")]
    pub sell_individual_value: Decimal,
    #[serde(serialize_with = "decimal")]
    pub buy_institutional_value: Decimal,
    #[serde(serialize_with = "decimal")]
    pub sell_institutional_value: Decimal,
    pub buy_individual_count: i32,
    pub sell_individual_count: i32,
    #[serde(serialize_with = "decimal")]
    pub individual_net_value: Decimal,
    #[serde(serialize_with = "opt_decimal")]
    pub per_capita_buy: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub per_capita_sell: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub buyer_power: Option<Decimal>,
    pub values_estimated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CorporateEventDto {
    pub date: Date,
    pub kind: String,
    pub source: Option<String>,
    #[serde(serialize_with = "opt_decimal")]
    pub factor: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub shares_before: Option<Decimal>,
    #[serde(serialize_with = "opt_decimal")]
    pub shares_after: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareholderDto {
    pub snapshot_date: Date,
    pub holder_name: String,
    #[serde(serialize_with = "decimal")]
    pub shares: Decimal,
    #[serde(serialize_with = "decimal")]
    pub percent: Decimal,
    pub change: i32,
    #[serde(serialize_with = "decimal")]
    pub change_amount: Decimal,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodalLetterDto {
    pub tracing_no: String,
    pub title: String,
    pub letter_code: Option<String>,
    pub letter_type_id: Option<i32>,
    pub published_at: Option<String>,
    pub disclosure_group: Option<String>,
    pub has_excel: bool,
    pub report_url: Option<String>,
    pub excel_url: Option<String>,
    pub pdf_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupervisorMessageDto {
    pub id: String,
    pub date: Date,
    pub title: String,
    pub body: Option<String>,
    pub kind: String,
    pub reason: String,
    pub reopens_without_limit: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatasetHealthDto {
    pub dataset: String,
    pub rows: i64,
    pub latest_date: Option<Date>,
    pub expected_date: Option<Date>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRunDto {
    pub job: String,
    pub node: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: String,
    pub items: i32,
    pub failures: i32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReachabilityDto {
    pub name: String,
    pub verdict: String,
    pub status: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketDataHealthDto {
    pub generated_at: String,
    pub node: String,
    pub expected_session: Date,
    pub healthy: bool,
    pub datasets: Vec<DatasetHealthDto>,
    pub runs: Vec<CollectionRunDto>,
    pub last_probe_at: Option<String>,
    pub sources: Vec<SourceReachabilityDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobStatusDto {
    pub id: String,
    pub job: String,
    pub state: String,
    pub queued_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPullRequestDto {
    pub node_url: String,
    pub since: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncPullResultDto {
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfoDto {
    pub version: String,
    pub node: String,
    pub data_root: String,
    pub transport: String,
    pub collect: bool,
}
