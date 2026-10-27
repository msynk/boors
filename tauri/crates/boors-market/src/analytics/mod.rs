//! Read-side analytics over the market store (C# `MarketAnalytics`): overview, breadth, sectors, movers,
//! per-instrument metrics, screens and search. Returns are `Close / Yesterday − 1` on priced days only, and
//! secondary listings are excluded.

pub mod market_watch;
pub mod point_in_time;
mod views;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::Duration;
use duckdb::types::Value;
use rust_decimal::prelude::ToPrimitive;

pub use market_watch::*;
pub use views::*;

use crate::domain::{DayStatus, Instrument};
use crate::sources::tgju::{self, series};
use crate::sources::tsetmc::TEDPIX_INS_CODE;
use crate::storage::values::*;
use crate::storage::{MarketStore, col, min_date};
use crate::text::match_key;
use crate::{Date, Decimal, Result};

/// Primary listings of shares (TSE, IFB, base market) — the universe for breadth, flows and screens.
pub const EQUITY_CLASSES: &str = "('tse_stock', 'ifb_stock', 'ifb_base_yellow', 'ifb_base_orange', 'ifb_base_red')";

pub const FUND_CLASSES: &str = "('etf_equity', 'etf_fixed_income', 'etf_real_estate', 'etf_mixed', 'fund_leveraged', 'fund_gold_silver', 'fund_of_funds')";

const EQUITY: &str = "i.parent_ins_code IS NULL AND i.instrument_class IN ('tse_stock', 'ifb_stock', 'ifb_base_yellow', 'ifb_base_orange', 'ifb_base_red')";

const RETURN: &str = "CASE WHEN b.trade_count > 0 AND b.yesterday > 0 AND b.day_status IN ('Traded', 'ReopenAuction', 'QueueLockedUp', 'QueueLockedDown')
     THEN CAST(b.close AS DOUBLE) / CAST(b.yesterday AS DOUBLE) - 1 END";

const HEADLINE_INDICES: [i64; 2] = [TEDPIX_INS_CODE, 67130298613737946];

const IFB_MAIN_INDEX_NAME: &str = "شاخص کل فرابورس";

/// Decimal → f64 (C# `(double)decimal`).
pub(crate) fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or(0.0)
}

#[derive(Debug, Clone)]
pub struct MarketAnalytics {
    store: Arc<MarketStore>,
}

impl MarketAnalytics {
    pub fn new(store: Arc<MarketStore>) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &MarketStore {
        &self.store
    }

    /// The most recent sessions (days on which anything traded), newest first.
    pub fn get_sessions(&self, count: i64) -> Result<Vec<Date>> {
        self.store.query("SELECT DISTINCT d FROM daily_bar WHERE trade_count > 0 ORDER BY d DESC LIMIT $1", vec![v_i64(count)], |r| {
            r_date(col(r, 0)).unwrap_or_default()
        })
    }

    pub fn latest_session(&self) -> Result<Option<Date>> {
        Ok(self.get_sessions(1)?.into_iter().next())
    }

    fn session_or_latest(&self, session: Option<Date>) -> Result<Option<Date>> {
        match session {
            Some(d) => Ok(Some(d)),
            None => self.latest_session(),
        }
    }

