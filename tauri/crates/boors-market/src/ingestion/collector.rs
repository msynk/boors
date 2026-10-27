//! Collection jobs (C# `MarketCollector`). Every job fetches through the source clients (so responses land in the
//! raw cache first), writes idempotent merges into the store, and records a run row for the data-health report.
//! The same jobs run from the CLI, from the server scheduler, and with [`CachePolicy::CacheOnly`] for rebuilds.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::Path;

use chrono::{Datelike, Duration, Utc, Weekday};
use rayon::prelude::*;

use crate::analytics::point_in_time::implied_adjustments_default;
use crate::context::MarketContext;
use crate::domain::*;
use crate::sources::CachePolicy;
use crate::sources::codal::CodalSearchQuery;
use crate::sources::tgju::series;
use crate::sources::tsetmc::{MarketWatchQuery, MarketWatchSnapshot, TEDPIX_INS_CODE, parser as tp};
use crate::storage::{CollectionRun, MacroMonthlyValue};
use crate::text::{iran_now, match_key};
use crate::{Date, Decimal, Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionResult {
    pub job: String,
    pub status: String,
    pub items: i32,
    pub failures: i32,
    pub message: Option<String>,
}

impl CollectionResult {
    pub fn succeeded(&self) -> bool {
        matches!(self.status.as_str(), "ok" | "skipped")
    }
}

/// Instrument classes that get daily history by default (options and debt are opt-in).
pub fn default_scope() -> HashSet<InstrumentClass> {
    use InstrumentClass::*;
    [
        TseStock,
        IfbStock,
        IfbBaseYellow,
        IfbBaseOrange,
        IfbBaseRed,
        Rights,
        EtfEquity,
        EtfFixedIncome,
        EtfRealEstate,
        EtfMixed,
        FundLeveraged,
        FundGoldSilver,
        FundOfFunds,
    ]
    .into_iter()
    .collect()
}

/// Main indices whose history is collected (TEDPIX, equal-weight, price, free-float, first and second market).
pub const MAIN_INDICES: [i64; 7] =
    [TEDPIX_INS_CODE, 67130298613737946, 5798407779416661, 8384385859414435, 49579049405614711, 62752761908615603, 71704845530629737];

type Work = (i32, i32, Option<String>);

pub struct MarketCollector<'a> {
    ctx: &'a MarketContext,
}

impl<'a> MarketCollector<'a> {
    pub fn new(ctx: &'a MarketContext) -> Self {
        Self { ctx }
    }

    fn today_in_iran(&self) -> Date {
        iran_now().date_naive()
    }

    // ------------------------------------------------------------------------------------------ plumbing

    async fn run<F>(&self, job: &str, work: F) -> Result<CollectionResult>
    where
        F: Future<Output = Result<Work>>,
    {
        let started = Utc::now();
        let node = self.ctx.options.node.clone();
        let run_id = format!("{}-{job}-{node}", started.format("%Y%m%d%H%M%S%3f"));
        self.ctx.store.record_run(&CollectionRun {
            run_id: run_id.clone(),
            job: job.into(),
            node: node.clone(),
            started_at: started,
            finished_at: None,
            status: "running".into(),
            items: 0,
            failures: 0,
            message: None,
        })?;

        let result = match work.await {
            Ok((items, failures, message)) => {
                CollectionResult { job: job.into(), status: if failures == 0 { "ok" } else { "partial" }.into(), items, failures, message }
            }
            Err(Error::Cancelled) => {
                self.ctx.store.record_run(&CollectionRun {
                    run_id,
                    job: job.into(),
                    node,
                    started_at: started,
                    finished_at: Some(Utc::now()),
                    status: "cancelled".into(),
                    items: 0,
                    failures: 0,
                    message: None,
                })?;
                return Err(Error::Cancelled);
            }
            Err(e) => {
                tracing::error!(job, error = %e, "collection job failed");
                CollectionResult { job: job.into(), status: "failed".into(), items: 0, failures: 1, message: Some(e.to_string()) }
            }
        };

        let finished = Utc::now();
        tracing::info!(job, status = %result.status, items = result.items, failures = result.failures, elapsed_ms = (finished - started).num_milliseconds(), "collection job finished");
        self.ctx.store.record_run(&CollectionRun {
            run_id,
            job: job.into(),
            node,
            started_at: started,
            finished_at: Some(finished),
            status: result.status.clone(),
            items: result.items,
            failures: result.failures,
            message: result.message.clone(),
        })?;
        Ok(result)
    }

