//! `boors-server` — the Boors HTTP node.
//!
//! Serves the API at `/api/v1/*` through `boors_market::api::dispatch` (the same implementation the desktop app
//! calls over Tauri IPC), RFC 7807 problem bodies, gzip/br compression, optional CORS for UI development, an
//! optional operator bearer token for `RunJob`/`Sync`, `/health`, the built UI with SPA fallback, and — with
//! `--collect` — the collection schedule in Iran time.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Path as UrlPath, RawQuery, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use boors_market::api::{self, ApiContext, Problem, Transport};
use boors_market::{MarketContext, MarketDataOptions};
use clap::Parser;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Parser, Debug)]
#[command(name = "boors-server", version, about = "Boors HTTP node")]
struct Args {
    /// Data root (raw cache + market.duckdb).
    #[arg(long, env = "BOORS_DATA", default_value = ".boors-data")]
    data: PathBuf,

    /// Node name recorded on every raw entry.
    #[arg(long, env = "BOORS_NODE")]
    node: Option<String>,

    /// Port to listen on.
    #[arg(long, env = "BOORS_PORT", default_value_t = 5080)]
    port: u16,

    /// Address to bind (use 0.0.0.0 to serve other machines).
    #[arg(long, env = "BOORS_HOST", default_value = "127.0.0.1")]
    host: String,

    /// Directory of the built UI (default: ui/dist next to the executable, then ../../ui/dist, then ./ui/dist).
    #[arg(long = "ui-dir", env = "BOORS_UI_DIR")]
    ui_dir: Option<PathBuf>,

    /// Schedule the collection jobs on this node (Iran time).
    #[arg(long, env = "BOORS_COLLECT")]
    collect: bool,

    /// Allow cross-origin requests (UI development against a separate Vite server).
    #[arg(long, env = "BOORS_CORS")]
    cors: bool,

    /// Optional rules.yaml overriding the embedded copy.
    #[arg(long, env = "BOORS_RULES")]
    rules: Option<PathBuf>,
}

#[derive(Clone)]
struct AppState {
    api: Arc<ApiContext>,
    operator_token: Option<String>,
}

fn problem_response(problem: &Problem) -> Response {
    let status = StatusCode::from_u16(problem.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, Json(problem)).into_response();
    response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/problem+json"));
    response
}

fn parse_query(raw: Option<&str>) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Some(raw) = raw else { return map };
    for pair in raw.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        map.insert(decode_component(key), decode_component(value));
    }
    map
}

fn decode_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => match std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(b) => {
                    out.push(b);
                    i += 3;
                }
                None => {
                    out.push(b'%');
                    i += 1;
                }
            },
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn authorized(state: &AppState, headers: &HeaderMap) -> bool {
    let Some(token) = &state.operator_token else { return true };
    let expected = format!("Bearer {token}");
    headers.get(header::AUTHORIZATION).is_some_and(|v| constant_time_eq(v.as_bytes(), expected.as_bytes()))
}

async fn api_handler(
    State(state): State<AppState>,
    method: Method,
    UrlPath(path): UrlPath<String>,
    RawQuery(raw): RawQuery,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let method = method.as_str().to_string();
    if api::requires_operator(&method, &path) && !authorized(&state, &headers) {
        return problem_response(&Problem::unauthorized("A valid operator token is required (Authorization: Bearer <BOORS_OPERATOR_TOKEN>)."));
    }
    let query = parse_query(raw.as_deref());

    if api::is_export_bundle(&method, &path) {
        return export_bundle(&state, &query).await;
    }

    let body = if body.is_empty() {
        None
    } else {
        match serde_json::from_slice::<serde_json::Value>(&body) {
            Ok(value) => Some(value),
            Err(e) => return problem_response(&Problem::bad_request(format!("The request body is not valid JSON: {e}"))),
        }
    };

    match api::dispatch(&state.api, &method, &path, &query, body).await {
        Ok(value) => Json(value).into_response(),
        Err(problem) => {
            if problem.status >= 500 {
                tracing::error!(%method, %path, detail = problem.detail.as_deref().unwrap_or(""), "request failed");
            }
            problem_response(&problem)
        }
    }
}

async fn export_bundle(state: &AppState, query: &HashMap<String, String>) -> Response {
    let since = match api::parse_date(query.iter().find(|(k, _)| k.eq_ignore_ascii_case("since")).map(|(_, v)| v.as_str())) {
        Ok(since) => since,
        Err(problem) => return problem_response(&problem),
    };
    let market = state.api.market.clone();
    let built = tokio::task::spawn_blocking(move || {
        clean_old_exports(&market.options.data_root.join("tmp"));
        api::export_bundle_file(&market, since)
    })
    .await;
    let path = match built {
        Ok(Ok(path)) => path,
        Ok(Err(e)) => return problem_response(&Problem::from(e)),
        Err(e) => return problem_response(&Problem::new(500, e.to_string())),
    };
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(e) => return problem_response(&Problem::new(500, e.to_string())),
    };
    let length = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    let stream = tokio_util::io::ReaderStream::new(file);
    let mut response = Response::new(Body::from_stream(stream));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("attachment; filename=\"boors-bundle.zip\""));
    if let Ok(value) = HeaderValue::from_str(&length.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    response
}