    pub fn get_overview(&self, session: Option<Date>) -> Result<Option<MarketOverview>> {
        let Some(d) = self.session_or_latest(session)? else { return Ok(None) };

        struct Equity {
            value: Decimal,
            trades: i64,
            advancers: i32,
            decliners: i32,
            unchanged: i32,
            not_traded: i32,
            halted: i32,
            has_snapshot: bool,
            buy_queues: i32,
            buy_queue_value: Decimal,
            sell_queues: i32,
            sell_queue_value: Decimal,
            locked_up: i32,
            locked_down: i32,
        }
        let equity = self
            .store
            .query(
                &format!(
                    "SELECT
                        coalesce(sum(b.value) FILTER (WHERE b.trade_count > 0), 0),
                        CAST(coalesce(sum(b.trade_count), 0) AS BIGINT),
                        count(*) FILTER (WHERE b.trade_count > 0 AND b.close > b.yesterday),
                        count(*) FILTER (WHERE b.trade_count > 0 AND b.close < b.yesterday),
                        count(*) FILTER (WHERE b.trade_count > 0 AND b.close = b.yesterday),
                        count(*) FILTER (WHERE b.trade_count = 0),
                        count(*) FILTER (WHERE b.day_status = 'SymbolHalted'),
                        count(*) FILTER (WHERE b.bid1_price IS NOT NULL),
                        count(*) FILTER (WHERE b.allowed_high > 0 AND b.bid1_price >= b.allowed_high AND coalesce(b.ask1_volume, 0) = 0),
                        sum(CAST(b.bid1_volume AS DECIMAL(38,4)) * b.bid1_price) FILTER (WHERE b.allowed_high > 0 AND b.bid1_price >= b.allowed_high AND coalesce(b.ask1_volume, 0) = 0),
                        count(*) FILTER (WHERE b.allowed_low > 0 AND b.ask1_price > 0 AND b.ask1_price <= b.allowed_low AND coalesce(b.bid1_volume, 0) = 0),
                        sum(CAST(b.ask1_volume AS DECIMAL(38,4)) * b.ask1_price) FILTER (WHERE b.allowed_low > 0 AND b.ask1_price > 0 AND b.ask1_price <= b.allowed_low AND coalesce(b.bid1_volume, 0) = 0),
                        count(*) FILTER (WHERE b.day_status = 'QueueLockedUp'),
                        count(*) FILTER (WHERE b.day_status = 'QueueLockedDown')
                    FROM daily_bar b JOIN instrument i USING (ins_code)
                    WHERE b.d = $1 AND {EQUITY}"
                ),
                vec![v_date(d)],
                |r| Equity {
                    value: r_dec(col(r, 0)).unwrap_or_default(),
                    trades: r_i64(col(r, 1)).unwrap_or_default(),
                    advancers: r_i32(col(r, 2)).unwrap_or_default(),
                    decliners: r_i32(col(r, 3)).unwrap_or_default(),
                    unchanged: r_i32(col(r, 4)).unwrap_or_default(),
                    not_traded: r_i32(col(r, 5)).unwrap_or_default(),
                    halted: r_i32(col(r, 6)).unwrap_or_default(),
                    has_snapshot: r_i64(col(r, 7)).unwrap_or_default() > 0,
                    buy_queues: r_i32(col(r, 8)).unwrap_or_default(),
                    buy_queue_value: r_dec(col(r, 9)).unwrap_or_default(),
                    sell_queues: r_i32(col(r, 10)).unwrap_or_default(),
                    sell_queue_value: r_dec(col(r, 11)).unwrap_or_default(),
                    locked_up: r_i32(col(r, 12)).unwrap_or_default(),
                    locked_down: r_i32(col(r, 13)).unwrap_or_default(),
                },
            )?
            .into_iter()
            .next()
            .ok_or_else(|| crate::Error::InvalidOperation("The overview query returned no row.".into()))?;

        let fund_value = self
            .store
            .query(
                &format!("SELECT coalesce(sum(b.value), 0) FROM daily_bar b JOIN instrument i USING (ins_code) WHERE b.d = $1 AND b.trade_count > 0 AND i.instrument_class IN {FUND_CLASSES}"),
                vec![v_date(d)],
                |r| r_dec(col(r, 0)).unwrap_or_default(),
            )?
            .into_iter()
            .next()
            .unwrap_or_default();

        let (net, per_capita_buy, per_capita_sell, estimated) = self
            .store
            .query(
                &format!(
                    "SELECT coalesce(sum(c.buy_i_value - c.sell_i_value), 0), sum(c.buy_i_value), CAST(sum(c.buy_i_count) AS BIGINT), sum(c.sell_i_value), CAST(sum(c.sell_i_count) AS BIGINT), coalesce(bool_or(c.values_estimated), false)
                     FROM client_type_daily c JOIN instrument i USING (ins_code)
                     WHERE c.d = $1 AND {EQUITY}"
                ),
                vec![v_date(d)],
                |r| {
                    let per_capita = |value: Option<Decimal>, count: Option<i64>| match (value, count) {
                        (Some(v), Some(n)) if n != 0 => v.checked_div(Decimal::from(n)),
                        _ => None,
                    };
                    (
                        r_dec(col(r, 0)).unwrap_or_default(),
                        per_capita(r_dec(col(r, 1)), r_i64(col(r, 2))),
                        per_capita(r_dec(col(r, 3)), r_i64(col(r, 4))),
                        r_bool(col(r, 5)).unwrap_or_default(),
                    )
                },
            )?
            .into_iter()
            .next()
            .unwrap_or_default();

        let indices = self.get_index_moves(d)?;
        let usd = self.get_macro_quote(series::USD_FREE_MARKET, d)?;
        let tedpix = indices.iter().find(|i| i.ins_code == TEDPIX_INS_CODE);
        let tedpix_usd = match (tedpix.and_then(|t| t.change_percent), usd.as_ref().and_then(|u| u.change_percent)) {
            (Some(change), Some(usd_change)) => Some(((1.0 + change / 100.0) / (1.0 + usd_change / 100.0) - 1.0) * 100.0),
            _ => None,
        };

        Ok(Some(MarketOverview {
            session: d,
            indices,
            equity_value: equity.value,
            fund_value,
            equity_trades: equity.trades,
            advancers: equity.advancers,
            decliners: equity.decliners,
            unchanged: equity.unchanged,
            not_traded: equity.not_traded,
            halted: equity.halted,
            buy_queues: if equity.has_snapshot { equity.buy_queues } else { equity.locked_up },
            buy_queue_value: equity.has_snapshot.then_some(equity.buy_queue_value),
            sell_queues: if equity.has_snapshot { equity.sell_queues } else { equity.locked_down },
            sell_queue_value: equity.has_snapshot.then_some(equity.sell_queue_value),
            individual_net_flow: net,
            individual_per_capita_buy: per_capita_buy,
            individual_per_capita_sell: per_capita_sell,
            flow_values_estimated: estimated,
            usd,
            coin: self.get_macro_quote(series::EMAMI_COIN, d)?,
            tedpix_usd_change_percent: tedpix_usd,
        }))
    }