    /// Runs one unit of work; logs and counts a failure instead of aborting the job (C# `TryAsync`).
    async fn attempt<F>(&self, what: &str, work: F) -> Result<i32>
    where
        F: Future<Output = Result<()>>,
    {
        match work.await {
            Ok(()) => Ok(0),
            Err(Error::NotCached(e)) => {
                tracing::debug!("{what} skipped: no cached response for {} {} [{}]", e.source, e.endpoint, e.params_key);
                Ok(0)
            }
            Err(e) if e.is_step_failure() => {
                tracing::warn!("{what} failed: {e}");
                Ok(1)
            }
            Err(e) => Err(e),
        }
    }

    /// Rows of the extended variant carry yVal and rank as "yval"; compact rows only have the ISIN.
    fn upsert_from_market_watch(&self, snapshot: &MarketWatchSnapshot, seen_on: Date) -> Result<()> {
        let (compact, extended): (Vec<&MarketWatchRow>, Vec<&MarketWatchRow>) = snapshot.rows.iter().partition(|r| r.y_val.is_none());
        self.ctx.store.upsert_instruments(&compact.into_iter().map(to_instrument).collect::<Vec<_>>(), seen_on, "isin", None)?;
        self.ctx.store.upsert_instruments(&extended.into_iter().map(to_instrument).collect::<Vec<_>>(), seen_on, "yval", None)
    }

    /// Normalized symbol → primary listing (secondary listings resolve to their parent).
    pub fn symbol_map(&self) -> Result<HashMap<String, i64>> {
        let mut groups: Vec<(String, Vec<Instrument>)> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for instrument in self.ctx.store.get_instruments()? {
            let key = match_key(&instrument.symbol);
            match index.get(&key) {
                Some(&i) => groups[i].1.push(instrument),
                None => {
                    index.insert(key.clone(), groups.len());
                    groups.push((key, vec![instrument]));
                }
            }
        }
        Ok(groups
            .into_iter()
            .filter_map(|(key, mut members)| {
                members.sort_by_key(|i| (i.parent_ins_code.is_some(), i.isin.is_none()));
                members.first().map(|i| (key, i.parent_ins_code.unwrap_or(i.ins_code)))
            })
            .collect())
    }

    // ------------------------------------------------------------------------------------------ instruments and funds

    /// Refreshes the instrument master from the market watch, applies fipiran fund types to ETFs, and (optionally)
    /// fetches GetInstrumentInfo for up to `max_details` instruments that have never been detailed.
    pub async fn sync_instruments(&self, details: bool, max_details: usize, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("instruments", async {
            let today = self.today_in_iran();
            let snapshot = self.ctx.tsetmc.get_market_watch(&MarketWatchQuery::default(), policy).await?;
            self.upsert_from_market_watch(&snapshot, today)?;
            let mut items = snapshot.rows.len() as i32;
            let mut failures = 0;

            failures += self.attempt("sector names", async { self.ctx.store.upsert_sectors(&self.ctx.tsetmc.get_static_data(policy.prefer()).await?) }).await?;

            failures += self
                .attempt("fund types", async {
                    let funds = self.ctx.fipiran.get_funds(policy).await?;
                    self.ctx.store.upsert_funds(&funds)?;
                    self.ctx.store.apply_fund_types(&funds)?;
                    Ok(())
                })
                .await?;

            if details {
                let pending = self.ctx.store.get_instruments_without_details()?;
                for ins_code in pending.into_iter().take(max_details) {
                    let mut stored = false;
                    failures += self
                        .attempt(&format!("details {ins_code}"), async {
                            let instrument = self.ctx.tsetmc.get_instrument_info(ins_code, policy.prefer()).await?;
                            self.ctx.store.upsert_instruments(&[instrument], today, "yval", Some(Utc::now()))?;
                            stored = true;
                            Ok(())
                        })
                        .await?;
                    if stored {
                        items += 1;
                    }
                }

                failures += self
                    .attempt("fund types (after details)", async {
                        self.ctx.store.apply_fund_types(&self.ctx.fipiran.get_funds(CachePolicy::PreferCache).await?)?;
                        Ok(())
                    })
                    .await?;
            }

            Ok((items, failures, Some(format!("{} instruments in the market watch", snapshot.rows.len()))))
        })
        .await
    }

    pub async fn sync_funds(&self, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("funds", async {
            let funds = self.ctx.fipiran.get_funds(policy).await?;
            self.ctx.store.upsert_funds(&funds)?;
            let updated = self.ctx.store.apply_fund_types(&funds)?;
            Ok((funds.len() as i32, 0, Some(format!("{} funds, {updated} instruments re-classified", funds.len()))))
        })
        .await
    }

    // ------------------------------------------------------------------------------------------ indices and calendar

