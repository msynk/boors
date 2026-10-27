//! Source contracts (C# `JsonShape` and `SourceContracts`): the structural shape of each endpoint's JSON, compared
//! with a committed baseline to detect format changes before parsers fail.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::context::MarketContext;
use crate::raw_cache::{RawRequest, ordinal_cmp};
use crate::sources::SourceClient;
use crate::sources::codal::CodalSearchQuery;
use crate::sources::tsetmc::{MarketWatchQuery, TEDPIX_INS_CODE};
use crate::sources::{fipiran, tgju};
use crate::{Error, Result};

/// A shape: JSON path → `|`-joined value kinds, ordered ordinally.
pub type Shape = BTreeMap<String, String>;

/// The baseline shipped with the library (a copy of the .NET `Contracts/baseline.json`).
pub const DEFAULT_BASELINE_JSON: &str = include_str!("../data/baseline.json");

/// Marks a baseline path that some responses omit.
pub const OPTIONAL_PREFIX: &str = "optional:";

const FOLAD: i64 = 46348559193224090;

/// Every path (arrays as `[]`) with the set of value kinds seen there, e.g. `$.closingPriceDaily[].pClosing: number`.
pub fn describe(json: &[u8]) -> Result<Shape> {
    let root: serde_json::Value = serde_json::from_slice(json)?;
    let mut kinds: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    walk(&root, "$".to_string(), &mut kinds);
    Ok(kinds.into_iter().map(|(k, v)| (k, join_ordinal(v.into_iter()))).collect())
}

fn join_ordinal(kinds: impl Iterator<Item = String>) -> String {
    let mut list: Vec<String> = kinds.collect();
    list.sort_by(|a, b| ordinal_cmp(a, b));
    list.dedup();
    list.join("|")
}