    pub fn get_breadth(&self, sessions: i64) -> Result<Vec<BreadthDay>> {
        let dates = self.get_sessions(sessions)?;
        let Some(oldest) = dates.last().copied() else { return Ok(Vec::new()) };
        let wanted: HashSet<Date> = dates.iter().copied().collect();
        Ok(self
            .store
            .query(
                &format!(
                    "SELECT b.d,
                        count(*) FILTER (WHERE b.trade_count > 0 AND b.close > b.yesterday),
                        count(*) FILTER (WHERE b.trade_count > 0 AND b.close < b.yesterday),
                        count(*) FILTER (WHERE b.trade_count > 0 AND b.close = b.yesterday),
                        count(*) FILTER (WHERE b.trade_count = 0),
                        count(*) FILTER (WHERE b.day_status = 'SymbolHalted'),
                        count(*) FILTER (WHERE b.day_status = 'QueueLockedUp'),
                        count(*) FILTER (WHERE b.day_status = 'QueueLockedDown'),
                        coalesce(sum(b.value) FILTER (WHERE b.trade_count > 0), 0)
                    FROM daily_bar b JOIN instrument i USING (ins_code)
                    WHERE {EQUITY} AND b.d >= $1
                    GROUP BY b.d ORDER BY b.d"
                ),
                vec![v_date(oldest)],
                |r| BreadthDay {
                    date: r_date(col(r, 0)).unwrap_or_default(),
                    advancers: r_i32(col(r, 1)).unwrap_or_default(),
                    decliners: r_i32(col(r, 2)).unwrap_or_default(),
                    unchanged: r_i32(col(r, 3)).unwrap_or_default(),
                    not_traded: r_i32(col(r, 4)).unwrap_or_default(),
                    halted: r_i32(col(r, 5)).unwrap_or_default(),
                    locked_up: r_i32(col(r, 6)).unwrap_or_default(),
                    locked_down: r_i32(col(r, 7)).unwrap_or_default(),
                    equity_value: r_dec(col(r, 8)).unwrap_or_default(),
                },
            )?
            .into_iter()
            .filter(|b| wanted.contains(&b.date))
            .collect())
    }