    /// Index list (TSE and IFB) and, with `history`, the full daily history of the main indices.
    pub async fn sync_indices(&self, history: bool, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("indices", async {
            let mut items = 0;
            let mut failures = 0;
            for flow in [1, 2] {
                let mut count = 0;
                failures += self
                    .attempt(&format!("index list flow {flow}"), async {
                        let snapshots = self.ctx.tsetmc.get_index_snapshots(flow, policy).await?;
                        self.ctx.store.upsert_index_info(&snapshots.iter().map(|s| (s.ins_code, s.name.clone(), flow)).collect::<Vec<_>>())?;
                        count = snapshots.len() as i32;
                        Ok(())
                    })
                    .await?;
                items += count;
            }

            if history {
                // The IFB main index is looked up by name: its code is only known from the IFB index list.
                let key = match_key("شاخص کل فرابورس");
                let ifb_main: Vec<i64> = self
                    .ctx
                    .store
                    .get_index_info()?
                    .into_iter()
                    .filter(|(_, name, flow)| *flow == Some(2) && match_key(name) == key)
                    .map(|(i, _, _)| i)
                    .collect();
                let mut codes: Vec<i64> = Vec::new();
                for code in MAIN_INDICES.iter().copied().chain(ifb_main) {
                    if !codes.contains(&code) {
                        codes.push(code);
                    }
                }
                for ins_code in codes {
                    let mut count = 0;
                    failures += self
                        .attempt(&format!("index history {ins_code}"), async {
                            let bars = self.ctx.tsetmc.get_index_history(ins_code, policy).await?;
                            self.ctx.store.upsert_index_bars(&bars, "history")?;
                            count = bars.len() as i32;
                            Ok(())
                        })
                        .await?;
                    items += count;
                }
            }

            self.refresh_calendar()?;
            Ok((items, failures, None))
        })
        .await
    }

    /// Rebuilds market_closure from rules.yaml plus days on which TEDPIX repeated its previous close exactly.
    pub fn refresh_calendar(&self) -> Result<MarketCalendar> {
        let tedpix = self.ctx.store.get_index_bars(TEDPIX_INS_CODE)?;
        let detected = detect_flat_index_days(&tedpix);
        let mut closures: Vec<(Date, String, String)> = Vec::new();
        for closure in self.ctx.rules.known_closures() {
            let mut day = closure.from;
            while day <= closure.to {
                closures.push((day, closure.reason.clone(), "rules".into()));
                day += Duration::days(1);
            }
        }
        closures.extend(detected.into_iter().map(|d| (d, "TEDPIX unchanged from the previous row".to_string(), "index".to_string())));
        let mut seen = HashSet::new();
        closures.retain(|c| seen.insert(c.0));
        self.ctx.store.replace_closures(&closures)?;
        Ok(MarketCalendar::new(Vec::new(), closures.into_iter().map(|c| c.0)))
    }

    fn load_calendar(&self) -> Result<MarketCalendar> {
        Ok(MarketCalendar::new(Vec::new(), self.ctx.store.get_closure_days()?))
    }

    async fn ensure_calendar(&self, policy: CachePolicy) -> Result<MarketCalendar> {
        if self.ctx.store.get_index_bars(TEDPIX_INS_CODE)?.is_empty() {
            let bars = self.ctx.tsetmc.get_index_history(TEDPIX_INS_CODE, policy.prefer()).await?;
            self.ctx.store.upsert_index_bars(&bars, "history")?;
        }
        self.refresh_calendar()
    }

    // ------------------------------------------------------------------------------------------ prices

