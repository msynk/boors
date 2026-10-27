//! Embedded DuckDB store for market data (C# `MarketStore`).

pub mod schema;
mod store;
pub mod values;

pub use store::*;
pub use values::Cell;