    pub fn get_sectors(&self, session: Option<Date>) -> Result<Vec<SectorPerformance>> {
        let Some(d) = self.session_or_latest(session)? else { return Ok(Vec::new()) };
        self.store.query(
            &format!(
                "WITH x AS (
                    SELECT i.sector_code, i.sector_name, b.ins_code, b.trade_count, b.value, b.close, b.yesterday,
                           {RETURN} AS ret,
                           CAST(b.yesterday AS DOUBLE) * CAST(i.total_shares AS DOUBLE) AS cap
                    FROM daily_bar b JOIN instrument i USING (ins_code)
                    WHERE b.d = $1 AND {EQUITY} AND i.sector_code IS NOT NULL),
                f AS (SELECT ins_code, buy_i_value - sell_i_value AS net FROM client_type_daily WHERE d = $1)
                SELECT x.sector_code, coalesce(any_value(s.name), max(x.sector_name)),
                       count(*), count(*) FILTER (WHERE x.trade_count > 0),
                       coalesce(sum(x.value) FILTER (WHERE x.trade_count > 0), 0),
                       sum(x.ret * x.cap) / nullif(sum(x.cap) FILTER (WHERE x.ret IS NOT NULL), 0),
                       median(x.ret),
                       coalesce(sum(f.net), 0),
                       count(*) FILTER (WHERE x.trade_count > 0 AND x.close > x.yesterday),
                       count(*) FILTER (WHERE x.trade_count > 0 AND x.close < x.yesterday)
                FROM x LEFT JOIN f USING (ins_code) LEFT JOIN sector s ON s.code = x.sector_code
                GROUP BY x.sector_code
                ORDER BY 5 DESC"
            ),
            vec![v_date(d)],
            |r| SectorPerformance {
                sector_code: r_str(col(r, 0)).unwrap_or_default(),
                sector_name: r_str(col(r, 1)),
                instruments: r_i32(col(r, 2)).unwrap_or_default(),
                traded: r_i32(col(r, 3)).unwrap_or_default(),
                value: r_dec(col(r, 4)).unwrap_or_default(),
                weighted_return: r_f64(col(r, 5)),
                median_return: r_f64(col(r, 6)),
                individual_net_flow: r_dec(col(r, 7)).unwrap_or_default(),
                advancers: r_i32(col(r, 8)).unwrap_or_default(),
                decliners: r_i32(col(r, 9)).unwrap_or_default(),
            },
        )
    }

    pub fn get_movers(&self, kind: MoverKind, take: i64, session: Option<Date>) -> Result<Vec<Mover>> {
        let Some(d) = self.session_or_latest(session)? else { return Ok(Vec::new()) };
        let order = match kind {
            MoverKind::Gainers => "ret DESC NULLS LAST",
            MoverKind::Losers => "ret ASC NULLS LAST",
            MoverKind::Value => "b.value DESC",
            MoverKind::Inflow => "net DESC NULLS LAST",
            MoverKind::Outflow => "net ASC NULLS LAST",
        };
        self.store.query(
            &format!(
                "SELECT b.ins_code, i.symbol, i.name, i.instrument_class, b.day_status, b.close, {RETURN} AS ret, b.value, c.buy_i_value - c.sell_i_value AS net
                 FROM daily_bar b JOIN instrument i USING (ins_code)
                 LEFT JOIN client_type_daily c ON c.ins_code = b.ins_code AND c.d = b.d
                 WHERE b.d = $1 AND {EQUITY} AND b.trade_count > 0
                 ORDER BY {order}
                 LIMIT $2"
            ),
            vec![v_date(d), v_i64(take)],
            |r| Mover {
                ins_code: r_i64(col(r, 0)).unwrap_or_default(),
                symbol: r_str(col(r, 1)).unwrap_or_default(),
                name: r_str(col(r, 2)),
                instrument_class: r_str(col(r, 3)).unwrap_or_default(),
                day_status: r_str(col(r, 4)),
                close: r_dec(col(r, 5)).unwrap_or_default(),
                change_percent: r_f64(col(r, 6)).map(|v| v * 100.0),
                value: r_dec(col(r, 7)).unwrap_or_default(),
                individual_net_flow: r_dec(col(r, 8)),
            },
        )
    }

    pub fn get_instrument_metrics(&self, ins_code: i64) -> Result<Option<InstrumentMetrics>> {
        let Some(instrument) = self.store.get_instrument(ins_code)? else { return Ok(None) };
        let sector_name = match (&instrument.sector_name, &instrument.sector_code) {
            (Some(name), _) => Some(name.clone()),
            (None, Some(code)) => {
                self.store.query("SELECT name FROM sector WHERE code = $1", vec![v_str(code)], |r| r_str(col(r, 0)))?.into_iter().flatten().next()
            }
            (None, None) => None,
        };
        let stored = self.store.get_daily_bars(ins_code, None)?;
        let mut metrics = InstrumentMetrics {
            ins_code,
            symbol: instrument.symbol.clone(),
            name: instrument.name.clone(),
            instrument_class: instrument.class.rule_key().to_string(),
            sector_code: instrument.sector_code.clone(),
            sector_name,
            as_of: None,
            day_status: None,
            close: None,
            change_percent: None,
            market_cap: None,
            high_52_week: None,
            low_52_week: None,
            from_high_52_week_percent: None,
            returns: Vec::new(),
            usd_return_1_year: None,
            average_value_20: None,
            individual_net_flow_20: None,
            per_capita_buy: None,
            per_capita_sell: None,
            buyer_power: None,
            buyer_power_percentile: None,
        };
        let Some(last) = stored.last() else { return Ok(Some(metrics)) };

        let as_of = last.bar.date;
        let pairs: Vec<_> = stored.iter().map(|s| (s.bar.clone(), s.status)).collect();
        let adjusted = point_in_time::adjust_from_bars(&pairs, as_of);
        let last_close = adjusted.last().map(|a| a.adjusted_close).unwrap_or_default();

        let close_on_or_before = |date: Date| adjusted.iter().rev().find(|a| a.date <= date).map(|a| a.adjusted_close);
        let return_since = |start: Date| -> Option<f64> {
            let c = close_on_or_before(start)?;
            if c > Decimal::ZERO { last_close.checked_div(c).map(|r| to_f64(r - Decimal::ONE)) } else { None }
        };

        let year_start = as_of - Duration::days(365);
        let year_window: Vec<Decimal> =
            adjusted.iter().filter(|a| a.date > year_start && a.status.is_none_or(DayStatus::has_price)).map(|a| a.adjusted_close).collect();
        let high = year_window.iter().max().copied();
        let low = year_window.iter().min().copied();

        let flows = self.store.get_client_types(ins_code)?;
        let traded: Vec<_> = stored.iter().filter(|s| s.bar.trade_count > 0).collect();
        let recent_sessions = &traded[traded.len().saturating_sub(20)..];
        let recent_dates: HashSet<Date> = recent_sessions.iter().map(|s| s.bar.date).collect();
        let last_flow = flows.iter().rev().find(|f| f.date == as_of);
        let power_history: Vec<Decimal> = flows[flows.len().saturating_sub(250)..].iter().filter_map(|f| f.buyer_power()).collect();

        let usd = self.store.get_macro_daily(series::USD_FREE_MARKET, Some(tgju::SOURCE))?;
        let usd_on = |date: Date| usd.iter().rev().find(|u| u.date <= date).map(|u| u.close);
        let one_year = return_since(year_start);
        let usd_return = match (one_year, usd_on(as_of), usd_on(year_start)) {
            (Some(r1), Some(now), Some(then)) if then > Decimal::ZERO => now.checked_div(then).map(|ratio| (1.0 + r1) / to_f64(ratio) - 1.0),
            _ => None,
        };

        metrics.as_of = Some(as_of);
        metrics.day_status = last.status.map(|s| s.as_str().to_string());
        metrics.close = Some(last.bar.close);
        metrics.change_percent = point_in_time::daily_return(&last.bar, last.status).map(|c| to_f64(c) * 100.0);
        metrics.market_cap = instrument.total_shares.and_then(|shares| last.bar.close.checked_mul(shares));
        metrics.high_52_week = high;
        metrics.low_52_week = low;
        metrics.from_high_52_week_percent =
            high.filter(|h| *h > Decimal::ZERO).and_then(|h| last_close.checked_div(h)).map(|r| to_f64(r - Decimal::ONE) * 100.0);
        metrics.returns = vec![
            ("1w".into(), return_since(as_of - Duration::days(7))),
            ("1m".into(), return_since(as_of - Duration::days(30))),
            ("3m".into(), return_since(as_of - Duration::days(91))),
            ("6m".into(), return_since(as_of - Duration::days(182))),
            ("1y".into(), one_year),
        ];
        metrics.usd_return_1_year = usd_return;
        metrics.average_value_20 = if recent_sessions.is_empty() {
            None
        } else {
            let sum: Decimal = recent_sessions.iter().map(|s| s.bar.value).sum();
            sum.checked_div(Decimal::from(recent_sessions.len()))
        };
        metrics.individual_net_flow_20 = Some(flows.iter().filter(|f| recent_dates.contains(&f.date)).map(|f| f.individual_net_value()).sum());
        metrics.per_capita_buy = last_flow.and_then(|f| f.per_capita_buy());
        metrics.per_capita_sell = last_flow.and_then(|f| f.per_capita_sell());
        metrics.buyer_power = last_flow.and_then(|f| f.buyer_power());
        metrics.buyer_power_percentile = match metrics.buyer_power {
            Some(power) if power_history.len() >= 20 => Some(power_history.iter().filter(|p| **p <= power).count() as f64 / power_history.len() as f64),
            _ => None,
        };
        Ok(Some(metrics))
    }

    /// Ranks equities by one factor as of `as_of` (default: latest session), best first.
    pub fn screen(&self, kind: ScreenKind, take: usize, as_of: Option<Date>) -> Result<Vec<ScreenRow>> {
        let Some(d) = self.session_or_latest(as_of)? else { return Ok(Vec::new()) };
        let sessions: Vec<Date> = self.get_sessions(400)?.into_iter().filter(|s| *s <= d).collect();
        let (sql, params): (String, Vec<Value>) = match kind {
            ScreenKind::Momentum6To1 => (
                format!(
                    "WITH r AS (SELECT b.ins_code, {RETURN} AS ret FROM daily_bar b JOIN instrument i USING (ins_code)
                               WHERE {EQUITY} AND b.d > $1 AND b.d <= $2)
                     SELECT ins_code, exp(sum(ln(1 + ret))) - 1, count(ret) FROM r GROUP BY ins_code HAVING count(ret) >= 60"
                ),
                vec![v_date(d - Duration::days(182)), v_date(d - Duration::days(30))],
            ),
            ScreenKind::High52WeekProximity => (
                format!(
                    "WITH r AS (SELECT b.ins_code, b.d, {RETURN} AS ret FROM daily_bar b JOIN instrument i USING (ins_code)
                               WHERE {EQUITY} AND b.d > $1 AND b.d <= $2),
                         lvl AS (SELECT ins_code, d, ret, exp(sum(ln(1 + coalesce(ret, 0))) OVER (PARTITION BY ins_code ORDER BY d)) AS level FROM r)
                     SELECT ins_code, last(level ORDER BY d) / max(level), count(ret) FROM lvl GROUP BY ins_code HAVING count(ret) >= 120"
                ),
                vec![v_date(d - Duration::days(365)), v_date(d)],
            ),
            ScreenKind::IndividualFlowIntensity20 => (
                format!(
                    "SELECT b.ins_code, sum(c.buy_i_value - c.sell_i_value) / nullif(sum(b.value), 0), count(*)
                     FROM daily_bar b JOIN instrument i USING (ins_code) JOIN client_type_daily c ON c.ins_code = b.ins_code AND c.d = b.d
                     WHERE {EQUITY} AND b.trade_count > 0 AND b.d > $1 AND b.d <= $2
                     GROUP BY b.ins_code HAVING count(*) >= 15"
                ),
                vec![v_date(sessions.get(20).copied().filter(|_| sessions.len() > 20).unwrap_or_else(min_date)), v_date(d)],
            ),
            ScreenKind::BuyerPower5 => (
                format!(
                    "SELECT c.ins_code, avg((c.buy_i_value / c.buy_i_count) / (c.sell_i_value / c.sell_i_count)), count(*)
                     FROM client_type_daily c JOIN instrument i USING (ins_code)
                     WHERE {EQUITY} AND c.buy_i_count > 0 AND c.sell_i_count > 0 AND c.sell_i_value > 0 AND c.d > $1 AND c.d <= $2
                     GROUP BY c.ins_code HAVING count(*) >= 4"
                ),
                vec![v_date(sessions.get(5).copied().filter(|_| sessions.len() > 5).unwrap_or_else(min_date)), v_date(d)],
            ),
        };

        let mut scores: Vec<(i64, f64, i32)> = self
            .store
            .query(&sql, params, |r| (r_i64(col(r, 0)).unwrap_or_default(), r_f64(col(r, 1)).unwrap_or(f64::NAN), r_i32(col(r, 2)).unwrap_or_default()))?
            .into_iter()
            .filter(|s| !s.1.is_nan())
            .collect();
        // Best score first. DuckDB returns groups in no particular order, so ties (e.g. many instruments exactly at
        // their 52-week high) are broken by insCode to make the ranking deterministic.
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        scores.truncate(take);

        let instruments: HashMap<i64, Instrument> = self.store.get_instruments()?.into_iter().map(|i| (i.ins_code, i)).collect();
        Ok(scores
            .into_iter()
            .filter_map(|(ins_code, score, count)| {
                let i = instruments.get(&ins_code)?;
                Some(ScreenRow {
                    ins_code,
                    symbol: i.symbol.clone(),
                    name: i.name.clone(),
                    instrument_class: i.class.rule_key().to_string(),
                    sector_code: i.sector_code.clone(),
                    score,
                    observations: count,
                })
            })
            .collect())
    }

    /// Symbol and name search for the command palette: exact symbol first, then prefix, then contains.
    pub fn search(&self, text: &str, take: usize) -> Result<Vec<Instrument>> {
        let key = match_key(text);
        if key.is_empty() {
            return Ok(Vec::new());
        }
        let mut ranked: Vec<(Instrument, i32)> = self
            .store
            .get_instruments()?
            .into_iter()
            .filter_map(|i| {
                let symbol = match_key(&i.symbol);
                let name = match_key(i.name.as_deref().unwrap_or(""));
                let rank = if symbol == key {
                    0
                } else if symbol.starts_with(&key) {
                    1
                } else if name.contains(&key) || symbol.contains(&key) {
                    2
                } else {
                    9
                };
                (rank < 9).then_some((i, rank))
            })
            .collect();
        ranked.sort_by_key(|(i, rank)| (*rank, i.parent_ins_code.is_some(), i.symbol.encode_utf16().count()));
        Ok(ranked.into_iter().take(take).map(|(i, _)| i).collect())
    }

    fn ifb_main_index(&self) -> Result<Option<i64>> {
        let key = match_key(IFB_MAIN_INDEX_NAME);
        Ok(self.store.get_index_info()?.into_iter().find(|(_, name, flow)| *flow == Some(2) && match_key(name) == key).map(|(ins, _, _)| ins))
    }

    fn get_index_moves(&self, d: Date) -> Result<Vec<IndexMove>> {
        let mut codes: Vec<i64> = HEADLINE_INDICES.to_vec();
        if let Some(ifb) = self.ifb_main_index()?.filter(|c| *c != 0) {
            codes.push(ifb);
        }
        let names: HashMap<i64, String> = self.store.get_index_info()?.into_iter().map(|(ins, name, _)| (ins, name)).collect();
        let list = codes.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        let mut moves = self.store.query(
            &format!(
                "SELECT ins_code, close, lag(close) OVER (PARTITION BY ins_code ORDER BY d) AS prev
                 FROM index_daily WHERE ins_code IN ({list}) AND d <= $1
                 QUALIFY row_number() OVER (PARTITION BY ins_code ORDER BY d DESC) = 1"
            ),
            vec![v_date(d)],
            |r| {
                let ins = r_i64(col(r, 0)).unwrap_or_default();
                let close = r_dec(col(r, 1)).unwrap_or_default();
                let previous = r_dec(col(r, 2)).filter(|p| !p.is_zero());
                IndexMove {
                    ins_code: ins,
                    name: names.get(&ins).cloned().unwrap_or_else(|| ins.to_string()),
                    value: close,
                    change_percent: previous.and_then(|p| close.checked_div(p)).map(|r| to_f64(r - Decimal::ONE) * 100.0),
                }
            },
        )?;
        moves.sort_by_key(|m| codes.iter().position(|c| *c == m.ins_code).unwrap_or(usize::MAX));
        Ok(moves)
    }

    fn get_macro_quote(&self, series: &str, d: Date) -> Result<Option<MacroQuote>> {
        Ok(self
            .store
            .query(
                "SELECT d, close, lag(close) OVER (ORDER BY d) FROM macro_daily WHERE series = $1 AND d <= $2
                 QUALIFY row_number() OVER (ORDER BY d DESC) = 1",
                vec![v_str(series), v_date(d)],
                |r| {
                    let close = r_dec(col(r, 1)).unwrap_or_default();
                    let previous = r_dec(col(r, 2)).filter(|p| !p.is_zero());
                    MacroQuote {
                        series: series.to_string(),
                        date: r_date(col(r, 0)).unwrap_or_default(),
                        value: close,
                        change_percent: previous.and_then(|p| close.checked_div(p)).map(|r| to_f64(r - Decimal::ONE) * 100.0),
                    }
                },
            )?
            .into_iter()
            .next())
    }
}
