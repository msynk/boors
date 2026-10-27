//! Dated market rules loaded from `rules/rules.yaml` (C# `MarketRules`).
//!
//! Blocks apply in date order; for a given instrument class, the latest block (on or before the date) that names
//! the class or sets `default` wins, and within one block an explicit class entry beats `default`.
//!
//! YAML is read with `serde_yaml_ng`, the maintained fork of the archived `serde_yaml` (same serde data model,
//! actively released, and `deny_unknown_fields` gives YamlDotNet's "misspelled keys are an error" behaviour).

use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use chrono::NaiveTime;
use serde::Deserialize;

use crate::domain::InstrumentClass;
use crate::{Date, Decimal, Error, Result, Time};

/// The rules shipped with the library (a copy of the repository's `rules/rules.yaml`).
pub const DEFAULT_RULES_YAML: &str = include_str!("../../../rules/rules.yaml");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseMethod {
    /// Pre-2025-12-22 main-market rule.
    BaseVolumeWeighted,
    /// Base volume = 1 share, so the close is the day's VWAP.
    Vwap,
    /// IFB base market (from Mehr 1404).
    DynamicBaseVolume,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RuleConfidence {
    Low,
    Medium,
    High,
}

/// A resolved rule value plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleValue<T> {
    pub value: T,
    pub effective_from: Date,
    pub confidence: RuleConfidence,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TradingSession {
    pub continuous_start: Time,
    pub continuous_end: Time,
    pub tal_start: Option<Time>,
    pub tal_end: Option<Time>,
}

impl TradingSession {
    pub fn close(&self) -> Time {
        self.tal_end.unwrap_or(self.continuous_end)
    }
}

/// Per-side trading costs as fractions of trade value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradingCosts {
    pub exchange: Decimal,
    pub csd: Decimal,
    pub technology: Decimal,
    pub brokerage_max: Decimal,
    pub sale_tax: Decimal,
}

impl TradingCosts {
    pub fn buy_cost(&self, brokerage: Decimal) -> Decimal {
        self.exchange + self.csd + self.technology + brokerage.clamp(Decimal::ZERO, self.brokerage_max)
    }

    pub fn sell_cost(&self, brokerage: Decimal) -> Decimal {
        self.buy_cost(brokerage) + self.sale_tax
    }

    pub fn round_trip(&self, brokerage: Decimal) -> Decimal {
        self.buy_cost(brokerage) + self.sell_cost(brokerage)
    }

