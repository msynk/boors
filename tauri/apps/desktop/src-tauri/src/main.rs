//! Boors desktop: the SolidJS UI in a Tauri 2 window with the market-data core in-process.
//!
//! The UI calls one command, `api(method, path, query, body)`, which goes to the same dispatcher as the HTTP server
//! (`boors_market::api::dispatch`). Data lives in `%LOCALAPPDATA%\dev.boors.desktop\data` unless `BOORS_DATA` says
//! otherwise; set `BOORS_COLLECT=true` (or pass `--collect`) to run the collection schedule in the background.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use boors_market::api::{self, ApiContext, Problem, Transport};
use boors_market::{MarketContext, MarketDataOptions};
use serde_json::Value;
use tauri::Manager;

struct AppState {
    api: Arc<ApiContext>,
}

/// Contract query values are `string | number | boolean | null`; null means "absent".
fn to_query(query: Option<HashMap<String, Value>>) -> api::Query {
    query
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| {
            let text = match value {
                Value::Null => return None,
                Value::String(s) => s,
                Value::Bool(b) => b.to_string(),
                Value::Number(n) => n.to_string(),
                other => other.to_string(),
            };
            Some((key, text))
        })
        .collect()
}

#[tauri::command]
async fn api(
    state: tauri::State<'_, AppState>,
    method: String,
    path: String,
    query: Option<HashMap<String, Value>>,
    body: Option<Value>,
) -> Result<Value, Problem> {
    api::dispatch(&state.api, &method, &path, &to_query(query), body).await
}

fn collect_enabled() -> bool {
    std::env::args().any(|a| a == "--collect")
        || std::env::var("BOORS_COLLECT").is_ok_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_env("BOORS_LOG").unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")))
        .init();

    let result = tauri::Builder::default()
        .setup(|app| {
            let data_root = match std::env::var_os("BOORS_DATA").filter(|v| !v.is_empty()) {
                Some(dir) => PathBuf::from(dir),
                None => app.path().app_local_data_dir()?.join("data"),
            };
            let collect = collect_enabled();
            let mut options = MarketDataOptions { data_root, collect, ..MarketDataOptions::default() };
            if let Ok(node) = std::env::var("BOORS_NODE")
                && !node.trim().is_empty()
            {
                options.node = node;
            }

            let market = MarketContext::open(options).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            let api = tauri::async_runtime::block_on(async { ApiContext::new(market, Transport::Ipc, collect) });
            if collect {
                tauri::async_runtime::spawn(api::schedule::run(api.clone()));
            }
            app.manage(AppState { api });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!());

    if let Err(e) = result {
        eprintln!("Boors could not start: {e}");
        std::process::exit(1);
    }
}
