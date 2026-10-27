//! The API dispatcher: one implementation of every route in `tauri/docs/api-contract.md`, shared by the HTTP
//! server (`/api/v1/*`) and the desktop's Tauri IPC command.

pub mod dto;
pub mod jobs;
pub mod schedule;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use self::dto::*;
use self::jobs::JobQueue;
use crate::analytics::point_in_time;
use crate::analytics::{MarketWatchRequest, MoverKind, ScreenKind, normalize_class_filter};
use crate::context::MarketContext;
use crate::raw_cache;
use crate::sources::CachePolicy;
use crate::storage::min_date;
use crate::text::iran_today;
use crate::{Date, Error};

/// RFC 7807 problem body: `{ title, detail, status }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Problem {
    pub title: String,
    pub detail: Option<String>,
    pub status: u16,
}

impl Problem {
    pub fn new(status: u16, detail: impl Into<String>) -> Self {
        let title = match status {
            400 => "Bad Request",
            401 => "Unauthorized",
            404 => "Not Found",
            _ => "Internal Server Error",
        };
        Self { title: title.into(), detail: Some(detail.into()), status }
    }

    pub fn bad_request(detail: impl Into<String>) -> Self {
        Self::new(400, detail)
    }

    pub fn not_found(detail: impl Into<String>) -> Self {
        Self::new(404, detail)
    }

    pub fn unauthorized(detail: impl Into<String>) -> Self {
        Self::new(401, detail)
    }
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}: {}", self.status, self.title, self.detail.as_deref().unwrap_or(""))
    }
}

impl From<Error> for Problem {
    fn from(error: Error) -> Self {
        match error {
            Error::Argument(m) | Error::FilterSyntax(m) => Problem::bad_request(m),
            Error::NotFound(m) => Problem::not_found(m),
            other => Problem::new(500, other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Http,
    Ipc,
}

impl Transport {
    pub fn as_str(self) -> &'static str {
        match self {
            Transport::Http => "http",
            Transport::Ipc => "ipc",
        }
    }
}

/// Everything a route needs.
#[derive(Debug, Clone)]
pub struct ApiContext {
    pub market: Arc<MarketContext>,
    pub jobs: JobQueue,
    pub transport: Transport,
    pub collect: bool,
    pub version: String,
}

impl ApiContext {
    /// Builds the context and starts the job worker (call inside a tokio runtime).
    pub fn new(market: Arc<MarketContext>, transport: Transport, collect: bool) -> Arc<ApiContext> {
        let jobs = JobQueue::start(market.clone());
        Arc::new(ApiContext { market, jobs, transport, collect, version: env!("CARGO_PKG_VERSION").to_string() })
    }
}

/// Query parameters; keys are matched case-insensitively, `null` values are absent.
pub type Query = HashMap<String, String>;

type ApiResult = Result<Value, Problem>;

fn json<T: Serialize>(value: &T) -> ApiResult {
    serde_json::to_value(value).map_err(|e| Problem::new(500, e.to_string()))
}

fn param<'q>(query: &'q Query, name: &str) -> Option<&'q str> {
    query.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
}

/// ASP.NET binds a missing or empty int to 0; the controllers then substitute their default.
fn int_param(query: &Query, name: &str) -> Result<i64, Problem> {
    match param(query, name) {
        None => Ok(0),
        Some(v) => v.trim().parse::<i64>().map_err(|_| Problem::bad_request(format!("The value '{v}' is not valid for {name}."))),
    }
}

fn take(query: &Query, name: &str, default: i64) -> Result<i64, Problem> {
    let value = int_param(query, name)?;
    Ok((if value == 0 { default } else { value }).clamp(1, 500))
}

/// C# `ParseDate`: blank → none; otherwise exactly yyyy-MM-dd.
pub fn parse_date(value: Option<&str>) -> Result<Option<Date>, Problem> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok(None),
        Some(v) => match (v.len() == 10).then(|| Date::parse_from_str(v, "%Y-%m-%d").ok()).flatten() {
            Some(d) => Ok(Some(d)),
            None => Err(Problem::bad_request(format!("Invalid date '{v}' (expected yyyy-MM-dd)."))),
        },
    }
}

fn ins_code(segment: &str) -> Result<i64, Problem> {
    segment.trim().parse::<i64>().map_err(|_| Problem::bad_request(format!("The value '{segment}' is not a valid insCode.")))
}

