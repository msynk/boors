//! Browser-like HTTP client with the same resilience pipeline as the .NET collector
//! (Microsoft.Extensions.Http.Resilience / Polly v8), outermost first:
//!
//! 1. total timeout (5 min),
//! 2. retry: exponential backoff with jitter, base 2 s, 4 retries, on transient failures plus 403 (and 429),
//! 3. circuit breaker: ≥ 50 % failures over 1 min with ≥ 10 calls → open for 1 min,
//! 4. token-bucket rate limiter per source (2/s TSETMC, one per 2 s Codal, 1/s others),
//! 5. per-attempt timeout (90 s).

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

use crate::options::MarketDataOptions;
use crate::raw_cache::RawRequest;
use crate::{Error, Result};

/// One HTTP answer that made it through the pipeline.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    retry_after: Option<Duration>,
}

/// Token bucket with discrete replenishment (System.Threading.RateLimiting `TokenBucketRateLimiter`).
#[derive(Debug)]
pub struct TokenBucket {
    permits: u32,
    period: Duration,
    state: tokio::sync::Mutex<(u32, tokio::time::Instant)>,
}

impl TokenBucket {
    /// Rates below 1/s become one permit per (1 / rate) seconds; rates of 1/s and above become N permits per second.
    pub fn for_rate(requests_per_second: f64) -> Self {
        let (permits, period) = if requests_per_second >= 1.0 {
            (requests_per_second.round().max(1.0) as u32, Duration::from_secs(1))
        } else {
            (1, Duration::from_secs_f64(1.0 / requests_per_second.max(0.01)))
        };
        Self { permits, period, state: tokio::sync::Mutex::new((permits, tokio::time::Instant::now())) }
    }

    /// Waits for a permit (FIFO: the tokio mutex is fair).
    pub async fn acquire(&self) {
        let mut state = self.state.lock().await;
        loop {
            let now = tokio::time::Instant::now();
            let elapsed = now.saturating_duration_since(state.1);
            if elapsed >= self.period {
                let periods = (elapsed.as_secs_f64() / self.period.as_secs_f64()).floor() as u32;
                state.0 = state.0.saturating_add(periods.saturating_mul(self.permits)).min(self.permits);
                state.1 += self.period * periods;
            }
            if state.0 > 0 {
                state.0 -= 1;
                return;
            }
            let wait = (state.1 + self.period).saturating_duration_since(now);
            tokio::time::sleep(wait.max(Duration::from_millis(1))).await;
        }
    }
}

/// Advanced circuit breaker (Polly `CircuitBreakerStrategy`): failure ratio over a sampling window.
#[derive(Debug)]
pub struct CircuitBreaker {
    failure_ratio: f64,
    minimum_throughput: usize,
    sampling: Duration,
    break_duration: Duration,
    state: Mutex<BreakerState>,
}

