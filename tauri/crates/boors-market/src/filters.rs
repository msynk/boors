//! TSETMC market-watch filters (C# `TsetmcFilter`), executed by QuickJS (rquickjs) so expressions — including the
//! `true==function(){...}()` code mode — behave as on TSETMC.
//!
//! Sandbox: a bare ECMAScript context (no host objects, modules or I/O), a per-row instruction budget and timeout
//! (interrupt handler), a stack-size cap standing in for the recursion cap, and a heap limit for the runtime.
//! `[ih]` and `[isN]` tokens are rewritten to identifiers before evaluation; everything else (`(pl)`,
//! `(ct).Buy_I_Volume`, `pd1`…) is already valid JavaScript.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use regex::Regex;
use rquickjs::function::Args;
use rquickjs::{Array, CatchResultExt, CaughtError, Context, Ctx, Function, Object, Persistent, Runtime};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::domain::{BestLimitLevel, ClientTypeDay, DailyBar};
use crate::{Error, Result};

/// Everything a TSETMC market-watch filter can reference for one instrument.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FilterRow {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    pub trade_count: i64,
    pub volume: i64,
    pub value: Decimal,
    pub yesterday: Decimal,
    pub first: Decimal,
    pub low: Decimal,
    pub high: Decimal,
    pub last: Decimal,
    pub close: Decimal,
    pub eps: Option<Decimal>,
    pub price_earnings: Option<Decimal>,
    pub allowed_low: Decimal,
    pub allowed_high: Decimal,
    pub total_shares: Decimal,
    pub base_volume: i64,
    pub sector_code: i32,
    /// Order-book levels 1..5 (only level 1 is known from the end-of-day snapshot).
    pub best_limits: Vec<BestLimitLevel>,
    pub client_type: Option<ClientTypeDay>,
    /// Previous trading days, newest first: `[ih][0]` is the day before this row.
    pub history: Vec<DailyBar>,
    /// TSETMC statistics by data type: `[isN]` reads data type N.
    pub statistics: HashMap<i32, f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilterResult {
    pub matches: Vec<i64>,
    pub evaluated: usize,
    pub errors: Vec<String>,
}

const SCALARS: [&str; 25] = [
    "l18", "l30", "tno", "tvol", "tval", "py", "pf", "pmin", "pmax", "pl", "plc", "plp", "pc", "pcc", "pcp", "eps", "pe", "tmin", "tmax", "z", "mv", "bvol",
    "cs", "buyop", "predtran",
];

const BOOK_SIDES: [&str; 6] = ["pd", "zd", "qd", "po", "zo", "qo"];

/// Jint ran with MaxStatements(20_000) and a 250 ms timeout per row. QuickJS calls the interrupt handler about
/// every 10,000 instructions; four calls is a budget of the same order.
const INTERRUPT_BUDGET: u64 = 4;
const ROW_TIMEOUT: Duration = Duration::from_millis(250);
const MEMORY_LIMIT: usize = 32 * 1024 * 1024;
const STACK_LIMIT: usize = 256 * 1024;

static IH_TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\s*ih\s*\]").expect("valid regex"));
static IH_IDENTIFIER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bih\b").expect("valid regex"));
static IS_TOKEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[\s*is(\d{1,3})\s*\]").expect("valid regex"));

struct Budget {
    ticks: AtomicU64,
    started: std::sync::Mutex<Instant>,
    exhausted_by_time: AtomicBool,
    exhausted_by_count: AtomicBool,
}

/// A compiled filter. Not `Send`: compile and apply it on the same thread.
pub struct TsetmcFilter {
    source: String,
    uses_history: bool,
    statistics: Vec<i32>,
    // Field order matters: the function must be dropped before the context and runtime.
    function: Option<Persistent<Function<'static>>>,
    context: Context,
    _runtime: Runtime,
    budget: Arc<Budget>,
}

impl Drop for TsetmcFilter {
    fn drop(&mut self) {
        // Release the persisted function inside its context before the runtime goes away.
        if let Some(function) = self.function.take() {
            self.context.with(|ctx| {
                let _ = function.restore(&ctx);
            });
        }
    }
}

