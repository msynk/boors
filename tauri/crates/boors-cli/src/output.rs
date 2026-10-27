//! Console and CSV output with the same formatting as the .NET CLI (`Output.cs` and .NET format strings), so the
//! two implementations can be compared line by line.

use std::io::Write;
use std::path::Path;

use boors_market::storage::Cell;
use boors_market::{Date, Decimal};
use chrono::{DateTime, NaiveTime, Utc};
use rust_decimal::RoundingStrategy;
use rust_decimal::prelude::FromPrimitive;

/// A value printed in a table or CSV (C# `object?`).
#[derive(Debug, Clone)]
pub enum V {
    Null,
    Str(String),
    Int(i64),
    Dec(Decimal),
    Dbl(f64),
    Date(Date),
    Time(NaiveTime),
    Ts(DateTime<Utc>),
}

impl From<&str> for V {
    fn from(v: &str) -> Self {
        V::Str(v.to_string())
    }
}

impl From<String> for V {
    fn from(v: String) -> Self {
        V::Str(v)
    }
}

impl From<Option<String>> for V {
    fn from(v: Option<String>) -> Self {
        v.map_or(V::Null, V::Str)
    }
}

impl From<Option<&str>> for V {
    fn from(v: Option<&str>) -> Self {
        v.map_or(V::Null, |s| V::Str(s.to_string()))
    }
}

impl From<i64> for V {
    fn from(v: i64) -> Self {
        V::Int(v)
    }
}

impl From<i32> for V {
    fn from(v: i32) -> Self {
        V::Int(i64::from(v))
    }
}

impl From<usize> for V {
    fn from(v: usize) -> Self {
        V::Int(v as i64)
    }
}

impl From<Option<i64>> for V {
    fn from(v: Option<i64>) -> Self {
        v.map_or(V::Null, V::Int)
    }
}

impl From<Option<i32>> for V {
    fn from(v: Option<i32>) -> Self {
        v.map_or(V::Null, |x| V::Int(i64::from(x)))
    }
}

impl From<Decimal> for V {
    fn from(v: Decimal) -> Self {
        V::Dec(v)
    }
}

impl From<Option<Decimal>> for V {
    fn from(v: Option<Decimal>) -> Self {
        v.map_or(V::Null, V::Dec)
    }
}

impl From<f64> for V {
    fn from(v: f64) -> Self {
        V::Dbl(v)
    }
}

impl From<Option<f64>> for V {
    fn from(v: Option<f64>) -> Self {
        v.map_or(V::Null, V::Dbl)
    }
}

impl From<Date> for V {
    fn from(v: Date) -> Self {
        V::Date(v)
    }
}

impl From<Option<Date>> for V {
    fn from(v: Option<Date>) -> Self {
        v.map_or(V::Null, V::Date)
    }
}

impl From<DateTime<Utc>> for V {
    fn from(v: DateTime<Utc>) -> Self {
        V::Ts(v)
    }
}

impl From<&Cell> for V {
    fn from(cell: &Cell) -> Self {
        match cell {
            Cell::Null => V::Null,
            Cell::Bool(b) => V::Str(if *b { "True".into() } else { "False".into() }),
            Cell::Int(i) => V::Int(*i),
            Cell::Huge(h) => V::Str(h.to_string()),
            Cell::Double(d) => V::Dbl(*d),
            Cell::Decimal(d) => V::Dec(*d),
            Cell::Text(t) => V::Str(t.clone()),
            Cell::Date(d) => V::Date(*d),
            Cell::Timestamp(t) => V::Ts(*t),
            Cell::Time(t) => V::Time(*t),
            Cell::Other(o) => V::Str(o.clone()),
        }
    }
}

/// C# `Output.Format`.
pub fn format(value: &V) -> String {
    match value {
        V::Null => String::new(),
        V::Str(s) => s.clone(),
        V::Int(i) => i.to_string(),
        V::Dec(d) => format_custom_0_4(*d),
        V::Dbl(f) => match Decimal::from_f64(*f) {
            Some(d) => format_custom_0_4(d),
            None => f.to_string(),
        },
        V::Date(d) => d.format("%Y-%m-%d").to_string(),
        V::Time(t) => t.format("%H:%M:%S").to_string(),
        V::Ts(t) => t.format("%Y-%m-%d %H:%M:%S+00:00").to_string(),
    }
}

/// `ToString("0.####")`: at most four decimals (half away from zero), trailing zeros dropped, no grouping.
fn format_custom_0_4(value: Decimal) -> String {
    let rounded = value.round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero).normalize();
    let text = rounded.to_string();
    if text == "-0" { "0".to_string() } else { text }
}

/// .NET string length (UTF-16 code units), which `PadRight` and `{x,-n}` use.
pub fn width(value: &str) -> usize {
    value.encode_utf16().count()
}

