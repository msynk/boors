//! What a daily row means (C# `DayStatus` and `DayStatusClassifier`).

use std::collections::{HashMap, HashSet};

use super::calendar::MarketCalendar;
use super::market_data::DailyBar;
use crate::{Date, Decimal};

/// What a daily row actually means. Statistics use traded-like days only; non-traded days get null returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DayStatus {
    Traded,
    NoTrade,
    SymbolHalted,
    MarketClosed,
    ReopenAuction,
    QueueLockedUp,
    QueueLockedDown,
}

impl DayStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DayStatus::Traded => "Traded",
            DayStatus::NoTrade => "NoTrade",
            DayStatus::SymbolHalted => "SymbolHalted",
            DayStatus::MarketClosed => "MarketClosed",
            DayStatus::ReopenAuction => "ReopenAuction",
            DayStatus::QueueLockedUp => "QueueLockedUp",
            DayStatus::QueueLockedDown => "QueueLockedDown",
        }
    }

    /// C# `Enum.TryParse<DayStatus>` (case-sensitive names).
    pub fn parse(value: &str) -> Option<DayStatus> {
        match value {
            "Traded" => Some(DayStatus::Traded),
            "NoTrade" => Some(DayStatus::NoTrade),
            "SymbolHalted" => Some(DayStatus::SymbolHalted),
            "MarketClosed" => Some(DayStatus::MarketClosed),
            "ReopenAuction" => Some(DayStatus::ReopenAuction),
            "QueueLockedUp" => Some(DayStatus::QueueLockedUp),
            "QueueLockedDown" => Some(DayStatus::QueueLockedDown),
            _ => None,
        }
    }

    /// True for days with a real, comparable trading price.
    pub fn has_price(self) -> bool {
        matches!(self, DayStatus::Traded | DayStatus::ReopenAuction | DayStatus::QueueLockedUp | DayStatus::QueueLockedDown)
    }
}

impl std::fmt::Display for DayStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifiedBar {
    pub bar: DailyBar,
    pub status: DayStatus,
}

impl ClassifiedBar {
    pub fn new(bar: DailyBar, status: DayStatus) -> Self {
        Self { bar, status }
    }

    pub fn has_price(&self) -> bool {
        self.status.has_price()
    }
}

/// Classifies one symbol's daily bars.
pub struct DayStatusClassifier<'a> {
    calendar: &'a MarketCalendar,
    price_limit: Box<dyn Fn(Date) -> Decimal + 'a>,
    /// Relative tolerance when comparing a price with the theoretical limit (TSETMC rounds the band to the tick).
    pub limit_tolerance: Decimal,
}

impl<'a> DayStatusClassifier<'a> {
    pub fn new(calendar: &'a MarketCalendar, price_limit: impl Fn(Date) -> Decimal + 'a) -> Self {
        Self { calendar, price_limit: Box::new(price_limit), limit_tolerance: Decimal::new(5, 3) }
    }

    pub fn classify(
        &self,
        bars: &[DailyBar],
        halted_days: Option<&HashSet<Date>>,
        allowed_ranges: Option<&HashMap<Date, (Decimal, Decimal)>>,
    ) -> Vec<ClassifiedBar> {
        let mut ordered: Vec<&DailyBar> = bars.iter().collect();
        ordered.sort_by_key(|b| b.date);
        let mut result = Vec::with_capacity(ordered.len());
        let mut previous_open_market_day_was_halted = false;

        for bar in ordered {
            let status = if self.calendar.is_market_closed(bar.date) {
                DayStatus::MarketClosed
            } else if bar.trade_count <= 0 {
                let status = if halted_days.is_some_and(|h| h.contains(&bar.date)) { DayStatus::SymbolHalted } else { DayStatus::NoTrade };
                previous_open_market_day_was_halted = status == DayStatus::SymbolHalted;
                status
            } else {
                let band =
                    allowed_ranges.and_then(|r| r.get(&bar.date).copied()).unwrap_or_else(|| theoretical_band(bar.yesterday, (self.price_limit)(bar.date)));

                let status = if previous_open_market_day_was_halted || self.breaks_band(bar, band) {
                    DayStatus::ReopenAuction
                } else if bar.high == bar.low && self.is_at_or_above(bar.close, band.1) && bar.close > bar.yesterday {
                    DayStatus::QueueLockedUp
                } else if bar.high == bar.low && self.is_at_or_below(bar.close, band.0) && bar.close < bar.yesterday {
                    DayStatus::QueueLockedDown
                } else {
                    DayStatus::Traded
                };
                previous_open_market_day_was_halted = false;
                status
            };

            result.push(ClassifiedBar::new(bar.clone(), status));
        }

        result
    }