impl TsetmcFilter {
    pub fn compile(expression: &str) -> Result<TsetmcFilter> {
        if crate::text::is_blank(expression) {
            return Err(Error::FilterSyntax("The filter is empty.".into()));
        }

        let trimmed = expression.trim().trim_end_matches(';').trim();
        let body = IH_TOKEN.replace_all(trimmed, "ih").into_owned();
        let mut statistics: Vec<i32> = IS_TOKEN.captures_iter(&body).filter_map(|c| c.get(1)?.as_str().parse().ok()).collect();
        statistics.sort_unstable();
        statistics.dedup();
        let body = IS_TOKEN.replace_all(&body, "is$1").into_owned();

        let mut parameters: Vec<String> = SCALARS.iter().map(|s| s.to_string()).collect();
        for side in BOOK_SIDES {
            for level in 1..=5 {
                parameters.push(format!("{side}{level}"));
            }
        }
        parameters.push("ct".into());
        parameters.push("ih".into());
        parameters.extend(statistics.iter().map(|s| format!("is{s}")));

        let runtime = Runtime::new().map_err(|e| Error::InvalidOperation(format!("Could not start the filter engine: {e}")))?;
        runtime.set_memory_limit(MEMORY_LIMIT);
        runtime.set_max_stack_size(STACK_LIMIT);
        let budget = Arc::new(Budget {
            ticks: AtomicU64::new(0),
            started: std::sync::Mutex::new(Instant::now()),
            exhausted_by_time: AtomicBool::new(false),
            exhausted_by_count: AtomicBool::new(false),
        });
        let handler_budget = budget.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            let ticks = handler_budget.ticks.fetch_add(1, Ordering::Relaxed) + 1;
            if ticks > INTERRUPT_BUDGET {
                handler_budget.exhausted_by_count.store(true, Ordering::Relaxed);
                return true;
            }
            let started = *handler_budget.started.lock().unwrap_or_else(|e| e.into_inner());
            if started.elapsed() > ROW_TIMEOUT {
                handler_budget.exhausted_by_time.store(true, Ordering::Relaxed);
                return true;
            }
            false
        })));
        let context = Context::full(&runtime).map_err(|e| Error::InvalidOperation(format!("Could not start the filter engine: {e}")))?;

        let source = format!("(function({}) {{ return ({body}\n); }})", parameters.join(", "));
        let function = context.with(|ctx| -> Result<Persistent<Function<'static>>> {
            let function: Function = ctx.eval(source.as_bytes().to_vec()).catch(&ctx).map_err(|e| Error::FilterSyntax(caught_message(&e)))?;
            Ok(Persistent::save(&ctx, function))
        })?;

        Ok(TsetmcFilter {
            source: expression.to_string(),
            uses_history: IH_IDENTIFIER.is_match(&body),
            statistics,
            function: Some(function),
            context,
            _runtime: runtime,
            budget,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    /// True when the expression reads `[ih]`, so callers only load ~60 days of history when needed.
    pub fn uses_history(&self) -> bool {
        self.uses_history
    }

    /// The `[isN]` data types the expression reads.
    pub fn statistic_types(&self) -> &[i32] {
        &self.statistics
    }

    /// Evaluates the filter for every row; a row matches when the result is JavaScript-truthy. Errors in single rows
    /// skip that row and are reported, up to 20 distinct messages.
    pub fn apply(&self, rows: &[FilterRow], cancelled: Option<&AtomicBool>) -> Result<FilterResult> {
        let Some(function) = self.function.as_ref() else {
            return Err(Error::InvalidOperation("The filter was disposed.".into()));
        };
        let mut result = FilterResult::default();
        self.context.with(|ctx| -> Result<()> {
            let function = function.clone().restore(&ctx).map_err(|e| Error::InvalidOperation(e.to_string()))?;
            for row in rows {
                if cancelled.is_some_and(|c| c.load(Ordering::Relaxed)) {
                    return Err(Error::Cancelled);
                }
                result.evaluated += 1;
                self.reset_budget();
                match self.evaluate(&ctx, &function, row) {
                    Ok(true) => result.matches.push(row.ins_code),
                    Ok(false) => {}
                    Err(message) => {
                        if result.errors.len() < 20 && !result.errors.iter().any(|m| m.ends_with(&message)) {
                            result.errors.push(format!("{}: {message}", row.symbol));
                        }
                    }
                }
            }
            Ok(())
        })?;
        Ok(result)
    }

    fn reset_budget(&self) {
        self.budget.ticks.store(0, Ordering::Relaxed);
        self.budget.exhausted_by_time.store(false, Ordering::Relaxed);
        self.budget.exhausted_by_count.store(false, Ordering::Relaxed);
        *self.budget.started.lock().unwrap_or_else(|e| e.into_inner()) = Instant::now();
    }

    fn evaluate<'js>(&self, ctx: &Ctx<'js>, function: &Function<'js>, row: &FilterRow) -> std::result::Result<bool, String> {
        let args = self.arguments(ctx, row).map_err(|e| e.to_string())?;
        let outcome: rquickjs::Result<rquickjs::convert::Coerced<bool>> = function.call_arg(args);
        match outcome.catch(ctx) {
            Ok(value) => Ok(value.0),
            Err(error) => {
                if self.budget.exhausted_by_count.load(Ordering::Relaxed) {
                    Err("The maximum number of statements executed have been reached.".into())
                } else if self.budget.exhausted_by_time.load(Ordering::Relaxed) {
                    Err(format!("The script execution exceeded the {} ms timeout.", ROW_TIMEOUT.as_millis()))
                } else {
                    Err(caught_message(&error))
                }
            }
        }
    }

    fn arguments<'js>(&self, ctx: &Ctx<'js>, row: &FilterRow) -> rquickjs::Result<Args<'js>> {
        let count = SCALARS.len() + 30 + 2 + self.statistics.len();
        let mut args = Args::new(ctx.clone(), count);

        let last_change = row.last - row.yesterday;
        let close_change = row.close - row.yesterday;
        let percent = |change: Decimal| -> f64 {
            if row.yesterday.is_zero() {
                0.0
            } else {
                change
                    .checked_div(row.yesterday)
                    .map(|r| (r * Decimal::ONE_HUNDRED).round_dp_with_strategy(2, RoundingStrategy::MidpointNearestEven))
                    .and_then(|r| r.to_f64())
                    .unwrap_or(0.0)
            }
        };

        args.push_arg(row.symbol.as_str())?;
        args.push_arg(row.name.as_deref().unwrap_or(""))?;
        args.push_arg(row.trade_count as f64)?;
        args.push_arg(row.volume as f64)?;
        args.push_arg(num(row.value))?;
        args.push_arg(num(row.yesterday))?;
        args.push_arg(num(row.first))?;
        args.push_arg(num(row.low))?;
        args.push_arg(num(row.high))?;
        args.push_arg(num(row.last))?;
        args.push_arg(num(last_change))?;
        args.push_arg(percent(last_change))?;
        args.push_arg(num(row.close))?;
        args.push_arg(num(close_change))?;
        args.push_arg(percent(close_change))?;
        args.push_arg(num(row.eps.unwrap_or_default()))?;
        args.push_arg(num(row.price_earnings.unwrap_or_default()))?;
        args.push_arg(num(row.allowed_low))?;
        args.push_arg(num(row.allowed_high))?;
        args.push_arg(num(row.total_shares))?;
        args.push_arg(num(row.close.checked_mul(row.total_shares).unwrap_or_default()))?;
        args.push_arg(row.base_volume as f64)?;
        args.push_arg(f64::from(row.sector_code))?;
        args.push_arg(0.0)?; // buyop: not available
        args.push_arg(0.0)?; // predtran: not available

        for side in BOOK_SIDES {
            for level in 1..=5 {
                let value = row.best_limits.iter().find(|b| b.level == level).map_or(0.0, |l| match side {
                    "pd" => num(l.bid_price),
                    "zd" => f64::from(l.bid_orders),
                    "qd" => l.bid_volume as f64,
                    "po" => num(l.ask_price),
                    "zo" => f64::from(l.ask_orders),
                    _ => l.ask_volume as f64,
                });
                args.push_arg(value)?;
            }
        }

        args.push_arg(client_type(ctx, row.client_type.as_ref())?)?;
        args.push_arg(if self.uses_history { history(ctx, &row.history)? } else { Array::new(ctx.clone())? })?;
        for data_type in &self.statistics {
            args.push_arg(row.statistics.get(data_type).copied().unwrap_or(0.0))?;
        }
        Ok(args)
    }
}