/// Routes as parsed from `{Controller}/{Action}[/{id}]`.
#[derive(Debug, Clone, PartialEq)]
enum Route {
    Overview,
    Breadth,
    Sectors,
    Movers(String),
    Screen(String),
    IndexHistory(String),
    MacroHistory(String),
    Search,
    MarketWatch,
    Metrics(String),
    Bars(String),
    Flows(String),
    Events(String),
    Shareholders(String),
    Letters(String),
    Messages(String),
    Health,
    RunJob(String),
    Jobs,
    SyncPull,
    SyncExport,
    Info,
}

fn parse_route(method: &str, path: &str) -> Result<Route, Problem> {
    let trimmed = path.trim().trim_start_matches('/');
    let trimmed = trimmed.split('?').next().unwrap_or(trimmed);
    let trimmed = strip_prefix_ci(trimmed, "api/v1/").unwrap_or(trimmed);
    let segments: Vec<String> = trimmed.split('/').filter(|s| !s.is_empty()).map(percent_decode).collect();
    let get = method.eq_ignore_ascii_case("GET");
    let post = method.eq_ignore_ascii_case("POST");
    let not_found = || Problem::not_found(format!("No route for {} /api/v1/{trimmed}.", method.to_uppercase()));

    let lower: Vec<String> = segments.iter().map(|s| s.to_ascii_lowercase()).collect();
    let id = || segments.get(2).cloned();
    let route = match (lower.first().map(String::as_str), lower.get(1).map(String::as_str), segments.len()) {
        (Some("market"), Some("getoverview"), 2) if get => Route::Overview,
        (Some("market"), Some("getbreadth"), 2) if get => Route::Breadth,
        (Some("market"), Some("getsectors"), 2) if get => Route::Sectors,
        (Some("market"), Some("getmovers"), 3) if get => Route::Movers(id().unwrap_or_default()),
        (Some("market"), Some("getscreen"), 3) if get => Route::Screen(id().unwrap_or_default()),
        (Some("market"), Some("getindexhistory"), 3) if get => Route::IndexHistory(id().unwrap_or_default()),
        (Some("market"), Some("getmacrohistory"), 3) if get => Route::MacroHistory(id().unwrap_or_default()),
        (Some("market"), Some("search"), 2) if get => Route::Search,
        (Some("market"), Some("getmarketwatch"), 2) if post => Route::MarketWatch,
        (Some("instrument"), Some("getmetrics"), 3) if get => Route::Metrics(id().unwrap_or_default()),
        (Some("instrument"), Some("getbars"), 3) if get => Route::Bars(id().unwrap_or_default()),
        (Some("instrument"), Some("getflows"), 3) if get => Route::Flows(id().unwrap_or_default()),
        (Some("instrument"), Some("getevents"), 3) if get => Route::Events(id().unwrap_or_default()),
        (Some("instrument"), Some("getshareholders"), 3) if get => Route::Shareholders(id().unwrap_or_default()),
        (Some("instrument"), Some("getletters"), 3) if get => Route::Letters(id().unwrap_or_default()),
        (Some("instrument"), Some("getmessages"), 3) if get => Route::Messages(id().unwrap_or_default()),
        (Some("marketdata"), Some("gethealth"), 2) if get => Route::Health,
        (Some("marketdata"), Some("runjob"), 3) if post => Route::RunJob(id().unwrap_or_default()),
        (Some("marketdata"), Some("getjobs"), 2) if get => Route::Jobs,
        (Some("sync"), Some("pull"), 2) if post => Route::SyncPull,
        (Some("sync"), Some("exportbundle"), 2) if get => Route::SyncExport,
        (Some("app"), Some("getinfo"), 2) if get => Route::Info,
        _ => return Err(not_found()),
    };
    Ok(route)
}

fn strip_prefix_ci<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    (value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix)).then(|| &value[prefix.len()..])
}