/// `{value,-n}` / `PadRight`.
pub fn pad_right(value: &str, n: usize) -> String {
    let w = width(value);
    if w >= n { value.to_string() } else { format!("{value}{}", " ".repeat(n - w)) }
}

/// `{value,n}`.
pub fn pad_left(value: &str, n: usize) -> String {
    let w = width(value);
    if w >= n { value.to_string() } else { format!("{}{value}", " ".repeat(n - w)) }
}

/// C# `Output.Table`.
pub fn table(header: &[&str], rows: Vec<Vec<V>>) {
    let data: Vec<Vec<String>> = rows.iter().map(|r| r.iter().map(format).collect()).collect();
    let widths: Vec<usize> =
        header.iter().enumerate().map(|(i, h)| width(h).max(data.iter().map(|r| r.get(i).map_or(0, |v| width(v))).max().unwrap_or(0))).collect();

    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", header.iter().enumerate().map(|(i, h)| pad_right(h, widths[i])).collect::<Vec<_>>().join("  "));
    let _ = writeln!(out, "{}", widths.iter().map(|w| "-".repeat(*w)).collect::<Vec<_>>().join("  "));
    for row in data {
        let line = row.iter().enumerate().map(|(i, v)| pad_right(v, widths.get(i).copied().unwrap_or(0))).collect::<Vec<_>>().join("  ");
        let _ = writeln!(out, "{line}");
    }
}

fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value.to_string() }
}

/// Writes a CSV with a UTF-8 BOM so spreadsheet tools show Persian text correctly.
pub fn write_csv(path: &Path, header: &[&str], rows: Vec<Vec<V>>) -> anyhow::Result<()> {
    if let Some(parent) = std::path::absolute(path)?.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
    file.write_all(b"\xEF\xBB\xBF")?;
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    write!(file, "{}{nl}", header.iter().map(|h| escape_csv(h)).collect::<Vec<_>>().join(","))?;
    for row in rows {
        write!(file, "{}{nl}", row.iter().map(|v| escape_csv(&format(v))).collect::<Vec<_>>().join(","))?;
    }
    file.flush()?;
    Ok(())
}

/// .NET "N{places}" for a decimal: grouping with ',' and rounding half away from zero.
pub fn n(value: Decimal, places: u32) -> String {
    let rounded = value.round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero);
    let negative = rounded.is_sign_negative() && !rounded.is_zero();
    let text = format!("{:.*}", places as usize, rounded.abs());
    let (integer, fraction) = match text.split_once('.') {
        Some((i, f)) => (i.to_string(), Some(f.to_string())),
        None => (text.clone(), None),
    };
    let mut grouped = String::new();
    for (i, c) in integer.chars().enumerate() {
        if i > 0 && (integer.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(c);
    }
    let mut out = if negative { format!("-{grouped}") } else { grouped };
    if let Some(f) = fraction {
        out.push('.');
        out.push_str(&f);
    }
    out
}

/// "N{places}" of a nullable decimal (empty when null, as string interpolation of a null prints nothing).
pub fn n_opt(value: Option<Decimal>, places: u32) -> String {
    value.map(|v| n(v, places)).unwrap_or_default()
}

/// C# `Math.Round(double, digits)` (scale, round half to even, unscale).
pub fn round_f64(value: f64, digits: i32) -> f64 {
    let power = 10f64.powi(digits);
    (value * power).round_ties_even() / power
}

/// `Round(double?)` of the market commands: `Math.Round(v, 2)`.
pub fn round2(value: Option<f64>) -> Option<f64> {
    value.map(|v| round_f64(v, 2))
}

/// `Pct(double?)`: a fraction as percent with two decimals.
pub fn pct(value: Option<f64>) -> Option<f64> {
    value.map(|v| round_f64(v * 100.0, 2))
}

/// .NET default `double.ToString()` (shortest round-trippable) of a nullable double, empty for null.
pub fn g(value: Option<f64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

/// C# `Math.Round(decimal, digits)`: banker's rounding.
pub fn round_dec(value: Decimal, digits: u32) -> Decimal {
    value.round_dp_with_strategy(digits, RoundingStrategy::MidpointNearestEven)
}

/// `value / divisor` as C# decimal division (`None` on division by zero).
pub fn div(value: Decimal, divisor: Decimal) -> Decimal {
    value.checked_div(divisor).unwrap_or_default()
}

/// `Short(string?, max)`: truncate to `max` UTF-16 units and add an ellipsis.
pub fn short(value: Option<&str>, max: usize) -> Option<String> {
    let value = value?;
    let units: Vec<u16> = value.encode_utf16().collect();
    if units.len() <= max { Some(value.to_string()) } else { Some(format!("{}…", String::from_utf16_lossy(&units[..max]))) }
}