    fn breaks_band(&self, bar: &DailyBar, band: (Decimal, Decimal)) -> bool {
        if bar.yesterday <= Decimal::ZERO {
            return false;
        }
        let slack = bar.yesterday * self.limit_tolerance;
        bar.high > band.1 + slack || bar.low < band.0 - slack
    }

    fn is_at_or_above(&self, price: Decimal, limit: Decimal) -> bool {
        price >= limit - limit * self.limit_tolerance
    }

    fn is_at_or_below(&self, price: Decimal, limit: Decimal) -> bool {
        price <= limit + limit * self.limit_tolerance
    }
}

fn theoretical_band(yesterday: Decimal, limit: Decimal) -> (Decimal, Decimal) {
    (yesterday * (Decimal::ONE - limit), yesterday * (Decimal::ONE + limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day1() -> Date {
        NaiveDate::from_ymd_opt(2026, 10, 11).unwrap()
    }

    fn day2() -> Date {
        NaiveDate::from_ymd_opt(2026, 10, 12).unwrap()
    }

    fn d(value: i64) -> Decimal {
        Decimal::from(value)
    }

    fn bar(date: Date, yesterday: i64, low: i64, high: i64, close: i64, trades: i64) -> DailyBar {
        DailyBar {
            ins_code: 1,
            date,
            yesterday: d(yesterday),
            open: d(low),
            low: d(low),
            high: d(high),
            close: d(close),
            last: d(close),
            trade_count: trades,
            volume: trades * 1000,
            value: d(trades * 1000 * close),
            last_trade_time: None,
        }
    }

    fn classify_one(b: DailyBar) -> DayStatus {
        let calendar = MarketCalendar::new(Vec::new(), Vec::new());
        let classifier = DayStatusClassifier::new(&calendar, |_| Decimal::new(3, 2));
        classifier.classify(&[b], None, None)[0].status
    }

    #[test]
    fn single_price_days_are_locked_only_at_the_band_edge() {
        assert_eq!(DayStatus::QueueLockedUp, classify_one(bar(day1(), 1000, 1030, 1030, 1030, 100)));
        assert_eq!(DayStatus::QueueLockedUp, classify_one(bar(day1(), 4540, 4670, 4670, 4670, 100)));
        assert_eq!(DayStatus::QueueLockedDown, classify_one(bar(day1(), 4540, 4410, 4410, 4410, 100)));
        assert_eq!(DayStatus::Traded, classify_one(bar(day1(), 1000, 1010, 1010, 1010, 100)));
    }

    #[test]
    fn a_range_beyond_the_band_is_an_unlimited_reopen_auction() {
        assert_eq!(DayStatus::ReopenAuction, classify_one(bar(day1(), 1000, 1100, 1200, 1150, 100)));
    }

    #[test]
    fn first_trade_after_a_known_halt_is_a_reopen_auction_even_inside_the_band() {
        let calendar = MarketCalendar::new(Vec::new(), Vec::new());
        let classifier = DayStatusClassifier::new(&calendar, |_| Decimal::new(3, 2));
        let halted: HashSet<Date> = [day1()].into_iter().collect();
        let result = classifier.classify(&[bar(day1(), 1000, 1000, 1000, 1000, 0), bar(day2(), 1000, 990, 1010, 1000, 100)], Some(&halted), None);
        assert_eq!(DayStatus::SymbolHalted, result[0].status);
        assert_eq!(DayStatus::ReopenAuction, result[1].status);
    }

    #[test]
    fn zero_trades_without_halt_information_is_no_trade() {
        assert_eq!(DayStatus::NoTrade, classify_one(bar(day1(), 1000, 1000, 1000, 1000, 0)));
    }

    #[test]
    fn known_allowed_range_takes_precedence_over_the_theoretical_band() {
        let calendar = MarketCalendar::new(Vec::new(), Vec::new());
        let classifier = DayStatusClassifier::new(&calendar, |_| Decimal::new(3, 2));
        let ranges: HashMap<Date, (Decimal, Decimal)> = [(day1(), (d(980), d(1020)))].into_iter().collect();
        let result = classifier.classify(&[bar(day1(), 1000, 1020, 1020, 1020, 100)], None, Some(&ranges));
        assert_eq!(DayStatus::QueueLockedUp, result[0].status);
    }
}