fn percent_decode(segment: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// True for routes that need the operator token on a server (`RunJob`, `Sync/Pull`, `Sync/ExportBundle`).
pub fn requires_operator(method: &str, path: &str) -> bool {
    matches!(parse_route(method, path), Ok(Route::RunJob(_) | Route::SyncPull | Route::SyncExport))
}

/// True for `GET Sync/ExportBundle`, which the HTTP server answers with a zip instead of JSON.
pub fn is_export_bundle(method: &str, path: &str) -> bool {
    matches!(parse_route(method, path), Ok(Route::SyncExport))
}

/// Dispatches one call. `path` is everything after `/api/v1/` (a leading `/api/v1/` is tolerated).
pub async fn dispatch(ctx: &Arc<ApiContext>, method: &str, path: &str, query: &Query, body: Option<Value>) -> ApiResult {
    let route = parse_route(method, path)?;
    match route {
        Route::Shareholders(id) => shareholders(ctx, ins_code(&id)?).await,
        Route::RunJob(job) => {
            let id = ctx.jobs.enqueue(&job).map_err(|_| Problem::bad_request(format!("Unknown market-data job '{job}'.")))?;
            Ok(Value::String(id))
        }
        Route::Jobs => json(&ctx.jobs.list()),
        Route::SyncPull => sync_pull(ctx, body).await,
        Route::SyncExport => Err(Problem::not_found("Sync/ExportBundle is only served over HTTP.")),
        Route::Info => json(&AppInfoDto {
            version: ctx.version.clone(),
            node: ctx.market.options.node.clone(),
            data_root: std::path::absolute(&ctx.market.options.data_root)
                .unwrap_or_else(|_| ctx.market.options.data_root.clone())
                .to_string_lossy()
                .into_owned(),
            transport: ctx.transport.as_str().into(),
            collect: ctx.collect,
        }),
        other => {
            let ctx = ctx.clone();
            let query = query.clone();
            tokio::task::spawn_blocking(move || handle_read(&ctx, other, &query, body))
                .await
                .map_err(|e| Problem::new(500, format!("The request handler panicked: {e}")))?
        }
    }
}

/// The routes that only read the store (run on a blocking thread).
fn handle_read(ctx: &ApiContext, route: Route, query: &Query, body: Option<Value>) -> ApiResult {
    let market = &ctx.market;
    let analytics = &market.analytics;
    let store = &market.store;
    match route {
        Route::Overview => {
            let overview = analytics.get_overview(parse_date(param(query, "date"))?)?;
            json(&overview.as_ref().map(MarketOverviewDto::from))
        }
        Route::Breadth => {
            let sessions = int_param(query, "sessions")?;
            let sessions = (if sessions == 0 { 60 } else { sessions }).clamp(1, 250);
            json(&analytics.get_breadth(sessions)?.iter().map(BreadthDayDto::from).collect::<Vec<_>>())
        }
        Route::Sectors => json(&analytics.get_sectors(parse_date(param(query, "date"))?)?.iter().map(SectorPerformanceDto::from).collect::<Vec<_>>()),
        Route::Movers(kind) => {
            let Some(kind_value) = MoverKind::parse(&kind) else { return Err(Problem::bad_request(format!("Unknown movers kind '{kind}'."))) };
            let movers = analytics.get_movers(kind_value, take(query, "take", 20)?, parse_date(param(query, "date"))?)?;
            json(&movers.iter().map(MoverDto::from).collect::<Vec<_>>())
        }
        Route::Screen(kind) => {
            let Some(kind_value) = ScreenKind::parse(&kind) else { return Err(Problem::bad_request(format!("Unknown screen '{kind}'."))) };
            let rows = analytics.screen(kind_value, take(query, "take", 50)? as usize, None)?;
            json(&rows.iter().map(ScreenRowDto::from).collect::<Vec<_>>())
        }
        Route::IndexHistory(id) => {
            let ins = ins_code(&id)?;
            let start = parse_date(param(query, "from"))?.unwrap_or_else(min_date);
            let points: Vec<SeriesPointDto> =
                store.get_index_bars(ins)?.into_iter().filter(|b| b.date >= start).map(|b| SeriesPointDto { date: b.date, value: b.close }).collect();
            json(&points)
        }
        Route::MacroHistory(series) => {
            let start = parse_date(param(query, "from"))?.unwrap_or_else(min_date);
            let points: Vec<SeriesPointDto> = store
                .get_macro_daily(&series, None)?
                .into_iter()
                .filter(|b| b.date >= start)
                .map(|b| SeriesPointDto { date: b.date, value: b.close })
                .collect();
            json(&points)
        }
        Route::Search => {
            let text = param(query, "text").unwrap_or("");
            let results: Vec<InstrumentSearchResultDto> = analytics
                .search(text, take(query, "take", 10)? as usize)?
                .into_iter()
                .map(|i| InstrumentSearchResultDto {
                    ins_code: i.ins_code.to_string(),
                    symbol: i.symbol,
                    name: i.name,
                    instrument_class: i.class.name().into(),
                    parent_ins_code: i.parent_ins_code.map(|p| p.to_string()),
                })
                .collect();
            json(&results)
        }
        Route::MarketWatch => {
            let request: MarketWatchRequestDto = match body {
                None | Some(Value::Null) => MarketWatchRequestDto::default(),
                Some(value) => serde_json::from_value(value).map_err(|e| Problem::bad_request(format!("Invalid request body: {e}")))?,
            };
            let result = market
                .market_watch
                .query(
                    &MarketWatchRequest {
                        session: parse_date(request.date.as_deref())?,
                        filter: request.filter,
                        classes: request.classes.filter(|c| !c.is_empty()).map(|c| normalize_class_filter(&c)),
                        traded_only: request.traded_only,
                    },
                    None,
                )
                .map_err(|e| match e {
                    Error::FilterSyntax(m) => Problem::bad_request(format!("Filter error: {m}")),
                    other => other.into(),
                })?;
            json(&MarketWatchResultDto {
                session: result.session,
                evaluated: result.filter.as_ref().map_or(result.entries.len(), |f| f.evaluated),
                filter_errors: result.filter.as_ref().map(|f| f.errors.clone()).unwrap_or_default(),
                entries: result.entries.iter().map(MarketWatchEntryDto::from).collect(),
            })
        }
        Route::Metrics(id) => {
            let ins = ins_code(&id)?;
            match analytics.get_instrument_metrics(ins)? {
                Some(metrics) => json(&InstrumentMetricsDto::from(&metrics)),
                None => Err(Problem::not_found(format!("Instrument {ins} is not in the store."))),
            }
        }
        Route::Bars(id) => {
            let ins = ins_code(&id)?;
            let bars = store.get_daily_bars(ins, None)?;
            let Some(last) = bars.last() else { return json(&Vec::<BarDto>::new()) };
            let start = parse_date(param(query, "from"))?.unwrap_or_else(min_date);
            // Adjust over the full history (point-in-time as of the last bar), then cut to the requested window.
            let pairs: Vec<_> = bars.iter().map(|b| (b.bar.clone(), b.status)).collect();
            let adjusted = point_in_time::adjust_from_bars(&pairs, last.bar.date);
            let result: Vec<BarDto> = bars
                .iter()
                .zip(adjusted.iter())
                .filter(|(b, _)| b.bar.date >= start)
                .map(|(b, a)| BarDto {
                    date: b.bar.date,
                    open: b.bar.open,
                    high: b.bar.high,
                    low: b.bar.low,
                    close: b.bar.close,
                    adjusted_close: a.adjusted_close,
                    volume: b.bar.volume,
                    value: b.bar.value,
                    trades: b.bar.trade_count,
                    status: b.status.map(|s| s.as_str().to_string()),
                    daily_return: a.daily_return.map(crate::analytics::to_f64),
                    allowed_low: b.allowed_low.filter(|v| !v.is_zero()),
                    allowed_high: b.allowed_high.filter(|v| !v.is_zero()),
                })
                .collect();
            json(&result)
        }
        Route::Flows(id) => {
            let ins = ins_code(&id)?;
            let start = parse_date(param(query, "from"))?.unwrap_or_else(min_date);
            let flows: Vec<FlowDayDto> = store
                .get_client_types(ins)?
                .into_iter()
                .filter(|f| f.date >= start)
                .map(|f| FlowDayDto {
                    date: f.date,
                    buy_individual_value: f.buy_individual_value,
                    sell_individual_value: f.sell_individual_value,
                    buy_institutional_value: f.buy_institutional_value,
                    sell_institutional_value: f.sell_institutional_value,
                    buy_individual_count: f.buy_individual_count,
                    sell_individual_count: f.sell_individual_count,
                    individual_net_value: f.individual_net_value(),
                    per_capita_buy: f.per_capita_buy(),
                    per_capita_sell: f.per_capita_sell(),
                    buyer_power: f.buyer_power(),
                    values_estimated: f.values_estimated,
                })
                .collect();
            json(&flows)
        }
        Route::Events(id) => {
            let ins = ins_code(&id)?;
            let mut events: Vec<CorporateEventDto> = Vec::new();
            for source in ["implied", "published"] {
                events.extend(store.get_price_adjustments(ins, source)?.into_iter().map(|a| CorporateEventDto {
                    date: a.date,
                    kind: "adjustment".into(),
                    source: Some(source.into()),
                    factor: Some(a.factor()),
                    shares_before: None,
                    shares_after: None,
                }));
            }
            events.extend(store.get_share_changes(ins)?.into_iter().map(|s| CorporateEventDto {
                date: s.date,
                kind: "share_change".into(),
                source: None,
                factor: None,
                shares_before: Some(s.shares_before),
                shares_after: Some(s.shares_after),
            }));
            events.sort_by_key(|e| e.date);
            json(&events)
        }
        Route::Letters(id) => {
            let ins = ins_code(&id)?;
            let letters: Vec<CodalLetterDto> = store
                .get_codal_letters(ins, take(query, "take", 50)?)?
                .into_iter()
                .map(|l| CodalLetterDto {
                    tracing_no: l.tracing_no.to_string(),
                    title: l.title,
                    letter_code: l.letter_code,
                    letter_type_id: l.letter_type_id,
                    published_at: l.published_at.map(|p| iso(&crate::text::to_iran(p.to_utc()))),
                    disclosure_group: l.disclosure_group,
                    has_excel: l.has_excel,
                    report_url: l.report_url,
                    excel_url: l.excel_url,
                    pdf_url: l.pdf_url,
                })
                .collect();
            json(&letters)
        }
        Route::Messages(id) => {
            let ins = ins_code(&id)?;
            let mut messages = store.get_messages(ins)?;
            // OrderByDescending(Date).ThenByDescending(Time), stable.
            messages.sort_by_key(|m| std::cmp::Reverse((m.date, m.time)));
            let take = take(query, "take", 50)? as usize;
            let result: Vec<SupervisorMessageDto> = messages
                .into_iter()
                .take(take)
                .map(|m| SupervisorMessageDto {
                    id: m.id.to_string(),
                    date: m.date,
                    title: m.title,
                    body: m.body,
                    kind: m.kind.as_str().into(),
                    reason: m.reason.as_str().into(),
                    reopens_without_limit: m.reopens_without_limit,
                })
                .collect();
            json(&result)
        }
        Route::Health => {
            let report = market.health().get_report()?;
            json(&MarketDataHealthDto {
                generated_at: iso(&report.generated_at),
                node: report.node.clone(),
                expected_session: report.expected_session,
                healthy: report.healthy(),
                datasets: report
                    .datasets
                    .iter()
                    .map(|d| DatasetHealthDto {
                        dataset: d.dataset.clone(),
                        rows: d.rows,
                        latest_date: d.latest_date,
                        expected_date: d.expected_date,
                        status: d.status.as_str().into(),
                    })
                    .collect(),
                runs: report
                    .latest_runs
                    .iter()
                    .map(|r| CollectionRunDto {
                        job: r.job.clone(),
                        node: r.node.clone(),
                        started_at: iso(&r.started_at),
                        finished_at: r.finished_at.as_ref().map(iso),
                        status: r.status.clone(),
                        items: r.items,
                        failures: r.failures,
                        message: r.message.clone(),
                    })
                    .collect(),
                last_probe_at: report.last_probe_at.as_ref().map(iso),
                sources: report
                    .sources
                    .iter()
                    .map(|s| SourceReachabilityDto { name: s.name.clone(), verdict: s.verdict.clone(), status: s.status, error: s.error.clone() })
                    .collect(),
            })
        }
        Route::Shareholders(_) | Route::RunJob(_) | Route::Jobs | Route::SyncPull | Route::SyncExport | Route::Info => {
            Err(Problem::new(500, "This route is not a read route."))
        }
    }
}

/// Latest major shareholders; refreshed from TSETMC at most once a day (errors fall back to the stored snapshot).
async fn shareholders(ctx: &Arc<ApiContext>, ins: i64) -> ApiResult {
    let market = ctx.market.clone();
    let today = iran_today();
    let read = {
        let market = market.clone();
        move || market.store.get_latest_shareholders(ins)
    };
    let mut holders = tokio::task::spawn_blocking(read.clone()).await.map_err(|e| Problem::new(500, e.to_string()))??;
    if holders.first().is_none_or(|h| h.snapshot_date < today) {
        // Bounded, so a dead link degrades to the stored snapshot instead of a spinner for minutes of retries.
        let refresh = market.tsetmc.get_shareholders(ins, today, CachePolicy::Network);
        match tokio::time::timeout(market.options.live_refresh_timeout, refresh).await {
            Ok(Ok(fresh)) => {
                let store_market = market.clone();
                tokio::task::spawn_blocking(move || store_market.store.upsert_shareholders(&fresh)).await.map_err(|e| Problem::new(500, e.to_string()))??;
                holders = tokio::task::spawn_blocking(read).await.map_err(|e| Problem::new(500, e.to_string()))??;
            }
            Ok(Err(e @ (Error::SourceRequest(_) | Error::Http(_) | Error::Rejected(_) | Error::Format(_)))) => {
                tracing::info!(ins_code = ins, error = %e, "shareholders not refreshed");
            }
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                tracing::info!(ins_code = ins, timeout = ?market.options.live_refresh_timeout, "shareholders refresh timed out");
            }
        }
    }
    let result: Vec<ShareholderDto> = holders
        .into_iter()
        .map(|h| ShareholderDto {
            snapshot_date: h.snapshot_date,
            holder_name: h.holder_name,
            shares: h.shares,
            percent: h.percent,
            change: h.change,
            change_amount: h.change_amount,
        })
        .collect();
    json(&result)
}

