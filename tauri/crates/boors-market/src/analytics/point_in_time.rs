//! Point-in-time price handling (C# `PointInTime`). Raw prices are stored unchanged; adjusted series are built on
//! demand "as of" a date using only corporate actions known by then.

use crate::domain::{DailyBar, DayStatus, PriceAdjustment};
use crate::{Date, Decimal};

/// A daily bar with its point-in-time adjusted close and its return.
#[derive(Debug, Clone, PartialEq)]
pub struct AdjustedBar {
    pub date: Date,
    pub status: Option<DayStatus>,
    pub close: Decimal,
    pub adjusted_close: Decimal,
    pub daily_return: Option<Decimal>,
}

/// Daily return of a day with a real price: `Close / Yesterday − 1`; `None` without a real price (never zero).
pub fn daily_return(bar: &DailyBar, status: Option<DayStatus>) -> Option<Decimal> {
    let priced = status.is_none_or(DayStatus::has_price);
    if priced && bar.trade_count > 0 && bar.yesterday > Decimal::ZERO { bar.close.checked_div(bar.yesterday).map(|r| r - Decimal::ONE) } else { None }
}

/// Adjusted closes as of `as_of`: every close before an adjustment event dated on or before `as_of` is multiplied by
/// that event's factor. Events after `as_of` are ignored, and bars after `as_of` are dropped.
pub fn adjust(bars: &[(DailyBar, Option<DayStatus>)], adjustments: &[PriceAdjustment], as_of: Date) -> Vec<AdjustedBar> {
    let mut ordered: Vec<&(DailyBar, Option<DayStatus>)> = bars.iter().filter(|b| b.0.date <= as_of).collect();
    ordered.sort_by_key(|b| b.0.date);
    let mut events: Vec<&PriceAdjustment> = adjustments.iter().filter(|a| a.date <= as_of && a.unadjusted_close > Decimal::ZERO).collect();
    // OrderByDescending(date), stable.
    events.sort_by_key(|e| std::cmp::Reverse(e.date));

    let mut result: Vec<Option<AdjustedBar>> = vec![None; ordered.len()];
    let mut factor = Decimal::ONE;
    let mut next_event = 0;
    for i in (0..ordered.len()).rev() {
        let (bar, status) = ordered[i];
        // An event dated D adjusts every price strictly before D.
        while next_event < events.len() && events[next_event].date > bar.date {
            factor = factor.checked_mul(events[next_event].factor()).unwrap_or(factor);
            next_event += 1;
        }
        result[i] = Some(AdjustedBar {
            date: bar.date,
            status: *status,
            close: bar.close,
            adjusted_close: bar.close.checked_mul(factor).unwrap_or(bar.close),
            daily_return: daily_return(bar, *status),
        });
    }
    result.into_iter().flatten().collect()
}

/// Adjusted closes as of `as_of` using the factors implied by the bars (the default; TSETMC's published list misses
/// most capital increases since 2020 and dates events by meeting date).
pub fn adjust_from_bars(bars: &[(DailyBar, Option<DayStatus>)], as_of: Date) -> Vec<AdjustedBar> {
    let plain: Vec<DailyBar> = bars.iter().map(|b| b.0.clone()).collect();
    adjust(bars, &implied_adjustments(&plain, Decimal::new(5, 4)), as_of)
}

/// Factors implied by the bars: where `Yesterday` differs from the previous row's close, factor = Yesterday ÷
/// previous close. No trade filter: most real events fall on days without trades.
pub fn implied_adjustments(bars: &[DailyBar], tolerance: Decimal) -> Vec<PriceAdjustment> {
    let mut ordered: Vec<&DailyBar> = bars.iter().collect();
    ordered.sort_by_key(|b| b.date);
    let mut implied = Vec::new();
    for pair in ordered.windows(2) {
        let previous_close = pair[0].close;
        let yesterday = pair[1].yesterday;
        if previous_close > Decimal::ZERO
            && yesterday > Decimal::ZERO
            && let Some(ratio) = yesterday.checked_div(previous_close)
            && (ratio - Decimal::ONE).abs() > tolerance
        {
            implied.push(PriceAdjustment { ins_code: pair[1].ins_code, date: pair[1].date, adjusted_close: yesterday, unadjusted_close: previous_close });
        }
    }
    implied
}

/// [`implied_adjustments`] with the default 0.05 % tolerance.
pub fn implied_adjustments_default(bars: &[DailyBar]) -> Vec<PriceAdjustment> {
    implied_adjustments(bars, Decimal::new(5, 4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn bar(day: u32, close: i64, yesterday: i64, trades: i64) -> DailyBar {
        DailyBar {
            ins_code: 1,
            date: NaiveDate::from_ymd_opt(2026, 1, day).unwrap(),
            close: Decimal::from(close),
            yesterday: Decimal::from(yesterday),
            trade_count: trades,
            open: Decimal::from(close),
            high: Decimal::from(close),
            low: Decimal::from(close),
            last: Decimal::from(close),
            ..DailyBar::default()
        }
    }

    fn bars() -> Vec<DailyBar> {
        vec![bar(1, 100, 100, 10), bar(2, 110, 100, 10), bar(3, 60, 55, 10), bar(4, 63, 60, 10)]
    }

    #[test]
    fn daily_return_uses_the_exchange_base_price_and_is_null_without_a_price() {
        let b = bars();
        assert_eq!(Some(Decimal::new(1, 1)), daily_return(&b[1], Some(DayStatus::Traded)));
        assert_eq!(Some(Decimal::from(60) / Decimal::from(55) - Decimal::ONE), daily_return(&b[2], Some(DayStatus::Traded)));
        assert_eq!(None, daily_return(&bar(5, 63, 63, 0), Some(DayStatus::NoTrade)));
        assert_eq!(None, daily_return(&b[1], Some(DayStatus::MarketClosed)));
    }

    #[test]
    fn implied_adjustments_come_from_base_price_breaks() {
        let implied = implied_adjustments_default(&bars());
        assert_eq!(1, implied.len());
        assert_eq!(NaiveDate::from_ymd_opt(2026, 1, 3).unwrap(), implied[0].date);
        assert_eq!(Decimal::new(5, 1), implied[0].factor());
    }

    #[test]
    fn adjustment_is_point_in_time() {
        let with_status: Vec<(DailyBar, Option<DayStatus>)> = bars().into_iter().map(|b| (b, Some(DayStatus::Traded))).collect();
        let before = adjust_from_bars(&with_status, NaiveDate::from_ymd_opt(2026, 1, 2).unwrap());
        let after = adjust_from_bars(&with_status, NaiveDate::from_ymd_opt(2026, 1, 4).unwrap());
        assert_eq!(Decimal::from(110), before.last().unwrap().adjusted_close);
        assert_eq!(Decimal::from(55), after[1].adjusted_close);
        assert_eq!(Decimal::from(63), after.last().unwrap().adjusted_close);
        let compounded = after.iter().skip(1).fold(Decimal::ONE, |acc, b| acc * (Decimal::ONE + b.daily_return.unwrap()));
        let expected = after.last().unwrap().adjusted_close / after[0].adjusted_close;
        assert!((compounded - expected).abs() < Decimal::new(1, 20), "{compounded} vs {expected}");
    }
}