    /// Full history (bars, flows, published and implied adjustments, share changes) for instruments in `scope` that
    /// have no official history yet (or all of them with `force`). Resumable: responses come from the raw cache on rerun.
    pub async fn backfill_history(
        &self,
        max_instruments: usize,
        force: bool,
        with_messages: bool,
        scope: Option<&HashSet<InstrumentClass>>,
        policy: CachePolicy,
    ) -> Result<CollectionResult> {
        self.run("backfill", async {
            let calendar = self.ensure_calendar(policy).await?;
            let with_history = if force { HashSet::new() } else { self.ctx.store.get_instruments_with_history()? };
            let default = default_scope();
            let classes = scope.unwrap_or(&default);
            let mut candidates: Vec<Instrument> = self
                .ctx
                .store
                .get_instruments()?
                .into_iter()
                .filter(|i| classes.contains(&i.class) && i.parent_ins_code.is_none() && !with_history.contains(&i.ins_code))
                .collect();
            candidates.sort_by(|a, b| a.class.cmp(&b.class).then_with(|| crate::raw_cache::ordinal_cmp(&a.symbol, &b.symbol)));
            candidates.truncate(max_instruments);
            let total = candidates.len();

            let symbols = if with_messages { Some(self.symbol_map()?) } else { None };
            let fetch_policy = if force { policy } else { policy.prefer() };
            let mut done = 0;
            let mut failures = 0;

            // A rebuild only replays what the cache holds; instruments never fetched are simply not part of it.
            if policy == CachePolicy::CacheOnly {
                candidates.retain(|i| self.ctx.cache.has_latest(&self.ctx.tsetmc.endpoints.closing_price_daily_list(i.ins_code, 0)));
            }

            let chunk_size = if policy == CachePolicy::CacheOnly { 48 } else { 16 };
            for chunk in candidates.chunks(chunk_size) {
                let fetched = if policy == CachePolicy::CacheOnly {
                    self.read_histories_from_cache(chunk, with_messages)
                } else {
                    let mut out = Vec::with_capacity(chunk.len());
                    for instrument in chunk {
                        out.push(self.fetch_history(instrument, fetch_policy, with_messages).await);
                    }
                    out
                };

                let mut batch = HistoryBatch::default();
                for (instrument, outcome) in chunk.iter().zip(fetched) {
                    match outcome {
                        Ok(history) => {
                            if let (Some(messages), Some(symbols)) = (&history.messages, &symbols) {
                                self.ctx.store.upsert_messages(messages, symbols)?;
                            }
                            let (classified, reasons) = self.classify_with_halts(instrument, &history.bars, &calendar)?;
                            batch.implied.extend(implied_adjustments_default(&history.bars));
                            batch.reasons.extend(reasons.into_iter().map(|(d, r)| ((instrument.ins_code, d), r)));
                            batch.bars.extend(classified);
                            batch.flows.extend(history.flows);
                            batch.published.extend(history.published);
                            batch.share_changes.extend(history.share_changes);
                            done += 1;
                        }
                        Err(Error::NotCached(_)) => {}
                        Err(e) if e.is_step_failure() => {
                            tracing::warn!("history {} ({}) failed: {e}", instrument.symbol, instrument.ins_code);
                            failures += 1;
                        }
                        Err(e) => return Err(e),
                    }
                    if (done + failures) % 50 == 0 && done + failures > 0 {
                        tracing::info!("Backfill: {done}/{total} instruments ({failures} failures)");
                    }
                }
                batch.write(self.ctx)?;
            }

            Ok((done, failures, Some(format!("{done} of {total} candidate instruments backfilled"))))
        })
        .await
    }

    async fn fetch_history(&self, instrument: &Instrument, policy: CachePolicy, with_messages: bool) -> Result<InstrumentHistory> {
        let t = &self.ctx.tsetmc;
        let ins = instrument.ins_code;
        Ok(InstrumentHistory {
            bars: t.get_daily_history(ins, policy).await?,
            flows: t.get_client_type_history(ins, policy).await?,
            published: t.get_price_adjustments(ins, policy).await?,
            share_changes: t.get_share_changes(ins, policy).await?,
            messages: if with_messages { Some(t.get_messages(ins, policy).await?) } else { None },
        })
    }

    /// Parallel decompress + parse of cached histories (rebuild path).
    fn read_histories_from_cache(&self, chunk: &[Instrument], with_messages: bool) -> Vec<Result<InstrumentHistory>> {
        let t = &self.ctx.tsetmc;
        chunk
            .par_iter()
            .map(|instrument| {
                let ins = instrument.ins_code;
                let e = &t.endpoints;
                Ok(InstrumentHistory {
                    bars: tp::parse_daily_history(&t.client.get_cached(&e.closing_price_daily_list(ins, 0))?, ins)?,
                    flows: tp::parse_client_type_history(&t.client.get_cached(&e.client_type_history(ins))?, ins)?,
                    published: tp::parse_price_adjustments(&t.client.get_cached(&e.price_adjust_list(ins))?, ins)?,
                    share_changes: tp::parse_share_changes(&t.client.get_cached(&e.instrument_share_change(ins))?, ins)?,
                    messages: if with_messages { Some(tp::parse_messages(&t.client.get_cached(&e.messages_by_instrument(ins))?)?) } else { None },
                })
            })
            .collect()
    }