    /// Round trip at the full (list) brokerage rate.
    pub fn list_round_trip(&self) -> Decimal {
        self.round_trip(self.brokerage_max)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClosureRange {
    pub from: Date,
    pub to: Date,
    pub reason: String,
}

impl ClosureRange {
    pub fn contains(&self, date: Date) -> bool {
        date >= self.from && date <= self.to
    }
}

#[derive(Debug, Clone)]
struct RuleBlock {
    from: Date,
    confidence: RuleConfidence,
    note: Option<String>,
    price_limit: Option<HashMap<String, Decimal>>,
    close_method: Option<HashMap<String, CloseMethod>>,
    session: Option<HashMap<String, TradingSession>>,
    costs: Option<TradingCosts>,
}

#[derive(Debug, Clone)]
pub struct MarketRules {
    blocks: Vec<RuleBlock>,
    settlement_days: i32,
    known_closures: Vec<ClosureRange>,
}

const DEFAULT_KEY: &str = "default";

impl MarketRules {
    pub fn load_default() -> Result<Self> {
        Self::parse(DEFAULT_RULES_YAML)
    }

    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        Self::parse(&std::fs::read_to_string(path)?)
    }

    /// The rules from `path`, or the embedded copy when `path` is `None` or blank.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        match path.filter(|p| !p.as_os_str().is_empty()) {
            Some(path) => Self::load_file(path),
            None => Self::load_default(),
        }
    }

    pub fn parse(yaml: &str) -> Result<Self> {
        let document: RulesDocument = serde_yaml_ng::from_str(yaml).map_err(|e| Error::Format(format!("Invalid rules.yaml: {e}")))?;

        let mut blocks = document.blocks.into_iter().map(BlockDocument::into_block).collect::<Result<Vec<_>>>()?;
        blocks.sort_by_key(|b| b.from);
        if blocks.is_empty() {
            return Err(Error::Format("rules.yaml defines no blocks.".into()));
        }

        let known_closures = document
            .known_closures
            .into_iter()
            .map(|c| Ok(ClosureRange { from: parse_date(c.from.as_deref())?, to: parse_date(c.to.as_deref())?, reason: c.reason.unwrap_or_default() }))
            .collect::<Result<Vec<_>>>()?;

        let _ = (document.version, document.reopen);
        Ok(Self { blocks, settlement_days: document.settlement_days, known_closures })
    }

    pub fn settlement_days(&self) -> i32 {
        self.settlement_days
    }

    pub fn known_closures(&self) -> &[ClosureRange] {
        &self.known_closures
    }

    pub fn is_known_closure(&self, date: Date) -> bool {
        self.known_closures.iter().any(|c| c.contains(date))
    }

    pub fn get_price_limit(&self, class: InstrumentClass, date: Date) -> Result<RuleValue<Decimal>> {
        self.resolve(|b| b.price_limit.as_ref(), class, date)
            .ok_or_else(|| Error::InvalidOperation(format!("No price limit defined for {} on {date}.", class.name())))
    }

    /// The price limit, or 0 when the rules define none (the classifier then flags every move as beyond the band).
    pub fn price_limit_or_zero(&self, class: InstrumentClass, date: Date) -> Decimal {
        self.get_price_limit(class, date).map(|v| v.value).unwrap_or_default()
    }

    pub fn get_close_method(&self, class: InstrumentClass, date: Date) -> Result<RuleValue<CloseMethod>> {
        self.resolve(|b| b.close_method.as_ref(), class, date)
            .ok_or_else(|| Error::InvalidOperation(format!("No close method defined for {} on {date}.", class.name())))
    }

    pub fn get_session(&self, class: InstrumentClass, date: Date) -> Result<RuleValue<TradingSession>> {
        self.resolve(|b| b.session.as_ref(), class, date).ok_or_else(|| Error::InvalidOperation(format!("No session defined for {} on {date}.", class.name())))
    }

    pub fn get_costs(&self, date: Date) -> Result<RuleValue<TradingCosts>> {
        let mut result = None;
        for block in &self.blocks {
            if block.from > date {
                break;
            }
            if let Some(costs) = block.costs {
                result = Some(RuleValue { value: costs, effective_from: block.from, confidence: block.confidence, note: block.note.clone() });
            }
        }
        result.ok_or_else(|| Error::InvalidOperation(format!("No costs defined on {date}.")))
    }

    fn resolve<T: Clone>(&self, selector: impl Fn(&RuleBlock) -> Option<&HashMap<String, T>>, class: InstrumentClass, date: Date) -> Option<RuleValue<T>> {
        let key = class.rule_key();
        let mut result = None;
        for block in &self.blocks {
            if block.from > date {
                break;
            }
            let Some(map) = selector(block) else { continue };
            let value = map.get(key).or_else(|| map.get(DEFAULT_KEY));
            if let Some(value) = value {
                result = Some(RuleValue { value: value.clone(), effective_from: block.from, confidence: block.confidence, note: block.note.clone() });
            }
        }
        result
    }
}

fn parse_date(value: Option<&str>) -> Result<Date> {
    value
        .filter(|v| v.len() == 10)
        .and_then(|v| Date::parse_from_str(v, "%Y-%m-%d").ok())
        .ok_or_else(|| Error::Format(format!("Invalid date '{}' in rules.yaml (expected yyyy-MM-dd).", value.unwrap_or_default())))
}

fn parse_time(value: &str) -> Result<Time> {
    let value = value.trim();
    if value.len() != 5 {
        return Err(Error::Format(format!("Invalid time '{value}' in rules.yaml (expected HH:mm).")));
    }
    NaiveTime::parse_from_str(value, "%H:%M").map_err(|_| Error::Format(format!("Invalid time '{value}' in rules.yaml (expected HH:mm).")))
}

fn parse_range(value: &str) -> Result<(Time, Time)> {
    let Some((start, end)) = value.split_once('-') else {
        return Err(Error::Format(format!("Invalid time range '{value}' in rules.yaml (expected HH:mm-HH:mm).")));
    };
    Ok((parse_time(start)?, parse_time(end)?))
}

// ------------------------------------------------------------------------------------------------ YAML model

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesDocument {
    #[serde(default)]
    version: i32,
    #[serde(default)]
    blocks: Vec<BlockDocument>,
    #[serde(default = "default_settlement_days")]
    settlement_days: i32,
    #[serde(default)]
    reopen: Option<ReopenDocument>,
    #[serde(default)]
    known_closures: Vec<ClosureDocument>,
}