/// Downloads `{nodeUrl}/api/v1/Sync/ExportBundle?since=` and imports it, then queues a cache replay into the store.
async fn sync_pull(ctx: &Arc<ApiContext>, body: Option<Value>) -> ApiResult {
    let request: SyncPullRequestDto = match body {
        None | Some(Value::Null) => return Err(Problem::bad_request("A body { nodeUrl, since } is required.")),
        Some(value) => serde_json::from_value(value).map_err(|e| Problem::bad_request(format!("Invalid request body: {e}")))?,
    };
    if request.node_url.trim().is_empty() {
        return Err(Problem::bad_request("nodeUrl is required."));
    }
    let since = parse_date(request.since.as_deref())?;
    let url = format!("{}/api/v1/Sync/ExportBundle{}", request.node_url.trim().trim_end_matches('/'), since.map(|s| format!("?since={s}")).unwrap_or_default());

    let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30 * 60)).build().map_err(|e| Problem::new(500, e.to_string()))?;
    let mut http = client.get(&url);
    if let Some(token) = std::env::var("BOORS_SYNC_TOKEN").ok().or_else(|| std::env::var("BOORS_OPERATOR_TOKEN").ok()).filter(|t| !t.is_empty()) {
        http = http.bearer_auth(token);
    }
    let response = http.send().await.map_err(|e| Problem::new(500, format!("Could not reach {url}: {e}")))?;
    if !response.status().is_success() {
        return Err(Problem::new(500, format!("{url} answered HTTP {}.", response.status().as_u16())));
    }
    let bytes = response.bytes().await.map_err(|e| Problem::new(500, format!("Download from {url} failed: {e}")))?;

    let market = ctx.market.clone();
    let (imported, skipped) = tokio::task::spawn_blocking(move || raw_cache::import_bundle_from(&market.cache, std::io::Cursor::new(bytes)))
        .await
        .map_err(|e| Problem::new(500, e.to_string()))??;
    if imported > 0 {
        ctx.jobs.enqueue(jobs::REPLAY_JOB)?;
    }
    json(&SyncPullResultDto { imported, skipped })
}