#[derive(Debug, Default)]
struct BreakerState {
    samples: VecDeque<(Instant, bool)>,
    open_until: Option<Instant>,
    half_open_trial: bool,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            failure_ratio: 0.5,
            minimum_throughput: 10,
            sampling: Duration::from_secs(60),
            break_duration: Duration::from_secs(60),
            state: Mutex::new(BreakerState::default()),
        }
    }

    /// `Err` while the circuit is open (C# `BrokenCircuitException`).
    fn before_call(&self, name: &str) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(until) = state.open_until {
            if Instant::now() < until || state.half_open_trial {
                return Err(Error::Rejected(format!("The circuit is now open and is not allowing calls ({name}).")));
            }
            // Half-open: let one trial call through.
            state.half_open_trial = true;
        }
        Ok(())
    }

    fn record(&self, success: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if state.half_open_trial {
            state.half_open_trial = false;
            if success {
                state.open_until = None;
                state.samples.clear();
            } else {
                state.open_until = Some(now + self.break_duration);
            }
            return;
        }

        state.samples.push_back((now, success));
        while state.samples.front().is_some_and(|(t, _)| now.saturating_duration_since(*t) > self.sampling) {
            state.samples.pop_front();
        }
        let total = state.samples.len();
        let failures = state.samples.iter().filter(|(_, ok)| !ok).count();
        if total >= self.minimum_throughput && failures as f64 / total as f64 >= self.failure_ratio {
            state.open_until = Some(now + self.break_duration);
            state.samples.clear();
        }
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP client of one source.
#[derive(Debug)]
pub struct SourceHttp {
    name: String,
    client: reqwest::Client,
    limiter: TokenBucket,
    breaker: CircuitBreaker,
    attempt_timeout: Duration,
    total_timeout: Duration,
    max_retries: u32,
    base_delay: Duration,
}

impl SourceHttp {
    pub fn new(name: &str, options: &MarketDataOptions, requests_per_second: f64) -> Result<Self> {
        let mut headers = HeaderMap::new();
        let mut add = |name: &'static str, value: &str| {
            if let Ok(value) = HeaderValue::from_str(value) {
                headers.insert(HeaderName::from_static(name), value);
            }
        };
        add("user-agent", &options.user_agent);
        add("accept", "application/json, text/plain, */*");
        add("accept-language", "fa-IR,fa;q=0.9,en;q=0.8");
        add("referer", "https://www.tsetmc.com/");
        add("origin", "https://www.tsetmc.com");

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .pool_idle_timeout(Duration::from_secs(90))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Http(format!("Could not create the {name} HTTP client: {e}")))?;

        Ok(Self {
            name: name.to_string(),
            client,
            limiter: TokenBucket::for_rate(requests_per_second),
            breaker: CircuitBreaker::new(),
            attempt_timeout: options.attempt_timeout,
            total_timeout: options.total_timeout,
            max_retries: options.max_retry_attempts,
            base_delay: Duration::from_secs(2),
        })
    }

    /// Sends the request through the whole pipeline. A final non-success status is returned as a response (the
    /// caller stores it in the raw cache); network failures, timeouts and an open circuit are errors.
    pub async fn send(&self, request: &RawRequest) -> Result<HttpResponse> {
        match tokio::time::timeout(self.total_timeout, self.with_retries(request)).await {
            Ok(result) => result,
            Err(_) => Err(Error::Rejected(format!(
                "The operation didn't complete within the allowed timeout of {} s ({} {}).",
                self.total_timeout.as_secs(),
                request.source,
                request.endpoint
            ))),
        }
    }

    async fn with_retries(&self, request: &RawRequest) -> Result<HttpResponse> {
        let mut attempt: u32 = 0;
        loop {
            let outcome = self.attempt(request).await;
            let retry = match &outcome {
                Ok(response) => is_transient_status(response.status) || response.status == 403,
                Err(Error::Http(_)) => true,
                Err(Error::Rejected(message)) => message.starts_with("Attempt timed out"),
                Err(_) => false,
            };
            if !retry || attempt >= self.max_retries {
                return outcome;
            }

            let retry_after = outcome.as_ref().ok().and_then(|r| r.retry_after);
            let delay = retry_after.unwrap_or_else(|| backoff_with_jitter(self.base_delay, attempt));
            tracing::debug!(source = %self.name, endpoint = %request.endpoint, attempt, delay_ms = delay.as_millis() as u64, "retrying");
            tokio::time::sleep(delay).await;
            attempt += 1;
        }
    }

    async fn attempt(&self, request: &RawRequest) -> Result<HttpResponse> {
        self.breaker.before_call(&self.name)?;
        self.limiter.acquire().await;

        let result = match tokio::time::timeout(self.attempt_timeout, self.send_once(request)).await {
            Ok(result) => result,
            Err(_) => Err(Error::Rejected(format!("Attempt timed out after {} s ({} {}).", self.attempt_timeout.as_secs(), request.source, request.endpoint))),
        };

        let success = match &result {
            Ok(response) => !is_transient_status(response.status),
            Err(_) => false,
        };
        self.breaker.record(success);
        result
    }

    async fn send_once(&self, request: &RawRequest) -> Result<HttpResponse> {
        let mut builder = match &request.json_body {
            Some(body) => self.client.post(&request.url).header("content-type", "application/json; charset=utf-8").body(body.clone()),
            None => self.client.get(&request.url),
        };
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }

        let response = builder.send().await.map_err(|e| Error::Http(describe(&e)))?;
        let status = response.status().as_u16();
        let content_type = response.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(str::to_string);
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<u64>().ok())
            .map(Duration::from_secs);
        let body = response.bytes().await.map_err(|e| Error::Http(describe(&e)))?.to_vec();
        Ok(HttpResponse { status, content_type, body, retry_after })
    }
}

/// Polly's `HttpClientResiliencePredicates.IsTransient` status codes: 5xx, 408 and 429.
fn is_transient_status(status: u16) -> bool {
    status >= 500 || status == 408 || status == 429
}

/// Exponential backoff with jitter: base × 2^attempt, randomized between 50 % and 150 %.
fn backoff_with_jitter(base: Duration, attempt: u32) -> Duration {
    let exponential = base.as_secs_f64() * 2f64.powi(attempt as i32);
    let jitter = 0.5 + rand::random::<f64>();
    Duration::from_secs_f64((exponential * jitter).min(60.0))
}

fn describe(error: &reqwest::Error) -> String {
    let mut message = error.to_string();
    let mut source = std::error::Error::source(error);
    while let Some(inner) = source {
        message.push_str(": ");
        message.push_str(&inner.to_string());
        source = inner.source();
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaker_opens_after_half_of_ten_calls_fail() {
        let breaker = CircuitBreaker::new();
        for i in 0..10 {
            assert!(breaker.before_call("t").is_ok());
            breaker.record(i % 2 == 0);
        }
        assert!(breaker.before_call("t").is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn token_bucket_spaces_requests() {
        let bucket = TokenBucket::for_rate(2.0);
        let start = tokio::time::Instant::now();
        for _ in 0..4 {
            bucket.acquire().await;
        }
        assert!(tokio::time::Instant::now() - start >= Duration::from_millis(900));
    }
}