fn default_settlement_days() -> i32 {
    2
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockDocument {
    #[serde(default, deserialize_with = "de_opt_string")]
    from: Option<String>,
    #[serde(default)]
    confidence: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    price_limit: Option<HashMap<String, YamlDecimal>>,
    #[serde(default)]
    close_method: Option<HashMap<String, String>>,
    #[serde(default)]
    session: Option<HashMap<String, SessionDocument>>,
    #[serde(default)]
    costs: Option<CostsDocument>,
}

impl BlockDocument {
    fn into_block(self) -> Result<RuleBlock> {
        let confidence = match self.confidence.as_deref().map(str::to_lowercase).as_deref() {
            Some("high") => RuleConfidence::High,
            Some("medium") => RuleConfidence::Medium,
            Some("low") | None => RuleConfidence::Low,
            Some(other) => return Err(Error::Format(format!("Unknown confidence '{other}' in rules.yaml."))),
        };

        let close_method = match self.close_method {
            Some(map) => Some(
                map.into_iter()
                    .map(|(k, v)| {
                        let method = match v.as_str() {
                            "base_volume_weighted" => CloseMethod::BaseVolumeWeighted,
                            "vwap" => CloseMethod::Vwap,
                            "dynamic_base_volume" => CloseMethod::DynamicBaseVolume,
                            _ => return Err(Error::Format(format!("Unknown close_method '{v}' in rules.yaml."))),
                        };
                        Ok((k, method))
                    })
                    .collect::<Result<HashMap<_, _>>>()?,
            ),
            None => None,
        };

        let session = match self.session {
            Some(map) => Some(map.into_iter().map(|(k, v)| Ok((k, v.into_session()?))).collect::<Result<HashMap<_, _>>>()?),
            None => None,
        };

        Ok(RuleBlock {
            from: parse_date(self.from.as_deref())?,
            confidence,
            note: self.note.map(|n| n.trim().to_string()),
            price_limit: self.price_limit.map(|m| m.into_iter().map(|(k, v)| (k, v.0)).collect()),
            close_method,
            session,
            costs: self.costs.map(|c| TradingCosts {
                exchange: c.exchange.0,
                csd: c.csd.0,
                technology: c.technology.0,
                brokerage_max: c.brokerage_max.0,
                sale_tax: c.sale_tax.0,
            }),
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SessionDocument {
    #[serde(default)]
    continuous: String,
    #[serde(default)]
    tal: Option<String>,
}

impl SessionDocument {
    fn into_session(self) -> Result<TradingSession> {
        let (start, end) = parse_range(&self.continuous)?;
        let (tal_start, tal_end) = match self.tal {
            Some(tal) => {
                let (s, e) = parse_range(&tal)?;
                (Some(s), Some(e))
            }
            None => (None, None),
        };
        Ok(TradingSession { continuous_start: start, continuous_end: end, tal_start, tal_end })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CostsDocument {
    #[serde(default)]
    exchange: YamlDecimal,
    #[serde(default)]
    csd: YamlDecimal,
    #[serde(default)]
    technology: YamlDecimal,
    #[serde(default)]
    brokerage_max: YamlDecimal,
    #[serde(default)]
    sale_tax: YamlDecimal,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ReopenDocument {
    #[serde(default)]
    method: Option<String>,
    #[serde(default)]
    disclosure_halt_max_business_days: i32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ClosureDocument {
    #[serde(default, deserialize_with = "de_opt_string")]
    from: Option<String>,
    #[serde(default, deserialize_with = "de_opt_string")]
    to: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

/// A YAML scalar read as a decimal, through its shortest text form so `0.05` stays exactly 0.05.
#[derive(Default, Clone, Copy)]
struct YamlDecimal(Decimal);

impl<'de> Deserialize<'de> for YamlDecimal {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl serde::de::Visitor<'_> for Visitor {
            type Value = YamlDecimal;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a decimal number")
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(YamlDecimal(Decimal::from(v)))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(YamlDecimal(Decimal::from(v)))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                Decimal::from_str(&v.to_string()).map(YamlDecimal).map_err(E::custom)
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> std::result::Result<Self::Value, E> {
                Decimal::from_str(v.trim()).map(YamlDecimal).map_err(E::custom)
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

/// Accepts any scalar (a date such as `2025-01-01` may come through as a string or a tagged value) as text.
fn de_opt_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> std::result::Result<Option<String>, D::Error> {
    let value = Option::<serde_yaml_ng::Value>::deserialize(deserializer)?;
    Ok(match value {
        None | Some(serde_yaml_ng::Value::Null) => None,
        Some(serde_yaml_ng::Value::String(s)) => Some(s),
        Some(serde_yaml_ng::Value::Number(n)) => Some(n.to_string()),
        Some(serde_yaml_ng::Value::Bool(b)) => Some(b.to_string()),
        Some(other) => Some(serde_yaml_ng::to_string(&other).unwrap_or_default().trim().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(s: &str) -> Date {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn rules() -> MarketRules {
        MarketRules::load_default().unwrap()
    }

    #[test]
    fn price_limit_resolves_by_class_and_date() {
        let rules = rules();
        let cases = [
            (InstrumentClass::TseStock, "2026-10-17", "0.03"),
            (InstrumentClass::TseStock, "2010-01-01", "0.05"),
            (InstrumentClass::FundLeveraged, "2026-10-17", "0.04"),
            (InstrumentClass::FundGoldSilver, "2026-10-17", "0.10"),
            (InstrumentClass::IfbBaseRed, "2026-10-17", "0.01"),
            (InstrumentClass::Option, "2026-10-17", "0.03"),
        ];
        for (class, date, expected) in cases {
            assert_eq!(Decimal::from_str(expected).unwrap(), rules.get_price_limit(class, d(date)).unwrap().value, "{class:?} {date}");
        }
    }

    #[test]
    fn placeholder_era_is_flagged_low_confidence() {
        assert_eq!(RuleConfidence::Low, rules().get_price_limit(InstrumentClass::TseStock, d("2010-01-01")).unwrap().confidence);
    }

    #[test]
    fn close_method_switches_to_vwap_on_main_markets_only() {
        let rules = rules();
        assert_eq!(CloseMethod::BaseVolumeWeighted, rules.get_close_method(InstrumentClass::TseStock, d("2025-12-21")).unwrap().value);
        assert_eq!(CloseMethod::Vwap, rules.get_close_method(InstrumentClass::TseStock, d("2025-12-22")).unwrap().value);
        assert_eq!(CloseMethod::BaseVolumeWeighted, rules.get_close_method(InstrumentClass::IfbBaseYellow, d("2025-09-22")).unwrap().value);
        assert_eq!(CloseMethod::DynamicBaseVolume, rules.get_close_method(InstrumentClass::IfbBaseYellow, d("2025-09-23")).unwrap().value);
        assert_eq!(CloseMethod::DynamicBaseVolume, rules.get_close_method(InstrumentClass::IfbBaseYellow, d("2026-10-17")).unwrap().value);
    }

    #[test]
    fn sessions_add_tal_in_june_2026_and_extend_non_equity_etfs_in_mehr_1405() {
        let rules = rules();
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        let before_tal = rules.get_session(InstrumentClass::TseStock, d("2026-06-05")).unwrap().value;
        let with_tal = rules.get_session(InstrumentClass::TseStock, d("2026-10-17")).unwrap().value;
        let etf_before = rules.get_session(InstrumentClass::EtfFixedIncome, d("2026-10-09")).unwrap().value;
        let etf_after = rules.get_session(InstrumentClass::EtfFixedIncome, d("2026-10-10")).unwrap().value;
        assert_eq!(None, before_tal.tal_start);
        assert_eq!(t(12, 30), before_tal.close());
        assert_eq!(Some(t(12, 45)), with_tal.tal_start);
        assert_eq!(t(13, 0), with_tal.close());
        assert_eq!(t(13, 0), etf_before.close());
        assert_eq!(t(18, 0), etf_after.close());
    }

    #[test]
    fn costs_give_list_and_discounted_round_trips() {
        let costs = rules().get_costs(d("2026-10-17")).unwrap().value;
        assert_eq!(Decimal::from_str("0.01428").unwrap(), costs.list_round_trip());
        assert_eq!(Decimal::from_str("0.0125").unwrap(), costs.round_trip(Decimal::from_str("0.00055").unwrap()));
        let b = Decimal::from_str("0.001").unwrap();
        assert_eq!(costs.buy_cost(b) + Decimal::from_str("0.005").unwrap(), costs.sell_cost(b));
    }

    #[test]
    fn known_closures_cover_both_wars() {
        let rules = rules();
        assert_eq!(2, rules.known_closures().len());
        assert!(rules.is_known_closure(d("2026-04-01")));
        assert!(rules.is_known_closure(d("2025-06-20")));
        assert!(!rules.is_known_closure(d("2026-05-19")));
        assert_eq!(2, rules.settlement_days());
    }

    #[test]
    fn a_later_default_overrides_an_earlier_class_entry() {
        let rules = MarketRules::parse(
            "blocks:\n  - from: 2020-01-01\n    price_limit: { default: 0.05, tse_stock: 0.04 }\n  - from: 2021-01-01\n    price_limit: { default: 0.02 }\n",
        )
        .unwrap();
        assert_eq!(Decimal::from_str("0.04").unwrap(), rules.get_price_limit(InstrumentClass::TseStock, d("2020-06-01")).unwrap().value);
        assert_eq!(Decimal::from_str("0.02").unwrap(), rules.get_price_limit(InstrumentClass::TseStock, d("2021-06-01")).unwrap().value);
    }

    #[test]
    fn unknown_close_method_is_rejected() {
        assert!(MarketRules::parse("blocks:\n  - from: 2020-01-01\n    close_method: { default: magic }\n").is_err());
    }

    #[test]
    fn misspelled_keys_are_rejected() {
        assert!(MarketRules::parse("blocks:\n  - from: 2020-01-01\n    price_limits: { default: 0.05 }\n").is_err());
    }
}
