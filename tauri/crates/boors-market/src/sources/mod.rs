//! Source clients: every response is written to the raw cache before anything parses it (C# `RawSourceClient`).

pub mod codal;
pub mod fipiran;
pub mod http;
pub mod json;
pub mod tgju;
pub mod tsetmc;

use std::sync::Arc;

use chrono::Utc;

use crate::raw_cache::{RawCache, RawRequest};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CachePolicy {
    /// Always fetch from the source (the response is still stored in the raw cache).
    Network,
    /// Use the latest cached response if one exists; otherwise fetch.
    PreferCache,
    /// Never touch the network; fail if nothing is cached. Used for replay and offline work.
    CacheOnly,
}

impl CachePolicy {
    /// C# `Prefer(policy)`: network becomes prefer-cache; the others stay.
    pub fn prefer(self) -> CachePolicy {
        if self == CachePolicy::Network { CachePolicy::PreferCache } else { self }
    }
}

/// Raw-cache-first access to one source.
#[derive(Debug, Clone)]
pub struct SourceClient {
    http: Arc<http::SourceHttp>,
    cache: Arc<RawCache>,
}

impl SourceClient {
    pub fn new(http: Arc<http::SourceHttp>, cache: Arc<RawCache>) -> Self {
        Self { http, cache }
    }

    pub fn cache(&self) -> &RawCache {
        &self.cache
    }

    /// The body of `request` under `policy`.
    pub async fn get_body(&self, request: &RawRequest, policy: CachePolicy) -> Result<Vec<u8>> {
        if policy != CachePolicy::Network {
            let cache = self.cache.clone();
            let req = request.clone();
            let cached = tokio::task::spawn_blocking(move || cache.read_latest(&req)).await.map_err(|e| Error::InvalidOperation(e.to_string()))??;
            if let Some(body) = cached {
                return Ok(body);
            }
        }
        if policy == CachePolicy::CacheOnly {
            return Err(Error::NotCached(Box::new(request.clone())));
        }

        let started = std::time::Instant::now();
        let response = self.http.send(request).await?;
        let cache = self.cache.clone();
        let req = request.clone();
        let body = response.body;
        let status = i32::from(response.status);
        let content_type = response.content_type;
        let (entry, body) =
            tokio::task::spawn_blocking(move || cache.store(&req, &body, status, content_type.as_deref(), Utc::now()).map(|entry| (entry, body)))
                .await
                .map_err(|e| Error::InvalidOperation(e.to_string()))??;

        tracing::debug!(
            source = %request.source,
            endpoint = %request.endpoint,
            params = %request.params_key,
            status = entry.status_code,
            bytes = entry.length,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "fetched"
        );

        if !entry.is_success() {
            return Err(Error::SourceRequest(Box::new(entry)));
        }
        Ok(body)
    }

    /// Synchronous cache read (replays and rebuilds read straight from disk on worker threads).
    pub fn get_cached(&self, request: &RawRequest) -> Result<Vec<u8>> {
        self.cache.read_latest(request)?.ok_or_else(|| Error::NotCached(Box::new(request.clone())))
    }
}

/// .NET `Uri.EscapeDataString`: percent-encodes everything except RFC 3986 unreserved characters (uppercase hex).
pub fn escape_data_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 3);
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(char::from(*byte)),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_data_string_matches_dotnet() {
        assert_eq!("%D9%81%D9%88%D9%84%D8%A7%D8%AF", escape_data_string("فولاد"));
        assert_eq!("paperTypes%5B0%5D", escape_data_string("paperTypes[0]"));
        assert_eq!("a%20b%26c", escape_data_string("a b&c"));
    }
}