fn num(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

fn client_type<'js>(ctx: &Ctx<'js>, flows: Option<&ClientTypeDay>) -> rquickjs::Result<Object<'js>> {
    let ct = Object::new(ctx.clone())?;
    let f = flows.cloned().unwrap_or_default();
    ct.set("Buy_CountI", f64::from(f.buy_individual_count))?;
    ct.set("Buy_CountN", f64::from(f.buy_institutional_count))?;
    ct.set("Buy_I_Volume", f.buy_individual_volume as f64)?;
    ct.set("Buy_N_Volume", f.buy_institutional_volume as f64)?;
    ct.set("Sell_CountI", f64::from(f.sell_individual_count))?;
    ct.set("Sell_CountN", f64::from(f.sell_institutional_count))?;
    ct.set("Sell_I_Volume", f.sell_individual_volume as f64)?;
    ct.set("Sell_N_Volume", f.sell_institutional_volume as f64)?;
    Ok(ct)
}

fn history<'js>(ctx: &Ctx<'js>, bars: &[DailyBar]) -> rquickjs::Result<Array<'js>> {
    let items = Array::new(ctx.clone())?;
    for (i, bar) in bars.iter().enumerate() {
        let day = Object::new(ctx.clone())?;
        day.set("PClosing", num(bar.close))?;
        day.set("PDrCotVal", num(bar.last))?;
        day.set("ZTotTran", bar.trade_count as f64)?;
        day.set("QTotTran5J", bar.volume as f64)?;
        day.set("QTotCap", num(bar.value))?;
        day.set("PriceMin", num(bar.low))?;
        day.set("PriceMax", num(bar.high))?;
        day.set("PriceYesterday", num(bar.yesterday))?;
        day.set("PriceFirst", num(bar.open))?;
        items.set(i, day)?;
    }
    Ok(items)
}

