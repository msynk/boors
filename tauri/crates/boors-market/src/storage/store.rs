use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use duckdb::types::{Value, ValueRef};
use duckdb::{Connection, Row, appender_params_from_iter, params_from_iter};

use super::schema;
use super::values::*;
use crate::domain::deven;
use crate::domain::*;
use crate::sources::codal::CodalLetter;
use crate::sources::fipiran::FundRecord;
use crate::sources::tgju::MacroBar;
use crate::text::match_key;
use crate::{Date, Decimal, Error, Result};

/// A stored bar with its classification and origin ("history" or "marketwatch").
#[derive(Debug, Clone, PartialEq)]
pub struct StoredBar {
    pub bar: DailyBar,
    pub status: Option<DayStatus>,
    pub source: String,
    /// The day's permitted band from the end-of-day snapshot (null on history-only days).
    pub allowed_low: Option<Decimal>,
    pub allowed_high: Option<Decimal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectionRun {
    pub run_id: String,
    pub job: String,
    pub node: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: String,
    pub items: i32,
    pub failures: i32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DatasetFreshness {
    pub dataset: String,
    pub rows: i64,
    pub latest_date: Option<Date>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MacroMonthlyValue {
    pub series: String,
    pub period: String,
    pub period_end: Date,
    pub value: Decimal,
    pub source: String,
    pub note: Option<String>,
}

/// Embedded DuckDB store. Single file, no server; every write is an idempotent merge by primary key, and the whole
/// file can be rebuilt from the raw cache. One writer at a time; reads use their own cloned connections.
pub struct MarketStore {
    path: String,
    base: Mutex<Connection>,
}

impl std::fmt::Debug for MarketStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarketStore").field("path", &self.path).finish()
    }
}

const IN_MEMORY: &str = ":memory:";

/// C# `DateOnly.MinValue` (0001-01-01).
pub fn min_date() -> Date {
    Date::from_ymd_opt(1, 1, 1).unwrap_or_default()
}

/// First occurrence wins (LINQ `DistinctBy`).
fn distinct_by<T, K: Eq + Hash>(items: impl IntoIterator<Item = T>, key: impl Fn(&T) -> K) -> Vec<T> {
    let mut seen = HashSet::new();
    items.into_iter().filter(|item| seen.insert(key(item))).collect()
}

pub(crate) fn col<'a>(row: &'a Row<'_>, index: usize) -> ValueRef<'a> {
    row.get_ref(index).unwrap_or(ValueRef::Null)
}

fn read_bar(row: &Row<'_>) -> StoredBar {
    let bar = DailyBar {
        ins_code: r_i64(col(row, 0)).unwrap_or_default(),
        date: r_date(col(row, 1)).unwrap_or_default(),
        open: r_dec(col(row, 2)).unwrap_or_default(),
        high: r_dec(col(row, 3)).unwrap_or_default(),
        low: r_dec(col(row, 4)).unwrap_or_default(),
        close: r_dec(col(row, 5)).unwrap_or_default(),
        last: r_dec(col(row, 6)).unwrap_or_default(),
        yesterday: r_dec(col(row, 7)).unwrap_or_default(),
        trade_count: r_i64(col(row, 8)).unwrap_or_default(),
        volume: r_i64(col(row, 9)).unwrap_or_default(),
        value: r_dec(col(row, 10)).unwrap_or_default(),
        last_trade_time: r_i32(col(row, 11)).and_then(deven::to_time),
    };
    let status = r_str(col(row, 12)).as_deref().and_then(DayStatus::parse);
    StoredBar { bar, status, source: r_str(col(row, 13)).unwrap_or_default(), allowed_low: r_dec(col(row, 14)), allowed_high: r_dec(col(row, 15)) }
}

const BAR_COLUMNS: &str =
    "ins_code, d, open, high, low, close, last, yesterday, trade_count, volume, value, h_even, day_status, source, allowed_low, allowed_high";

const INSTRUMENT_COLUMNS: &str = "ins_code, symbol, name, latin_name, isin, company_isin, y_val, flow, board, board_title, sector_code, sector_name, \
     sub_sector_code, sub_sector_name, base_volume, total_shares, free_float, instrument_class, parent_ins_code";

fn read_instrument(row: &Row<'_>) -> Instrument {
    Instrument {
        ins_code: r_i64(col(row, 0)).unwrap_or_default(),
        symbol: r_str(col(row, 1)).unwrap_or_default(),
        name: r_str(col(row, 2)),
        latin_name: r_str(col(row, 3)),
        isin: r_str(col(row, 4)),
        company_isin: r_str(col(row, 5)),
        y_val: r_i32(col(row, 6)),
        flow: r_i32(col(row, 7)),
        board: r_str(col(row, 8)),
        board_title: r_str(col(row, 9)),
        sector_code: r_str(col(row, 10)),
        sector_name: r_str(col(row, 11)),
        sub_sector_code: r_i32(col(row, 12)),
        sub_sector_name: r_str(col(row, 13)),
        base_volume: r_i64(col(row, 14)),
        total_shares: r_dec(col(row, 15)),
        free_float_percent: r_dec(col(row, 16)),
        class: InstrumentClass::from_rule_key(&r_str(col(row, 17)).unwrap_or_default()),
        parent_ins_code: r_i64(col(row, 18)),
    }
}

const MESSAGE_COLUMNS: &str = "m.msg_id, m.d, m.h_even, m.title, m.body, m.flow, m.kind, m.reason, m.reopens_without_limit";

fn read_message(row: &Row<'_>) -> SupervisorMessage {
    SupervisorMessage {
        id: r_i64(col(row, 0)).unwrap_or_default(),
        date: r_date(col(row, 1)).unwrap_or_default(),
        time: r_i32(col(row, 2)).and_then(deven::to_time),
        title: r_str(col(row, 3)).unwrap_or_default(),
        body: r_str(col(row, 4)),
        flow: r_i32(col(row, 5)).unwrap_or_default(),
        kind: r_str(col(row, 6)).as_deref().and_then(MessageKind::parse).unwrap_or_default(),
        reason: r_str(col(row, 7)).as_deref().and_then(HaltReason::parse).unwrap_or_default(),
        reopens_without_limit: r_bool(col(row, 8)).unwrap_or_default(),
        mentioned_symbols: Vec::new(),
    }
}

fn h_even_of(time: Option<crate::Time>) -> Value {
    time.map_or(Value::Null, |t| Value::Int(deven::from_time(t)))
}

impl MarketStore {
    /// Opens (creating if needed) the store at `path`; `:memory:` gives an in-memory store.
    pub fn open(path: &str) -> Result<Self> {
        if path == IN_MEMORY {
            return Self::open_in_memory();
        }
        let full = std::path::absolute(Path::new(path))?;
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&full)?;
        let store = Self { path: full.to_string_lossy().into_owned(), base: Mutex::new(connection) };
        store.ensure_schema()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let store = Self { path: IN_MEMORY.into(), base: Mutex::new(Connection::open_in_memory()?) };
        store.ensure_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    /// A new connection to the same database (cheap; each operation uses its own).
    pub fn connect(&self) -> Result<Connection> {
        let base = self.base.lock().unwrap_or_else(|e| e.into_inner());
        Ok(base.try_clone()?)
    }

