//! Codal's open search API (C# `CodalClient`, `CodalParser`, `CodalModels`). Codal's JSON is PascalCase.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use chrono::{DateTime, FixedOffset};
use regex::Regex;
use serde::Deserialize;

use crate::raw_cache::RawRequest;
use crate::sources::json::{self as j, parse};
use crate::sources::{CachePolicy, SourceClient, escape_data_string};
use crate::text::{normalize, parse_jalali};
use crate::{Error, Result};

pub const SOURCE: &str = "codal";

/// Well-known Codal LetterType ids.
pub mod letter_types {
    pub const MONTHLY_ACTIVITY: i32 = 58;
    pub const FINANCIAL_STATEMENTS: i32 = 6;
    pub const MATERIAL_DISCLOSURE: i32 = 11;
    pub const DIVIDEND_SCHEDULE: i32 = 30;
    pub const BOARD_ACTIVITY_REPORT: i32 = 32;
    pub const PORTFOLIO_STATEMENT: i32 = 8;
    pub const AGM_INVITATIONS: [i32; 3] = [16, 17, 18];
    pub const AGM_DECISIONS: [i32; 3] = [20, 21, 22];
    pub const CAPITAL_INCREASE_CHAIN: [i32; 13] = [23, 24, 55, 80, 56, 45, 53, 57, 25, 27, 119, 28, 164];
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodalLetterType {
    pub id: i32,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodalCategory {
    pub code: i32,
    pub name: String,
    pub letter_types: Vec<CodalLetterType>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodalTaxonomy {
    pub categories: Vec<CodalCategory>,
}

impl CodalTaxonomy {
    /// Letter types by id (first occurrence wins).
    pub fn letter_types_by_id(&self) -> BTreeMap<i32, CodalLetterType> {
        let mut map = BTreeMap::new();
        for letter in self.categories.iter().flat_map(|c| c.letter_types.iter()) {
            map.entry(letter.id).or_insert_with(|| letter.clone());
        }
        map
    }

    /// Category codes that list a letter type.
    pub fn categories_of(&self, letter_type_id: i32) -> Vec<i32> {
        self.categories.iter().filter(|c| c.letter_types.iter().any(|l| l.id == letter_type_id)).map(|c| c.code).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CodalLetter {
    pub tracing_no: i64,
    pub symbol: Option<String>,
    pub company_name: Option<String>,
    pub title: String,
    /// Form code such as "ن-30" (digits normalized to ASCII).
    pub letter_code: Option<String>,
    /// LetterType id parsed from the report URL's `let=` parameter.
    pub letter_type_id: Option<i32>,
    pub sent_at: Option<DateTime<FixedOffset>>,
    pub published_at: Option<DateTime<FixedOffset>>,
    /// "الف" or "ب" for material disclosures, parsed from the title.
    pub disclosure_group: Option<String>,
    pub has_html: bool,
    pub has_excel: bool,
    pub has_pdf: bool,
    pub has_xbrl: bool,
    pub has_attachment: bool,
    pub is_estimate: bool,
    pub under_supervision: i32,
    pub report_url: Option<String>,
    pub excel_url: Option<String>,
    pub pdf_url: Option<String>,
    pub attachment_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CodalSearchPage {
    pub total: i32,
    pub page_count: i32,
    pub letters: Vec<CodalLetter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodalSearchQuery {
    pub symbol: Option<String>,
    pub letter_type: i32,
    pub category: i32,
    pub page_number: i32,
}

impl Default for CodalSearchQuery {
    fn default() -> Self {
        Self { symbol: None, letter_type: -1, category: -1, page_number: 1 }
    }
}

#[derive(Debug, Clone)]
pub struct CodalClient {
    pub client: SourceClient,
    search_base: String,
    codal_base: String,
}

impl CodalClient {
    pub fn new(client: SourceClient, search_base_url: &str, codal_base_url: &str) -> Self {
        let search_base = if search_base_url.ends_with('/') { search_base_url.to_string() } else { format!("{search_base_url}/") };
        Self { client, search_base, codal_base: codal_base_url.to_string() }
    }

    pub fn categories_request(&self) -> RawRequest {
        RawRequest::new(SOURCE, "search/v1/categories", format!("{}v1/categories", self.search_base), "")
    }

    pub fn search_request(&self, query: &CodalSearchQuery) -> RawRequest {
        let is_filtered = query.symbol.is_some() || query.letter_type != -1 || query.category != -1;
        let mut parameters: Vec<(&str, String)> = vec![
            ("Audited", "true".into()),
            ("AuditorRef", "-1".into()),
            ("Category", query.category.to_string()),
            ("Childs", "true".into()),
            ("CompanyState", "-1".into()),
            ("CompanyType", "-1".into()),
            ("Consolidatable", "true".into()),
            ("IsNotAudited", "false".into()),
            ("Length", "-1".into()),
            ("LetterType", query.letter_type.to_string()),
            ("Mains", "true".into()),
            ("NotAudited", "true".into()),
            ("NotConsolidatable", "true".into()),
            ("PageNumber", query.page_number.to_string()),
            ("Publisher", "false".into()),
        ];
        if let Some(symbol) = &query.symbol {
            parameters.push(("Symbol", symbol.clone()));
        }
        parameters.push(("TracingNo", "-1".into()));
        parameters.push(("search", if is_filtered { "true" } else { "false" }.into()));

        let query_string = parameters.iter().map(|(k, v)| format!("{k}={}", escape_data_string(v))).collect::<Vec<_>>().join("&");
        let params_key = parameters.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
        RawRequest::new(SOURCE, "search/v2/q", format!("{}v2/q?{query_string}", self.search_base), params_key)
    }

    pub async fn get_taxonomy(&self, policy: CachePolicy) -> Result<CodalTaxonomy> {
        parse_categories(&self.client.get_body(&self.categories_request(), policy).await?)
    }

    pub async fn search(&self, query: &CodalSearchQuery, policy: CachePolicy) -> Result<CodalSearchPage> {
        parse_search(&self.client.get_body(&self.search_request(query), policy).await?, &self.codal_base)
    }
}

// ------------------------------------------------------------------------------------------------ parsers

#[derive(Deserialize, Default)]
struct CategoryDto {
    #[serde(rename = "Code", default, deserialize_with = "j::int")]
    code: i32,
    #[serde(rename = "Name", default, deserialize_with = "j::opt_string")]
    name: Option<String>,
    #[serde(rename = "PublisherTypes", default, deserialize_with = "j::list")]
    publisher_types: Vec<PublisherTypeDto>,
}

#[derive(Deserialize, Default)]
struct PublisherTypeDto {
    #[serde(rename = "LetterTypes", default, deserialize_with = "j::list")]
    letter_types: Vec<LetterTypeDto>,
}

#[derive(Deserialize, Default)]
struct LetterTypeDto {
    #[serde(rename = "Id", default, deserialize_with = "j::int")]
    id: i32,
    #[serde(rename = "Name", default, deserialize_with = "j::opt_string")]
    name: Option<String>,
}

pub fn parse_categories(json: &[u8]) -> Result<CodalTaxonomy> {
    let categories: Vec<CategoryDto> = parse(json)?.unwrap_or_default();
    Ok(CodalTaxonomy {
        categories: categories
            .into_iter()
            .filter(|c| c.code >= 0)
            .map(|c| {
                let mut by_id: BTreeMap<i32, String> = BTreeMap::new();
                for letter in c.publisher_types.iter().flat_map(|p| p.letter_types.iter()) {
                    by_id.entry(letter.id).or_insert_with(|| normalize(letter.name.as_deref()).unwrap_or_default());
                }
                CodalCategory {
                    code: c.code,
                    name: normalize(c.name.as_deref()).unwrap_or_default(),
                    letter_types: by_id.into_iter().map(|(id, name)| CodalLetterType { id, name }).collect(),
                }
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct SearchDto {
    #[serde(rename = "Total", default, deserialize_with = "j::int")]
    total: i32,
    #[serde(rename = "Page", default, deserialize_with = "j::int")]
    page: i32,
    #[serde(rename = "Letters", default, deserialize_with = "j::list")]
    letters: Vec<LetterDto>,
}

#[derive(Deserialize, Default)]
struct LetterDto {
    #[serde(rename = "TracingNo", default, deserialize_with = "j::long")]
    tracing_no: i64,
    #[serde(rename = "Symbol", default, deserialize_with = "j::opt_string")]
    symbol: Option<String>,
    #[serde(rename = "CompanyName", default, deserialize_with = "j::opt_string")]
    company_name: Option<String>,
    #[serde(rename = "UnderSupervision", default, deserialize_with = "j::int")]
    under_supervision: i32,
    #[serde(rename = "Title", default, deserialize_with = "j::opt_string")]
    title: Option<String>,
    #[serde(rename = "LetterCode", default, deserialize_with = "j::opt_string")]
    letter_code: Option<String>,
    #[serde(rename = "SentDateTime", default, deserialize_with = "j::opt_string")]
    sent_date_time: Option<String>,
    #[serde(rename = "PublishDateTime", default, deserialize_with = "j::opt_string")]
    publish_date_time: Option<String>,
    #[serde(rename = "HasHtml", default, deserialize_with = "j::boolean")]
    has_html: bool,
    #[serde(rename = "IsEstimate", default, deserialize_with = "j::boolean")]
    is_estimate: bool,
    #[serde(rename = "Url", default, deserialize_with = "j::opt_string")]
    url: Option<String>,
    #[serde(rename = "HasExcel", default, deserialize_with = "j::boolean")]
    has_excel: bool,
    #[serde(rename = "HasPdf", default, deserialize_with = "j::boolean")]
    has_pdf: bool,
    #[serde(rename = "HasXbrl", default, deserialize_with = "j::boolean")]
    has_xbrl: bool,
    #[serde(rename = "HasAttachment", default, deserialize_with = "j::boolean")]
    has_attachment: bool,
    #[serde(rename = "AttachmentUrl", default, deserialize_with = "j::opt_string")]
    attachment_url: Option<String>,
    #[serde(rename = "PdfUrl", default, deserialize_with = "j::opt_string")]
    pdf_url: Option<String>,
    #[serde(rename = "ExcelUrl", default, deserialize_with = "j::opt_string")]
    excel_url: Option<String>,
}

pub fn parse_search(json: &[u8], codal_base_url: &str) -> Result<CodalSearchPage> {
    let dto: SearchDto = parse(json)?.ok_or_else(|| Error::format("Codal search response is empty."))?;
    let letters = dto
        .letters
        .into_iter()
        .map(|l| {
            let title = normalize(l.title.as_deref()).unwrap_or_default();
            CodalLetter {
                tracing_no: l.tracing_no,
                symbol: normalize(l.symbol.as_deref()),
                company_name: normalize(l.company_name.as_deref()),
                disclosure_group: parse_disclosure_group(Some(&title)),
                title,
                letter_code: normalize(l.letter_code.as_deref()),
                letter_type_id: parse_letter_type_id(l.url.as_deref()),
                sent_at: parse_jalali(l.sent_date_time.as_deref()),
                published_at: parse_jalali(l.publish_date_time.as_deref()),
                has_html: l.has_html,
                has_excel: l.has_excel,
                has_pdf: l.has_pdf,
                has_xbrl: l.has_xbrl,
                has_attachment: l.has_attachment,
                is_estimate: l.is_estimate,
                under_supervision: l.under_supervision,
                report_url: absolute(codal_base_url, l.url.as_deref(), "Reports/"),
                excel_url: absolute(codal_base_url, l.excel_url.as_deref(), "Reports/"),
                pdf_url: absolute(codal_base_url, l.pdf_url.as_deref(), "Reports/"),
                attachment_url: absolute(codal_base_url, l.attachment_url.as_deref(), "Reports/"),
            }
        })
        .collect();
    Ok(CodalSearchPage { total: dto.total, page_count: dto.page, letters })
}

static LETTER_TYPE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)[?&]let=(-?\d+)").expect("valid regex"));
static DISCLOSURE_GROUP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[-–]\s*گروه\s*(الف|ب)").expect("valid regex"));

/// Extracts the `let=` LetterType id from a Decision.aspx URL.
pub fn parse_letter_type_id(url: Option<&str>) -> Option<i32> {
    let url = url.filter(|u| !u.is_empty())?;
    LETTER_TYPE.captures(url).and_then(|c| c.get(1)).and_then(|m| m.as_str().parse().ok())
}

/// Material-disclosure group from a title such as "افشای اطلاعات بااهمیت - (موضوع - گروه ب) ...".
pub fn parse_disclosure_group(title: Option<&str>) -> Option<String> {
    let title = title.filter(|t| !t.is_empty())?;
    DISCLOSURE_GROUP.captures(title).and_then(|c| c.get(1)).map(|m| m.as_str().to_string())
}

/// .NET `new Uri(base, relative).ToString()` for the URL shapes Codal sends.
fn absolute(base_url: &str, url: Option<&str>, relative_root: &str) -> Option<String> {
    let url = url.filter(|u| !crate::text::is_blank(u))?;
    if let Ok(parsed) = reqwest::Url::parse(url)
        && matches!(parsed.scheme(), "http" | "https")
    {
        return Some(dotnet_uri_string(&parsed));
    }

    let root = if base_url.ends_with('/') { base_url.to_string() } else { format!("{base_url}/") };
    let root = reqwest::Url::parse(&root).ok()?;
    let joined = if url.starts_with('/') { root.join(url).ok()? } else { root.join(relative_root).ok()?.join(url).ok()? };
    Some(dotnet_uri_string(&joined))
}

/// `Uri.ToString()` unescapes percent-encoded characters that are safe to show; the `%3d` escapes Codal uses
/// (`=` inside a query value) stay escaped in .NET because `=` is reserved, so the URL text is kept as is.
fn dotnet_uri_string(url: &reqwest::Url) -> String {
    url.as_str().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disclosure_group_is_parsed_from_the_title() {
        assert_eq!(Some("ب".into()), parse_disclosure_group(Some("افشای اطلاعات بااهمیت - (توقف تولید - گروه ب) منتهی به سال مالی ۱۴۰۵/۱۲/۲۹")));
        assert_eq!(Some("الف".into()), parse_disclosure_group(Some("افشای اطلاعات بااهمیت - (افزایش سرمایه – گروه الف)")));
        assert_eq!(Some("الف".into()), parse_disclosure_group(Some("افشای اطلاعات بااهمیت - (موضوع -گروه الف)")));
        assert_eq!(None, parse_disclosure_group(Some("گزارش فعالیت ماهانه دوره ۱ ماهه منتهی به ۱۴۰۵/۰۶/۳۱")));
    }

    #[test]
    fn letter_type_id_is_read_from_the_report_url() {
        assert_eq!(Some(58), parse_letter_type_id(Some("/Reports/Decision.aspx?LetterSerial=abc%3d&rt=0&let=58&ct=0&ft=-1")));
    }

    #[test]
    fn absolute_urls_follow_dotnet_uri_rules() {
        let base = "https://codal.ir/";
        assert_eq!(Some("https://codal.ir/Reports/Decision.aspx?a=1".into()), absolute(base, Some("/Reports/Decision.aspx?a=1"), "Reports/"));
        assert_eq!(Some("https://codal.ir/Reports/DownloadFile.aspx?hs=x".into()), absolute(base, Some("DownloadFile.aspx?hs=x"), "Reports/"));
        assert_eq!(
            Some("https://excel.codal.ir/service/Excel/GetAll/x/0".into()),
            absolute(base, Some("https://excel.codal.ir/service/Excel/GetAll/x/0"), "Reports/")
        );
        assert_eq!(None, absolute(base, Some(" "), "Reports/"));
    }
}
