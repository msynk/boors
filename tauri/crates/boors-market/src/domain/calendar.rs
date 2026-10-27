//! Which dates the whole market was closed (C# `MarketCalendar`).

use std::collections::HashSet;

use super::market_data::IndexBar;
use crate::Date;
use crate::rules::{ClosureRange, MarketRules};

/// Closures come from the seeded list in rules.yaml and from days on which the index did not move at all.
#[derive(Debug, Clone, Default)]
pub struct MarketCalendar {
    known_closures: Vec<ClosureRange>,
    detected_closed_days: HashSet<Date>,
}

impl MarketCalendar {
    pub fn new(known_closures: Vec<ClosureRange>, detected_closed_days: impl IntoIterator<Item = Date>) -> Self {
        Self { known_closures, detected_closed_days: detected_closed_days.into_iter().collect() }
    }

    pub fn from_rules(rules: &MarketRules, reference_index: Option<&[IndexBar]>) -> Self {
        Self::new(rules.known_closures().to_vec(), reference_index.map(detect_flat_index_days).unwrap_or_default())
    }

    pub fn is_market_closed(&self, date: Date) -> bool {
        self.detected_closed_days.contains(&date) || self.known_closures.iter().any(|c| c.contains(date))
    }

    pub fn detected_closed_days(&self) -> &HashSet<Date> {
        &self.detected_closed_days
    }
}

/// Days on which the reference index (normally TEDPIX) closed exactly where it closed on the previous row.
pub fn detect_flat_index_days(bars: &[IndexBar]) -> Vec<Date> {
    let mut ordered: Vec<&IndexBar> = bars.iter().collect();
    ordered.sort_by_key(|b| b.date);
    ordered.windows(2).filter(|w| w[1].close == w[0].close).map(|w| w[1].date).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn calendar_combines_seeded_and_detected_closures() {
        let d = |y, m, day| NaiveDate::from_ymd_opt(y, m, day).unwrap();
        let calendar = MarketCalendar::new(vec![ClosureRange { from: d(2026, 3, 1), to: d(2026, 3, 3), reason: "test".into() }], [d(2018, 7, 26)]);
        assert!(calendar.is_market_closed(d(2026, 3, 2)));
        assert!(calendar.is_market_closed(d(2018, 7, 26)));
        assert!(!calendar.is_market_closed(d(2026, 3, 4)));
    }
}