    /// End-of-day append from bulk calls (market watch, same-day flows, index snapshots). Bars are tagged
    /// source=marketwatch; a later history backfill replaces them with official rows.
    pub async fn end_of_day(&self, force: bool, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("eod", async {
            let now = iran_now();
            let today = now.date_naive();
            let session = self.ctx.rules.get_session(InstrumentClass::TseStock, today)?.value;
            let is_trading_weekday = !matches!(now.weekday(), Weekday::Thu | Weekday::Fri);
            let close_plus_five = session.close().overflowing_add_signed(Duration::minutes(5)).0;
            if !force && is_trading_weekday && now.time() < close_plus_five {
                return Ok((
                    0,
                    0,
                    Some(format!("skipped: the session closes at {} Iran time (use --force to snapshot mid-session)", session.close().format("%H:%M"))),
                ));
            }

            let snapshot = self.ctx.tsetmc.get_market_watch(&MarketWatchQuery::default(), policy).await?;
            let Some(session_date) = self.determine_session_date(&snapshot, policy).await? else {
                return Ok((0, 0, Some("skipped: nothing traded in the snapshot (market closed)".into())));
            };

            self.upsert_from_market_watch(&snapshot, today)?;
            let instruments: HashMap<i64, Instrument> = self.ctx.store.get_instruments()?.into_iter().map(|i| (i.ins_code, i)).collect();
            let recent = self.ctx.store.get_recent_bars(10)?;
            let calendar = self.load_calendar()?;
            let mut failures = 0;

            let mut new_bars: Vec<ClassifiedBar> = Vec::with_capacity(snapshot.rows.len());
            let mut corporate_actions: Vec<PriceAdjustment> = Vec::new();
            let threshold = Decimal::new(5, 4);
            for row in &snapshot.rows {
                let bar = DailyBar {
                    ins_code: row.ins_code,
                    date: session_date,
                    open: row.open,
                    high: row.high,
                    low: row.low,
                    close: row.close,
                    last: row.last,
                    yesterday: row.yesterday,
                    trade_count: row.trade_count,
                    volume: row.volume,
                    value: row.value,
                    last_trade_time: row.last_trade_time,
                };
                let mut history: Vec<DailyBar> = recent
                    .get(&row.ins_code)
                    .map(|bars| bars.iter().filter(|b| b.bar.date < session_date).map(|b| b.bar.clone()).collect())
                    .unwrap_or_default();
                let class = instruments.get(&row.ins_code).map_or_else(|| row.class(), |i| i.class);
                let classifier = DayStatusClassifier::new(&calendar, |d| self.ctx.rules.price_limit_or_zero(class, d));
                let previous_close = history.last().map(|b| b.close);
                history.push(bar.clone());
                if let Some(classified) = classifier.classify(&history, None, None).pop() {
                    new_bars.push(classified);
                }

                if let Some(previous_close) = previous_close
                    && previous_close > Decimal::ZERO
                    && bar.trade_count > 0
                    && bar.yesterday > Decimal::ZERO
                    && let Some(ratio) = bar.yesterday.checked_div(previous_close)
                    && (ratio - Decimal::ONE).abs() > threshold
                {
                    corporate_actions.push(PriceAdjustment {
                        ins_code: row.ins_code,
                        date: session_date,
                        adjusted_close: bar.yesterday,
                        unadjusted_close: previous_close,
                    });
                }
            }

            self.ctx.store.upsert_daily_bars(&new_bars, "marketwatch", None)?;
            self.ctx.store.update_session_snapshots(&snapshot.rows.iter().map(|r| SessionSnapshot::from_row(r, session_date)).collect::<Vec<_>>())?;
            self.ctx.store.upsert_price_adjustments(&corporate_actions, "implied")?;

            let scope = default_scope();
            for action in corporate_actions.iter().filter(|a| instruments.get(&a.ins_code).is_some_and(|i| scope.contains(&i.class))) {
                failures += self
                    .attempt(&format!("corporate action {}", action.ins_code), async {
                        self.ctx.store.upsert_price_adjustments(&self.ctx.tsetmc.get_price_adjustments(action.ins_code, policy).await?, "published")?;
                        self.ctx.store.upsert_share_changes(&self.ctx.tsetmc.get_share_changes(action.ins_code, policy).await?)?;
                        Ok(())
                    })
                    .await?;
            }

            // Same-day flows: volumes and counts are official, rial values are estimated from the day's average price.
            failures += self
                .attempt("client types", async {
                    let bars: HashMap<i64, &DailyBar> = new_bars.iter().map(|b| (b.bar.ins_code, &b.bar)).collect();
                    let flows: Vec<ClientTypeDay> = self
                        .ctx
                        .tsetmc
                        .get_client_type_all(policy)
                        .await?
                        .into_iter()
                        .filter_map(|f| {
                            let bar = bars.get(&f.ins_code).filter(|b| b.volume > 0)?;
                            let average = bar.value.checked_div(Decimal::from(bar.volume))?;
                            Some(f.to_estimated_day(session_date, average))
                        })
                        .collect();
                    self.ctx.store.upsert_client_types(&flows)
                })
                .await?;

            failures += self
                .attempt("instrument statistics", async {
                    self.ctx.store.upsert_instrument_statistics(&self.ctx.tsetmc.get_instrument_statistics(policy).await?)
                })
                .await?;

            for flow in [1, 2] {
                failures += self
                    .attempt(&format!("index snapshots flow {flow}"), async {
                        let indices = self.ctx.tsetmc.get_index_snapshots(flow, policy).await?;
                        self.ctx.store.upsert_index_info(&indices.iter().map(|i| (i.ins_code, i.name.clone(), flow)).collect::<Vec<_>>())?;
                        self.ctx.store.upsert_index_bars(
                            &indices
                                .iter()
                                .map(|i| IndexBar { ins_code: i.ins_code, date: session_date, close: i.value, low: i.low, high: i.high })
                                .collect::<Vec<_>>(),
                            "snapshot",
                        )
                    })
                    .await?;
            }

            self.refresh_calendar()?;
            let pending_backfill = new_bars
                .iter()
                .filter(|b| instruments.get(&b.bar.ins_code).is_some_and(|i| scope.contains(&i.class)) && !recent.contains_key(&b.bar.ins_code))
                .count();
            let traded = new_bars.iter().filter(|b| b.bar.trade_count > 0).count();
            Ok((
                new_bars.len() as i32,
                failures,
                Some(format!(
                    "session {session_date}: {traded} traded, {} corporate actions, {pending_backfill} new instruments awaiting backfill",
                    corporate_actions.len()
                )),
            ))
        })
        .await
    }

