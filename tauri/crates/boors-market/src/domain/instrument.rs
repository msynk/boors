//! Instrument classes and the instrument master record.

use serde::{Deserialize, Serialize};

use crate::Decimal;

/// Instrument classes drive every class-dependent rule (price limit, close method, session).
/// Names map 1:1 to the snake_case keys in `rules/rules.yaml`. Variant order matches the C# enum (sorting uses it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum InstrumentClass {
    #[default]
    Other,
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
    Option,
    Debt,
}

impl InstrumentClass {
    pub const ALL: [InstrumentClass; 16] = [
        InstrumentClass::Other,
        InstrumentClass::TseStock,
        InstrumentClass::IfbStock,
        InstrumentClass::IfbBaseYellow,
        InstrumentClass::IfbBaseOrange,
        InstrumentClass::IfbBaseRed,
        InstrumentClass::Rights,
        InstrumentClass::EtfEquity,
        InstrumentClass::EtfFixedIncome,
        InstrumentClass::EtfRealEstate,
        InstrumentClass::EtfMixed,
        InstrumentClass::FundLeveraged,
        InstrumentClass::FundGoldSilver,
        InstrumentClass::FundOfFunds,
        InstrumentClass::Option,
        InstrumentClass::Debt,
    ];

    /// The snake_case key used for this class in rules.yaml and in the store.
    pub fn rule_key(self) -> &'static str {
        match self {
            InstrumentClass::TseStock => "tse_stock",
            InstrumentClass::IfbStock => "ifb_stock",
            InstrumentClass::IfbBaseYellow => "ifb_base_yellow",
            InstrumentClass::IfbBaseOrange => "ifb_base_orange",
            InstrumentClass::IfbBaseRed => "ifb_base_red",
            InstrumentClass::Rights => "rights",
            InstrumentClass::EtfEquity => "etf_equity",
            InstrumentClass::EtfFixedIncome => "etf_fixed_income",
            InstrumentClass::EtfRealEstate => "etf_real_estate",
            InstrumentClass::EtfMixed => "etf_mixed",
            InstrumentClass::FundLeveraged => "fund_leveraged",
            InstrumentClass::FundGoldSilver => "fund_gold_silver",
            InstrumentClass::FundOfFunds => "fund_of_funds",
            InstrumentClass::Option => "option",
            InstrumentClass::Debt => "debt",
            InstrumentClass::Other => "other",
        }
    }

    /// The C# enum member name (`TseStock`), used by the API contract.
    pub fn name(self) -> &'static str {
        match self {
            InstrumentClass::Other => "Other",
            InstrumentClass::TseStock => "TseStock",
            InstrumentClass::IfbStock => "IfbStock",
            InstrumentClass::IfbBaseYellow => "IfbBaseYellow",
            InstrumentClass::IfbBaseOrange => "IfbBaseOrange",
            InstrumentClass::IfbBaseRed => "IfbBaseRed",
            InstrumentClass::Rights => "Rights",
            InstrumentClass::EtfEquity => "EtfEquity",
            InstrumentClass::EtfFixedIncome => "EtfFixedIncome",
            InstrumentClass::EtfRealEstate => "EtfRealEstate",
            InstrumentClass::EtfMixed => "EtfMixed",
            InstrumentClass::FundLeveraged => "FundLeveraged",
            InstrumentClass::FundGoldSilver => "FundGoldSilver",
            InstrumentClass::FundOfFunds => "FundOfFunds",
            InstrumentClass::Option => "Option",
            InstrumentClass::Debt => "Debt",
        }
    }

    /// C# `InstrumentClassifier.FromRuleKey`: unknown keys give `Other`.
    pub fn from_rule_key(key: &str) -> InstrumentClass {
        InstrumentClass::ALL.into_iter().find(|c| c.rule_key() == key).unwrap_or_default()
    }

    /// Accepts a rule key (`tse_stock`) or an enum name (`TseStock`, case-insensitive).
    pub fn parse(value: &str) -> Option<InstrumentClass> {
        let value = value.trim();
        InstrumentClass::ALL.into_iter().find(|c| c.rule_key() == value || c.name().eq_ignore_ascii_case(value))
    }
}