/// Export files are streamed and left behind; anything older than an hour is removed before the next export.
fn clean_old_exports(directory: &Path) {
    let Ok(entries) = std::fs::read_dir(directory) else { return };
    for entry in entries.flatten() {
        let old = entry.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age.as_secs() > 3600);
        if old && entry.file_name().to_string_lossy().starts_with("export-") {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

async fn health(State(state): State<AppState>) -> Response {
    if !state.api.collect {
        return Json(serde_json::json!({ "status": "Healthy", "description": "This node does not collect." })).into_response();
    }
    let market = state.api.market.clone();
    let report = tokio::task::spawn_blocking(move || market.health().get_report()).await;
    match report {
        Ok(Ok(report)) if report.healthy() => {
            Json(serde_json::json!({ "status": "Healthy", "description": format!("Market data current to {}.", report.expected_session) })).into_response()
        }
        Ok(Ok(report)) => {
            let problems: Vec<String> = report
                .datasets
                .iter()
                .filter(|d| matches!(d.status, boors_market::ingestion::DatasetStatus::Stale | boors_market::ingestion::DatasetStatus::Empty))
                .map(|d| format!("{}: {}", d.dataset, d.status.as_str()))
                .chain(report.latest_runs.iter().filter(|r| matches!(r.status.as_str(), "failed" | "partial")).map(|r| format!("job {}: {}", r.job, r.status)))
                .chain(report.sources.iter().filter(|s| !matches!(s.verdict.as_str(), "ok" | "reachable")).map(|s| format!("source {}: {}", s.name, s.verdict)))
                .collect();
            Json(serde_json::json!({ "status": "Degraded", "description": problems.join("; ") })).into_response()
        }
        Ok(Err(e)) => Json(serde_json::json!({ "status": "Degraded", "description": e.to_string() })).into_response(),
        Err(e) => Json(serde_json::json!({ "status": "Degraded", "description": e.to_string() })).into_response(),
    }
}

fn resolve_ui_dir(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(dir) = explicit {
        return Some(dir);
    }
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
    let mut candidates = Vec::new();
    if let Some(dir) = &exe_dir {
        candidates.push(dir.join("ui").join("dist"));
        candidates.push(dir.join("..").join("..").join("ui").join("dist"));
    }
    candidates.push(PathBuf::from("ui").join("dist"));
    candidates.into_iter().find(|c| c.join("index.html").is_file())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("BOORS_LOG")
                .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,tower_http=warn")),
        )
        .init();

    let args = Args::parse();
    let mut options = MarketDataOptions { data_root: args.data.clone(), rules_path: args.rules.clone(), collect: args.collect, ..MarketDataOptions::default() };
    if let Some(node) = args.node.clone().filter(|n| !n.trim().is_empty()) {
        options.node = node;
    }

    let market = tokio::task::spawn_blocking(move || MarketContext::open(options)).await??;
    let api = ApiContext::new(market.clone(), Transport::Http, args.collect);
    let operator_token = std::env::var("BOORS_OPERATOR_TOKEN").ok().filter(|t| !t.is_empty());
    if args.collect {
        tokio::spawn(api::schedule::run(api.clone()));
    }

    let state = AppState { api: api.clone(), operator_token };
    let mut app = Router::new().route("/health", get(health)).route("/api/v1/{*path}", any(api_handler)).with_state(state);

    match resolve_ui_dir(args.ui_dir.clone()) {
        Some(dir) => {
            tracing::info!(ui = %dir.display(), "serving the UI");
            let index = dir.join("index.html");
            app = app.fallback_service(ServeDir::new(&dir).append_index_html_on_directories(true).fallback(ServeFile::new(index)));
        }
        None => {
            tracing::warn!("no built UI found (pass --ui-dir); only the API is served");
            app = app.fallback(|| async { (StatusCode::NOT_FOUND, "The UI is not built. Run `npm --prefix ui run build` or pass --ui-dir.") });
        }
    }

    app = app.layer(CompressionLayer::new().gzip(true).br(true));
    if args.cors {
        app = app.layer(CorsLayer::very_permissive());
    }

    let address: SocketAddr = format!("{}:{}", args.host, args.port).parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, node = %market.options.node, data = %market.options.data_root.display(), collect = args.collect, "boors-server listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
