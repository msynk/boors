//! fipiran.com fund data (C# `FipiranClient`; base `https://www.fipiran.com/services/`, needs a fipiran Referer).

use serde::Deserialize;

use crate::domain::InstrumentClass;
use crate::raw_cache::RawRequest;
use crate::sources::json::{self as j, parse};
use crate::sources::{CachePolicy, SourceClient};
use crate::text::normalize;
use crate::{Date, Decimal, Result};

pub const SOURCE: &str = "fipiran";

/// One fund as reported by fipiran's comparison list (one row per fund, values as of `date`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FundRecord {
    pub reg_no: String,
    pub ins_code: Option<i64>,
    pub name: String,
    pub symbol: Option<String>,
    /// fipiran fundType: 4 fixed income, 5 commodity (gold), 6 equity, 7 mixed, 13 project, 14 land and building,
    /// 17 fund of funds, 18 real estate, 21 sector equity, 22 leveraged, 23 index, 24 principal-guaranteed, ...
    pub fund_type: i32,
    /// "Negotiable" = exchange-traded (ETF); "IssuanceAndCancellation" = issue/redeem only.
    pub type_of_invest: Option<String>,
    pub date: Option<Date>,
    pub issue_nav: Option<Decimal>,
    pub cancel_nav: Option<Decimal>,
    pub statistical_nav: Option<Decimal>,
    pub net_asset: Option<Decimal>,
    pub invested_units: Option<Decimal>,
    pub daily_return: Option<Decimal>,
    pub weekly_return: Option<Decimal>,
    pub monthly_return: Option<Decimal>,
    pub quarterly_return: Option<Decimal>,
    pub six_month_return: Option<Decimal>,
    pub annual_return: Option<Decimal>,
    pub stock_share: Option<Decimal>,
    pub bond_share: Option<Decimal>,
    pub cash_share: Option<Decimal>,
    pub deposit_share: Option<Decimal>,
    pub other_share: Option<Decimal>,
    pub manager: Option<String>,
    pub initiation_date: Option<Date>,
}

impl FundRecord {
    pub fn is_exchange_traded(&self) -> bool {
        self.type_of_invest.as_deref().is_some_and(|t| t.eq_ignore_ascii_case("Negotiable"))
    }

    /// Instrument class implied by the fund type, for ETFs; `None` when the type does not change the rules.
    pub fn implied_class(&self) -> Option<InstrumentClass> {
        match self.fund_type {
            4 | 24 => Some(InstrumentClass::EtfFixedIncome),
            5 => Some(InstrumentClass::FundGoldSilver),
            6 | 21 | 23 => Some(InstrumentClass::EtfEquity),
            7 => Some(InstrumentClass::EtfMixed),
            13 | 14 | 18 => Some(InstrumentClass::EtfRealEstate),
            17 => Some(InstrumentClass::FundOfFunds),
            22 => Some(InstrumentClass::FundLeveraged),
            _ => None,
        }
    }
}

