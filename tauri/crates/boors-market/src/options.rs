//! Configuration of the market-data layer (C# `MarketDataOptions`).

use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct MarketDataOptions {
    /// Root for the raw cache and parsed outputs.
    pub data_root: PathBuf,

    /// Path of the DuckDB market store; defaults to `{data_root}/market.duckdb`. `:memory:` keeps it in memory.
    pub store_path: Option<String>,

    /// Name of this collector node (e.g. "iran-1", "eu-1"), recorded on every raw entry.
    pub node: String,

    pub tsetmc_base_url: String,
    pub codal_search_base_url: String,
    pub codal_base_url: String,

    /// Requests per second to cdn.tsetmc.com (≤ 2 is the agreed conservative default).
    pub tsetmc_requests_per_second: f64,

    /// Codal answers HTTP 429 to bursts; one request every 2 s.
    pub codal_requests_per_second: f64,

    /// tgju, fipiran and other secondary sources.
    pub other_sources_requests_per_second: f64,

    /// A browser User-Agent. cdn.tsetmc.com answers 403 without one.
    pub user_agent: String,

    /// Timeout for one attempt. Large exports need up to 90 s.
    pub attempt_timeout: Duration,

    /// Overall timeout including retries.
    pub total_timeout: Duration,

    pub max_retry_attempts: u32,
    /// Upper bound for an on-demand refresh made while serving a page (e.g. shareholders); on expiry the stored
    /// snapshot is served instead of keeping the page waiting through every retry.
    pub live_refresh_timeout: Duration,

    /// Server only: schedule the collection jobs on this node.
    pub collect: bool,

    pub max_instrument_details_per_run: usize,
    pub max_backfill_per_run: usize,
    pub max_halt_queries_per_run: usize,

    /// Optional path to a rules.yaml that overrides the embedded copy.
    pub rules_path: Option<PathBuf>,
}

impl Default for MarketDataOptions {
    fn default() -> Self {
        Self {
            data_root: PathBuf::from(".boors-data"),
            store_path: None,
            node: machine_name(),
            tsetmc_base_url: "https://cdn.tsetmc.com/api/".into(),
            codal_search_base_url: "https://search.codal.ir/api/search/".into(),
            codal_base_url: "https://codal.ir/".into(),
            tsetmc_requests_per_second: 2.0,
            codal_requests_per_second: 0.5,
            other_sources_requests_per_second: 1.0,
            user_agent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36".into(),
            attempt_timeout: Duration::from_secs(90),
            total_timeout: Duration::from_secs(5 * 60),
            max_retry_attempts: 4,
            live_refresh_timeout: Duration::from_secs(10),
            collect: false,
            max_instrument_details_per_run: 300,
            max_backfill_per_run: 400,
            max_halt_queries_per_run: 500,
            rules_path: None,
        }
    }
}

impl MarketDataOptions {
    /// The DuckDB file (or `:memory:`).
    pub fn resolved_store_path(&self) -> String {
        match &self.store_path {
            Some(path) => path.clone(),
            None => self.data_root.join("market.duckdb").to_string_lossy().into_owned(),
        }
    }
}

/// The machine name, like .NET's `Environment.MachineName`.
pub fn machine_name() -> String {
    std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok().filter(|name| !name.trim().is_empty()).unwrap_or_else(|| "localhost".to_string())
}
