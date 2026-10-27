//! cdn.tsetmc.com: endpoints, the typed client and the parsers.

pub mod parser;

use crate::domain::deven;
use crate::domain::*;
use crate::raw_cache::RawRequest;
use crate::sources::{CachePolicy, SourceClient, escape_data_string};
use crate::text;
use crate::{Date, Result};

pub use parser::{InstrumentSearchHit, MarketWatchSnapshot};

pub const DEFAULT_BASE_URL: &str = "https://cdn.tsetmc.com/api/";
pub const SOURCE: &str = "tsetmc";

/// TEDPIX (شاخص کل).
pub const TEDPIX_INS_CODE: i64 = 32097828799138957;

/// Parameters of the bulk market-watch call; hEven/RefID enable incremental polling.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketWatchQuery {
    /// PaperType codes from GetStaticData: 1 shares, 2 IFB base market, ... 9 commodity exchange.
    pub paper_types: Vec<i32>,
    pub h_even: i32,
    pub ref_id: i64,
    pub with_best_limits: bool,
    pub show_traded: bool,
}

impl Default for MarketWatchQuery {
    fn default() -> Self {
        Self { paper_types: (1..=9).collect(), h_even: 0, ref_id: 0, with_best_limits: true, show_traded: false }
    }
}

/// Builds [`RawRequest`]s for cdn.tsetmc.com. Endpoint names are the API paths.
#[derive(Debug, Clone)]
pub struct TsetmcEndpoints {
    base_url: String,
}

impl Default for TsetmcEndpoints {
    fn default() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

impl TsetmcEndpoints {
    pub fn new(base_url: &str) -> Self {
        let base_url = if base_url.ends_with('/') { base_url.to_string() } else { format!("{base_url}/") };
        Self { base_url }
    }

    pub fn instrument_info(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("Instrument/GetInstrumentInfo", ins_code)
    }

    pub fn instrument_identity(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("Instrument/GetInstrumentIdentity", ins_code)
    }

    pub fn instrument_share_change(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("Instrument/GetInstrumentShareChange", ins_code)
    }

    pub fn instrument_search(&self, text: &str) -> RawRequest {
        RawRequest::new(
            SOURCE,
            "Instrument/GetInstrumentSearch",
            format!("{}Instrument/GetInstrumentSearch/{}", self.base_url, escape_data_string(text)),
            format!("text={text}"),
        )
    }

    pub fn closing_price_info(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("ClosingPrice/GetClosingPriceInfo", ins_code)
    }

    /// Full daily history (n = 0 means all rows).
    pub fn closing_price_daily_list(&self, ins_code: i64, count: i32) -> RawRequest {
        RawRequest::new(
            SOURCE,
            "ClosingPrice/GetClosingPriceDailyList",
            format!("{}ClosingPrice/GetClosingPriceDailyList/{ins_code}/{count}", self.base_url),
            format!("insCode={ins_code}&n={count}"),
        )
    }

    pub fn price_adjust_list(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("ClosingPrice/GetPriceAdjustList", ins_code)
    }

    pub fn client_type_history(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("ClientType/GetClientTypeHistory", ins_code)
    }

    pub fn best_limits(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("BestLimits", ins_code)
    }

    pub fn index_history(&self, index_ins_code: i64) -> RawRequest {
        self.for_instrument("Index/GetIndexB2History", index_ins_code)
    }

    /// Same-day individual/institutional volumes and counts for every instrument.
    pub fn client_type_all(&self) -> RawRequest {
        RawRequest::new(SOURCE, "ClientType/GetClientTypeAll", format!("{}ClientType/GetClientTypeAll", self.base_url), "")
    }

    /// Latest values of all indices of a market (flow 1 = TSE, 2 = IFB).
    pub fn index_snapshots(&self, flow: i32) -> RawRequest {
        RawRequest::new(SOURCE, "Index/GetIndexB1LastAll", format!("{}Index/GetIndexB1LastAll/All/{flow}", self.base_url), format!("flow={flow}"))
    }

    pub fn messages_by_instrument(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("Msg/GetMsgByInsCode", ins_code)
    }

    /// The latest `top` supervisor messages of a market.
    pub fn messages_by_flow(&self, flow: i32, top: i32) -> RawRequest {
        RawRequest::new(SOURCE, "Msg/GetMsgByFlow", format!("{}Msg/GetMsgByFlow/{flow}/{top}", self.base_url), format!("flow={flow}&top={top}"))
    }

