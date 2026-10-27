//! Everything the market-data layer needs, wired once (the C# DI registrations in
//! `MarketDataServiceCollectionExtensions`).

use std::sync::Arc;

use crate::Result;
use crate::analytics::{MarketAnalytics, MarketWatchService};
use crate::ingestion::{DataHealthService, MarketCollector};
use crate::options::MarketDataOptions;
use crate::raw_cache::RawCache;
use crate::rules::MarketRules;
use crate::sources::SourceClient;
use crate::sources::codal::CodalClient;
use crate::sources::fipiran::FipiranClient;
use crate::sources::http::SourceHttp;
use crate::sources::tgju::TgjuClient;
use crate::sources::tsetmc::TsetmcClient;
use crate::storage::MarketStore;

#[derive(Debug)]
pub struct MarketContext {
    pub options: MarketDataOptions,
    pub rules: MarketRules,
    pub cache: Arc<RawCache>,
    pub store: Arc<MarketStore>,
    pub tsetmc: TsetmcClient,
    pub codal: CodalClient,
    pub tgju: TgjuClient,
    pub fipiran: FipiranClient,
    pub analytics: MarketAnalytics,
    pub market_watch: MarketWatchService,
}

impl MarketContext {
    /// Opens the raw cache, the rules and the store, and builds the source clients.
    pub fn open(options: MarketDataOptions) -> Result<Arc<MarketContext>> {
        let store = Arc::new(MarketStore::open(&options.resolved_store_path())?);
        Self::with_store(options, store)
    }

    /// Like [`open`](Self::open) with an already opened store (tests use an in-memory one).
    pub fn with_store(options: MarketDataOptions, store: Arc<MarketStore>) -> Result<Arc<MarketContext>> {
        let rules = MarketRules::load(options.rules_path.as_deref())?;
        let cache = Arc::new(RawCache::new(options.data_root.clone(), options.node.clone()));

        let client = |name: &str, rate: f64| -> Result<SourceClient> { Ok(SourceClient::new(Arc::new(SourceHttp::new(name, &options, rate)?), cache.clone())) };
        let tsetmc = TsetmcClient::new(client("tsetmc", options.tsetmc_requests_per_second)?, &options.tsetmc_base_url);
        let codal = CodalClient::new(client("codal", options.codal_requests_per_second)?, &options.codal_search_base_url, &options.codal_base_url);
        let tgju = TgjuClient::new(client("tgju", options.other_sources_requests_per_second)?);
        let fipiran = FipiranClient::new(client("fipiran", options.other_sources_requests_per_second)?);

        let analytics = MarketAnalytics::new(store.clone());
        let market_watch = MarketWatchService::new(analytics.clone());
        Ok(Arc::new(MarketContext { options, rules, cache, store, tsetmc, codal, tgju, fipiran, analytics, market_watch }))
    }

    pub fn collector(&self) -> MarketCollector<'_> {
        MarketCollector::new(self)
    }

    pub fn health(&self) -> DataHealthService<'_> {
        DataHealthService::new(self)
    }
}