    fn ensure_schema(&self) -> Result<()> {
        let connection = self.connect()?;
        connection.execute_batch(schema::DDL)?;
        let version: Option<i32> = connection.query_row("SELECT max(version) FROM schema_info", [], |r| Ok(r_i32(col(r, 0))))?;
        match version {
            None => {
                connection.execute_batch(&format!("INSERT INTO schema_info VALUES ({})", schema::VERSION))?;
            }
            Some(v) if v != schema::VERSION => {
                return Err(Error::InvalidOperation(format!(
                    "Market store '{}' has schema version {v}, this build expects {}. Delete the file and run 'boors rebuild'.",
                    self.path,
                    schema::VERSION
                )));
            }
            Some(_) => {}
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------ plumbing

    /// Runs a read query and maps every row.
    pub fn query<T>(&self, sql: &str, params: Vec<Value>, mut read: impl FnMut(&Row<'_>) -> T) -> Result<Vec<T>> {
        let connection = self.connect()?;
        let mut statement = connection.prepare(sql)?;
        let mut rows = statement.query(params_from_iter(params))?;
        let mut result = Vec::new();
        while let Some(row) = rows.next()? {
            result.push(read(row));
        }
        Ok(result)
    }

    fn execute(connection: &Connection, sql: &str, params: Vec<Value>) -> Result<usize> {
        Ok(connection.execute(sql, params_from_iter(params))?)
    }

    fn in_transaction<T>(connection: &Connection, work: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        connection.execute_batch("BEGIN TRANSACTION")?;
        match work(connection) {
            Ok(value) => {
                connection.execute_batch("COMMIT")?;
                Ok(value)
            }
            Err(e) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Creates an empty staging table with the target's column types and fills it with an appender.
    fn stage(connection: &Connection, table: &str, columns: &[&str], rows: &[Vec<Value>]) -> Result<String> {
        let stage = format!("stage_{table}_{}", uuid::Uuid::new_v4().simple());
        let column_list = columns.join(", ");
        connection.execute_batch(&format!("CREATE TEMP TABLE {stage} AS SELECT {column_list} FROM {table} LIMIT 0"))?;
        {
            let mut appender = connection.appender_to_catalog_and_db(&stage, "temp", "main")?;
            for row in rows {
                appender.append_row(appender_params_from_iter(row.iter()))?;
            }
            appender.flush()?;
        }
        Ok(stage)
    }

    /// Idempotent bulk merge: rows go into a staging table, then `INSERT … ON CONFLICT` in one transaction. Callers
    /// de-duplicate keys (DuckDB cannot update the same row twice in one statement).
    fn merge(&self, table: &str, columns: &[&str], key: &[&str], rows: Vec<Vec<Value>>, on_conflict: Option<&str>) -> Result<()> {
        let connection = self.connect()?;
        Self::in_transaction(&connection, |c| {
            let stage = Self::stage(c, table, columns, &rows)?;
            if !rows.is_empty() {
                let column_list = columns.join(", ");
                let default_conflict;
                let conflict = match on_conflict {
                    Some(conflict) => conflict,
                    None => {
                        default_conflict = if columns.len() > key.len() {
                            format!(
                                "DO UPDATE SET {}",
                                columns.iter().filter(|c| !key.contains(c)).map(|c| format!("{c} = EXCLUDED.{c}")).collect::<Vec<_>>().join(", ")
                            )
                        } else {
                            "DO NOTHING".to_string()
                        };
                        &default_conflict
                    }
                };
                c.execute_batch(&format!("INSERT INTO {table} ({column_list}) SELECT {column_list} FROM {stage} ON CONFLICT ({}) {conflict}", key.join(", ")))?;
            }
            c.execute_batch(&format!("DROP TABLE {stage}"))?;
            Ok(())
        })
    }

    /// Bulk UPDATE of existing rows from a staging table (rows that do not exist are ignored).
    fn update_from(&self, table: &str, columns: &[&str], key: &[&str], rows: Vec<Vec<Value>>) -> Result<()> {
        let connection = self.connect()?;
        Self::in_transaction(&connection, |c| {
            let stage = Self::stage(c, table, columns, &rows)?;
            let set = columns.iter().filter(|c| !key.contains(c)).map(|c| format!("{c} = s.{c}")).collect::<Vec<_>>().join(", ");
            let join = key.iter().map(|k| format!("{table}.{k} = s.{k}")).collect::<Vec<_>>().join(" AND ");
            c.execute_batch(&format!("UPDATE {table} SET {set} FROM {stage} AS s WHERE {join}"))?;
            c.execute_batch(&format!("DROP TABLE {stage}"))?;
            Ok(())
        })
    }

    // ------------------------------------------------------------------------------------------ instruments

    /// Merges instruments. Missing (null) fields never erase known values, and the instrument class only changes when
    /// the new source ranks at least as high (isin < yval < fund).
    pub fn upsert_instruments(&self, instruments: &[Instrument], seen_on: Date, class_source: &str, details_at: Option<DateTime<Utc>>) -> Result<()> {
        const COLUMNS: [&str; 23] = [
            "ins_code",
            "symbol",
            "name",
            "latin_name",
            "isin",
            "company_isin",
            "y_val",
            "flow",
            "board",
            "board_title",
            "sector_code",
            "sector_name",
            "sub_sector_code",
            "sub_sector_name",
            "base_volume",
            "total_shares",
            "free_float",
            "instrument_class",
            "class_source",
            "fund_type",
            "details_at",
            "first_seen",
            "last_seen",
        ];
        let nullable = COLUMNS
            .iter()
            .filter(|c| !["ins_code", "symbol", "instrument_class", "class_source", "first_seen", "last_seen"].contains(c))
            .map(|c| format!("{c} = COALESCE(EXCLUDED.{c}, instrument.{c})"))
            .collect::<Vec<_>>()
            .join(",\n    ");
        let rank = |column: &str| format!("(CASE {column} WHEN 'fund' THEN 3 WHEN 'yval' THEN 2 ELSE 1 END)");
        let on_conflict = format!(
            "DO UPDATE SET\n    symbol = EXCLUDED.symbol,\n    {nullable},\n    instrument_class = CASE WHEN {} >= {} THEN EXCLUDED.instrument_class ELSE instrument.instrument_class END,\n    class_source = CASE WHEN {} >= {} THEN EXCLUDED.class_source ELSE instrument.class_source END,\n    first_seen = LEAST(instrument.first_seen, EXCLUDED.first_seen),\n    last_seen = GREATEST(instrument.last_seen, EXCLUDED.last_seen)",
            rank("EXCLUDED.class_source"),
            rank("instrument.class_source"),
            rank("EXCLUDED.class_source"),
            rank("instrument.class_source")
        );

        let details = details_at.as_ref().map_or(Value::Null, v_ts);
        let rows = distinct_by(instruments.iter(), |i| i.ins_code)
            .into_iter()
            .map(|i| {
                vec![
                    v_i64(i.ins_code),
                    v_str(&i.symbol),
                    v_opt_str(i.name.as_deref()),
                    v_opt_str(i.latin_name.as_deref()),
                    v_opt_str(i.isin.as_deref()),
                    v_opt_str(i.company_isin.as_deref()),
                    v_opt_i32(i.y_val),
                    v_opt_i32(i.flow),
                    v_opt_str(i.board.as_deref()),
                    v_opt_str(i.board_title.as_deref()),
                    v_opt_str(i.sector_code.as_deref()),
                    v_opt_str(i.sector_name.as_deref()),
                    v_opt_i32(i.sub_sector_code),
                    v_opt_str(i.sub_sector_name.as_deref()),
                    v_opt_i64(i.base_volume),
                    v_opt_dec0(i.total_shares),
                    v_opt_dec(i.free_float_percent),
                    v_str(i.class.rule_key()),
                    v_str(class_source),
                    Value::Null,
                    details.clone(),
                    v_date(seen_on),
                    v_date(seen_on),
                ]
            })
            .collect();
        self.merge("instrument", &COLUMNS, &["ins_code"], rows, Some(&on_conflict))?;
        self.link_secondary_listings()
    }

    /// Links stock listings whose ISIN does not end in 0001 to the listing with the same ISIN root ending in 0001.
    fn link_secondary_listings(&self) -> Result<()> {
        let connection = self.connect()?;
        connection.execute_batch(
            "UPDATE instrument AS c SET parent_ins_code = p.ins_code
             FROM instrument AS p
             WHERE c.isin IS NOT NULL AND length(c.isin) = 12 AND right(c.isin, 4) <> '0001'
               AND c.instrument_class IN ('tse_stock', 'ifb_stock')
               AND p.isin = left(c.isin, 8) || '0001' AND p.ins_code <> c.ins_code
               AND c.parent_ins_code IS DISTINCT FROM p.ins_code",
        )?;
        Ok(())
    }

    /// Applies fund types from fipiran to ETFs: sets fund_type and, when the type implies a rules class, the class.
    pub fn apply_fund_types(&self, funds: &[FundRecord]) -> Result<usize> {
        let connection = self.connect()?;
        Self::in_transaction(&connection, |c| {
            let mut statement = c.prepare(
                "UPDATE instrument SET fund_type = $1,
                    instrument_class = COALESCE(CAST($2 AS VARCHAR), instrument_class),
                    class_source = CASE WHEN CAST($2 AS VARCHAR) IS NULL THEN class_source ELSE 'fund' END
                 WHERE ins_code = $3",
            )?;
            let mut updated = 0;
            for fund in distinct_by(funds.iter().filter(|f| f.ins_code.is_some_and(|c| c > 0)), |f| f.ins_code) {
                let class = fund.implied_class().map_or(Value::Null, |c| v_str(c.rule_key()));
                let params = vec![v_i32(fund.fund_type), class, v_opt_i64(fund.ins_code)];
                updated += statement.execute(params_from_iter(params))?;
            }
            Ok(updated)
        })
    }

    pub fn get_instruments(&self) -> Result<Vec<Instrument>> {
        self.query(&format!("SELECT {INSTRUMENT_COLUMNS} FROM instrument ORDER BY symbol"), vec![], read_instrument)
    }

    pub fn get_instrument(&self, ins_code: i64) -> Result<Option<Instrument>> {
        Ok(self.query(&format!("SELECT {INSTRUMENT_COLUMNS} FROM instrument WHERE ins_code = $1"), vec![v_i64(ins_code)], read_instrument)?.into_iter().next())
    }

    /// Instruments whose details (yVal, flow, board) have never been fetched.
    pub fn get_instruments_without_details(&self) -> Result<Vec<i64>> {
        self.query("SELECT ins_code FROM instrument WHERE details_at IS NULL ORDER BY ins_code", vec![], |r| r_i64(col(r, 0)).unwrap_or_default())
    }

    // ------------------------------------------------------------------------------------------ daily bars

    /// Writes bars with their classification; a market-watch row never replaces a history row for the same day.
    pub fn upsert_daily_bars(&self, bars: &[ClassifiedBar], source: &str, halt_reasons: Option<&HashMap<Date, HaltReason>>) -> Result<()> {
        self.upsert_daily_bars_with(bars, source, |bar| halt_reasons.and_then(|r| r.get(&bar.date)).copied())
    }

    /// [`upsert_daily_bars`](Self::upsert_daily_bars) with a halt-reason lookup per bar (batched backfills).
    pub fn upsert_daily_bars_with(&self, bars: &[ClassifiedBar], source: &str, halt_reason: impl Fn(&DailyBar) -> Option<HaltReason>) -> Result<()> {
        let rows = distinct_by(bars.iter(), |b| (b.bar.ins_code, b.bar.date))
            .into_iter()
            .map(|c| {
                let reason = if c.status == DayStatus::SymbolHalted { halt_reason(&c.bar).map(HaltReason::as_str) } else { None };
                vec![
                    v_i64(c.bar.ins_code),
                    v_date(c.bar.date),
                    v_dec(c.bar.open),
                    v_dec(c.bar.high),
                    v_dec(c.bar.low),
                    v_dec(c.bar.close),
                    v_dec(c.bar.last),
                    v_dec(c.bar.yesterday),
                    v_i64(c.bar.trade_count),
                    v_i64(c.bar.volume),
                    v_dec(c.bar.value),
                    h_even_of(c.bar.last_trade_time),
                    v_str(c.status.as_str()),
                    v_opt_str(reason),
                    v_str(source),
                ]
            })
            .collect();
        self.merge(
            "daily_bar",
            &[
                "ins_code",
                "d",
                "open",
                "high",
                "low",
                "close",
                "last",
                "yesterday",
                "trade_count",
                "volume",
                "value",
                "h_even",
                "day_status",
                "halt_reason",
                "source",
            ],
            &["ins_code", "d"],
            rows,
            Some(
                "DO UPDATE SET open = EXCLUDED.open, high = EXCLUDED.high, low = EXCLUDED.low, close = EXCLUDED.close, last = EXCLUDED.last,
                    yesterday = EXCLUDED.yesterday, trade_count = EXCLUDED.trade_count, volume = EXCLUDED.volume, value = EXCLUDED.value,
                    h_even = EXCLUDED.h_even, day_status = EXCLUDED.day_status, halt_reason = COALESCE(EXCLUDED.halt_reason, daily_bar.halt_reason),
                    source = EXCLUDED.source
                WHERE NOT (EXCLUDED.source = 'marketwatch' AND daily_bar.source = 'history')",
            ),
        )
    }

    /// Adds the end-of-day market-watch snapshot (band, EPS/PE, top of the book) to bars already stored.
    pub fn update_session_snapshots(&self, snapshots: &[SessionSnapshot]) -> Result<()> {
        let rows = distinct_by(snapshots.iter(), |s| (s.ins_code, s.date))
            .into_iter()
            .map(|s| {
                vec![
                    v_i64(s.ins_code),
                    v_date(s.date),
                    v_opt_dec(s.eps),
                    v_opt_dec(s.price_earnings),
                    v_dec(s.allowed_low),
                    v_dec(s.allowed_high),
                    v_opt_dec(s.top.as_ref().map(|t| t.bid_price)),
                    v_opt_i64(s.top.as_ref().map(|t| t.bid_volume)),
                    v_opt_i32(s.top.as_ref().map(|t| t.bid_orders)),
                    v_opt_dec(s.top.as_ref().map(|t| t.ask_price)),
                    v_opt_i64(s.top.as_ref().map(|t| t.ask_volume)),
                    v_opt_i32(s.top.as_ref().map(|t| t.ask_orders)),
                ]
            })
            .collect();
        self.update_from(
            "daily_bar",
            &[
                "ins_code",
                "d",
                "eps",
                "pe",
                "allowed_low",
                "allowed_high",
                "bid1_price",
                "bid1_volume",
                "bid1_orders",
                "ask1_price",
                "ask1_volume",
                "ask1_orders",
            ],
            &["ins_code", "d"],
            rows,
        )
    }

    pub fn upsert_sectors(&self, items: &[StaticDataItem]) -> Result<()> {
        let rows = distinct_by(items.iter().filter(|i| i.kind == "IndustrialGroup"), |i| i.code)
            .into_iter()
            .map(|i| vec![v_str(&format!("{:02}", i.code)), v_str(&i.name)])
            .collect();
        self.merge("sector", &["code", "name"], &["code"], rows, None)
    }

    pub fn upsert_instrument_statistics(&self, statistics: &[InstrumentStatistic]) -> Result<()> {
        let rows = distinct_by(statistics.iter(), |s| (s.ins_code, s.data_type))
            .into_iter()
            .map(|s| vec![v_i64(s.ins_code), v_i32(s.data_type), v_opt_date(s.date), v_f64(s.value)])
            .collect();
        self.merge("inst_value", &["ins_code", "data_type", "d", "value"], &["ins_code", "data_type"], rows, None)
    }

    /// Updates only the classification of already stored bars.
    pub fn update_day_statuses(&self, ins_code: i64, statuses: &[(Date, DayStatus, Option<HaltReason>)]) -> Result<()> {
        let connection = self.connect()?;
        Self::in_transaction(&connection, |c| {
            let mut statement = c.prepare("UPDATE daily_bar SET day_status = $1, halt_reason = $2 WHERE ins_code = $3 AND d = $4")?;
            for (date, status, reason) in statuses {
                let params = vec![v_str(status.as_str()), v_opt_str(reason.map(HaltReason::as_str)), v_i64(ins_code), v_date(*date)];
                statement.execute(params_from_iter(params))?;
            }
            Ok(())
        })
    }

    pub fn get_daily_bars(&self, ins_code: i64, from: Option<Date>) -> Result<Vec<StoredBar>> {
        let from = from.unwrap_or_else(min_date);
        self.query(&format!("SELECT {BAR_COLUMNS} FROM daily_bar WHERE ins_code = $1 AND d >= $2 ORDER BY d"), vec![v_i64(ins_code), v_date(from)], read_bar)
    }

    /// Stored bars of several instruments in one scan, grouped by instrument (bulk readers such as halt resolution).
    pub fn get_daily_bars_for(&self, ins_codes: &[i64]) -> Result<HashMap<i64, Vec<StoredBar>>> {
        let mut grouped: HashMap<i64, Vec<StoredBar>> = HashMap::new();
        if ins_codes.is_empty() {
            return Ok(grouped);
        }
        let list = ins_codes.iter().map(i64::to_string).collect::<Vec<_>>().join(",");
        for bar in self.query(&format!("SELECT {BAR_COLUMNS} FROM daily_bar WHERE ins_code IN ({list}) ORDER BY ins_code, d"), vec![], read_bar)? {
            grouped.entry(bar.bar.ins_code).or_default().push(bar);
        }
        Ok(grouped)
    }

    /// The last `count` stored bars of every instrument.
    pub fn get_recent_bars(&self, count: i64) -> Result<HashMap<i64, Vec<StoredBar>>> {
        let bars = self.query(
            &format!("SELECT {BAR_COLUMNS} FROM daily_bar QUALIFY row_number() OVER (PARTITION BY ins_code ORDER BY d DESC) <= $1 ORDER BY ins_code, d"),
            vec![v_i64(count)],
            read_bar,
        )?;
        let mut grouped: HashMap<i64, Vec<StoredBar>> = HashMap::new();
        for bar in bars {
            grouped.entry(bar.bar.ins_code).or_default().push(bar);
        }
        Ok(grouped)
    }

    pub fn get_latest_bar_dates(&self) -> Result<HashMap<i64, Date>> {
        Ok(self
            .query("SELECT ins_code, max(d) FROM daily_bar GROUP BY ins_code", vec![], |r| {
                (r_i64(col(r, 0)).unwrap_or_default(), r_date(col(r, 1)).unwrap_or_default())
            })?
            .into_iter()
            .collect())
    }

    // ------------------------------------------------------------------------------------------ flows and corporate events

    /// Merges flows; an estimated row never overwrites an official one.
    pub fn upsert_client_types(&self, days: &[ClientTypeDay]) -> Result<()> {
        const VALUE_COLUMNS: [&str; 13] = [
            "buy_i_volume",
            "buy_n_volume",
            "sell_i_volume",
            "sell_n_volume",
            "buy_i_value",
            "buy_n_value",
            "sell_i_value",
            "sell_n_value",
            "buy_i_count",
            "buy_n_count",
            "sell_i_count",
            "sell_n_count",
            "values_estimated",
        ];
        let rows = distinct_by(days.iter(), |d| (d.ins_code, d.date))
            .into_iter()
            .map(|d| {
                vec![
                    v_i64(d.ins_code),
                    v_date(d.date),
                    v_i64(d.buy_individual_volume),
                    v_i64(d.buy_institutional_volume),
                    v_i64(d.sell_individual_volume),
                    v_i64(d.sell_institutional_volume),
                    v_dec(d.buy_individual_value),
                    v_dec(d.buy_institutional_value),
                    v_dec(d.sell_individual_value),
                    v_dec(d.sell_institutional_value),
                    v_i32(d.buy_individual_count),
                    v_i32(d.buy_institutional_count),
                    v_i32(d.sell_individual_count),
                    v_i32(d.sell_institutional_count),
                    v_bool(d.values_estimated),
                ]
            })
            .collect();
        let mut columns = vec!["ins_code", "d"];
        columns.extend(VALUE_COLUMNS);
        let on_conflict = format!(
            "DO UPDATE SET {} WHERE NOT (EXCLUDED.values_estimated AND NOT client_type_daily.values_estimated)",
            VALUE_COLUMNS.iter().map(|c| format!("{c} = EXCLUDED.{c}")).collect::<Vec<_>>().join(", ")
        );
        self.merge("client_type_daily", &columns, &["ins_code", "d"], rows, Some(&on_conflict))
    }

    pub fn get_client_types(&self, ins_code: i64) -> Result<Vec<ClientTypeDay>> {
        self.query(
            "SELECT ins_code, d, buy_i_volume, buy_n_volume, sell_i_volume, sell_n_volume, buy_i_value, buy_n_value, sell_i_value, sell_n_value,
                    buy_i_count, buy_n_count, sell_i_count, sell_n_count, values_estimated
             FROM client_type_daily WHERE ins_code = $1 ORDER BY d",
            vec![v_i64(ins_code)],
            read_client_type,
        )
    }

    /// Stores adjustments; `source` is "published" (TSETMC list) or "implied" (from bars).
    pub fn upsert_price_adjustments(&self, adjustments: &[PriceAdjustment], source: &str) -> Result<()> {
        let rows = distinct_by(adjustments.iter(), |a| (a.ins_code, a.date))
            .into_iter()
            .map(|a| vec![v_i64(a.ins_code), v_date(a.date), v_str(source), v_dec(a.adjusted_close), v_dec(a.unadjusted_close)])
            .collect();
        self.merge("price_adjustment", &["ins_code", "d", "source", "adjusted_close", "unadjusted_close"], &["ins_code", "d", "source"], rows, None)
    }

    pub fn get_share_changes(&self, ins_code: i64) -> Result<Vec<ShareChange>> {
        self.query("SELECT ins_code, d, shares_before, shares_after FROM share_change WHERE ins_code = $1 ORDER BY d", vec![v_i64(ins_code)], |r| ShareChange {
            ins_code: r_i64(col(r, 0)).unwrap_or_default(),
            date: r_date(col(r, 1)).unwrap_or_default(),
            shares_before: r_dec(col(r, 2)).unwrap_or_default(),
            shares_after: r_dec(col(r, 3)).unwrap_or_default(),
        })
    }

    pub fn get_price_adjustments(&self, ins_code: i64, source: &str) -> Result<Vec<PriceAdjustment>> {
        self.query(
            "SELECT ins_code, d, adjusted_close, unadjusted_close FROM price_adjustment WHERE ins_code = $1 AND source = $2 ORDER BY d",
            vec![v_i64(ins_code), v_str(source)],
            |r| PriceAdjustment {
                ins_code: r_i64(col(r, 0)).unwrap_or_default(),
                date: r_date(col(r, 1)).unwrap_or_default(),
                adjusted_close: r_dec(col(r, 2)).unwrap_or_default(),
                unadjusted_close: r_dec(col(r, 3)).unwrap_or_default(),
            },
        )
    }

    pub fn upsert_share_changes(&self, changes: &[ShareChange]) -> Result<()> {
        let rows = distinct_by(changes.iter(), |c| (c.ins_code, c.date))
            .into_iter()
            .map(|c| vec![v_i64(c.ins_code), v_date(c.date), v_dec0(c.shares_before), v_dec0(c.shares_after)])
            .collect();
        self.merge("share_change", &["ins_code", "d", "shares_before", "shares_after"], &["ins_code", "d"], rows, None)
    }

    pub fn upsert_shareholders(&self, positions: &[ShareholderPosition]) -> Result<()> {
        let rows = distinct_by(positions.iter(), |p| (p.ins_code, p.snapshot_date, p.holder_share_id))
            .into_iter()
            .map(|p| {
                vec![
                    v_i64(p.ins_code),
                    v_date(p.snapshot_date),
                    v_i64(p.holder_share_id),
                    v_str(&p.holder_name),
                    v_dec0(p.shares),
                    v_dec(p.percent),
                    v_i32(p.change),
                    v_dec0(p.change_amount),
                ]
            })
            .collect();
        self.merge(
            "shareholder",
            &["ins_code", "snapshot_date", "holder_share_id", "holder_name", "shares", "percent", "change", "change_amount"],
            &["ins_code", "snapshot_date", "holder_share_id"],
            rows,
            None,
        )
    }

    // ------------------------------------------------------------------------------------------ indices

    pub fn upsert_index_info(&self, indices: &[(i64, String, i32)]) -> Result<()> {
        let rows = distinct_by(indices.iter(), |i| i.0).into_iter().map(|(ins, name, flow)| vec![v_i64(*ins), v_str(name), v_i32(*flow)]).collect();
        self.merge("index_info", &["ins_code", "name", "flow"], &["ins_code"], rows, None)
    }

    pub fn get_index_info(&self) -> Result<Vec<(i64, String, Option<i32>)>> {
        self.query("SELECT ins_code, name, flow FROM index_info ORDER BY name", vec![], |r| {
            (r_i64(col(r, 0)).unwrap_or_default(), r_str(col(r, 1)).unwrap_or_default(), r_i32(col(r, 2)))
        })
    }

    /// Stores index bars; a same-day "snapshot" row never replaces a "history" row.
    pub fn upsert_index_bars(&self, bars: &[IndexBar], source: &str) -> Result<()> {
        let rows = distinct_by(bars.iter(), |b| (b.ins_code, b.date))
            .into_iter()
            .map(|b| vec![v_i64(b.ins_code), v_date(b.date), v_dec(b.close), v_dec(b.low), v_dec(b.high), v_str(source)])
            .collect();
        self.merge(
            "index_daily",
            &["ins_code", "d", "close", "low", "high", "source"],
            &["ins_code", "d"],
            rows,
            Some("DO UPDATE SET close = EXCLUDED.close, low = EXCLUDED.low, high = EXCLUDED.high, source = EXCLUDED.source WHERE NOT (EXCLUDED.source = 'snapshot' AND index_daily.source = 'history')"),
        )
    }

    /// Instruments that already have official (history-sourced) bars.
    pub fn get_instruments_with_history(&self) -> Result<HashSet<i64>> {
        Ok(self
            .query("SELECT DISTINCT ins_code FROM daily_bar WHERE source = 'history'", vec![], |r| r_i64(col(r, 0)).unwrap_or_default())?
            .into_iter()
            .collect())
    }

    pub fn get_index_bars(&self, ins_code: i64) -> Result<Vec<IndexBar>> {
        self.query("SELECT ins_code, d, close, low, high FROM index_daily WHERE ins_code = $1 ORDER BY d", vec![v_i64(ins_code)], |r| IndexBar {
            ins_code: r_i64(col(r, 0)).unwrap_or_default(),
            date: r_date(col(r, 1)).unwrap_or_default(),
            close: r_dec(col(r, 2)).unwrap_or_default(),
            low: r_dec(col(r, 3)).unwrap_or_default(),
            high: r_dec(col(r, 4)).unwrap_or_default(),
        })
    }

    // ------------------------------------------------------------------------------------------ messages, states, closures

    pub fn upsert_messages(&self, messages: &[SupervisorMessage], symbol_to_ins_code: &HashMap<String, i64>) -> Result<()> {
        let list = distinct_by(messages.iter(), |m| m.id);
        let rows = list
            .iter()
            .map(|m| {
                vec![
                    v_i64(m.id),
                    v_date(m.date),
                    h_even_of(m.time),
                    v_str(&m.title),
                    v_opt_str(m.body.as_deref()),
                    v_i32(m.flow),
                    v_str(m.kind.as_str()),
                    v_str(m.reason.as_str()),
                    v_bool(m.reopens_without_limit),
                ]
            })
            .collect();
        self.merge(
            "supervisor_message",
            &["msg_id", "d", "h_even", "title", "body", "flow", "kind", "reason", "reopens_without_limit"],
            &["msg_id"],
            rows,
            None,
        )?;

        // Messages often name a numbered sibling listing ("فولاد1"); fall back to the symbol without trailing digits.
        let mut links: Vec<(i64, i64)> = Vec::new();
        for message in &list {
            for symbol in &message.mentioned_symbols {
                let key = match_key(symbol);
                let ins = symbol_to_ins_code.get(&key).or_else(|| symbol_to_ins_code.get(key.trim_end_matches(|c: char| c.is_ascii_digit())));
                if let Some(ins) = ins
                    && !links.contains(&(message.id, *ins))
                {
                    links.push((message.id, *ins));
                }
            }
        }
        let rows = links.into_iter().map(|(id, ins)| vec![v_i64(id), v_i64(ins)]).collect();
        self.merge("message_instrument", &["msg_id", "ins_code"], &["msg_id", "ins_code"], rows, Some("DO NOTHING"))
    }

    pub fn get_messages(&self, ins_code: i64) -> Result<Vec<SupervisorMessage>> {
        self.query(
            &format!("SELECT {MESSAGE_COLUMNS} FROM supervisor_message m JOIN message_instrument mi ON mi.msg_id = m.msg_id WHERE mi.ins_code = $1 ORDER BY m.d, m.h_even"),
            vec![v_i64(ins_code)],
            read_message,
        )
    }

    pub fn upsert_instrument_states(&self, states: &[InstrumentStateOnDate]) -> Result<()> {
        let rows = distinct_by(states.iter(), |s| (s.ins_code, s.date))
            .into_iter()
            .map(|s| vec![v_i64(s.ins_code), v_date(s.date), v_opt_str(s.state.as_deref())])
            .collect();
        self.merge("instrument_state", &["ins_code", "d", "state"], &["ins_code", "d"], rows, None)
    }

    pub fn get_instrument_states(&self, ins_code: i64) -> Result<HashMap<Date, Option<String>>> {
        Ok(self
            .query("SELECT d, state FROM instrument_state WHERE ins_code = $1", vec![v_i64(ins_code)], |r| {
                (r_date(col(r, 0)).unwrap_or_default(), r_str(col(r, 1)))
            })?
            .into_iter()
            .collect())
    }

    /// All instrument states, grouped by instrument (bulk read for rebuilds).
    pub fn get_all_instrument_states(&self) -> Result<HashMap<i64, HashMap<Date, Option<String>>>> {
        let rows = self.query("SELECT ins_code, d, state FROM instrument_state", vec![], |r| {
            (r_i64(col(r, 0)).unwrap_or_default(), r_date(col(r, 1)).unwrap_or_default(), r_str(col(r, 2)))
        })?;
        let mut map: HashMap<i64, HashMap<Date, Option<String>>> = HashMap::new();
        for (ins, d, state) in rows {
            map.entry(ins).or_default().insert(d, state);
        }
        Ok(map)
    }

    pub fn replace_closures(&self, closures: &[(Date, String, String)]) -> Result<()> {
        let rows: Vec<Vec<Value>> =
            distinct_by(closures.iter(), |c| c.0).into_iter().map(|(d, reason, origin)| vec![v_date(*d), v_str(reason), v_str(origin)]).collect();
        let connection = self.connect()?;
        Self::in_transaction(&connection, |c| {
            c.execute_batch("DELETE FROM market_closure")?;
            let mut appender = c.appender("market_closure")?;
            for row in &rows {
                appender.append_row(appender_params_from_iter(row.iter()))?;
            }
            appender.flush()?;
            Ok(())
        })
    }

    pub fn get_closure_days(&self) -> Result<Vec<Date>> {
        self.query("SELECT d FROM market_closure ORDER BY d", vec![], |r| r_date(col(r, 0)).unwrap_or_default())
    }

    // ------------------------------------------------------------------------------------------ codal, funds, macro

    pub fn upsert_codal_letters(&self, letters: &[CodalLetter], symbol_to_ins_code: &HashMap<String, i64>) -> Result<()> {
        let rows = distinct_by(letters.iter(), |l| l.tracing_no)
            .into_iter()
            .map(|l| {
                let ins = l.symbol.as_deref().and_then(|s| symbol_to_ins_code.get(&match_key(s)).copied());
                vec![
                    v_i64(l.tracing_no),
                    v_opt_i64(ins),
                    v_opt_str(l.symbol.as_deref()),
                    v_opt_str(l.company_name.as_deref()),
                    v_str(&l.title),
                    v_opt_str(l.letter_code.as_deref()),
                    v_opt_i32(l.letter_type_id),
                    v_opt_ts(l.published_at.as_ref()),
                    v_opt_ts(l.sent_at.as_ref()),
                    v_opt_str(l.disclosure_group.as_deref()),
                    v_bool(l.has_html),
                    v_bool(l.has_excel),
                    v_bool(l.has_pdf),
                    v_bool(l.has_xbrl),
                    v_bool(l.has_attachment),
                    v_bool(l.is_estimate),
                    v_i32(l.under_supervision),
                    v_opt_str(l.report_url.as_deref()),
                    v_opt_str(l.excel_url.as_deref()),
                    v_opt_str(l.pdf_url.as_deref()),
                    v_opt_str(l.attachment_url.as_deref()),
                ]
            })
            .collect();
        self.merge(
            "codal_letter",
            &[
                "tracing_no",
                "ins_code",
                "symbol",
                "company_name",
                "title",
                "letter_code",
                "letter_type_id",
                "published_at",
                "sent_at",
                "disclosure_group",
                "has_html",
                "has_excel",
                "has_pdf",
                "has_xbrl",
                "has_attachment",
                "is_estimate",
                "under_supervision",
                "report_url",
                "excel_url",
                "pdf_url",
                "attachment_url",
            ],
            &["tracing_no"],
            rows,
            None,
        )
    }

    pub fn get_codal_letters(&self, ins_code: i64, take: i64) -> Result<Vec<CodalLetter>> {
        self.query(
            "SELECT tracing_no, symbol, company_name, title, letter_code, letter_type_id, published_at, disclosure_group, has_excel, report_url, excel_url, pdf_url, attachment_url
             FROM codal_letter WHERE ins_code = $1 ORDER BY published_at DESC NULLS LAST LIMIT $2",
            vec![v_i64(ins_code), v_i64(take)],
            |r| CodalLetter {
                tracing_no: r_i64(col(r, 0)).unwrap_or_default(),
                symbol: r_str(col(r, 1)),
                company_name: r_str(col(r, 2)),
                title: r_str(col(r, 3)).unwrap_or_default(),
                letter_code: r_str(col(r, 4)),
                letter_type_id: r_i32(col(r, 5)),
                published_at: r_ts(col(r, 6)).map(|t| t.fixed_offset()),
                disclosure_group: r_str(col(r, 7)),
                has_excel: r_bool(col(r, 8)).unwrap_or(false),
                report_url: r_str(col(r, 9)),
                excel_url: r_str(col(r, 10)),
                pdf_url: r_str(col(r, 11)),
                attachment_url: r_str(col(r, 12)),
                ..CodalLetter::default()
            },
        )
    }

    /// The most recent shareholder snapshot of an instrument.
    pub fn get_latest_shareholders(&self, ins_code: i64) -> Result<Vec<ShareholderPosition>> {
        self.query(
            "SELECT ins_code, snapshot_date, holder_share_id, holder_name, shares, percent, change, change_amount FROM shareholder
             WHERE ins_code = $1 AND snapshot_date = (SELECT max(snapshot_date) FROM shareholder WHERE ins_code = $1)
             ORDER BY percent DESC",
            vec![v_i64(ins_code)],
            |r| ShareholderPosition {
                ins_code: r_i64(col(r, 0)).unwrap_or_default(),
                snapshot_date: r_date(col(r, 1)).unwrap_or_default(),
                holder_share_id: r_i64(col(r, 2)).unwrap_or_default(),
                holder_name: r_str(col(r, 3)).unwrap_or_default(),
                shares: r_dec(col(r, 4)).unwrap_or_default(),
                percent: r_dec(col(r, 5)).unwrap_or_default(),
                change: r_i32(col(r, 6)).unwrap_or_default(),
                change_amount: r_dec(col(r, 7)).unwrap_or_default(),
            },
        )
    }

    pub fn get_known_tracing_numbers(&self, candidates: &[i64]) -> Result<HashSet<i64>> {
        let list: Vec<i64> = distinct_by(candidates.iter().copied(), |c| *c);
        if list.is_empty() {
            return Ok(HashSet::new());
        }
        let sql = format!("SELECT tracing_no FROM codal_letter WHERE tracing_no IN ({})", list.iter().map(i64::to_string).collect::<Vec<_>>().join(","));
        Ok(self.query(&sql, vec![], |r| r_i64(col(r, 0)).unwrap_or_default())?.into_iter().collect())
    }

    pub fn upsert_funds(&self, funds: &[FundRecord]) -> Result<()> {
        let list = distinct_by(funds.iter(), |f| f.reg_no.clone());
        let rows = list
            .iter()
            .map(|f| {
                vec![
                    v_str(&f.reg_no),
                    v_opt_i64(f.ins_code),
                    v_str(&f.name),
                    v_opt_str(f.symbol.as_deref()),
                    v_i32(f.fund_type),
                    v_opt_str(f.type_of_invest.as_deref()),
                    v_opt_str(f.manager.as_deref()),
                    v_opt_date(f.initiation_date),
                ]
            })
            .collect();
        self.merge("fund", &["reg_no", "ins_code", "name", "symbol", "fund_type", "type_of_invest", "manager", "initiation_date"], &["reg_no"], rows, None)?;

        let rows = list
            .iter()
            .filter(|f| f.date.is_some())
            .map(|f| {
                vec![
                    v_str(&f.reg_no),
                    v_opt_date(f.date),
                    v_opt_dec(f.issue_nav),
                    v_opt_dec(f.cancel_nav),
                    v_opt_dec(f.statistical_nav),
                    v_opt_dec(f.net_asset),
                    v_opt_dec0(f.invested_units),
                    v_opt_dec(f.daily_return),
                    v_opt_dec(f.weekly_return),
                    v_opt_dec(f.monthly_return),
                    v_opt_dec(f.quarterly_return),
                    v_opt_dec(f.six_month_return),
                    v_opt_dec(f.annual_return),
                    v_opt_dec(f.stock_share),
                    v_opt_dec(f.bond_share),
                    v_opt_dec(f.cash_share),
                    v_opt_dec(f.deposit_share),
                    v_opt_dec(f.other_share),
                ]
            })
            .collect();
        self.merge(
            "fund_daily",
            &[
                "reg_no",
                "d",
                "issue_nav",
                "cancel_nav",
                "statistical_nav",
                "net_asset",
                "invested_units",
                "daily_return",
                "weekly_return",
                "monthly_return",
                "quarterly_return",
                "six_month_return",
                "annual_return",
                "stock_share",
                "bond_share",
                "cash_share",
                "deposit_share",
                "other_share",
            ],
            &["reg_no", "d"],
            rows,
            None,
        )
    }

    pub fn upsert_macro_daily(&self, bars: &[MacroBar]) -> Result<()> {
        let rows = distinct_by(bars.iter(), |b| (b.series.clone(), b.date, b.source.clone()))
            .into_iter()
            .map(|b| vec![v_str(&b.series), v_date(b.date), v_str(&b.source), v_dec(b.open), v_dec(b.low), v_dec(b.high), v_dec(b.close)])
            .collect();
        self.merge("macro_daily", &["series", "d", "source", "open", "low", "high", "close"], &["series", "d", "source"], rows, None)
    }

    pub fn get_macro_daily(&self, series: &str, source: Option<&str>) -> Result<Vec<MacroBar>> {
        let (sql, params) = match source {
            Some(source) => (
                "SELECT series, d, open, low, high, close, source FROM macro_daily WHERE series = $1 AND source = $2 ORDER BY d",
                vec![v_str(series), v_str(source)],
            ),
            None => ("SELECT series, d, open, low, high, close, source FROM macro_daily WHERE series = $1 ORDER BY d", vec![v_str(series)]),
        };
        self.query(sql, params, |r| MacroBar {
            series: r_str(col(r, 0)).unwrap_or_default(),
            date: r_date(col(r, 1)).unwrap_or_default(),
            open: r_dec(col(r, 2)).unwrap_or_default(),
            low: r_dec(col(r, 3)).unwrap_or_default(),
            high: r_dec(col(r, 4)).unwrap_or_default(),
            close: r_dec(col(r, 5)).unwrap_or_default(),
            source: r_str(col(r, 6)).unwrap_or_default(),
        })
    }

    pub fn upsert_macro_monthly(&self, values: &[MacroMonthlyValue]) -> Result<()> {
        let rows = distinct_by(values.iter(), |v| (v.series.clone(), v.period.clone(), v.source.clone()))
            .into_iter()
            .map(|v| vec![v_str(&v.series), v_str(&v.period), v_str(&v.source), v_date(v.period_end), v_dec_scale(v.value, 6), v_opt_str(v.note.as_deref())])
            .collect();
        self.merge("macro_monthly", &["series", "period", "source", "period_end", "value", "note"], &["series", "period", "source"], rows, None)
    }

    pub fn get_macro_monthly(&self, series: &str) -> Result<Vec<MacroMonthlyValue>> {
        self.query(
            "SELECT series, period, period_end, value, source, note FROM macro_monthly WHERE series = $1 ORDER BY period_end, source",
            vec![v_str(series)],
            |r| MacroMonthlyValue {
                series: r_str(col(r, 0)).unwrap_or_default(),
                period: r_str(col(r, 1)).unwrap_or_default(),
                period_end: r_date(col(r, 2)).unwrap_or_default(),
                value: r_dec(col(r, 3)).unwrap_or_default(),
                source: r_str(col(r, 4)).unwrap_or_default(),
                note: r_str(col(r, 5)),
            },
        )
    }

    // ------------------------------------------------------------------------------------------ runs and health

    pub fn record_run(&self, run: &CollectionRun) -> Result<()> {
        let row = vec![
            v_str(&run.run_id),
            v_str(&run.job),
            v_str(&run.node),
            v_ts(&run.started_at),
            run.finished_at.as_ref().map_or(Value::Null, v_ts),
            v_str(&run.status),
            v_i32(run.items),
            v_i32(run.failures),
            v_opt_str(run.message.as_deref()),
        ];
        self.merge(
            "collection_run",
            &["run_id", "job", "node", "started_at", "finished_at", "status", "items", "failures", "message"],
            &["run_id"],
            vec![row],
            None,
        )
    }

    pub fn get_latest_runs(&self) -> Result<Vec<CollectionRun>> {
        self.query(
            "SELECT run_id, job, node, started_at, finished_at, status, items, failures, message FROM collection_run
             QUALIFY row_number() OVER (PARTITION BY job ORDER BY started_at DESC) = 1 ORDER BY job",
            vec![],
            |r| CollectionRun {
                run_id: r_str(col(r, 0)).unwrap_or_default(),
                job: r_str(col(r, 1)).unwrap_or_default(),
                node: r_str(col(r, 2)).unwrap_or_default(),
                started_at: r_ts(col(r, 3)).unwrap_or_default(),
                finished_at: r_ts(col(r, 4)),
                status: r_str(col(r, 5)).unwrap_or_default(),
                items: r_i32(col(r, 6)).unwrap_or_default(),
                failures: r_i32(col(r, 7)).unwrap_or_default(),
                message: r_str(col(r, 8)),
            },
        )
    }

    pub fn get_freshness(&self) -> Result<Vec<DatasetFreshness>> {
        self.query(
            "SELECT 'instrument', count(*), max(last_seen) FROM instrument
             UNION ALL SELECT 'daily_bar', count(*), max(d) FROM daily_bar
             UNION ALL SELECT 'client_type_daily', count(*), max(d) FROM client_type_daily
             UNION ALL SELECT 'price_adjustment', count(*), max(d) FROM price_adjustment
             UNION ALL SELECT 'share_change', count(*), max(d) FROM share_change
             UNION ALL SELECT 'index_daily', count(*), max(d) FROM index_daily
             UNION ALL SELECT 'shareholder', count(*), max(snapshot_date) FROM shareholder
             UNION ALL SELECT 'supervisor_message', count(*), max(d) FROM supervisor_message
             UNION ALL SELECT 'codal_letter', count(*), CAST(CAST(max(published_at) AS TIMESTAMP) + INTERVAL 210 MINUTE AS DATE) FROM codal_letter
             UNION ALL SELECT 'fund_daily', count(*), max(d) FROM fund_daily
             UNION ALL SELECT 'macro_daily', count(*), max(d) FROM macro_daily
             UNION ALL SELECT 'macro_monthly', count(*), max(period_end) FROM macro_monthly
             UNION ALL SELECT 'market_closure', count(*), max(d) FROM market_closure",
            vec![],
            |r| DatasetFreshness { dataset: r_str(col(r, 0)).unwrap_or_default(), rows: r_i64(col(r, 1)).unwrap_or_default(), latest_date: r_date(col(r, 2)) },
        )
    }

    /// Runs an arbitrary query (CLI/research use). Returns column names and up to `max_rows` rows.
    pub fn run_query(&self, sql: &str, max_rows: usize) -> Result<(Vec<String>, Vec<Vec<Cell>>)> {
        let connection = self.connect()?;
        // Several statements (e.g. `ATTACH …; SELECT …`): run all but the last on the same connection.
        let statements = split_statements(sql);
        let Some((last, before)) = statements.split_last() else { return Ok((Vec::new(), Vec::new())) };
        for statement in before {
            connection.execute_batch(statement)?;
        }
        let mut statement = connection.prepare(last)?;
        let mut rows = statement.query([])?;
        let columns: Vec<String> = rows.as_ref().map(|s| s.column_names()).unwrap_or_default();
        let mut result = Vec::new();
        while result.len() < max_rows {
            let Some(row) = rows.next()? else { break };
            let cells = (0..columns.len()).map(|i| Cell::from_ref(col(row, i))).collect();
            result.push(cells);
        }
        Ok((columns, result))
    }

    /// Executes statements that return no rows (CLI `query` of a DDL/DML statement, tests).
    pub fn execute_batch(&self, sql: &str) -> Result<()> {
        Ok(self.connect()?.execute_batch(sql)?)
    }

    /// Exposed for collectors that issue a single parameterized statement.
    pub fn execute_sql(&self, sql: &str, params: Vec<Value>) -> Result<usize> {
        Self::execute(&self.connect()?, sql, params)
    }
}

/// Splits SQL on top-level semicolons (outside quotes and comments); empty statements are dropped.
fn split_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql.chars().peekable();
    let mut quote: Option<char> = None;
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    current.push(c);
                }
                '-' if chars.peek() == Some(&'-') => {
                    for c in chars.by_ref() {
                        if c == '\n' {
                            current.push('\n');
                            break;
                        }
                    }
                }
                ';' => statements.push(std::mem::take(&mut current)),
                _ => current.push(c),
            },
        }
    }
    statements.push(current);
    statements.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

pub(crate) fn read_client_type(r: &Row<'_>) -> ClientTypeDay {
    ClientTypeDay {
        ins_code: r_i64(col(r, 0)).unwrap_or_default(),
        date: r_date(col(r, 1)).unwrap_or_default(),
        buy_individual_volume: r_i64(col(r, 2)).unwrap_or_default(),
        buy_institutional_volume: r_i64(col(r, 3)).unwrap_or_default(),
        sell_individual_volume: r_i64(col(r, 4)).unwrap_or_default(),
        sell_institutional_volume: r_i64(col(r, 5)).unwrap_or_default(),
        buy_individual_value: r_dec(col(r, 6)).unwrap_or_default(),
        buy_institutional_value: r_dec(col(r, 7)).unwrap_or_default(),
        sell_individual_value: r_dec(col(r, 8)).unwrap_or_default(),
        sell_institutional_value: r_dec(col(r, 9)).unwrap_or_default(),
        buy_individual_count: r_i32(col(r, 10)).unwrap_or_default(),
        buy_institutional_count: r_i32(col(r, 11)).unwrap_or_default(),
        sell_individual_count: r_i32(col(r, 12)).unwrap_or_default(),
        sell_institutional_count: r_i32(col(r, 13)).unwrap_or_default(),
        values_estimated: r_bool(col(r, 14)).unwrap_or_default(),
    }
}
