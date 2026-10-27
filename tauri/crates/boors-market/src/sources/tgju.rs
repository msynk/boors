//! tgju.org macro series (C# `TgjuClient`). tgju has no official API; these are the JSON endpoints behind its pages.

use std::sync::LazyLock;

use regex::Regex;

use crate::raw_cache::RawRequest;
use crate::sources::json::parse_decimal_float;
use crate::sources::{CachePolicy, SourceClient, escape_data_string};
use crate::{Date, Decimal, Error, Result};

pub const SOURCE: &str = "tgju";

/// tgju item codes used by Boors.
pub mod series {
    pub const USD_FREE_MARKET: &str = "price_dollar_rl";
    pub const EMAMI_COIN: &str = "sekee";
    pub const GOLD_18K: &str = "geram18";
    pub const GOLD_OUNCE_USD: &str = "ons";
    pub const EUR: &str = "price_eur";
    pub const DEFAULT: [&str; 5] = [USD_FREE_MARKET, EMAMI_COIN, GOLD_18K, GOLD_OUNCE_USD, EUR];
}

/// A daily OHLC bar of a macro series. Rial unless noted.
#[derive(Debug, Clone, PartialEq)]
pub struct MacroBar {
    pub series: String,
    pub date: Date,
    pub open: Decimal,
    pub low: Decimal,
    pub high: Decimal,
    pub close: Decimal,
    pub source: String,
}

pub fn history_request(series: &str) -> RawRequest {
    RawRequest::new(
        SOURCE,
        "summary-table-data",
        format!("https://api.tgju.org/v1/market/indicator/summary-table-data/{}", escape_data_string(series)),
        format!("series={series}"),
    )
}

#[derive(Debug, Clone)]
pub struct TgjuClient {
    pub client: SourceClient,
}

impl TgjuClient {
    pub fn new(client: SourceClient) -> Self {
        Self { client }
    }

    pub async fn get_history(&self, series: &str, policy: CachePolicy) -> Result<Vec<MacroBar>> {
        parse_history(&self.client.get_body(&history_request(series), policy).await?, series)
    }
}

static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new("<[^>]+>").expect("valid regex"));

/// Parses summary-table-data. Each row is [open, low, high, close, change (HTML), change % (HTML), "yyyy/MM/dd",
/// jalali date].
pub fn parse_history(json: &[u8], series: &str) -> Result<Vec<MacroBar>> {
    let root: serde_json::Value = serde_json::from_slice(json)?;
    let rows = root.get("data").and_then(|d| d.as_array()).ok_or_else(|| Error::format("tgju: 'data' is missing."))?;
    let mut bars = Vec::new();
    for row in rows {
        let Some(row) = row.as_array() else { continue };
        if row.len() < 7 {
            continue;
        }
        let Some(date) = row[6].as_str().filter(|s| s.len() == 10).and_then(|s| Date::parse_from_str(s, "%Y/%m/%d").ok()) else {
            continue;
        };
        let (Some(open), Some(low), Some(high), Some(close)) = (number(&row[0]), number(&row[1]), number(&row[2]), number(&row[3])) else {
            continue;
        };
        bars.push(MacroBar { series: series.to_string(), date, open, low, high, close, source: SOURCE.to_string() });
    }
    bars.sort_by_key(|b| b.date);
    Ok(bars)
}

fn number(element: &serde_json::Value) -> Option<Decimal> {
    let text = match element {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    if crate::text::is_blank(&text) {
        return None;
    }
    let text = HTML_TAG.replace_all(&text, "").replace(',', "");
    parse_decimal_float(text.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_and_thousands_separators() {
        assert_eq!(Some(Decimal::from(40000)), number(&serde_json::json!("<span class=\"high\" dir=\"ltr\">40000</span>")));
        assert_eq!(Some(Decimal::from(2_674_050)), number(&serde_json::json!("2,674,050")));
        assert_eq!(None, number(&serde_json::json!("")));
    }
}
