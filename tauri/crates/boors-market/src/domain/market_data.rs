//! Market-data records (C# `Domain/MarketData.cs`).

use super::instrument::{InstrumentClass, classify};
use crate::{Date, Decimal, Time};

/// One unadjusted daily bar from TSETMC's closing-price history. Prices are rials.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DailyBar {
    pub ins_code: i64,
    pub date: Date,
    /// First trade price (priceFirst).
    pub open: Decimal,
    /// Day high (priceMax).
    pub high: Decimal,
    /// Day low (priceMin).
    pub low: Decimal,
    /// Official closing price (pClosing).
    pub close: Decimal,
    /// Last trade price (pDrCotVal).
    pub last: Decimal,
    /// Reference price of the previous day (priceYesterday).
    pub yesterday: Decimal,
    /// Number of trades (zTotTran).
    pub trade_count: i64,
    /// Shares traded (qTotTran5J).
    pub volume: i64,
    /// Rial value traded (qTotCap).
    pub value: Decimal,
    pub last_trade_time: Option<Time>,
}

/// Individual (حقیقی, "I") vs institutional (حقوقی, "N") flows for one symbol-day.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClientTypeDay {
    pub ins_code: i64,
    pub date: Date,
    pub buy_individual_volume: i64,
    pub buy_institutional_volume: i64,
    pub buy_individual_value: Decimal,
    pub buy_institutional_value: Decimal,
    pub buy_individual_count: i32,
    pub buy_institutional_count: i32,
    pub sell_individual_volume: i64,
    pub sell_institutional_volume: i64,
    pub sell_individual_value: Decimal,
    pub sell_institutional_value: Decimal,
    pub sell_individual_count: i32,
    pub sell_institutional_count: i32,
    /// True when rial values were estimated from volumes × average price, not official.
    pub values_estimated: bool,
}

impl ClientTypeDay {
    /// Individual buy value minus individual sell value (rials).
    pub fn individual_net_value(&self) -> Decimal {
        self.buy_individual_value - self.sell_individual_value
    }

    /// Average individual buy ticket (سرانه خرید).
    pub fn per_capita_buy(&self) -> Option<Decimal> {
        if self.buy_individual_count > 0 { self.buy_individual_value.checked_div(Decimal::from(self.buy_individual_count)) } else { None }
    }

    /// Average individual sell ticket (سرانه فروش).
    pub fn per_capita_sell(&self) -> Option<Decimal> {
        if self.sell_individual_count > 0 { self.sell_individual_value.checked_div(Decimal::from(self.sell_individual_count)) } else { None }
    }

    /// Buyer power (قدرت خریدار) = per-capita buy / per-capita sell.
    pub fn buyer_power(&self) -> Option<Decimal> {
        match (self.per_capita_buy(), self.per_capita_sell()) {
            (Some(b), Some(s)) if !s.is_zero() => b.checked_div(s),
            _ => None,
        }
    }
}

/// A corporate-action price adjustment; `factor` multiplies prices before the event.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PriceAdjustment {
    pub ins_code: i64,
    pub date: Date,
    pub adjusted_close: Decimal,
    pub unadjusted_close: Decimal,
}