    pub fn shareholders(&self, ins_code: i64) -> RawRequest {
        self.for_instrument("Shareholder/GetInstrumentShareHolderLast", ins_code)
    }

    pub fn instrument_state(&self, ins_code: i64, date: Date) -> RawRequest {
        let d = deven::from_date(date);
        RawRequest::new(
            SOURCE,
            "MarketData/GetInstrumentState",
            format!("{}MarketData/GetInstrumentState/{ins_code}/{d}", self.base_url),
            format!("insCode={ins_code}&date={d}"),
        )
    }

    /// Per-instrument statistics for all instruments; data type N is the filter variable `[isN]`.
    pub fn instrument_statistics(&self) -> RawRequest {
        RawRequest::new(SOURCE, "MarketData/GetInstValueAllInstAllParam", format!("{}MarketData/GetInstValueAllInstAllParam", self.base_url), "")
    }

    pub fn static_data(&self) -> RawRequest {
        RawRequest::new(SOURCE, "StaticData/GetStaticData", format!("{}StaticData/GetStaticData", self.base_url), "")
    }

    pub fn market_watch(&self, query: &MarketWatchQuery) -> RawRequest {
        let mut parameters: Vec<(String, String)> = vec![("market".into(), "0".into()), ("industrialGroup".into(), String::new())];
        for (i, paper_type) in query.paper_types.iter().enumerate() {
            parameters.push((format!("paperTypes[{i}]"), paper_type.to_string()));
        }
        parameters.push(("showTraded".into(), if query.show_traded { "true" } else { "false" }.into()));
        parameters.push(("withBestLimits".into(), if query.with_best_limits { "true" } else { "false" }.into()));
        parameters.push(("hEven".into(), query.h_even.to_string()));
        parameters.push(("RefID".into(), query.ref_id.to_string()));

        let query_string = parameters.iter().map(|(k, v)| format!("{}={}", escape_data_string(k), escape_data_string(v))).collect::<Vec<_>>().join("&");
        let params_key = parameters.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
        RawRequest::new(SOURCE, "ClosingPrice/GetMarketWatch", format!("{}ClosingPrice/GetMarketWatch?{query_string}", self.base_url), params_key)
    }

    fn for_instrument(&self, endpoint: &str, ins_code: i64) -> RawRequest {
        RawRequest::new(SOURCE, endpoint, format!("{}{endpoint}/{ins_code}", self.base_url), format!("insCode={ins_code}"))
    }
}

/// Typed access to cdn.tsetmc.com. Each call stores the raw response, then parses it.
#[derive(Debug, Clone)]
pub struct TsetmcClient {
    pub client: SourceClient,
    pub endpoints: TsetmcEndpoints,
}

impl TsetmcClient {
    pub fn new(client: SourceClient, base_url: &str) -> Self {
        Self { client, endpoints: TsetmcEndpoints::new(base_url) }
    }

    pub async fn get_body(&self, request: &RawRequest, policy: CachePolicy) -> Result<Vec<u8>> {
        self.client.get_body(request, policy).await
    }

    pub async fn get_instrument_info(&self, ins_code: i64, policy: CachePolicy) -> Result<Instrument> {
        parser::parse_instrument_info(&self.get_body(&self.endpoints.instrument_info(ins_code), policy).await?, ins_code)
    }

    pub async fn search(&self, text: &str, policy: CachePolicy) -> Result<Vec<InstrumentSearchHit>> {
        parser::parse_instrument_search(&self.get_body(&self.endpoints.instrument_search(text), policy).await?)
    }

    /// Resolves a ticker to the currently active instrument: exact normalized symbol match, preferring the row
    /// TSETMC marks active. Tries the Persian spelling first, then the Arabic one.
    pub async fn resolve_symbol(&self, symbol: &str, policy: CachePolicy) -> Result<Option<InstrumentSearchHit>> {
        let key = text::match_key(symbol);
        let persian = text::normalize_str(symbol).unwrap_or_else(|| symbol.to_string());
        let mut candidates = vec![persian.clone()];
        let arabic = text::to_arabic_letters(&persian);
        if arabic != persian {
            candidates.push(arabic);
        }

        for candidate in candidates {
            let mut hits: Vec<InstrumentSearchHit> = self.search(&candidate, policy).await?.into_iter().filter(|h| text::match_key(&h.symbol) == key).collect();
            // OrderByDescending(IsActive).ThenBy(flow 1/2 first); stable.
            hits.sort_by_key(|h| (!h.is_active, if matches!(h.flow, 1 | 2) { 0 } else { 1 }));
            if let Some(hit) = hits.into_iter().next() {
                return Ok(Some(hit));
            }
        }
        Ok(None)
    }

