//! One request per external dependency, reporting whether this node can reach it (C# CLI `ProbeCommand`). The JSON
//! report (`{data}/reports/probe-{node}-{yyyyMMdd-HHmmss}.json`) has the same shape, so the data-health report reads
//! reports from either implementation.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::Result;
use crate::options::MarketDataOptions;
use crate::raw_cache::format_fetched_at;

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub group: String,
    pub name: String,
    pub url: String,
    pub status: Option<i32>,
    pub bytes: i64,
    pub elapsed_ms: i64,
    pub verdict: String,
    pub error: Option<String>,
}

struct Target {
    group: &'static str,
    name: &'static str,
    url: &'static str,
    referer: Option<&'static str>,
    expect_json: bool,
}

const fn target(group: &'static str, name: &'static str, url: &'static str, referer: Option<&'static str>, expect_json: bool) -> Target {
    Target { group, name, url, referer, expect_json }
}

const TARGETS: [Target; 15] = [
    target(
        "market-data",
        "TSETMC cdn API (instrument info)",
        "https://cdn.tsetmc.com/api/Instrument/GetInstrumentInfo/46348559193224090",
        Some("https://www.tsetmc.com/"),
        true,
    ),
    target(
        "market-data",
        "TSETMC cdn API (market watch, shares)",
        "https://cdn.tsetmc.com/api/ClosingPrice/GetMarketWatch?market=0&industrialGroup=&paperTypes%5B0%5D=1&showTraded=false&withBestLimits=true&hEven=0&RefID=0",
        Some("https://www.tsetmc.com/"),
        true,
    ),
    target(
        "market-data",
        "TSETMC legacy (old.tsetmc.com)",
        "https://old.tsetmc.com/tsev2/data/InstTradeHistory.aspx?i=46348559193224090&Top=5&A=0",
        Some("https://old.tsetmc.com/"),
        false,
    ),
    target("market-data", "TSE gateway (webgw.tse.ir)", "https://webgw.tse.ir/InstrumentProvider/api/v1/Instrument/MarketSummary/fa", None, true),
    target("market-data", "Codal search API", "https://search.codal.ir/api/search/v1/categories", None, true),
    target("market-data", "Codal main site", "https://codal.ir/", None, false),
    target("market-data", "fipiran fund API", "https://www.fipiran.com/services/fund/fundtype", Some("https://www.fipiran.com/"), true),
    target("market-data", "tgju live feed", "https://call1.tgju.org/ajax.json", None, true),
    target("market-data", "BrsApi", "https://brsapi.ir/", None, false),
    target("market-data", "Iran Mercantile Exchange", "https://www.ime.co.ir/", None, false),
    target("messaging", "Telegram Bot API", "https://api.telegram.org/", None, false),
    target("messaging", "Bale Bot API", "https://tapi.bale.ai/", None, false),
    target("build", "nuget.org", "https://api.nuget.org/v3/index.json", None, true),
    target("build", "ParsPack NuGet mirror", "https://mirror.abrha.net/repository/nuget/index.json", None, true),
    target("ai", "AvalAI gateway", "https://api.avalai.ir/v1/models", None, true),
];

fn client(accept_invalid_certs: bool) -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .gzip(true)
        .brotli(true)
        .deflate(true)
        .danger_accept_invalid_certs(accept_invalid_certs)
        .build()
        .map_err(|e| crate::Error::Http(e.to_string()))
}

/// Probes every target, reporting each result through `progress`, and writes the JSON report.
pub async fn run(options: &MarketDataOptions, mut progress: impl FnMut(&ProbeResult)) -> Result<(Vec<ProbeResult>, PathBuf)> {
    // A plain client without retries; old.tsetmc.com's self-signed certificate is accepted for that host only.
    let strict = client(false)?;
    let relaxed = client(true)?;
    let mut results = Vec::new();
    for target in &TARGETS {
        let http = if target.url.starts_with("https://old.tsetmc.com/") { &relaxed } else { &strict };
        let mut request = http
            .get(target.url)
            .header("user-agent", &options.user_agent)
            .header("accept", if target.expect_json { "application/json, text/plain, */*" } else { "text/html, */*" });
        if let Some(referer) = target.referer {
            request = request.header("referer", referer);
        }

        let started = Instant::now();
        let result = match request.send().await {
            Ok(response) => {
                let status = i32::from(response.status().as_u16());
                let ok = response.status().is_success();
                let bytes = response.bytes().await.map(|b| b.len() as i64).unwrap_or(0);
                let verdict = if ok {
                    "ok"
                } else if target.group == "market-data" {
                    "http-error"
                } else {
                    "reachable"
                };
                ProbeResult {
                    group: target.group.into(),
                    name: target.name.into(),
                    url: target.url.into(),
                    status: Some(status),
                    bytes,
                    elapsed_ms: started.elapsed().as_millis() as i64,
                    verdict: verdict.into(),
                    error: None,
                }
            }
            Err(e) => {
                let message =
                    if e.is_timeout() { "timeout".to_string() } else { std::error::Error::source(&e).map_or_else(|| e.to_string(), |s| s.to_string()) };
                ProbeResult {
                    group: target.group.into(),
                    name: target.name.into(),
                    url: target.url.into(),
                    status: None,
                    bytes: 0,
                    elapsed_ms: started.elapsed().as_millis() as i64,
                    verdict: "unreachable".into(),
                    error: Some(message),
                }
            }
        };
        progress(&result);
        results.push(result);
    }

    let now = Utc::now();
    let directory = options.data_root.join("reports");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("probe-{}-{}.json", options.node, now.format("%Y%m%d-%H%M%S")));
    std::fs::write(&path, report_json(&options.node, now, &results))?;
    Ok((results, path))
}

/// `JsonSerializer.Serialize(new { node, probedAtUtc, results }, WriteIndented)`.
fn report_json(node: &str, now: chrono::DateTime<Utc>, results: &[ProbeResult]) -> String {
    let nl = if cfg!(windows) { "\r\n" } else { "\n" };
    let quote = |value: &str| {
        let mut out = String::new();
        crate::raw_cache::escape_json_default(value, &mut out);
        out
    };
    let mut out = format!("{{{nl}  \"node\": {},{nl}  \"probedAtUtc\": {},{nl}  \"results\": [", quote(node), format_fetched_at(now));
    for (i, r) in results.iter().enumerate() {
        out.push_str(if i == 0 { nl } else { "," });
        if i > 0 {
            out.push_str(nl);
        }
        out.push_str(&format!(
            "    {{{nl}      \"Group\": {},{nl}      \"Name\": {},{nl}      \"Url\": {},{nl}      \"Status\": {},{nl}      \"Bytes\": {},{nl}      \"ElapsedMs\": {},{nl}      \"Verdict\": {},{nl}      \"Error\": {}{nl}    }}",
            quote(&r.group),
            quote(&r.name),
            quote(&r.url),
            r.status.map_or_else(|| "null".to_string(), |s| s.to_string()),
            r.bytes,
            r.elapsed_ms,
            quote(&r.verdict),
            r.error.as_deref().map_or_else(|| "null".to_string(), quote),
        ));
    }
    if !results.is_empty() {
        out.push_str(nl);
        out.push_str("  ");
    }
    out.push_str(&format!("]{nl}}}"));
    out
}