    /// Resolves zero-trade runs: asks TSETMC for the instrument state on the first day of each unresolved run; a
    /// halted state ("IS") turns the run into SymbolHalted with a reason from the nearest supervisor message.
    pub async fn resolve_halts(&self, max_queries: usize, scope: Option<&HashSet<InstrumentClass>>, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("halts", async {
            let calendar = self.load_calendar()?;
            let default = default_scope();
            let classes = scope.unwrap_or(&default);
            let mut queries = 0usize;
            let mut updated = 0usize;
            let mut failures = 0;
            let instruments: Vec<Instrument> =
                self.ctx.store.get_instruments()?.into_iter().filter(|i| classes.contains(&i.class) && i.parent_ins_code.is_none()).collect();
            // Bars and states are read in bulk (one scan per chunk instead of one per instrument). Only the instrument
            // being processed writes its own bars and states, so reading ahead sees exactly what a per-instrument read would.
            let mut all_states = self.ctx.store.get_all_instrument_states()?;
            let mut prefetched: HashMap<i64, Vec<crate::storage::StoredBar>> = HashMap::new();
            for (index, instrument) in instruments.iter().enumerate() {
                if queries >= max_queries {
                    break;
                }
                if index % 64 == 0 {
                    let codes: Vec<i64> = instruments[index..(index + 64).min(instruments.len())].iter().map(|i| i.ins_code).collect();
                    prefetched = self.ctx.store.get_daily_bars_for(&codes)?;
                }
                let stored = prefetched.remove(&instrument.ins_code).unwrap_or_default();
                if stored.is_empty() {
                    continue;
                }

                let states = all_states.remove(&instrument.ins_code).unwrap_or_default();
                let classified: Vec<ClassifiedBar> = stored.iter().map(|s| ClassifiedBar::new(s.bar.clone(), s.status.unwrap_or(DayStatus::Traded))).collect();
                let unresolved: Vec<(Date, Vec<Date>)> = no_trade_runs(&classified).into_iter().filter(|run| !states.contains_key(&run.0)).collect();
                if unresolved.is_empty() {
                    continue;
                }

                for (first, _) in &unresolved {
                    let current = queries;
                    queries += 1;
                    if current >= max_queries {
                        break;
                    }
                    failures += self
                        .attempt(&format!("state {} {first}", instrument.symbol), async {
                            let state = self.ctx.tsetmc.get_instrument_state(instrument.ins_code, *first, policy.prefer()).await?;
                            self.ctx.store.upsert_instrument_states(&[state])
                        })
                        .await?;
                }

                let bars: Vec<DailyBar> = stored.iter().map(|s| s.bar.clone()).collect();
                let (reclassified, reasons) = self.classify_with_halts(instrument, &bars, &calendar)?;
                let changes: Vec<(Date, DayStatus, Option<HaltReason>)> = reclassified
                    .iter()
                    .zip(stored.iter())
                    .filter(|(now, before)| Some(now.status) != before.status || reasons.contains_key(&now.bar.date))
                    .map(|(now, _)| (now.bar.date, now.status, reasons.get(&now.bar.date).copied()))
                    .collect();
                if !changes.is_empty() {
                    self.ctx.store.update_day_statuses(instrument.ins_code, &changes)?;
                }
                updated += changes.len();
            }

            Ok((updated as i32, failures, Some(format!("{queries} state queries, {updated} day statuses updated"))))
        })
        .await
    }