/// Writes a raw-cache bundle for `Sync/ExportBundle` into a temporary file and returns its path.
pub fn export_bundle_file(market: &MarketContext, since: Option<Date>) -> crate::Result<PathBuf> {
    let directory = market.options.data_root.join("tmp");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("export-{}.zip", uuid::Uuid::new_v4().simple()));
    raw_cache::export_bundle(&market.cache, &path, since)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_parse_case_insensitively_with_or_without_prefix() {
        assert_eq!(Ok(Route::Bars("46348559193224090".into())), parse_route("GET", "Instrument/GetBars/46348559193224090"));
        assert_eq!(Ok(Route::Overview), parse_route("get", "/api/v1/market/getoverview"));
        assert_eq!(Ok(Route::MacroHistory("price_dollar_rl".into())), parse_route("GET", "Market/GetMacroHistory/price_dollar_rl"));
        assert!(parse_route("POST", "Market/GetOverview").is_err());
        assert!(requires_operator("POST", "MarketData/RunJob/eod"));
        assert!(!requires_operator("GET", "MarketData/GetJobs"));
    }

    #[test]
    fn dates_are_strict() {
        assert_eq!(Ok(None), parse_date(Some(" ")));
        assert!(parse_date(Some("2026-1-1")).is_err());
        assert_eq!(Ok(Date::from_ymd_opt(2026, 10, 17)), parse_date(Some("2026-10-17")));
    }
}