fn walk(element: &serde_json::Value, path: String, kinds: &mut BTreeMap<String, BTreeSet<String>>) {
    let kind = match element {
        serde_json::Value::Object(_) => "object",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Null => "null",
    };
    kinds.entry(path.clone()).or_default().insert(kind.to_string());
    match element {
        serde_json::Value::Object(map) => {
            for (name, value) in map {
                walk(value, format!("{path}.{name}"), kinds);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                walk(item, format!("{path}[]"), kinds);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContractDiff {
    pub contract: String,
    pub missing: Vec<String>,
    pub added: Vec<String>,
    pub kind_changed: Vec<String>,
}

impl ContractDiff {
    /// Missing fields or new kinds can break parsers; added fields cannot.
    pub fn is_breaking(&self) -> bool {
        !self.missing.is_empty() || !self.kind_changed.is_empty()
    }
}

fn kinds(value: &str) -> Vec<&str> {
    value.strip_prefix(OPTIONAL_PREFIX).unwrap_or(value).split('|').collect()
}

fn kinds_compatible(baseline: &str, current: &str) -> bool {
    let allowed = kinds(baseline);
    kinds(current).iter().all(|k| allowed.contains(k))
}

fn sorted(mut list: Vec<String>) -> Vec<String> {
    list.sort_by(|a, b| ordinal_cmp(a, b));
    list
}

pub fn compare(contract: &str, baseline: &Shape, current: &Shape) -> ContractDiff {
    let missing =
        sorted(baseline.iter().filter(|(_, v)| !v.starts_with(OPTIONAL_PREFIX)).map(|(k, _)| k.clone()).filter(|k| !current.contains_key(k)).collect());
    let added = sorted(current.keys().filter(|k| !baseline.contains_key(*k)).cloned().collect());
    let kind_changed =
        sorted(baseline.iter().filter_map(|(k, b)| current.get(k).filter(|c| !kinds_compatible(b, c)).map(|c| format!("{k}: {b} -> {c}"))).collect());
    ContractDiff { contract: contract.into(), missing, added, kind_changed }
}

/// Merges a new sample into a baseline: kinds are unioned, and a path missing from either side becomes optional.
pub fn merge(baseline: Option<&Shape>, sample: &Shape) -> Shape {
    let Some(baseline) = baseline else { return sample.clone() };
    let mut merged = Shape::new();
    let paths: BTreeSet<&String> = baseline.keys().chain(sample.keys()).collect();
    for path in paths {
        let before = baseline.get(path);
        let now = sample.get(path);
        let optional = before.is_none() || now.is_none() || before.is_some_and(|b| b.starts_with(OPTIONAL_PREFIX));
        let all = before.into_iter().chain(now).flat_map(|k| kinds(k)).map(str::to_string);
        merged.insert(path.clone(), format!("{}{}", if optional { OPTIONAL_PREFIX } else { "" }, join_ordinal(all)));
    }
    merged
}

/// Paths a source is known to send only sometimes (TSETMC's market watch alternates between two variants).
pub fn known_optional_paths(contract: &str) -> &'static [&'static str] {
    match contract {
        "tsetmc.market-watch" => &[
            "$.marketwatch[].buyOP",
            "$.marketwatch[].cGrValCot",
            "$.marketwatch[].flow",
            "$.marketwatch[].pRedTran",
            "$.marketwatch[].yMarNSC",
            "$.marketwatch[].yVal",
        ],
        _ => &[],
    }
}

/// Marks the known optional paths of a contract in a shape (adding them when absent).
pub fn with_known_optionals(contract: &str, mut shape: Shape) -> Shape {
    for path in known_optional_paths(contract) {
        let current = shape.get(*path).cloned().unwrap_or_else(|| "null".to_string());
        let value = if current.starts_with(OPTIONAL_PREFIX) { current } else { format!("{OPTIONAL_PREFIX}{current}") };
        shape.insert((*path).to_string(), value);
    }
    shape
}

/// The endpoints Boors depends on, each with a fixed sample request (فولاد, TEDPIX).
pub fn all(ctx: &MarketContext) -> Vec<(&'static str, SourceClient, RawRequest)> {
    let e = &ctx.tsetmc.endpoints;
    let t = ctx.tsetmc.client.clone();
    vec![
        ("tsetmc.instrument-info", t.clone(), e.instrument_info(FOLAD)),
        ("tsetmc.instrument-search", t.clone(), e.instrument_search("فولاد")),
        ("tsetmc.closing-price-info", t.clone(), e.closing_price_info(FOLAD)),
        ("tsetmc.closing-price-daily", t.clone(), e.closing_price_daily_list(FOLAD, 10)),
        ("tsetmc.client-type-history", t.clone(), e.client_type_history(FOLAD)),
        ("tsetmc.client-type-all", t.clone(), e.client_type_all()),
        ("tsetmc.price-adjust", t.clone(), e.price_adjust_list(FOLAD)),
        ("tsetmc.share-change", t.clone(), e.instrument_share_change(FOLAD)),
        ("tsetmc.best-limits", t.clone(), e.best_limits(FOLAD)),
        ("tsetmc.index-history", t.clone(), e.index_history(TEDPIX_INS_CODE)),
        ("tsetmc.index-snapshots", t.clone(), e.index_snapshots(1)),
        ("tsetmc.market-watch", t.clone(), e.market_watch(&MarketWatchQuery { paper_types: vec![1], ..MarketWatchQuery::default() })),
        ("tsetmc.messages-by-flow", t.clone(), e.messages_by_flow(1, 20)),
        ("tsetmc.shareholders", t.clone(), e.shareholders(FOLAD)),
        ("tsetmc.static-data", t, e.static_data()),
        ("codal.categories", ctx.codal.client.clone(), ctx.codal.categories_request()),
        ("codal.search", ctx.codal.client.clone(), ctx.codal.search_request(&CodalSearchQuery::default())),
        ("tgju.history", ctx.tgju.client.clone(), tgju::history_request(tgju::series::USD_FREE_MARKET)),
        ("fipiran.fund-compare", ctx.fipiran.client.clone(), fipiran::fund_compare_request()),
    ]
}

pub fn parse_baseline(json: &str) -> Result<BTreeMap<String, Shape>> {
    serde_json::from_str(json).map_err(|e| Error::Format(format!("Invalid contract baseline: {e}")))
}

/// The baseline at `path`, or the embedded copy when the file does not exist.
pub fn load_baseline(path: &Path) -> Result<BTreeMap<String, Shape>> {
    if path.is_file() { parse_baseline(&std::fs::read_to_string(path)?) } else { parse_baseline(DEFAULT_BASELINE_JSON) }
}

/// Writes the baseline sorted ordinally, indented, with unescaped Persian text and a trailing newline.
pub fn save_baseline(path: &Path, shapes: &BTreeMap<String, Shape>) -> Result<()> {
    if let Some(parent) = std::path::absolute(path)?.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut contracts: Vec<(&String, &Shape)> = shapes.iter().collect();
    contracts.sort_by(|a, b| ordinal_cmp(a.0, b.0));
    let mut out = String::from("{");
    for (i, (name, shape)) in contracts.iter().enumerate() {
        out.push_str(if i == 0 { "\n" } else { ",\n" });
        out.push_str(&format!("  {}: {{", serde_json::to_string(name)?));
        let mut paths: Vec<(&String, &String)> = shape.iter().collect();
        paths.sort_by(|a, b| ordinal_cmp(a.0, b.0));
        for (j, (path, kinds)) in paths.iter().enumerate() {
            out.push_str(if j == 0 { "\n" } else { ",\n" });
            out.push_str(&format!("    {}: {}", serde_json::to_string(path)?, serde_json::to_string(kinds)?));
        }
        out.push_str("\n  }");
    }
    out.push_str("\n}\n");
    std::fs::write(path, out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shape_lists_paths_with_value_kinds_across_array_items() {
        let shape = describe(br#"{"rows":[{"a":1,"b":"x"},{"a":null}]}"#).unwrap();
        assert_eq!("null|number", shape["$.rows[].a"]);
        assert_eq!("string", shape["$.rows[].b"]);
    }

    #[test]
    fn missing_fields_and_new_kinds_are_breaking_but_added_fields_are_not() {
        let baseline = describe(br#"{"a":1,"b":"x"}"#).unwrap();
        let added = compare("c", &baseline, &describe(br#"{"a":1,"b":"x","c":true}"#).unwrap());
        let missing = compare("c", &baseline, &describe(br#"{"a":1}"#).unwrap());
        let retyped = compare("c", &baseline, &describe(br#"{"a":"1","b":"x"}"#).unwrap());
        assert!(!added.is_breaking());
        assert_eq!(1, added.added.len());
        assert!(missing.is_breaking());
        assert!(retyped.is_breaking());
    }

    #[test]
    fn embedded_baseline_parses() {
        let baseline = parse_baseline(DEFAULT_BASELINE_JSON).unwrap();
        assert_eq!(19, baseline.len());
        assert!(baseline["tsetmc.market-watch"]["$.marketwatch[].yVal"].starts_with(OPTIONAL_PREFIX));
    }
}
