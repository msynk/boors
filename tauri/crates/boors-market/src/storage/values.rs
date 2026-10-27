//! Conversions between domain values and DuckDB values.

use std::str::FromStr;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};
use duckdb::types::{TimeUnit, Value, ValueRef};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};

use crate::raw_cache::format_fetched_at;
use crate::{Date, Decimal};

const UNIX_EPOCH_DAYS_FROM_CE: i32 = 719_163;

pub fn v_date(date: Date) -> Value {
    Value::Date32(date.num_days_from_ce() - UNIX_EPOCH_DAYS_FROM_CE)
}

pub fn v_opt_date(date: Option<Date>) -> Value {
    date.map_or(Value::Null, v_date)
}

/// A value for a `DECIMAL(w, 4)` column. Digits beyond the column scale are truncated, as DuckDB.NET's appender does
/// (DuckDB's own cast would round), so stores built by either implementation hold identical numbers.
pub fn v_dec(value: Decimal) -> Value {
    v_dec_scale(value, 4)
}

/// A value for a `DECIMAL(w, 0)` column (share counts).
pub fn v_dec0(value: Decimal) -> Value {
    v_dec_scale(value, 0)
}

/// A value for a `DECIMAL(w, scale)` column, truncated to the scale. Integral values go in as BIGINT: DuckDB's
/// appender casts them into the DECIMAL column on its fast path, without allocating a boxed value per cell.
pub fn v_dec_scale(value: Decimal, scale: u32) -> Value {
    let value = if value.scale() > scale { value.trunc_with_scale(scale) } else { value };
    if value.fract().is_zero()
        && let Some(integral) = value.to_i64()
    {
        return Value::BigInt(integral);
    }
    Value::Decimal(duckdb::types::Decimal::from(value))
}

pub fn v_opt_dec0(value: Option<Decimal>) -> Value {
    value.map_or(Value::Null, v_dec0)
}

fn from_duck(value: duckdb::types::Decimal) -> Option<Decimal> {
    Decimal::try_from(value).ok().or_else(|| {
        // Wider than 28 digits: drop fractional digits until it fits.
        let mut mantissa = value.value();
        let mut scale = u32::from(value.scale());
        while scale > 0 {
            mantissa /= 10;
            scale -= 1;
            if let Ok(d) = Decimal::try_from_i128_with_scale(mantissa, scale) {
                return Some(d);
            }
        }
        None
    })
}

pub fn v_opt_dec(value: Option<Decimal>) -> Value {
    value.map_or(Value::Null, v_dec)
}

pub fn v_i64(value: i64) -> Value {
    Value::BigInt(value)
}

pub fn v_opt_i64(value: Option<i64>) -> Value {
    value.map_or(Value::Null, Value::BigInt)
}

pub fn v_i32(value: i32) -> Value {
    Value::Int(value)
}

pub fn v_opt_i32(value: Option<i32>) -> Value {
    value.map_or(Value::Null, Value::Int)
}

pub fn v_str(value: &str) -> Value {
    Value::Text(value.to_string())
}

pub fn v_opt_str(value: Option<&str>) -> Value {
    value.map_or(Value::Null, v_str)
}

pub fn v_bool(value: bool) -> Value {
    Value::Boolean(value)
}

pub fn v_f64(value: f64) -> Value {
    Value::Double(value)
}

/// TIMESTAMPTZ values are bound as ISO text with an explicit offset so no session time zone is involved.
pub fn v_ts<Tz: TimeZone>(value: &DateTime<Tz>) -> Value {
    Value::Text(format_fetched_at(value.with_timezone(&Utc)).trim_matches('"').to_string())
}

pub fn v_opt_ts<Tz: TimeZone>(value: Option<&DateTime<Tz>>) -> Value {
    value.map_or(Value::Null, |v| v_ts(v))
}

// ------------------------------------------------------------------------------------------------ reading

pub fn r_i64(value: ValueRef<'_>) -> Option<i64> {
    match value {
        ValueRef::TinyInt(v) => Some(i64::from(v)),
        ValueRef::SmallInt(v) => Some(i64::from(v)),
        ValueRef::Int(v) => Some(i64::from(v)),
        ValueRef::BigInt(v) => Some(v),
        ValueRef::HugeInt(v) => i64::try_from(v).ok(),
        ValueRef::UTinyInt(v) => Some(i64::from(v)),
        ValueRef::USmallInt(v) => Some(i64::from(v)),
        ValueRef::UInt(v) => Some(i64::from(v)),
        ValueRef::UBigInt(v) => i64::try_from(v).ok(),
        ValueRef::Decimal(d) => from_duck(d).and_then(|d| d.trunc().to_i64()),
        ValueRef::Double(v) => Some(v as i64),
        ValueRef::Float(v) => Some(v as i64),
        ValueRef::Text(t) => std::str::from_utf8(t).ok()?.trim().parse().ok(),
        _ => None,
    }
}

pub fn r_i32(value: ValueRef<'_>) -> Option<i32> {
    r_i64(value).and_then(|v| i32::try_from(v).ok())
}

pub fn r_dec(value: ValueRef<'_>) -> Option<Decimal> {
    match value {
        ValueRef::Decimal(d) => from_duck(d),
        ValueRef::TinyInt(v) => Some(Decimal::from(v)),
        ValueRef::SmallInt(v) => Some(Decimal::from(v)),
        ValueRef::Int(v) => Some(Decimal::from(v)),
        ValueRef::BigInt(v) => Some(Decimal::from(v)),
        ValueRef::HugeInt(v) => Decimal::from_i128(v),
        ValueRef::UTinyInt(v) => Some(Decimal::from(v)),
        ValueRef::USmallInt(v) => Some(Decimal::from(v)),
        ValueRef::UInt(v) => Some(Decimal::from(v)),
        ValueRef::UBigInt(v) => Some(Decimal::from(v)),
        ValueRef::Double(v) => Decimal::from_f64(v),
        ValueRef::Float(v) => Decimal::from_f32(v),
        ValueRef::Text(t) => Decimal::from_str(std::str::from_utf8(t).ok()?.trim()).ok(),
        _ => None,
    }
}

