//! The Boors market-data core: a port of the .NET `Boors.Market` library.
//!
//! Data flows `source → HTTP (browser UA, retry, circuit breaker, token bucket) → raw cache (write-once, gzip) →
//! parsers (pure) → classifier (day status, halts) → market store (DuckDB, idempotent merges) → analytics / API`.
//!
//! The raw cache is format-compatible with the .NET `FileSystemRawCache`, so caches and bundles move freely
//! between the .NET, WinUI and Rust implementations. The store is a projection of the cache and can be rebuilt
//! from it at any time (`boors rebuild`).

pub mod analytics;
pub mod api;
pub mod context;
pub mod contracts;
pub mod domain;
pub mod error;
pub mod filters;
pub mod ingestion;
pub mod options;
pub mod probe;
pub mod raw_cache;
pub mod rules;
pub mod sources;
pub mod storage;
pub mod text;

pub use context::MarketContext;
pub use error::{Error, Result};
pub use options::MarketDataOptions;

/// Re-exported so consumers use the same decimal type as the library (C# `decimal` semantics: 96-bit mantissa).
pub use rust_decimal::Decimal;

/// Calendar date (C# `DateOnly`).
pub type Date = chrono::NaiveDate;

/// Time of day (C# `TimeOnly`).
pub type Time = chrono::NaiveTime;

/// An instant in UTC (C# `DateTimeOffset` as written by the collectors).
pub type Timestamp = chrono::DateTime<chrono::Utc>;