    /// Classifies bars, turning zero-trade runs that start on a known halted day into SymbolHalted.
    fn classify_with_halts(
        &self,
        instrument: &Instrument,
        bars: &[DailyBar],
        calendar: &MarketCalendar,
    ) -> Result<(Vec<ClassifiedBar>, HashMap<Date, HaltReason>)> {
        let class = instrument.class;
        let classifier = DayStatusClassifier::new(calendar, |d| self.ctx.rules.price_limit_or_zero(class, d));
        let first_pass = classifier.classify(bars, None, None);
        let states = self.ctx.store.get_instrument_states(instrument.ins_code)?;
        if states.is_empty() {
            return Ok((first_pass, HashMap::new()));
        }

        let messages: Vec<SupervisorMessage> = self.ctx.store.get_messages(instrument.ins_code)?.into_iter().filter(|m| m.kind == MessageKind::Halt).collect();
        let mut halted: HashSet<Date> = HashSet::new();
        let mut reasons: HashMap<Date, HaltReason> = HashMap::new();
        for (first, days) in no_trade_runs(&first_pass) {
            match states.get(&first) {
                Some(Some(state)) if state != "A" => {}
                _ => continue,
            }
            let window_start = first - Duration::days(5);
            let mut candidates: Vec<&SupervisorMessage> = messages.iter().filter(|m| m.date <= first && m.date >= window_start).collect();
            // OrderByDescending(Date), stable.
            candidates.sort_by_key(|m| std::cmp::Reverse(m.date));
            let reason = candidates.first().map_or(HaltReason::Unknown, |m| m.reason);
            for day in days {
                halted.insert(day);
                reasons.insert(day, reason);
            }
        }

        let bars = if halted.is_empty() { first_pass } else { classifier.classify(bars, Some(&halted), None) };
        Ok((bars, reasons))
    }

    /// The trading date of a market-watch snapshot: the latest last-trade date among the most-traded instruments.
    async fn determine_session_date(&self, snapshot: &MarketWatchSnapshot, policy: CachePolicy) -> Result<Option<Date>> {
        let mut traded: Vec<&MarketWatchRow> = snapshot.rows.iter().filter(|r| r.trade_count > 0).collect();
        traded.sort_by_key(|r| std::cmp::Reverse(r.value));
        let mut latest: Option<Date> = None;
        for row in traded.into_iter().take(3) {
            let info = match self.ctx.tsetmc.get_closing_price_info(row.ins_code, policy).await {
                Ok(info) => info,
                Err(e @ (Error::SourceRequest(_) | Error::NotCached(_) | Error::Http(_) | Error::Rejected(_))) => {
                    tracing::debug!("Session date: {} unavailable ({e})", row.ins_code);
                    continue;
                }
                Err(e) => return Err(e),
            };
            if let Some(date) = info.date
                && latest.is_none_or(|l| date > l)
            {
                latest = Some(date);
            }
        }
        Ok(latest)
    }

    // ------------------------------------------------------------------------------------------ disclosures, messages, macro

    /// Pages through the newest Codal letters until a page contains only letters already stored.
    pub async fn sync_codal(&self, max_pages: i32, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("codal", async {
            let symbols = self.symbol_map()?;
            let mut stored = 0;
            let mut pages = 0;
            for page in 1..=max_pages {
                let result = self.ctx.codal.search(&CodalSearchQuery { page_number: page, ..CodalSearchQuery::default() }, policy).await?;
                pages += 1;
                if result.letters.is_empty() {
                    break;
                }
                let tracing: Vec<i64> = result.letters.iter().map(|l| l.tracing_no).collect();
                let known = self.ctx.store.get_known_tracing_numbers(&tracing)?;
                self.ctx.store.upsert_codal_letters(&result.letters, &symbols)?;
                stored += result.letters.len() as i32 - known.len() as i32;
                if known.len() == result.letters.len() || policy == CachePolicy::CacheOnly {
                    break;
                }
            }
            Ok((stored, 0, Some(format!("{stored} new letters from {pages} page(s)"))))
        })
        .await
    }

    /// Latest supervisor messages of both markets, linked to instruments by the symbols they mention.
    pub async fn sync_messages(&self, top: i32, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("messages", async {
            let symbols = self.symbol_map()?;
            let mut items = 0;
            let mut failures = 0;
            for flow in [1, 2] {
                let mut count = 0;
                failures += self
                    .attempt(&format!("messages flow {flow}"), async {
                        let messages = self.ctx.tsetmc.get_latest_messages(flow, top, policy).await?;
                        self.ctx.store.upsert_messages(&messages, &symbols)?;
                        count = messages.len() as i32;
                        Ok(())
                    })
                    .await?;
                items += count;
            }
            Ok((items, failures, None))
        })
        .await
    }

    /// Daily history of the tgju series (free-market dollar, Emami coin, 18k gold, ounce, euro).
    pub async fn sync_macro(&self, codes: Option<&[String]>, policy: CachePolicy) -> Result<CollectionResult> {
        self.run("macro", async {
            let list: Vec<String> = codes.map(<[String]>::to_vec).unwrap_or_else(|| series::DEFAULT.iter().map(|s| s.to_string()).collect());
            let mut items = 0;
            let mut failures = 0;
            for code in &list {
                let mut count = 0;
                failures += self
                    .attempt(&format!("tgju {code}"), async {
                        let bars = self.ctx.tgju.get_history(code, policy).await?;
                        self.ctx.store.upsert_macro_daily(&bars)?;
                        count = bars.len() as i32;
                        Ok(())
                    })
                    .await?;
                items += count;
            }
            Ok((items, failures, None))
        })
        .await
    }

