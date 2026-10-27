//! Read models of the analytics (C# `MarketViews.cs`).

use crate::{Date, Decimal};

/// The latest value of an index on a session, with its change from the previous index row.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexMove {
    pub ins_code: i64,
    pub name: String,
    pub value: Decimal,
    pub change_percent: Option<f64>,
}

/// A macro quote (free-market dollar, Emami coin...) on or before a session.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroQuote {
    pub series: String,
    pub date: Date,
    pub value: Decimal,
    pub change_percent: Option<f64>,
}

/// Headline numbers of one session.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketOverview {
    pub session: Date,
    pub indices: Vec<IndexMove>,
    pub equity_value: Decimal,
    pub fund_value: Decimal,
    pub equity_trades: i64,
    pub advancers: i32,
    pub decliners: i32,
    pub unchanged: i32,
    pub not_traded: i32,
    pub halted: i32,
    pub buy_queues: i32,
    pub buy_queue_value: Option<Decimal>,
    pub sell_queues: i32,
    pub sell_queue_value: Option<Decimal>,
    pub individual_net_flow: Decimal,
    pub individual_per_capita_buy: Option<Decimal>,
    pub individual_per_capita_sell: Option<Decimal>,
    pub flow_values_estimated: bool,
    pub usd: Option<MacroQuote>,
    pub coin: Option<MacroQuote>,
    /// TEDPIX change measured in free-market dollars.
    pub tedpix_usd_change_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BreadthDay {
    pub date: Date,
    pub advancers: i32,
    pub decliners: i32,
    pub unchanged: i32,
    pub not_traded: i32,
    pub halted: i32,
    pub locked_up: i32,
    pub locked_down: i32,
    pub equity_value: Decimal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SectorPerformance {
    pub sector_code: String,
    pub sector_name: Option<String>,
    pub instruments: i32,
    pub traded: i32,
    pub value: Decimal,
    /// Market-cap-weighted return of traded members (weight = yesterday's price × shares).
    pub weighted_return: Option<f64>,
    pub median_return: Option<f64>,
    pub individual_net_flow: Decimal,
    pub advancers: i32,
    pub decliners: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoverKind {
    Gainers,
    Losers,
    Value,
    Inflow,
    Outflow,
}

impl MoverKind {
    /// C# `Enum.TryParse<MoverKind>(kind, ignoreCase: true)`.
    pub fn parse(value: &str) -> Option<MoverKind> {
        match value.trim().to_ascii_lowercase().as_str() {
            "gainers" => Some(MoverKind::Gainers),
            "losers" => Some(MoverKind::Losers),
            "value" => Some(MoverKind::Value),
            "inflow" => Some(MoverKind::Inflow),
            "outflow" => Some(MoverKind::Outflow),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mover {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    /// The rules-class key as stored (`tse_stock`).
    pub instrument_class: String,
    pub day_status: Option<String>,
    pub close: Decimal,
    pub change_percent: Option<f64>,
    pub value: Decimal,
    pub individual_net_flow: Option<Decimal>,
}

/// Everything the symbol page's header and returns card need, computed point-in-time as of `as_of`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentMetrics {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub sector_code: Option<String>,
    pub sector_name: Option<String>,
    pub as_of: Option<Date>,
    pub day_status: Option<String>,
    pub close: Option<Decimal>,
    pub change_percent: Option<f64>,
    pub market_cap: Option<Decimal>,
    pub high_52_week: Option<Decimal>,
    pub low_52_week: Option<Decimal>,
    pub from_high_52_week_percent: Option<f64>,
    /// Total returns over calendar windows ending at `as_of`, in insertion order: 1w, 1m, 3m, 6m, 1y.
    pub returns: Vec<(String, Option<f64>)>,
    pub usd_return_1_year: Option<f64>,
    pub average_value_20: Option<Decimal>,
    pub individual_net_flow_20: Option<Decimal>,
    pub per_capita_buy: Option<Decimal>,
    pub per_capita_sell: Option<Decimal>,
    pub buyer_power: Option<Decimal>,
    pub buyer_power_percentile: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenKind {
    /// Return from 6 months to 1 month ago.
    Momentum6To1,
    /// Last adjusted close relative to the 52-week closing high (1 = at the high).
    High52WeekProximity,
    /// Individual net flow over 20 sessions divided by value traded over the same sessions.
    IndividualFlowIntensity20,
    /// Average buyer power over the last 5 sessions.
    BuyerPower5,
}

impl ScreenKind {
    /// C# `Enum.TryParse<ScreenKind>(kind, ignoreCase: true)`.
    pub fn parse(value: &str) -> Option<ScreenKind> {
        match value.trim().to_ascii_lowercase().as_str() {
            "momentum6to1" => Some(ScreenKind::Momentum6To1),
            "high52weekproximity" => Some(ScreenKind::High52WeekProximity),
            "individualflowintensity20" => Some(ScreenKind::IndividualFlowIntensity20),
            "buyerpower5" => Some(ScreenKind::BuyerPower5),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScreenRow {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    pub instrument_class: String,
    pub sector_code: Option<String>,
    pub score: f64,
    pub observations: i32,
}
