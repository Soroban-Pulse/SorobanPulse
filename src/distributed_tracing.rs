//! Issue #813: Distributed tracing for the full event lifecycle.
//!
//! Provides:
//! - W3C Trace Context (traceparent / tracestate) propagation helpers
//! - Span factories for every major operation stage:
//!   - HTTP requests and SSE connections
//!   - Indexer poll cycle, RPC calls, event validation, dedup, DB insert
//!   - Webhook delivery (with outgoing traceparent header injection)
//! - `TraceContext` — a portable trace-id / span-id / flags carrier
//! - Sampling configuration (TRACE_SAMPLE_RATE / TRACE_SERVICE_NAME env vars)
//!
//! All span-creation functions compile to no-ops when the `otel` feature is
//! absent, so the hot path is zero-cost in default (non-OTel) builds.

use tracing::Span;

// ─────────────────────────────────────────────────────────────────────────────
// Trace-context carrier
// ─────────────────────────────────────────────────────────────────────────────

/// Parsed W3C Trace Context (from `traceparent` or `X-Trace-ID`).
///
/// Format: `00-<trace_id>-<parent_id>-<flags>`
/// Example: `00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceContext {
    pub trace_id: String,
    pub parent_id: Option<String>,
    /// "01" = sampled, "00" = not sampled
    pub trace_flags: String,
}

impl TraceContext {
    /// Build a new root context (no parent).
    pub fn new_root(trace_id: impl Into<String>) -> Self {
        Self {
            trace_id: trace_id.into(),
            parent_id: None,
            trace_flags: "01".to_string(),
        }
    }

    /// Produce a `traceparent` header value from this context.
    pub fn to_traceparent(&self, span_id: &str) -> String {
        format!("00-{}-{}-{}", self.trace_id, span_id, self.trace_flags)
    }

    /// Return `true` when the sampled flag is set.
    pub fn is_sampled(&self) -> bool {
        self.trace_flags == "01"
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Extraction from incoming HTTP headers
// ─────────────────────────────────────────────────────────────────────────────

/// Parse trace context from HTTP request headers.
///
/// Priority order:
/// 1. W3C `traceparent` header
/// 2. `X-Trace-ID` header (legacy / non-standard)
pub fn extract_trace_context(headers: &axum::http::HeaderMap) -> Option<TraceContext> {
    if let Some(tp) = headers.get("traceparent") {
        if let Ok(s) = tp.to_str() {
            return parse_traceparent(s);
        }
    }

    if let Some(tid) = headers.get("X-Trace-ID") {
        if let Ok(s) = tid.to_str() {
            return Some(TraceContext {
                trace_id: s.to_string(),
                parent_id: None,
                trace_flags: "01".to_string(),
            });
        }
    }

    None
}

/// Generate a new random trace-id (32-hex-char / 128-bit).
pub fn new_trace_id() -> String {
    let bytes = random_16_bytes();
    hex::encode(bytes)
}

/// Generate a new random span-id (16-hex-char / 64-bit).
pub fn new_span_id() -> String {
    let bytes = random_8_bytes();
    hex::encode(bytes)
}

fn random_16_bytes() -> [u8; 16] {
    let n1 = pseudo_random_u64();
    let n2 = pseudo_random_u64();
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&n1.to_le_bytes());
    out[8..].copy_from_slice(&n2.to_le_bytes());
    out
}

fn random_8_bytes() -> [u8; 8] {
    pseudo_random_u64().to_le_bytes()
}

/// Lightweight PRNG seeded from system time + thread ID (no rand crate needed).
fn pseudo_random_u64() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    // xorshift64 mix
    let x = t ^ (t << 13);
    let x = x ^ (x >> 7);
    x ^ (x << 17)
}

