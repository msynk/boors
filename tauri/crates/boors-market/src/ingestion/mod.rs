//! Collection jobs and data health.

mod collector;
mod health;

pub use collector::{CollectionResult, MAIN_INDICES, MarketCollector, default_scope};
pub use health::{DataHealthReport, DataHealthService, DatasetHealth, DatasetStatus, SourceReachability, is_trading_day};