fn caught_message(error: &CaughtError<'_>) -> String {
    match error {
        CaughtError::Exception(exception) => {
            let message = exception.message().unwrap_or_default();
            if message.is_empty() { exception.to_string() } else { message }
        }
        CaughtError::Value(value) => value.as_string().and_then(|s| s.to_string().ok()).unwrap_or_else(|| format!("{value:?}")),
        CaughtError::Error(error) => error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 17).unwrap()
    }

    fn row(ins_code: i64, close: i64, yesterday: i64, buy_i: i64, sell_i: i64, previous_closes: &[i64]) -> FilterRow {
        FilterRow {
            ins_code,
            symbol: format!("S{ins_code}"),
            close: Decimal::from(close),
            last: Decimal::from(close),
            yesterday: Decimal::from(yesterday),
            value: Decimal::from(1000),
            trade_count: 10,
            client_type: Some(ClientTypeDay { ins_code, date: day(), buy_individual_volume: buy_i, sell_individual_volume: sell_i, ..Default::default() }),
            history: previous_closes
                .iter()
                .enumerate()
                .map(|(i, c)| DailyBar {
                    ins_code,
                    date: day() - chrono::Duration::days(i as i64 + 1),
                    close: Decimal::from(*c),
                    yesterday: Decimal::from(*c - 1),
                    ..Default::default()
                })
                .collect(),
            statistics: [(5, (ins_code * 10) as f64)].into_iter().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn tsetmc_expressions_with_parenthesized_variables_and_client_types_work() {
        let filter = TsetmcFilter::compile("(pc)>(py) && (ct).Buy_I_Volume>(ct).Sell_I_Volume*1.5").unwrap();
        let result = filter.apply(&[row(1, 110, 100, 100, 50, &[]), row(2, 90, 100, 100, 50, &[]), row(3, 110, 100, 10, 50, &[])], None).unwrap();
        assert_eq!(vec![1], result.matches);
        assert!(!filter.uses_history());
    }

    #[test]
    fn history_and_statistics_tokens_are_rewritten_and_loaded_only_when_used() {
        let filter = TsetmcFilter::compile("[ih][0].PClosing<[ih][1].PClosing && [is5]>15").unwrap();
        let result = filter.apply(&[row(1, 10, 9, 100, 50, &[9, 10]), row(2, 10, 9, 100, 50, &[9, 10]), row(3, 10, 9, 100, 50, &[11, 10])], None).unwrap();
        assert!(filter.uses_history());
        assert_eq!(&[5], filter.statistic_types());
        assert_eq!(vec![2], result.matches);
    }

    #[test]
    fn code_mode_runs_and_a_row_error_skips_only_that_row() {
        let filter =
            TsetmcFilter::compile("true==function(){ var up=0; for(var i=0;i<2;i++){ if([ih][i].PClosing>[ih][i].PriceYesterday) up++; } return up==2; }()")
                .unwrap();
        let result = filter.apply(&[row(1, 10, 9, 100, 50, &[9, 8]), row(2, 10, 9, 100, 50, &[])], None).unwrap();
        assert_eq!(vec![1], result.matches);
        assert_eq!(1, result.errors.len());
        assert_eq!(2, result.evaluated);
    }

    #[test]
    fn syntax_errors_are_reported_at_compile_time() {
        assert!(matches!(TsetmcFilter::compile("(pc)>(py) &&"), Err(Error::FilterSyntax(_))));
    }

    #[test]
    fn the_sandbox_has_no_host_access_and_stops_runaway_loops() {
        let probe = TsetmcFilter::compile("typeof System === 'undefined' && typeof importNamespace === 'undefined' && typeof require === 'undefined'").unwrap();
        let runaway = TsetmcFilter::compile("true==function(){ while(true){} }()").unwrap();
        assert_eq!(1, probe.apply(&[row(1, 1, 1, 0, 0, &[])], None).unwrap().matches.len());
        let started = Instant::now();
        let result = runaway.apply(&[row(1, 1, 1, 0, 0, &[])], None).unwrap();
        assert!(result.matches.is_empty());
        assert_eq!(1, result.errors.len());
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