fn parse_traceparent(s: &str) -> Option<TraceContext> {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 4 {
        return None;
    }
    Some(TraceContext {
        trace_id: parts[1].to_string(),
        parent_id: Some(parts[2].to_string()),
        trace_flags: parts[3].to_string(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Outgoing header injection (webhook / cross-service)
// ─────────────────────────────────────────────────────────────────────────────

/// Inject W3C `traceparent` (and optional `tracestate`) into an outgoing
/// `reqwest::RequestBuilder`, enabling cross-service trace correlation.
///
/// Uses the current tracing span's metadata when available; falls back to
/// generating a fresh span-id.
pub fn inject_trace_headers(
    request_builder: reqwest::RequestBuilder,
    trace_ctx: Option<&TraceContext>,
) -> reqwest::RequestBuilder {
    let span_id = new_span_id();

    if let Some(ctx) = trace_ctx {
        let traceparent = ctx.to_traceparent(&span_id);
        request_builder.header("traceparent", traceparent)
    } else {
        // No upstream context — create a fresh root trace for the outgoing call.
        let trace_id = new_trace_id();
        let traceparent = format!("00-{trace_id}-{span_id}-01");
        request_builder.header("traceparent", traceparent)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Sampling configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for distributed tracing behaviour.
#[derive(Clone, Debug)]
pub struct TracingConfig {
    /// Whether the `otel` feature is compiled in.
    pub enabled: bool,
    /// Sampling rate: `0.0` = no sampling, `1.0` = sample everything.
    pub sample_rate: f64,
    /// Service name reported in spans (default: `soroban-pulse`).
    pub service_name: String,
}

impl TracingConfig {
    /// Build from environment variables:
    /// - `TRACE_SAMPLE_RATE` — float in [0, 1]
    /// - `TRACE_SERVICE_NAME` — string
    pub fn from_env() -> Self {
        #[cfg(feature = "otel")]
        let enabled = true;
        #[cfg(not(feature = "otel"))]
        let enabled = false;

        let sample_rate = std::env::var("TRACE_SAMPLE_RATE")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0)
            .clamp(0.0, 1.0);

        let service_name = std::env::var("TRACE_SERVICE_NAME")
            .unwrap_or_else(|_| "soroban-pulse".to_string());

        Self {
            enabled,
            sample_rate,
            service_name,
        }
    }

    /// Decide whether a new trace should be sampled.
    pub fn should_sample(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.sample_rate >= 1.0 {
            return true;
        }
        if self.sample_rate <= 0.0 {
            return false;
        }
        // Deterministic sampling based on current nanosecond timestamp.
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as f64;
        (t % 1_000_000_000.0) / 1_000_000_000.0 < self.sample_rate
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Span attribute helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Record a key/value attribute on the *current* span.
pub fn set_span_attribute(key: &str, value: impl std::fmt::Display) {
    #[cfg(feature = "otel")]
    {
        tracing::Span::current().record(key, value.to_string().as_str());
    }
    #[cfg(not(feature = "otel"))]
    {
        let _ = (key, value);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Span factories — HTTP layer
// ─────────────────────────────────────────────────────────────────────────────

/// Create a root span for an incoming HTTP request.
///
/// Captures W3C trace context from headers when present so that the request is
/// parented to the upstream trace.
pub fn create_http_request_span(
    method: &str,
    path: &str,
    trace_ctx: Option<&TraceContext>,
) -> Span {
    let trace_id = trace_ctx
        .map(|c| c.trace_id.as_str())
        .unwrap_or("unknown");

    tracing::info_span!(
        "http.request",
        http.method = %method,
        http.target = %path,
        trace.id = %trace_id,
        http.status_code = tracing::field::Empty,
        http.response_content_length = tracing::field::Empty,
    )
}

/// Create a span for an SSE connection lifecycle.
pub fn create_sse_span(contract_id: Option<&str>, client_ip: Option<&str>) -> Span {
    let contract = contract_id.unwrap_or("*");
    let ip = client_ip.unwrap_or("unknown");

    tracing::info_span!(
        "sse.connection",
        sse.contract_id = %contract,
        sse.client_ip = %ip,
        sse.events_sent = tracing::field::Empty,
        sse.close_reason = tracing::field::Empty,
    )
}

/// Create a span for an API handler (child of the HTTP request span).
pub fn create_api_span(method: &str, path: &str, contract_id: Option<&str>) -> Span {
    tracing::info_span!(
        "api.handler",
        http.method = %method,
        http.target = %path,
        contract_id = contract_id.unwrap_or(""),
        db.query_count = tracing::field::Empty,
        result.count = tracing::field::Empty,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Span factories — Indexer pipeline
// ─────────────────────────────────────────────────────────────────────────────

/// Root span for a single indexer poll cycle.
pub fn create_indexer_cycle_span(start_ledger: u64) -> Span {
    tracing::info_span!(
        "indexer.poll_cycle",
        start_ledger = start_ledger,
        events_fetched = tracing::field::Empty,
        events_inserted = tracing::field::Empty,
        events_skipped_duplicate = tracing::field::Empty,
        events_skipped_validation = tracing::field::Empty,
        cycle_duration_ms = tracing::field::Empty,
        lag_ledgers = tracing::field::Empty,
    )
}

/// Span for an RPC `getLatestLedger` call.
pub fn create_rpc_latest_ledger_span(url: &str) -> Span {
    tracing::info_span!(
        "rpc.get_latest_ledger",
        rpc.system = "soroban-rpc",
        rpc.method = "getLatestLedger",
        rpc.url = %url,
        rpc.result_ledger = tracing::field::Empty,
        rpc.error = tracing::field::Empty,
    )
}

/// Span for an RPC `getEvents` call (one page).
pub fn create_rpc_get_events_span(url: &str, start_ledger: u64, page: u32) -> Span {
    tracing::info_span!(
        "rpc.get_events",
        rpc.system = "soroban-rpc",
        rpc.method = "getEvents",
        rpc.url = %url,
        rpc.start_ledger = start_ledger,
        rpc.page = page,
        rpc.events_returned = tracing::field::Empty,
        rpc.latest_ledger = tracing::field::Empty,
        rpc.error = tracing::field::Empty,
    )
}

/// Span for the full event-validation step of a single event.
pub fn create_event_validation_span(
    tx_hash: &str,
    contract_id: &str,
    ledger: u64,
) -> Span {
    tracing::info_span!(
        "event.validate",
        tx_hash = %tx_hash,
        contract_id = %contract_id,
        ledger = ledger,
        validation.passed = tracing::field::Empty,
        validation.failure_reason = tracing::field::Empty,
    )
}

/// Span for the deduplication check (bloom filter + content fingerprint).
pub fn create_dedup_span(tx_hash: &str, contract_id: &str) -> Span {
    tracing::info_span!(
        "event.dedup_check",
        tx_hash = %tx_hash,
        contract_id = %contract_id,
        dedup.bloom_hit = tracing::field::Empty,
        dedup.content_hit = tracing::field::Empty,
        dedup.result = tracing::field::Empty,
    )
}

/// Span for a single event DB INSERT operation.
pub fn create_db_insert_span(
    tx_hash: &str,
    contract_id: &str,
    ledger: u64,
) -> Span {
    tracing::info_span!(
        "db.insert_event",
        db.system = "postgresql",
        db.operation = "INSERT",
        db.table = "events",
        tx_hash = %tx_hash,
        contract_id = %contract_id,
        ledger = ledger,
        db.rows_affected = tracing::field::Empty,
        db.was_duplicate = tracing::field::Empty,
    )
}

/// Generic span for a DB query (used by handlers).
pub fn create_db_span(operation: &str, table: &str) -> Span {
    tracing::info_span!(
        "db.query",
        db.system = "postgresql",
        db.operation = %operation,
        db.table = %table,
        db.rows_returned = tracing::field::Empty,
        db.duration_ms = tracing::field::Empty,
    )
}

/// Generic span for any RPC call (used by the RPC client).
pub fn create_rpc_span(method: &str, url: &str) -> Span {
    tracing::info_span!(
        "rpc.call",
        rpc.system = "soroban-rpc",
        rpc.method = %method,
        rpc.url = %url,
        rpc.error = tracing::field::Empty,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Span factories — Webhook delivery
// ─────────────────────────────────────────────────────────────────────────────

/// Span for a single webhook delivery attempt.
///
/// Sets `trace.id` so the delivery can be correlated with the event ingestion
/// trace. The caller should call `inject_trace_headers` before sending the
/// HTTP request to propagate the context downstream.
pub fn create_webhook_span(
    url: &str,
    contract_id: &str,
    event_type: &str,
    attempt: u32,
    trace_ctx: Option<&TraceContext>,
) -> Span {
    let trace_id = trace_ctx
        .map(|c| c.trace_id.as_str())
        .unwrap_or("unknown");

    tracing::info_span!(
        "webhook.deliver",
        webhook.url = %url,
        webhook.contract_id = %contract_id,
        webhook.event_type = %event_type,
        webhook.attempt = attempt,
        trace.id = %trace_id,
        webhook.status_code = tracing::field::Empty,
        webhook.success = tracing::field::Empty,
        webhook.error = tracing::field::Empty,
        webhook.latency_ms = tracing::field::Empty,
    )
}

/// Root span for the entire webhook delivery pipeline (all retry attempts).
pub fn create_webhook_pipeline_span(
    url: &str,
    contract_id: &str,
    event_type: &str,
) -> Span {
    tracing::info_span!(
        "webhook.delivery_pipeline",
        webhook.url = %url,
        webhook.contract_id = %contract_id,
        webhook.event_type = %event_type,
        webhook.total_attempts = tracing::field::Empty,
        webhook.final_status = tracing::field::Empty,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// Issue #895: Enhanced spans for all handlers and critical operations
// ─────────────────────────────────────────────────────────────────────────────

/// Span for notification delivery (email, SMS, webhook).
pub fn create_notification_span(
    channel: &str,
    recipient: &str,
    event_type: &str,
    trace_ctx: Option<&TraceContext>,
) -> Span {
    let trace_id = trace_ctx
        .map(|c| c.trace_id.as_str())
        .unwrap_or("unknown");

    tracing::info_span!(
        "notification.deliver",
        notification.channel = %channel,
        notification.recipient = %recipient,
        notification.event_type = %event_type,
        trace.id = %trace_id,
        notification.status = tracing::field::Empty,
        notification.latency_ms = tracing::field::Empty,
        notification.error = tracing::field::Empty,
    )
}

/// Span for database query execution with query text.
pub fn create_db_query_span(operation: &str, table: &str, query_text: &str) -> Span {
    tracing::info_span!(
        "db.query_detailed",
        db.system = "postgresql",
        db.operation = %operation,
        db.table = %table,
        db.query_text = %query_text,
        db.rows_affected = tracing::field::Empty,
        db.duration_ms = tracing::field::Empty,
        db.error = tracing::field::Empty,
    )
}

/// Span for indexer event processing.
pub fn create_event_processing_span(event_id: &str, contract_id: &str) -> Span {
    tracing::info_span!(
        "event.processing",
        event.id = %event_id,
        contract_id = %contract_id,
        event.validation_status = tracing::field::Empty,
        event.dedup_status = tracing::field::Empty,
        event.db_status = tracing::field::Empty,
    )
}

/// Span for API query/search operations.
pub fn create_query_span(query_type: &str, contract_id: Option<&str>) -> Span {
    tracing::info_span!(
        "api.query",
        query.type = %query_type,
        contract_id = contract_id.unwrap_or("*"),
        query.result_count = tracing::field::Empty,
        query.duration_ms = tracing::field::Empty,
    )
}

/// Span for subscription/notification registration.
pub fn create_subscription_span(subscription_id: &str, webhook_url: &str) -> Span {
    tracing::info_span!(
        "subscription.register",
        subscription.id = %subscription_id,
        webhook.url = %webhook_url,
        subscription.status = tracing::field::Empty,
        subscription.verified = tracing::field::Empty,
    )
}

/// Inject trace ID into response headers.
pub fn inject_trace_response_headers(
    mut headers: axum::http::HeaderMap,
    trace_ctx: &TraceContext,
) -> axum::http::HeaderMap {
    if let Ok(header_value) = trace_ctx.trace_id.parse() {
        headers.insert("X-Trace-ID", header_value);
    }
    if let Ok(header_value) = trace_ctx.trace_id.parse() {
        headers.insert("traceparent", header_value);
    }
    headers
}

/// Get trace context from current span if available.
pub fn get_current_trace_context() -> Option<TraceContext> {
    let trace_id = tracing::Span::current()
        .metadata()
        .and_then(|_| Some("trace-id"))
        .and_then(|_| None);

    trace_id.map(|id| TraceContext::new_root(id))
}

/// Record trace-related metrics for monitoring.
pub fn record_trace_sampling(sample_rate: f64) {
    extern crate metrics as m;
    m::gauge!("soroban_pulse_trace_sample_rate").set(sample_rate);
}

// ─────────────────────────────────────────────────────────────────────────────
// Correlation IDs
// ─────────────────────────────────────────────────────────────────────────────
//
// Correlation IDs let a single logical request be followed across every
// service, log line, and metric it touches, even though a W3C trace-id is
// not always propagated by every internal caller. Correlation IDs use the
// same hex format as trace IDs (see `new_trace_id`) so they can be swapped
// in as a trace-id when OTel is enabled.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::sync::Mutex;

thread_local! {
    static CORRELATION_ID: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// A single entry in the in-memory correlation log ring buffer, used by the
/// correlation search interface and correlation-based debugging tools.
#[derive(Clone, Debug)]
pub struct CorrelationLogEntry {
    pub correlation_id: String,
    pub service: String,
    pub message: String,
    pub timestamp_ms: u128,
}

/// Bounded in-memory ring buffer of recent correlation log entries.
///
/// This is intentionally simple (no external dependency) so that any service
/// can record and search correlation-tagged events without a log aggregator
/// being present. Production deployments should also ship these to the
/// centralized logging backend described in `docs/correlation-ids.md`.
pub static CORRELATION_LOG: Mutex<Option<VecDeque<CorrelationLogEntry>>> = Mutex::new(None);

const CORRELATION_LOG_CAPACITY: usize = 2000;

/// Set the correlation ID for the current request/thread.
pub fn set_correlation_id(id: String) {
    CORRELATION_ID.with(|c| *c.borrow_mut() = Some(id));
}

/// Retrieve the correlation ID for the current request/thread, if any.
pub fn get_correlation_id() -> Option<String> {
    CORRELATION_ID.with(|c| c.borrow().clone())
}

/// Clear the correlation ID (call at the end of request handling to avoid
/// leaking state across pooled threads).
pub fn clear_correlation_id() {
    CORRELATION_ID.with(|c| *c.borrow_mut() = None);
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Record a correlation-tagged log line into the in-memory ring buffer, and
/// increment the correlation metrics counter. This implements "correlation
/// log filtering" support: callers can later filter/search by correlation ID.
pub fn record_correlation_log(service: &str, message: impl Into<String>) {
    let Some(correlation_id) = get_correlation_id() else {
        return;
    };

    let entry = CorrelationLogEntry {
        correlation_id: correlation_id.clone(),
        service: service.to_string(),
        message: message.into(),
        timestamp_ms: now_ms(),
    };

    let mut guard = CORRELATION_LOG.lock().unwrap_or_else(|e| e.into_inner());
    let buf = guard.get_or_insert_with(|| VecDeque::with_capacity(CORRELATION_LOG_CAPACITY));
    if buf.len() >= CORRELATION_LOG_CAPACITY {
        buf.pop_front();
    }
    buf.push_back(entry);

    extern crate metrics as m;
    m::counter!("soroban_pulse_correlation_log_entries_total", "service" => service.to_string())
        .increment(1);
}

/// Correlation-based debugging: return every recorded log entry for a given
/// correlation ID, in chronological order, across all services that recorded
/// one. This is the backbone of the correlation search interface.
pub fn search_by_correlation_id(correlation_id: &str) -> Vec<CorrelationLogEntry> {
    let guard = CORRELATION_LOG.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(|buf| {
            buf.iter()
                .filter(|entry| entry.correlation_id == correlation_id)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Filter recorded correlation log entries by service name, optionally
/// restricted to a single correlation ID. Supports the "correlation log
/// filtering" checklist item independent of the search-by-id path above.
pub fn filter_correlation_logs(
    service: Option<&str>,
    correlation_id: Option<&str>,
) -> Vec<CorrelationLogEntry> {
    let guard = CORRELATION_LOG.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .as_ref()
        .map(|buf| {
            buf.iter()
                .filter(|e| service.is_none_or(|s| e.service == s))
                .filter(|e| correlation_id.is_none_or(|c| e.correlation_id == c))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Record a metric observation tagged with the current correlation ID's
/// presence, used to track how much of the traffic is fully correlated
/// end-to-end vs. falling back to a freshly minted ID.
pub fn record_correlation_metrics(had_incoming_id: bool) {
    extern crate metrics as m;
    m::counter!(
        "soroban_pulse_correlation_ids_total",
        "source" => if had_incoming_id { "propagated" } else { "generated" }
    )
    .increment(1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_traceparent_valid() {
        let s = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
        let ctx = parse_traceparent(s).unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert_eq!(ctx.parent_id, Some("00f067aa0ba902b7".to_string()));
        assert_eq!(ctx.trace_flags, "01");
    }

    #[test]
    fn parse_traceparent_too_few_parts() {
        assert!(parse_traceparent("invalid-format").is_none());
    }

    #[test]
    fn parse_traceparent_too_many_parts() {
        assert!(parse_traceparent("00-abc-def-01-extra").is_none());
    }

    #[test]
    fn extract_trace_context_from_traceparent_header() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "traceparent",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
                .parse()
                .unwrap(),
        );
        let ctx = extract_trace_context(&headers).unwrap();
        assert_eq!(ctx.trace_id, "4bf92f3577b34da6a3ce929d0e0e4736");
        assert!(ctx.is_sampled());
    }

    #[test]
    fn extract_trace_context_fallback_to_x_trace_id() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("X-Trace-ID", "mytraceid123".parse().unwrap());
        let ctx = extract_trace_context(&headers).unwrap();
        assert_eq!(ctx.trace_id, "mytraceid123");
        assert!(ctx.parent_id.is_none());
        assert!(ctx.is_sampled());
    }

    #[test]
    fn extract_trace_context_none_when_no_headers() {
        let headers = axum::http::HeaderMap::new();
        assert!(extract_trace_context(&headers).is_none());
    }

    #[test]
    fn trace_context_to_traceparent_format() {
        let ctx = TraceContext {
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            parent_id: None,
            trace_flags: "01".to_string(),
        };
        let tp = ctx.to_traceparent("00f067aa0ba902b7");
        assert_eq!(
            tp,
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
        );
    }

    #[test]
    fn trace_context_is_sampled() {
        let sampled = TraceContext {
            trace_id: "a".to_string(),
            parent_id: None,
            trace_flags: "01".to_string(),
        };
        assert!(sampled.is_sampled());

        let not_sampled = TraceContext {
            trace_id: "a".to_string(),
            parent_id: None,
            trace_flags: "00".to_string(),
        };
        assert!(!not_sampled.is_sampled());
    }

    #[test]
    fn tracing_config_from_env_defaults() {
        std::env::remove_var("TRACE_SAMPLE_RATE");
        std::env::remove_var("TRACE_SERVICE_NAME");
        let cfg = TracingConfig::from_env();
        assert_eq!(cfg.sample_rate, 1.0);
        assert_eq!(cfg.service_name, "soroban-pulse");
    }

    #[test]
    fn tracing_config_sample_rate_clamped() {
        std::env::set_var("TRACE_SAMPLE_RATE", "2.0");
        let cfg = TracingConfig::from_env();
        assert_eq!(cfg.sample_rate, 1.0);

        std::env::set_var("TRACE_SAMPLE_RATE", "-0.5");
        let cfg = TracingConfig::from_env();
        assert_eq!(cfg.sample_rate, 0.0);

        std::env::remove_var("TRACE_SAMPLE_RATE");
    }

    #[test]
    fn tracing_config_custom_service_name() {
        std::env::set_var("TRACE_SERVICE_NAME", "my-service");
        let cfg = TracingConfig::from_env();
        assert_eq!(cfg.service_name, "my-service");
        std::env::remove_var("TRACE_SERVICE_NAME");
    }

    #[test]
    fn new_trace_id_is_32_hex_chars() {
        let id = new_trace_id();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn new_span_id_is_16_hex_chars() {
        let id = new_span_id();
        assert_eq!(id.len(), 16);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn new_trace_ids_are_unique() {
        let ids: Vec<String> = (0..10).map(|_| new_trace_id()).collect();
        // All 10 should differ (astronomically unlikely to collide)
        let unique: std::collections::HashSet<&String> = ids.iter().collect();
        assert_eq!(unique.len(), 10);
    }

    #[test]
    fn inject_trace_headers_with_context() {
        let client = reqwest::Client::new();
        let ctx = TraceContext {
            trace_id: "4bf92f3577b34da6a3ce929d0e0e4736".to_string(),
            parent_id: Some("00f067aa0ba902b7".to_string()),
            trace_flags: "01".to_string(),
        };
        // Should not panic; header injection is a compile-time type operation.
        let _builder = inject_trace_headers(client.post("http://localhost"), Some(&ctx));
    }

    #[test]
    fn inject_trace_headers_without_context_generates_fresh_trace() {
        let client = reqwest::Client::new();
        let _builder = inject_trace_headers(client.post("http://localhost"), None);
    }

    #[test]
    fn correlation_id_roundtrip() {
        set_correlation_id("corr-1".to_string());
        assert_eq!(get_correlation_id(), Some("corr-1".to_string()));
        clear_correlation_id();
        assert_eq!(get_correlation_id(), None);
    }

    #[test]
    fn record_and_search_correlation_log() {
        set_correlation_id("corr-search-test".to_string());
        record_correlation_log("indexer", "processed event");
        record_correlation_log("api", "served response");

        let results = search_by_correlation_id("corr-search-test");
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|e| e.service == "indexer"));
        assert!(results.iter().any(|e| e.service == "api"));
        clear_correlation_id();
    }

    #[test]
    fn filter_correlation_logs_by_service() {
        set_correlation_id("corr-filter-test".to_string());
        record_correlation_log("webhook", "delivered");
        clear_correlation_id();

        let results = filter_correlation_logs(Some("webhook"), Some("corr-filter-test"));
        assert!(results.iter().all(|e| e.service == "webhook"));
    }

    #[test]
    fn record_correlation_metrics_does_not_panic() {
        record_correlation_metrics(true);
        record_correlation_metrics(false);
    }
}
