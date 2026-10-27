//! Domain records, instrument classes, the day-status classifier and the market calendar.

mod calendar;
mod day_status;
pub mod deven;
mod instrument;
mod market_data;

pub use calendar::{MarketCalendar, detect_flat_index_days};
pub use day_status::{ClassifiedBar, DayStatus, DayStatusClassifier};
pub use instrument::{Instrument, InstrumentClass, classify, classify_by_isin};
pub use market_data::*;
