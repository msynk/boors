//! The market watch for a session, with TSETMC filters (C# `MarketWatchService`).

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;

use super::{MarketAnalytics, to_f64};
use crate::domain::{BestLimitLevel, ClientTypeDay, DailyBar, InstrumentClass};
use crate::filters::{FilterResult, FilterRow, TsetmcFilter};
use crate::storage::values::*;
use crate::storage::{col, read_client_type};
use crate::{Date, Decimal, Result};

/// One row of the market-watch grid (دیده‌بان) for a session.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketWatchEntry {
    pub ins_code: i64,
    pub symbol: String,
    pub name: Option<String>,
    /// The rules-class key as stored (`tse_stock`).
    pub instrument_class: String,
    pub sector_code: Option<String>,
    pub day_status: Option<String>,
    pub close: Decimal,
    pub change_percent: Option<f64>,
    pub last: Decimal,
    pub last_change_percent: Option<f64>,
    pub volume: i64,
    pub value: Decimal,
    pub trades: i64,
    pub allowed_low: Option<Decimal>,
    pub allowed_high: Option<Decimal>,
    /// "buy" or "sell" when the close snapshot shows a queue at the band.
    pub queue: Option<String>,
    pub queue_value: Option<Decimal>,
    pub price_earnings: Option<Decimal>,
    pub market_cap: Option<Decimal>,
    pub individual_net_value: Option<Decimal>,
    pub per_capita_buy: Option<Decimal>,
    pub per_capita_sell: Option<Decimal>,
    pub buyer_power: Option<Decimal>,
    /// Share of the day's volume bought by individuals (0–1).
    pub individual_buy_share: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct MarketWatchRequest {
    pub session: Option<Date>,
    /// A TSETMC filter expression, e.g. `(pc)>(py) && (ct).Buy_I_Volume>(ct).Sell_I_Volume`.
    pub filter: Option<String>,
    /// Rules-class keys to include (`None` = all); secondary listings are always excluded.
    pub classes: Option<HashSet<String>>,
    pub traded_only: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketWatchResult {
    pub session: Option<Date>,
    pub entries: Vec<MarketWatchEntry>,
    pub filter: Option<FilterResult>,
}

struct LoadedRow {
    entry: MarketWatchEntry,
    filter: FilterRow,
    parent_ins_code: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MarketWatchService {
    analytics: MarketAnalytics,
}

impl MarketWatchService {
    pub fn new(analytics: MarketAnalytics) -> Self {
        Self { analytics }
    }

    pub fn query(&self, request: &MarketWatchRequest, cancelled: Option<&AtomicBool>) -> Result<MarketWatchResult> {
        let session = match request.session {
            Some(d) => d,
            None => match self.analytics.latest_session()? {
                Some(d) => d,
                None => return Ok(MarketWatchResult { session: None, entries: Vec::new(), filter: None }),
            },
        };

        let filter = match request.filter.as_deref().filter(|f| !crate::text::is_blank(f)) {
            Some(expression) => Some(TsetmcFilter::compile(expression)?),
            None => None,
        };
        let mut rows: Vec<LoadedRow> = self
            .load_rows(session, filter.as_ref().is_some_and(TsetmcFilter::uses_history), filter.as_ref().map(TsetmcFilter::statistic_types).unwrap_or(&[]))?
            .into_iter()
            .filter(|r| r.parent_ins_code.is_none())
            .filter(|r| request.classes.as_ref().is_none_or(|c| c.contains(&r.entry.instrument_class)))
            .filter(|r| !request.traded_only || r.entry.trades > 0)
            .collect();

        let mut filter_result = None;
        if let Some(filter) = &filter {
            let filter_rows: Vec<FilterRow> = rows.iter().map(|r| r.filter.clone()).collect();
            let started = std::time::Instant::now();
            let result = filter.apply(&filter_rows, cancelled)?;
            tracing::debug!(
                rows = filter_rows.len(),
                uses_history = filter.uses_history(),
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "filter evaluated"
            );
            let matches: HashSet<i64> = result.matches.iter().copied().collect();
            rows.retain(|r| matches.contains(&r.entry.ins_code));
            filter_result = Some(result);
        }

        let mut entries: Vec<MarketWatchEntry> = rows.into_iter().map(|r| r.entry).collect();
        // OrderByDescending(Value), stable.
        entries.sort_by_key(|e| std::cmp::Reverse(e.value));
        Ok(MarketWatchResult { session: Some(session), entries, filter: filter_result })
    }

    /// The variables a filter sees for every instrument of the session (exposed for tests and the CLI).
    pub fn load_filter_rows(&self, session: Date, include_history: bool, statistics: &[i32]) -> Result<Vec<FilterRow>> {
        Ok(self.load_rows(session, include_history, statistics)?.into_iter().map(|r| r.filter).collect())
    }

    fn load_rows(&self, session: Date, include_history: bool, statistics: &[i32]) -> Result<Vec<LoadedRow>> {
        let mut history = if include_history { self.load_history(session)? } else { HashMap::new() };
        let mut stats = if statistics.is_empty() { HashMap::new() } else { self.load_statistics(statistics)? };

        let rows = self.analytics.store().query(
            "SELECT i.ins_code, i.symbol, i.name, i.instrument_class, i.sector_code, i.total_shares, i.base_volume, i.parent_ins_code,
                    b.open, b.high, b.low, b.close, b.last, b.yesterday, b.trade_count, b.volume, b.value, b.day_status,
                    b.eps, b.pe, b.allowed_low, b.allowed_high, b.bid1_price, b.bid1_volume, b.bid1_orders, b.ask1_price, b.ask1_volume, b.ask1_orders,
                    c.ins_code, c.d, c.buy_i_volume, c.buy_n_volume, c.sell_i_volume, c.sell_n_volume, c.buy_i_value, c.buy_n_value, c.sell_i_value, c.sell_n_value,
                    c.buy_i_count, c.buy_n_count, c.sell_i_count, c.sell_n_count, c.values_estimated
             FROM daily_bar b JOIN instrument i USING (ins_code)
             LEFT JOIN client_type_daily c ON c.ins_code = b.ins_code AND c.d = b.d
             WHERE b.d = $1",
            vec![v_date(session)],
            |r| {
                let ins_code = r_i64(col(r, 0)).unwrap_or_default();
                let bar = DailyBar {
                    ins_code,
                    date: session,
                    open: r_dec(col(r, 8)).unwrap_or_default(),
                    high: r_dec(col(r, 9)).unwrap_or_default(),
                    low: r_dec(col(r, 10)).unwrap_or_default(),
                    close: r_dec(col(r, 11)).unwrap_or_default(),
                    last: r_dec(col(r, 12)).unwrap_or_default(),
                    yesterday: r_dec(col(r, 13)).unwrap_or_default(),
                    trade_count: r_i64(col(r, 14)).unwrap_or_default(),
                    volume: r_i64(col(r, 15)).unwrap_or_default(),
                    value: r_dec(col(r, 16)).unwrap_or_default(),
                    last_trade_time: None,
                };
                let flows: Option<ClientTypeDay> = r_i64(col(r, 30)).map(|_| {
                    // Columns 28.. mirror get_client_types' layout.
                    let mut day = ClientTypeDay {
                        ins_code,
                        date: session,
                        buy_individual_volume: r_i64(col(r, 30)).unwrap_or_default(),
                        buy_institutional_volume: r_i64(col(r, 31)).unwrap_or_default(),
                        sell_individual_volume: r_i64(col(r, 32)).unwrap_or_default(),
                        sell_institutional_volume: r_i64(col(r, 33)).unwrap_or_default(),
                        buy_individual_value: r_dec(col(r, 34)).unwrap_or_default(),
                        buy_institutional_value: r_dec(col(r, 35)).unwrap_or_default(),
                        sell_individual_value: r_dec(col(r, 36)).unwrap_or_default(),
                        sell_institutional_value: r_dec(col(r, 37)).unwrap_or_default(),
                        buy_individual_count: r_i32(col(r, 38)).unwrap_or_default(),
                        buy_institutional_count: r_i32(col(r, 39)).unwrap_or_default(),
                        sell_individual_count: r_i32(col(r, 40)).unwrap_or_default(),
                        sell_institutional_count: r_i32(col(r, 41)).unwrap_or_default(),
                        values_estimated: r_bool(col(r, 42)).unwrap_or_default(),
                    };
                    day.date = session;
                    day
                });
                let _ = read_client_type;

                let allowed_low = r_dec(col(r, 20));
                let allowed_high = r_dec(col(r, 21));
                let bid_price = r_dec(col(r, 22));
                let ask_price = r_dec(col(r, 25));
                let top = if bid_price.is_none() && ask_price.is_none() {
                    None
                } else {
                    Some(BestLimitLevel {
                        level: 1,
                        bid_price: bid_price.unwrap_or_default(),
                        bid_volume: r_i64(col(r, 23)).unwrap_or_default(),
                        bid_orders: r_i32(col(r, 24)).unwrap_or_default(),
                        ask_price: ask_price.unwrap_or_default(),
                        ask_volume: r_i64(col(r, 26)).unwrap_or_default(),
                        ask_orders: r_i32(col(r, 27)).unwrap_or_default(),
                    })
                };
                let queue = top.as_ref().and_then(|t| {
                    if allowed_high.is_some_and(|h| h > Decimal::ZERO && t.bid_price >= h) && t.ask_volume == 0 {
                        Some("buy")
                    } else if allowed_low.is_some_and(|l| l > Decimal::ZERO && t.ask_price > Decimal::ZERO && t.ask_price <= l) && t.bid_volume == 0 {
                        Some("sell")
                    } else {
                        None
                    }
                });
                let queue_value = match (queue, &top) {
                    (Some("buy"), Some(t)) => Decimal::from(t.bid_volume).checked_mul(t.bid_price),
                    (Some("sell"), Some(t)) => Decimal::from(t.ask_volume).checked_mul(t.ask_price),
                    _ => None,
                };
                let total_shares = r_dec(col(r, 5)).unwrap_or_default();
                let symbol = r_str(col(r, 1)).unwrap_or_default();
                let sector_code = r_str(col(r, 4));
                let positive_yesterday = bar.yesterday > Decimal::ZERO;

                let entry = MarketWatchEntry {
                    ins_code,
                    symbol: symbol.clone(),
                    name: r_str(col(r, 2)),
                    instrument_class: r_str(col(r, 3)).unwrap_or_default(),
                    sector_code: sector_code.clone(),
                    day_status: r_str(col(r, 17)),
                    close: bar.close,
                    change_percent: if positive_yesterday { bar.close.checked_div(bar.yesterday).map(|x| to_f64(x - Decimal::ONE) * 100.0) } else { None },
                    last: bar.last,
                    last_change_percent: if positive_yesterday && bar.last > Decimal::ZERO {
                        bar.last.checked_div(bar.yesterday).map(|x| to_f64(x - Decimal::ONE) * 100.0)
                    } else {
                        None
                    },
                    volume: bar.volume,
                    value: bar.value,
                    trades: bar.trade_count,
                    allowed_low,
                    allowed_high,
                    queue: queue.map(str::to_string),
                    queue_value,
                    price_earnings: r_dec(col(r, 19)),
                    market_cap: if total_shares > Decimal::ZERO { bar.close.checked_mul(total_shares) } else { None },
                    individual_net_value: flows.as_ref().map(ClientTypeDay::individual_net_value),
                    per_capita_buy: flows.as_ref().and_then(ClientTypeDay::per_capita_buy),
                    per_capita_sell: flows.as_ref().and_then(ClientTypeDay::per_capita_sell),
                    buyer_power: flows.as_ref().and_then(ClientTypeDay::buyer_power),
                    individual_buy_share: match &flows {
                        Some(f) if bar.volume > 0 => Some(f.buy_individual_volume as f64 / bar.volume as f64),
                        _ => None,
                    },
                };

                let filter = FilterRow {
                    ins_code,
                    symbol,
                    name: entry.name.clone(),
                    trade_count: bar.trade_count,
                    volume: bar.volume,
                    value: bar.value,
                    yesterday: bar.yesterday,
                    first: bar.open,
                    low: bar.low,
                    high: bar.high,
                    last: bar.last,
                    close: bar.close,
                    eps: r_dec(col(r, 18)),
                    price_earnings: entry.price_earnings,
                    allowed_low: allowed_low.unwrap_or_default(),
                    allowed_high: allowed_high.unwrap_or_default(),
                    total_shares,
                    base_volume: r_i64(col(r, 6)).unwrap_or_default(),
                    sector_code: sector_code.as_deref().and_then(|s| s.trim().parse().ok()).unwrap_or(0),
                    best_limits: top.into_iter().collect(),
                    client_type: flows,
                    history: Vec::new(),
                    statistics: HashMap::new(),
                };
                LoadedRow { entry, filter, parent_ins_code: r_i64(col(r, 7)) }
            },
        )?;

        Ok(rows
            .into_iter()
            .map(|mut row| {
                row.filter.history = history.remove(&row.entry.ins_code).unwrap_or_default();
                row.filter.statistics = stats.remove(&row.entry.ins_code).unwrap_or_default();
                row
            })
            .collect())
    }

    /// Up to 60 previous trading rows per instrument, newest first (market-closed rows are not trading days).
    fn load_history(&self, session: Date) -> Result<HashMap<i64, Vec<DailyBar>>> {
        let sessions: Vec<Date> = self.analytics.get_sessions(62)?.into_iter().filter(|s| *s < session).collect();
        let Some(oldest) = sessions.last().copied() else { return Ok(HashMap::new()) };
        let bars = self.analytics.store().query(
            "SELECT ins_code, d, open, high, low, close, last, yesterday, trade_count, volume, value FROM daily_bar
             WHERE d < $1 AND d >= $2 AND day_status IS DISTINCT FROM 'MarketClosed'
             ORDER BY ins_code, d DESC",
            vec![v_date(session), v_date(oldest)],
            |r| DailyBar {
                ins_code: r_i64(col(r, 0)).unwrap_or_default(),
                date: r_date(col(r, 1)).unwrap_or_default(),
                open: r_dec(col(r, 2)).unwrap_or_default(),
                high: r_dec(col(r, 3)).unwrap_or_default(),
                low: r_dec(col(r, 4)).unwrap_or_default(),
                close: r_dec(col(r, 5)).unwrap_or_default(),
                last: r_dec(col(r, 6)).unwrap_or_default(),
                yesterday: r_dec(col(r, 7)).unwrap_or_default(),
                trade_count: r_i64(col(r, 8)).unwrap_or_default(),
                volume: r_i64(col(r, 9)).unwrap_or_default(),
                value: r_dec(col(r, 10)).unwrap_or_default(),
                last_trade_time: None,
            },
        )?;
        let mut grouped: HashMap<i64, Vec<DailyBar>> = HashMap::new();
        for bar in bars {
            let list = grouped.entry(bar.ins_code).or_default();
            if list.len() < 60 {
                list.push(bar);
            }
        }
        Ok(grouped)
    }

    fn load_statistics(&self, types: &[i32]) -> Result<HashMap<i64, HashMap<i32, f64>>> {
        let list = types.iter().map(i32::to_string).collect::<Vec<_>>().join(",");
        let rows = self.analytics.store().query(&format!("SELECT ins_code, data_type, value FROM inst_value WHERE data_type IN ({list})"), vec![], |r| {
            (r_i64(col(r, 0)).unwrap_or_default(), r_i32(col(r, 1)).unwrap_or_default(), r_f64(col(r, 2)).unwrap_or_default())
        })?;
        let mut map: HashMap<i64, HashMap<i32, f64>> = HashMap::new();
        for (ins, data_type, value) in rows {
            map.entry(ins).or_default().insert(data_type, value);
        }
        Ok(map)
    }
}

/// Accepts rules keys (`tse_stock`) and C# enum names (`TseStock`) for the class filter and returns rules keys.
pub fn normalize_class_filter(classes: &[String]) -> HashSet<String> {
    classes.iter().map(|c| InstrumentClass::parse(c).map_or_else(|| c.trim().to_string(), |k| k.rule_key().to_string())).collect()
}