pub fn r_f64(value: ValueRef<'_>) -> Option<f64> {
    match value {
        ValueRef::Double(v) => Some(v),
        ValueRef::Float(v) => Some(f64::from(v)),
        ValueRef::Decimal(d) => from_duck(d).and_then(|d| d.to_f64()),
        ValueRef::HugeInt(v) => Some(v as f64),
        ValueRef::UBigInt(v) => Some(v as f64),
        other => r_i64(other).map(|v| v as f64),
    }
}

pub fn r_str(value: ValueRef<'_>) -> Option<String> {
    match value {
        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Null => None,
        _ => None,
    }
}

pub fn r_bool(value: ValueRef<'_>) -> Option<bool> {
    match value {
        ValueRef::Boolean(b) => Some(b),
        ValueRef::Null => None,
        other => r_i64(other).map(|v| v != 0),
    }
}

pub fn r_date(value: ValueRef<'_>) -> Option<Date> {
    match value {
        ValueRef::Date32(days) => NaiveDate::from_num_days_from_ce_opt(days + UNIX_EPOCH_DAYS_FROM_CE),
        ValueRef::Timestamp(unit, v) => r_ts(ValueRef::Timestamp(unit, v)).map(|t| t.date_naive()),
        ValueRef::Text(t) => NaiveDate::parse_from_str(std::str::from_utf8(t).ok()?, "%Y-%m-%d").ok(),
        _ => None,
    }
}

pub fn r_ts(value: ValueRef<'_>) -> Option<DateTime<Utc>> {
    match value {
        ValueRef::Timestamp(unit, v) => {
            let micros = match unit {
                TimeUnit::Second => v.checked_mul(1_000_000)?,
                TimeUnit::Millisecond => v.checked_mul(1_000)?,
                TimeUnit::Microsecond => v,
                TimeUnit::Nanosecond => v / 1_000,
            };
            DateTime::<Utc>::from_timestamp_micros(micros)
        }
        ValueRef::Text(t) => DateTime::parse_from_rfc3339(std::str::from_utf8(t).ok()?).ok().map(|d| d.with_timezone(&Utc)),
        _ => None,
    }
}

pub fn r_time(value: ValueRef<'_>) -> Option<NaiveTime> {
    match value {
        ValueRef::Time64(unit, v) => {
            let micros = match unit {
                TimeUnit::Second => v * 1_000_000,
                TimeUnit::Millisecond => v * 1_000,
                TimeUnit::Microsecond => v,
                TimeUnit::Nanosecond => v / 1_000,
            };
            NaiveTime::from_num_seconds_from_midnight_opt((micros / 1_000_000) as u32, ((micros % 1_000_000) * 1_000) as u32)
        }
        _ => None,
    }
}

/// A generic cell for ad-hoc queries (`boors query`), formatted like the .NET CLI's `Output.Format`.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Null,
    Bool(bool),
    Int(i64),
    Huge(i128),
    Double(f64),
    Decimal(Decimal),
    Text(String),
    Date(Date),
    Timestamp(DateTime<Utc>),
    Time(NaiveTime),
    Other(String),
}

impl Cell {
    pub fn from_ref(value: ValueRef<'_>) -> Cell {
        match value {
            ValueRef::Null => Cell::Null,
            ValueRef::Boolean(b) => Cell::Bool(b),
            ValueRef::HugeInt(v) => Cell::Huge(v),
            ValueRef::UBigInt(v) => Cell::Huge(i128::from(v)),
            ValueRef::Double(v) => Cell::Double(v),
            ValueRef::Float(v) => Cell::Double(f64::from(v)),
            ValueRef::Decimal(d) => from_duck(d).map_or(Cell::Other(d.to_string()), Cell::Decimal),
            ValueRef::Text(t) => Cell::Text(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Date32(_) => r_date(value).map_or(Cell::Null, Cell::Date),
            ValueRef::Timestamp(..) => r_ts(value).map_or(Cell::Null, Cell::Timestamp),
            ValueRef::Time64(..) => r_time(value).map_or(Cell::Null, Cell::Time),
            other => match r_i64(other) {
                Some(v) => Cell::Int(v),
                None => Cell::Other(format!("{:?}", other.to_owned())),
            },
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Cell::Int(v) => Some(*v),
            Cell::Huge(v) => i64::try_from(*v).ok(),
            Cell::Decimal(d) => d.to_i64(),
            _ => None,
        }
    }

    pub fn as_date(&self) -> Option<Date> {
        match self {
            Cell::Date(d) => Some(*d),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Cell::Text(s) => Some(s),
            _ => None,
        }
    }
}

/// Days since 1970-01-01 → date (exposed for tests).
pub fn date_from_epoch_days(days: i32) -> Option<Date> {
    NaiveDate::from_num_days_from_ce_opt(days + UNIX_EPOCH_DAYS_FROM_CE)
}

/// `Utc.timestamp_opt` helper used by callers that need a fixed origin.
pub fn epoch() -> DateTime<Utc> {
    Utc.timestamp_opt(0, 0).single().unwrap_or_default()
}