impl PriceAdjustment {
    pub fn factor(&self) -> Decimal {
        if self.unadjusted_close.is_zero() { Decimal::ONE } else { self.adjusted_close.checked_div(self.unadjusted_close).unwrap_or(Decimal::ONE) }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShareChange {
    pub ins_code: i64,
    pub date: Date,
    pub shares_before: Decimal,
    pub shares_after: Decimal,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct IndexBar {
    pub ins_code: i64,
    pub date: Date,
    pub close: Decimal,
    pub low: Decimal,
    pub high: Decimal,
}

/// One order-book level. Demand = bids (MeDem), offer = asks (MeOf).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct BestLimitLevel {
    pub level: i32,
    pub bid_price: Decimal,
    pub bid_volume: i64,
    pub bid_orders: i32,
    pub ask_price: Decimal,
    pub ask_volume: i64,
    pub ask_orders: i32,
}

/// Live state of one symbol from GetClosingPriceInfo.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClosingPriceSnapshot {
    pub ins_code: i64,
    pub date: Option<Date>,
    pub time: Option<Time>,
    pub last: Decimal,
    pub close: Decimal,
    pub open: Decimal,
    pub low: Decimal,
    pub high: Decimal,
    pub yesterday: Decimal,
    pub trade_count: i64,
    pub volume: i64,
    pub value: Decimal,
    /// cEtaval, trimmed: "A" = permitted, "IS" = halted.
    pub state: Option<String>,
    pub state_title: Option<String>,
}

impl ClosingPriceSnapshot {
    pub fn is_halted(&self) -> bool {
        self.state.as_deref().is_some_and(|s| s != "A")
    }
}

/// One row of the market watch (compact wire keys mapped to names).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MarketWatchRow {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    pub isin: Option<String>,
    pub sector_code: Option<String>,
    pub last: Decimal,
    pub close: Decimal,
    pub open: Decimal,
    pub low: Decimal,
    pub high: Decimal,
    pub yesterday: Decimal,
    /// Upper allowed price for the day (pMax).
    pub allowed_high: Decimal,
    /// Lower allowed price for the day (pMin).
    pub allowed_low: Decimal,
    pub trade_count: i64,
    pub volume: i64,
    pub value: Decimal,
    pub base_volume: i64,
    pub total_shares: Decimal,
    pub eps: Option<Decimal>,
    pub price_earnings: Option<Decimal>,
    pub visit_count: i32,
    pub last_trade_time: Option<Time>,
    pub h_even: i32,
    pub best_limits: Vec<BestLimitLevel>,
    /// Only in the "extended" response variant.
    pub y_val: Option<i32>,
    pub flow: Option<i32>,
    pub board: Option<String>,
}

impl MarketWatchRow {
    pub fn class(&self) -> InstrumentClass {
        classify(self.y_val, self.flow, self.isin.as_deref())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StaticDataItem {
    pub id: i32,
    pub code: i32,
    pub kind: String,
    pub name: String,
    pub description: Option<String>,
}

/// The part of an end-of-day market-watch row that history does not carry.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    pub ins_code: i64,
    pub date: Date,
    pub eps: Option<Decimal>,
    pub price_earnings: Option<Decimal>,
    pub allowed_low: Decimal,
    pub allowed_high: Decimal,
    pub top: Option<BestLimitLevel>,
}

impl SessionSnapshot {
    pub fn from_row(row: &MarketWatchRow, date: Date) -> Self {
        Self {
            ins_code: row.ins_code,
            date,
            eps: row.eps,
            price_earnings: row.price_earnings,
            allowed_low: row.allowed_low,
            allowed_high: row.allowed_high,
            top: row.best_limits.iter().find(|l| l.level == 1).cloned(),
        }
    }
}

/// One TSETMC statistic of an instrument; data type N is the filter variable `[isN]`.
#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentStatistic {
    pub ins_code: i64,
    pub data_type: i32,
    pub date: Option<Date>,
    pub value: f64,
}

/// Today's individual/institutional volumes and counts for one instrument (GetClientTypeAll; no rial values).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClientTypeSnapshot {
    pub ins_code: i64,
    pub buy_individual_volume: i64,
    pub buy_institutional_volume: i64,
    pub buy_individual_count: i32,
    pub buy_institutional_count: i32,
    pub sell_individual_volume: i64,
    pub sell_institutional_volume: i64,
    pub sell_individual_count: i32,
    pub sell_institutional_count: i32,
}