    pub async fn get_daily_history(&self, ins_code: i64, policy: CachePolicy) -> Result<Vec<DailyBar>> {
        parser::parse_daily_history(&self.get_body(&self.endpoints.closing_price_daily_list(ins_code, 0), policy).await?, ins_code)
    }

    pub async fn get_closing_price_info(&self, ins_code: i64, policy: CachePolicy) -> Result<ClosingPriceSnapshot> {
        parser::parse_closing_price_info(&self.get_body(&self.endpoints.closing_price_info(ins_code), policy).await?, ins_code)
    }

    pub async fn get_client_type_history(&self, ins_code: i64, policy: CachePolicy) -> Result<Vec<ClientTypeDay>> {
        parser::parse_client_type_history(&self.get_body(&self.endpoints.client_type_history(ins_code), policy).await?, ins_code)
    }

    pub async fn get_price_adjustments(&self, ins_code: i64, policy: CachePolicy) -> Result<Vec<PriceAdjustment>> {
        parser::parse_price_adjustments(&self.get_body(&self.endpoints.price_adjust_list(ins_code), policy).await?, ins_code)
    }

    pub async fn get_share_changes(&self, ins_code: i64, policy: CachePolicy) -> Result<Vec<ShareChange>> {
        parser::parse_share_changes(&self.get_body(&self.endpoints.instrument_share_change(ins_code), policy).await?, ins_code)
    }

    pub async fn get_best_limits(&self, ins_code: i64, policy: CachePolicy) -> Result<Vec<BestLimitLevel>> {
        parser::parse_best_limits(&self.get_body(&self.endpoints.best_limits(ins_code), policy).await?)
    }

    pub async fn get_index_history(&self, index_ins_code: i64, policy: CachePolicy) -> Result<Vec<IndexBar>> {
        parser::parse_index_history(&self.get_body(&self.endpoints.index_history(index_ins_code), policy).await?, index_ins_code)
    }

    pub async fn get_static_data(&self, policy: CachePolicy) -> Result<Vec<StaticDataItem>> {
        parser::parse_static_data(&self.get_body(&self.endpoints.static_data(), policy).await?)
    }

    pub async fn get_client_type_all(&self, policy: CachePolicy) -> Result<Vec<ClientTypeSnapshot>> {
        parser::parse_client_type_all(&self.get_body(&self.endpoints.client_type_all(), policy).await?)
    }

    pub async fn get_index_snapshots(&self, flow: i32, policy: CachePolicy) -> Result<Vec<IndexSnapshot>> {
        parser::parse_index_snapshots(&self.get_body(&self.endpoints.index_snapshots(flow), policy).await?)
    }

    pub async fn get_messages(&self, ins_code: i64, policy: CachePolicy) -> Result<Vec<SupervisorMessage>> {
        parser::parse_messages(&self.get_body(&self.endpoints.messages_by_instrument(ins_code), policy).await?)
    }

    pub async fn get_latest_messages(&self, flow: i32, top: i32, policy: CachePolicy) -> Result<Vec<SupervisorMessage>> {
        parser::parse_messages(&self.get_body(&self.endpoints.messages_by_flow(flow, top), policy).await?)
    }

    pub async fn get_shareholders(&self, ins_code: i64, snapshot_date: Date, policy: CachePolicy) -> Result<Vec<ShareholderPosition>> {
        parser::parse_shareholders(&self.get_body(&self.endpoints.shareholders(ins_code), policy).await?, ins_code, snapshot_date)
    }

    pub async fn get_instrument_state(&self, ins_code: i64, date: Date, policy: CachePolicy) -> Result<InstrumentStateOnDate> {
        parser::parse_instrument_state(&self.get_body(&self.endpoints.instrument_state(ins_code, date), policy).await?, ins_code, date)
    }

    pub async fn get_instrument_statistics(&self, policy: CachePolicy) -> Result<Vec<InstrumentStatistic>> {
        parser::parse_instrument_statistics(&self.get_body(&self.endpoints.instrument_statistics(), policy).await?)
    }

    pub async fn get_market_watch(&self, query: &MarketWatchQuery, policy: CachePolicy) -> Result<MarketWatchSnapshot> {
        parser::parse_market_watch(&self.get_body(&self.endpoints.market_watch(query), policy).await?)
    }
}