/// An instrument in the master list. `ins_code` is the join key everywhere; symbol names never are.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Instrument {
    pub ins_code: i64,
    /// Short Persian ticker (lVal18AFC / lva), normalized.
    pub symbol: String,
    /// Persian name (lVal30 / lvc), normalized.
    pub name: Option<String>,
    pub latin_name: Option<String>,
    /// Instrument ISIN (instrumentID / insID), e.g. IRO1FOLD0001.
    pub isin: Option<String>,
    /// Company ISIN (cIsin).
    pub company_isin: Option<String>,
    /// TSETMC instrument type code (yVal).
    pub y_val: Option<i32>,
    /// Market code (flow): 1 TSE, 2 IFB, 3 derivatives.
    pub flow: Option<i32>,
    /// Board code (cgrValCot).
    pub board: Option<String>,
    pub board_title: Option<String>,
    /// Two-character sector code (cSecVal, trimmed).
    pub sector_code: Option<String>,
    pub sector_name: Option<String>,
    pub sub_sector_code: Option<i32>,
    pub sub_sector_name: Option<String>,
    pub base_volume: Option<i64>,
    pub total_shares: Option<Decimal>,
    /// kAjCapValCpsIdx; probably the free-float percentage.
    pub free_float_percent: Option<Decimal>,
    pub class: InstrumentClass,
    /// For a secondary listing (ISIN not ending in 0001), the primary listing's InsCode.
    pub parent_ins_code: Option<i64>,
}

/// Derives the class from TSETMC codes. yVal is authoritative; the ISIN prefix is a fallback.
pub fn classify(y_val: Option<i32>, flow: Option<i32>, isin: Option<&str>) -> InstrumentClass {
    if let Some(y) = y_val {
        match y {
            300 => return InstrumentClass::TseStock,
            303 => return InstrumentClass::IfbStock,
            309 => return InstrumentClass::IfbBaseYellow,
            400 | 403 | 404 => return InstrumentClass::Rights,
            305 => return InstrumentClass::EtfEquity,
            320 => return InstrumentClass::Option,
            208 | 301 => return InstrumentClass::Debt,
            _ => {}
        }
    }
    classify_by_isin(isin, flow)
}

/// The ISIN-prefix fallback.
pub fn classify_by_isin(isin: Option<&str>, flow: Option<i32>) -> InstrumentClass {
    let Some(isin) = isin.filter(|i| !crate::text::is_blank(i)) else {
        return InstrumentClass::Other;
    };
    let units: Vec<u16> = isin.encode_utf16().collect();
    if units.len() < 4 {
        return InstrumentClass::Other;
    }

    let prefix = String::from_utf16_lossy(&units[..4]).to_uppercase();
    match prefix.as_str() {
        "IRO1" => InstrumentClass::TseStock,
        "IRO3" => InstrumentClass::IfbStock,
        "IRR1" | "IRR3" => InstrumentClass::Rights,
        "IRT1" | "IRT3" => InstrumentClass::EtfEquity,
        "IRO9" | "IROA" | "IROB" | "IROF" => InstrumentClass::Option,
        p if p.starts_with("IRB") => InstrumentClass::Debt,
        _ if flow == Some(3) => InstrumentClass::Option,
        _ => InstrumentClass::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_yval_codes_classify() {
        let cases = [
            (300, 1, "IRO1FOLD0001", InstrumentClass::TseStock),
            (303, 2, "IRO3KRMZ0001", InstrumentClass::IfbStock),
            (305, 1, "IRT1BLOT0001", InstrumentClass::EtfEquity),
            (320, 3, "IROAZGDZ0281", InstrumentClass::Option),
            (208, 3, "IRB6AF130811", InstrumentClass::Debt),
            (400, 1, "IRR1DAML0101", InstrumentClass::Rights),
        ];
        for (y, flow, isin, expected) in cases {
            assert_eq!(expected, classify(Some(y), Some(flow), Some(isin)));
        }
    }

    #[test]
    fn isin_prefix_is_the_fallback() {
        let cases = [
            ("IRO1FOLD0001", InstrumentClass::TseStock),
            ("IRO3SAIZ0001", InstrumentClass::IfbStock),
            ("IRB3TR000001", InstrumentClass::Debt),
            ("IRO9ABCD0001", InstrumentClass::Option),
            ("XX", InstrumentClass::Other),
        ];
        for (isin, expected) in cases {
            assert_eq!(expected, classify(None, None, Some(isin)));
        }
    }

    #[test]
    fn rule_keys_round_trip() {
        for class in InstrumentClass::ALL {
            assert_eq!(class, InstrumentClass::from_rule_key(class.rule_key()));
            assert_eq!(Some(class), InstrumentClass::parse(class.name()));
        }
    }
}