    /// Imports monthly macro values with no machine-readable source. CSV columns: series,period,period_end,value,source[,note].
    pub async fn import_macro_monthly(&self, csv_path: &Path) -> Result<CollectionResult> {
        self.run("macro-import", async {
            let text = std::fs::read_to_string(csv_path)?;
            let mut values = Vec::new();
            let mut failures = 0;
            for (index, raw) in text.lines().enumerate() {
                let line = raw.trim();
                if line.is_empty() || line.starts_with('#') || (index == 0 && line.to_ascii_lowercase().starts_with("series")) {
                    continue;
                }
                let fields: Vec<&str> = line.split(',').collect();
                let period_end = fields.get(2).and_then(|f| Date::parse_from_str(f.trim(), "%Y-%m-%d").ok());
                let value = fields.get(3).and_then(|f| crate::sources::json::parse_decimal_float(f));
                match (fields.len() >= 5, period_end, value) {
                    (true, Some(period_end), Some(value)) => values.push(MacroMonthlyValue {
                        series: fields[0].trim().to_string(),
                        period: fields[1].trim().to_string(),
                        period_end,
                        value,
                        source: fields[4].trim().to_string(),
                        note: (fields.len() > 5).then(|| fields[5..].join(",").trim().to_string()),
                    }),
                    _ => failures += 1,
                }
            }
            self.ctx.store.upsert_macro_monthly(&values)?;
            Ok((values.len() as i32, failures, (failures > 0).then(|| format!("{failures} malformed line(s) skipped"))))
        })
        .await
    }
}

/// Consecutive NoTrade days; market-closed days inside a run neither break nor join it.
fn no_trade_runs(classified: &[ClassifiedBar]) -> Vec<(Date, Vec<Date>)> {
    let mut ordered: Vec<&ClassifiedBar> = classified.iter().collect();
    ordered.sort_by_key(|b| b.bar.date);
    let mut runs = Vec::new();
    let mut current: Vec<Date> = Vec::new();
    for bar in ordered {
        match bar.status {
            DayStatus::NoTrade | DayStatus::SymbolHalted => current.push(bar.bar.date),
            DayStatus::MarketClosed => {}
            _ => {
                if let Some(first) = current.first().copied() {
                    runs.push((first, std::mem::take(&mut current)));
                }
                current.clear();
            }
        }
    }
    if let Some(first) = current.first().copied() {
        runs.push((first, current));
    }
    runs
}

fn to_instrument(row: &MarketWatchRow) -> Instrument {
    Instrument {
        ins_code: row.ins_code,
        symbol: row.symbol.clone(),
        name: row.name.clone(),
        isin: row.isin.clone(),
        sector_code: row.sector_code.clone(),
        y_val: row.y_val,
        flow: row.flow,
        board: row.board.clone(),
        base_volume: (row.base_volume > 0).then_some(row.base_volume),
        total_shares: (row.total_shares > Decimal::ZERO).then_some(row.total_shares),
        class: row.class(),
        ..Instrument::default()
    }
}

struct InstrumentHistory {
    bars: Vec<DailyBar>,
    flows: Vec<ClientTypeDay>,
    published: Vec<PriceAdjustment>,
    share_changes: Vec<ShareChange>,
    messages: Option<Vec<SupervisorMessage>>,
}

/// Rows of several instruments written together (one merge per table instead of one per instrument).
#[derive(Default)]
struct HistoryBatch {
    bars: Vec<ClassifiedBar>,
    reasons: HashMap<(i64, Date), HaltReason>,
    flows: Vec<ClientTypeDay>,
    published: Vec<PriceAdjustment>,
    implied: Vec<PriceAdjustment>,
    share_changes: Vec<ShareChange>,
}

impl HistoryBatch {
    fn write(self, ctx: &MarketContext) -> Result<()> {
        if self.bars.is_empty() && self.flows.is_empty() && self.published.is_empty() && self.share_changes.is_empty() {
            return Ok(());
        }
        ctx.store.upsert_daily_bars_with(&self.bars, "history", |bar| self.reasons.get(&(bar.ins_code, bar.date)).copied())?;
        ctx.store.upsert_client_types(&self.flows)?;
        ctx.store.upsert_price_adjustments(&self.published, "published")?;
        ctx.store.upsert_price_adjustments(&self.implied, "implied")?;
        ctx.store.upsert_share_changes(&self.share_changes)
    }
}