pub fn fund_compare_request() -> RawRequest {
    let mut request = RawRequest::new(SOURCE, "fund/fundcompare", "https://www.fipiran.com/services/fund/fundcompare/", "regNos=[]&showMarketMakers=false");
    request.json_body = Some(r#"{"regNos":[],"showMarketMakers":false}"#.to_string());
    request.headers = vec![("Referer".to_string(), "https://www.fipiran.com/".to_string())];
    request
}

#[derive(Debug, Clone)]
pub struct FipiranClient {
    pub client: SourceClient,
}

impl FipiranClient {
    pub fn new(client: SourceClient) -> Self {
        Self { client }
    }

    pub async fn get_funds(&self, policy: CachePolicy) -> Result<Vec<FundRecord>> {
        parse_fund_compare(&self.client.get_body(&fund_compare_request(), policy).await?)
    }
}

#[derive(Deserialize, Default)]
struct FundCompareEnvelope {
    #[serde(rename = "items", default, deserialize_with = "j::list")]
    items: Vec<FundCompareDto>,
}

#[derive(Deserialize, Default)]
struct FundCompareDto {
    #[serde(rename = "regNo", default, deserialize_with = "j::opt_string")]
    reg_no: Option<String>,
    #[serde(rename = "insCode", default, deserialize_with = "j::opt_long")]
    ins_code: Option<i64>,
    #[serde(rename = "name", default, deserialize_with = "j::opt_string")]
    name: Option<String>,
    #[serde(rename = "smallSymbolName", default, deserialize_with = "j::opt_string")]
    small_symbol_name: Option<String>,
    #[serde(rename = "fundType", default, deserialize_with = "j::int")]
    fund_type: i32,
    #[serde(rename = "typeOfInvest", default, deserialize_with = "j::opt_string")]
    type_of_invest: Option<String>,
    #[serde(rename = "date", default, deserialize_with = "j::opt_string")]
    date: Option<String>,
    #[serde(rename = "issueNav", default, deserialize_with = "j::flex_decimal")]
    issue_nav: Option<Decimal>,
    #[serde(rename = "cancelNav", default, deserialize_with = "j::flex_decimal")]
    cancel_nav: Option<Decimal>,
    #[serde(rename = "statisticalNav", default, deserialize_with = "j::flex_decimal")]
    statistical_nav: Option<Decimal>,
    #[serde(rename = "netAsset", default, deserialize_with = "j::flex_decimal")]
    net_asset: Option<Decimal>,
    #[serde(rename = "investedUnits", default, deserialize_with = "j::flex_decimal")]
    invested_units: Option<Decimal>,
    #[serde(rename = "dailyEfficiency", default, deserialize_with = "j::flex_decimal")]
    daily_efficiency: Option<Decimal>,
    #[serde(rename = "weeklyEfficiency", default, deserialize_with = "j::flex_decimal")]
    weekly_efficiency: Option<Decimal>,
    #[serde(rename = "monthlyEfficiency", default, deserialize_with = "j::flex_decimal")]
    monthly_efficiency: Option<Decimal>,
    #[serde(rename = "quarterlyEfficiency", default, deserialize_with = "j::flex_decimal")]
    quarterly_efficiency: Option<Decimal>,
    #[serde(rename = "sixMonthEfficiency", default, deserialize_with = "j::flex_decimal")]
    six_month_efficiency: Option<Decimal>,
    #[serde(rename = "annualEfficiency", default, deserialize_with = "j::flex_decimal")]
    annual_efficiency: Option<Decimal>,
    #[serde(rename = "stock", default, deserialize_with = "j::flex_decimal")]
    stock: Option<Decimal>,
    #[serde(rename = "bond", default, deserialize_with = "j::flex_decimal")]
    bond: Option<Decimal>,
    #[serde(rename = "cash", default, deserialize_with = "j::flex_decimal")]
    cash: Option<Decimal>,
    #[serde(rename = "deposit", default, deserialize_with = "j::flex_decimal")]
    deposit: Option<Decimal>,
    #[serde(rename = "other", default, deserialize_with = "j::flex_decimal")]
    other: Option<Decimal>,
    #[serde(rename = "manager", default, deserialize_with = "j::opt_string")]
    manager: Option<String>,
    #[serde(rename = "initiationDate", default, deserialize_with = "j::opt_string")]
    initiation_date: Option<String>,
}

pub fn parse_fund_compare(json: &[u8]) -> Result<Vec<FundRecord>> {
    let envelope: FundCompareEnvelope = parse(json)?.unwrap_or_default();
    Ok(envelope
        .items
        .into_iter()
        .filter(|d| d.reg_no.as_deref().is_some_and(|r| !crate::text::is_blank(r)))
        .map(|d| {
            let reg_no = d.reg_no.unwrap_or_default();
            FundRecord {
                name: normalize(d.name.as_deref()).unwrap_or_else(|| reg_no.clone()),
                reg_no: reg_no.trim().to_string(),
                ins_code: d.ins_code.filter(|c| *c > 0),
                symbol: normalize(d.small_symbol_name.as_deref()),
                fund_type: d.fund_type,
                type_of_invest: d.type_of_invest,
                date: parse_date(d.date.as_deref()),
                issue_nav: d.issue_nav,
                cancel_nav: d.cancel_nav,
                statistical_nav: d.statistical_nav,
                net_asset: d.net_asset,
                invested_units: d.invested_units,
                daily_return: d.daily_efficiency,
                weekly_return: d.weekly_efficiency,
                monthly_return: d.monthly_efficiency,
                quarterly_return: d.quarterly_efficiency,
                six_month_return: d.six_month_efficiency,
                annual_return: d.annual_efficiency,
                stock_share: d.stock,
                bond_share: d.bond,
                cash_share: d.cash,
                deposit_share: d.deposit,
                other_share: d.other,
                manager: normalize(d.manager.as_deref()),
                initiation_date: parse_date(d.initiation_date.as_deref()),
            }
        })
        .collect())
}

/// fipiran dates are ISO strings, sometimes with an offset and sometimes "0001-01-01T00:00:00" for unknown.
fn parse_date(value: Option<&str>) -> Option<Date> {
    let value = value?;
    let head = value.get(..10)?;
    let date = Date::parse_from_str(head, "%Y-%m-%d").ok()?;
    (chrono::Datelike::year(&date) >= 1900).then_some(date)
}