impl ClientTypeSnapshot {
    /// Estimates rial values from the day's average price; the history endpoint later replaces them.
    pub fn to_estimated_day(&self, date: Date, average_price: Decimal) -> ClientTypeDay {
        let value = |volume: i64| Decimal::from(volume).checked_mul(average_price).unwrap_or_default();
        ClientTypeDay {
            ins_code: self.ins_code,
            date,
            buy_individual_volume: self.buy_individual_volume,
            buy_institutional_volume: self.buy_institutional_volume,
            buy_individual_value: value(self.buy_individual_volume),
            buy_institutional_value: value(self.buy_institutional_volume),
            buy_individual_count: self.buy_individual_count,
            buy_institutional_count: self.buy_institutional_count,
            sell_individual_volume: self.sell_individual_volume,
            sell_institutional_volume: self.sell_institutional_volume,
            sell_individual_value: value(self.sell_individual_volume),
            sell_institutional_value: value(self.sell_institutional_volume),
            sell_individual_count: self.sell_individual_count,
            sell_institutional_count: self.sell_institutional_count,
            values_estimated: true,
        }
    }
}

/// Latest value of an index (GetIndexB1LastAll).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IndexSnapshot {
    pub ins_code: i64,
    pub name: String,
    pub time: Option<Time>,
    pub value: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub change: Decimal,
    pub change_percent: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MessageKind {
    #[default]
    Other,
    Halt,
    Reopen,
}

impl MessageKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MessageKind::Other => "Other",
            MessageKind::Halt => "Halt",
            MessageKind::Reopen => "Reopen",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Other" => Some(MessageKind::Other),
            "Halt" => Some(MessageKind::Halt),
            "Reopen" => Some(MessageKind::Reopen),
            _ => None,
        }
    }
}

/// Why a symbol was halted, from the supervisor message text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum HaltReason {
    #[default]
    Unknown,
    Disclosure,
    GeneralMeeting,
    Supervisory,
    Underlying,
}

impl HaltReason {
    pub fn as_str(self) -> &'static str {
        match self {
            HaltReason::Unknown => "Unknown",
            HaltReason::Disclosure => "Disclosure",
            HaltReason::GeneralMeeting => "GeneralMeeting",
            HaltReason::Supervisory => "Supervisory",
            HaltReason::Underlying => "Underlying",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Unknown" => Some(HaltReason::Unknown),
            "Disclosure" => Some(HaltReason::Disclosure),
            "GeneralMeeting" => Some(HaltReason::GeneralMeeting),
            "Supervisory" => Some(HaltReason::Supervisory),
            "Underlying" => Some(HaltReason::Underlying),
            _ => None,
        }
    }
}

/// A market-supervisor message (پیام ناظر).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SupervisorMessage {
    pub id: i64,
    pub date: Date,
    pub time: Option<Time>,
    pub title: String,
    pub body: Option<String>,
    pub flow: i32,
    pub kind: MessageKind,
    pub reason: HaltReason,
    /// True when the text says the symbol reopens without a price limit (بدون محدودیت).
    pub reopens_without_limit: bool,
    /// Symbols written in parentheses in the title or body, normalized.
    pub mentioned_symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShareholderPosition {
    pub ins_code: i64,
    pub snapshot_date: Date,
    pub holder_share_id: i64,
    pub holder_name: String,
    pub shares: Decimal,
    pub percent: Decimal,
    pub change: i32,
    pub change_amount: Decimal,
}

/// State of an instrument on one date (GetInstrumentState).
#[derive(Debug, Clone, PartialEq)]
pub struct InstrumentStateOnDate {
    pub ins_code: i64,
    pub date: Date,
    pub state: Option<String>,
    pub time: Option<Time>,
}

impl InstrumentStateOnDate {
    pub fn is_halted(&self) -> bool {
        self.state.as_deref().is_some_and(|s| s != "A")
    }
}

/// `Zero` re-export for callers that compare decimals.
pub fn is_zero(value: Decimal) -> bool {
    value.is_zero()
}
