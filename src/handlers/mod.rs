//! HTTP request handlers, split into per-domain modules (Issues #1239, #1240, #1241).
//!
//! | Module          | Contents                                                        |
//! |-----------------|-----------------------------------------------------------------|
//! | `events`        | Event queries, streaming (SSE/WebSocket), export, search        |
//! | `health`        | Liveness, readiness and dependency health                       |
//! | `subscriptions` | Subscription creation, unsubscribe links, per-IP limits         |
//! | `webhooks`      | Notification channels, email tracking, circuit breakers, DLQ    |
//! | `admin`         | Indexer control, replay, anonymization, archive, config, export |
//! | `metrics`       | Prometheus scrape, status and operational reports               |
//! | `contracts`     | Contract listing/summary, ABI, metadata, schema registry        |
//! | `chain`         | Networks, ledger hashes, cross-chain correlation                |
//! | `docs`          | OpenAPI JSON and Swagger UI                                     |
//! | `common`        | Helpers shared across domains                                   |
//!
//! Every public handler is re-exported here, so `crate::handlers::<name>`
//! paths keep working. Integration handlers live in `crate::integration_handlers`.
//!
//! The imports below are shared with the submodules via `use super::*;`.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use axum::body::Body;
use axum::http::{header, HeaderMap};
use axum::response::sse::{Event, Sse};
use axum::response::Response;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use futures::stream::{self, Stream, StreamExt};
use reqwest;
use secrecy::ExposeSecret;
use regex::Regex;
use serde_json::{json, Value};
use sqlx::{QueryBuilder, Row};
use std::convert::Infallible;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::{info_span, instrument};
use uuid::Uuid;

use crate::{
    error::AppError,
    middleware::TenantId,
    models::{
        self, BatchTxRequest, ContractSummary, ExportParams, PaginationParams, ReplayRequest,
        StreamParams, ContractDetailSummary, ContractSearchParams, ContractSearchResult,
        EventTypeBreakdown, LedgerRange,
    },
    routes::AppState,
    notification_formatter,
    pagerduty,
};
use std::sync::atomic::Ordering;

use crate::anonymization::AnonymizationRuleRequest;

mod admin;
mod chain;
mod common;
mod contracts;
mod docs;
mod events;
mod health;
mod metrics;
mod subscriptions;
mod webhooks;

pub use admin::*;
pub use chain::*;
pub use common::*;
pub use contracts::*;
pub use docs::*;
pub use events::*;
pub use health::*;

/// Execute a future (typically a DB query), record its duration, and warn if it
/// exceeds `threshold_ms`. Returns the future's output unchanged.
async fn timed_query<F, T>(
    fut: F,
    query_type: &str,
    threshold_ms: u64,
    context: Option<&str>,
) -> T
where
    F: std::future::Future<Output = T>,
{
    let start = std::time::Instant::now();
    let result = fut.await;
    let elapsed = start.elapsed();
    crate::metrics::record_query_duration(query_type, elapsed);
    if elapsed.as_millis() as u64 > threshold_ms {
        crate::metrics::record_slow_query(query_type);
        tracing::warn!(
            query_type = %query_type,
            duration_ms = elapsed.as_millis(),
            threshold_ms = threshold_ms,
            context = context.unwrap_or("-"),
            "slow query detected"
        );
    }
    result
}

/// Compute a lightweight ETag from the last event id, created_at, and total count.
/// Uses SHA-256 truncated to 8 bytes, base64-encoded — no double-serialization needed.
const MAX_DATA_PATTERN_LEN: usize = 256;
const MAX_REGEX_PATTERN_LEN: usize = 256;

fn validate_jsonpath_expr(expr: &str) -> Result<(), AppError> {
    if expr.is_empty() || expr.len() > MAX_DATA_PATTERN_LEN {
        return Err(AppError::Validation(
            "data_pattern expression is invalid or too long".to_string(),
        ));
    }

    // The pattern is a compile-time constant; a failure here is a programming
    // error, not input-dependent, so a documented panic is acceptable.
    let path_re = Regex::new(r"^\$(?:\.[A-Za-z_][A-Za-z0-9_]*|\['[A-Za-z0-9_]+\'])*$")
        .expect("JSONPath validator regex is a valid compile-time constant");
    if !path_re.is_match(expr) {
        return Err(AppError::Validation(
            "data_pattern must be a simple JSONPath expression like $.amount".to_string(),
        ));
    }
    Ok(())
}

fn validate_regex_pattern(pattern: &str) -> Result<(), AppError> {
    if pattern.is_empty() || pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(AppError::Validation(
            "pattern is invalid or too complex".to_string(),
        ));
    }
    Regex::new(pattern).map_err(|_| AppError::Validation("pattern is not a valid regular expression".to_string()))?;
    Ok(())
}

/// Fallback for a `Response::builder()` that failed (only possible with an
/// invalid header value, e.g. one derived from user input): log and return 500
/// instead of panicking (issue #1246).
fn response_build_error(e: axum::http::Error) -> Response<Body> {
    tracing::error!(error = %e, "Failed to build HTTP response");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

/// Attach `ETag` + `Cache-Control: no-cache`, skipping an ETag that is not a
/// valid header value rather than panicking.
fn insert_etag_headers(headers: &mut HeaderMap, tag: &str) {
    if let Ok(value) = axum::http::HeaderValue::from_str(tag) {
        headers.insert("ETag", value);
    }
    headers.insert("Cache-Control", axum::http::HeaderValue::from_static("no-cache"));
}

/// Gzip an in-memory buffer. Replaces a `Handle::block_on` + `unwrap()` that
/// panicked when called from inside the async runtime (issue #1246).
async fn compress_gzip(input: &[u8]) -> Result<Vec<u8>, AppError> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    async_compression::tokio::bufread::GzipEncoder::new(input)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "gzip compression failed");
            AppError::Internal("internal server error".to_string())
        })?;
    Ok(buf)
}

/// Brotli-compress an in-memory buffer. See [`compress_gzip`].
async fn compress_brotli(input: &[u8]) -> Result<Vec<u8>, AppError> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::new();
    async_compression::tokio::bufread::BrotliEncoder::new(input)
        .read_to_end(&mut buf)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "brotli compression failed");
            AppError::Internal("internal server error".to_string())
        })?;
    Ok(buf)
}

fn format_jsonpath_filter(path: &str, pattern: &str) -> String {
    let escaped = pattern.replace('\\', "\\\\").replace('"', "\\\"");
    format!("{path} ? (@ like_regex \"{escaped}\")")
}

fn compute_etag(last_id: &Uuid, last_created_at: &DateTime<Utc>, total: Option<i64>) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(last_id.as_bytes());
    h.update(
        last_created_at
            .timestamp_nanos_opt()
            .unwrap_or(0)
            .to_le_bytes(),
    );
    if let Some(t) = total {
        h.update(t.to_le_bytes());
    } else {
        h.update(0i64.to_le_bytes());
    }
    let digest = h.finalize();
    format!("\"{}\"", URL_SAFE_NO_PAD.encode(&digest[..8]))
}

// ── Issue #1268: per-IP rate limiting for subscriptions and SSE ─────────────

/// Maximum number of `POST /v1/subscriptions` requests allowed per IP per minute.
pub const SUBSCRIPTION_RATE_LIMIT_PER_MINUTE: usize = 5;

/// Maximum number of concurrent SSE connections allowed per IP.
pub const SSE_MAX_CONNECTIONS_PER_IP: usize = 5;

/// Per-IP sliding-window limiter for webhook subscription registration.
///
/// Tracks request timestamps per client IP in a `DashMap`; requests older than
/// one minute are pruned on each check. When the limit is exceeded the handler
/// returns `429 Too Many Requests` with a `Retry-After` header.
pub struct SubscriptionRateLimiter {
    hits: dashmap::DashMap<String, Vec<std::time::Instant>>,
    limit: usize,
    window: std::time::Duration,
}

impl SubscriptionRateLimiter {
    pub fn new(limit: usize, window: std::time::Duration) -> Self {
        Self {
            hits: dashmap::DashMap::new(),
            limit,
            window,
        }
    }

    /// Record a hit for `ip`. Returns `Ok(())` when allowed, or `Err(retry_after_secs)`
    /// when the per-IP limit has been exceeded.
    pub fn check(&self, ip: &str) -> Result<(), u64> {
        let now = std::time::Instant::now();
        let mut entry = self.hits.entry(ip.to_string()).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.limit {
            // `entry.len() >= self.limit` and `limit >= 1` guarantee a first element.
            let oldest = entry.first().copied().unwrap_or(now);
            let elapsed = now.duration_since(oldest);
            let retry_after = self.window.saturating_sub(elapsed).as_secs().max(1);
            return Err(retry_after);
        }
        entry.push(now);
        Ok(())
    }
}

/// Per-IP concurrent-connection limiter for SSE endpoints.
///
/// Uses a `DashMap` of active connection counts. A guard is returned on success
/// and decrements the counter when dropped, so disconnects free the slot.
pub struct SseConnectionLimiter {
    active: dashmap::DashMap<String, usize>,
    limit: usize,
}

/// RAII guard that releases an SSE connection slot on drop.
pub struct SseConnectionGuard {
    limiter: Arc<SseConnectionLimiter>,
    ip: String,
}

impl Drop for SseConnectionGuard {
    fn drop(&mut self) {
        if let Some(mut count) = self.limiter.active.get_mut(&self.ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                drop(count);
                self.limiter.active.remove(&self.ip);
            }
        }
    }
}

impl SseConnectionLimiter {
    pub fn new(limit: usize) -> Self {
        Self {
            active: dashmap::DashMap::new(),
            limit,
        }
    }

    /// Try to acquire a connection slot for `ip`.
    pub fn try_acquire(self: &Arc<Self>, ip: &str) -> Option<SseConnectionGuard> {
        let mut entry = self.active.entry(ip.to_string()).or_insert(0);
        if *entry >= self.limit {
            return None;
        }
        *entry += 1;
        drop(entry);
        Some(SseConnectionGuard {
            limiter: Arc::clone(self),
            ip: ip.to_string(),
        })
    }
}

/// Extract the client IP from forwarding headers, falling back to `"unknown"`.
fn client_ip_from_headers(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|h| h.to_str().ok())
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Build a `429 Too Many Requests` response with a `Retry-After` header.
fn too_many_requests(retry_after_secs: u64, message: &str) -> Response {
    let mut response = Response::new(Body::from(
        json!({ "error": message, "retry_after": retry_after_secs }).to_string(),
    ));
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    let headers = response.headers_mut();
    headers.insert(header::RETRY_AFTER, header::HeaderValue::from(retry_after_secs));
    headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
    response
}

/// Simple in-process cache entry for the contracts list.
struct CacheEntry {
    data: Value,
    expires_at: std::time::Instant,
}

static CONTRACTS_CACHE: OnceLock<Mutex<Option<CacheEntry>>> = OnceLock::new();

fn contracts_cache() -> &'static Mutex<Option<CacheEntry>> {
    CONTRACTS_CACHE.get_or_init(|| Mutex::new(None))
}

static EVENTS_FEED_CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();

/// Extract the tenant_id from request extensions when multi-tenant mode is active.
/// Returns `None` in single-tenant mode (extension not present).
fn extract_tenant_id(extensions: &axum::http::Extensions) -> Option<&str> {
    extensions.get::<TenantId>().map(|t| t.0.as_str())
}

/// Append a `tenant_id = $N` condition when multi-tenant mode is active.
/// Returns the bind index incremented by 1 if a condition was added.
fn maybe_add_tenant_condition(
    conditions: &mut Vec<String>,
    bind_idx: &mut i32,
    tenant_id: Option<&str>,
) {
    if tenant_id.is_some() {
        conditions.push(format!("tenant_id = ${bind_idx}"));
        *bind_idx += 1;
    }
}

/// Encode a cursor with a tag indicating the sort column.
fn encode_cursor_tagged(tag: &str, value: &str, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!("{tag}:{value}:{id}"))
}

/// Decode a tagged cursor back to (tag, value, id).
fn decode_cursor_tagged(cursor: &str) -> Result<(String, String, Uuid), AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    let s = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    
    let parts: Vec<&str> = s.splitn(3, ':').collect();
    if parts.len() != 3 {
        return Err(AppError::Validation("invalid cursor format".to_string()));
    }
    
    let tag = parts[0].to_string();
    let value = parts[1].to_string();
    let id = Uuid::parse_str(parts[2])
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    
    if id.get_version() != Some(uuid::Version::Random) {
        return Err(AppError::Validation("invalid cursor".to_string()));
    }
    
    Ok((tag, value, id))
}

/// Encode a `created_at` keyset position. Microseconds keep the value free of `:`.
fn encode_created_at_cursor(created_at: DateTime<Utc>, id: Uuid) -> String {
    encode_cursor_tagged("created_at", &created_at.timestamp_micros().to_string(), id)
}

/// Decode a `created_at` keyset cursor back into its (created_at, id) position.
fn decode_created_at_cursor(cursor: &str) -> Result<(DateTime<Utc>, Uuid), AppError> {
    let (tag, value, id) = decode_cursor_tagged(cursor)?;
    if tag != "created_at" {
        return Err(AppError::Validation("invalid cursor".to_string()));
    }
    let micros: i64 = value
        .parse()
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    let created_at = DateTime::<Utc>::from_timestamp_micros(micros)
        .ok_or_else(|| AppError::Validation("invalid cursor".to_string()))?;
    Ok((created_at, id))
}

/// Encode a (ledger, id) pair as an opaque URL-safe base64 cursor.
fn encode_cursor(ledger: i64, id: Uuid) -> String {
    URL_SAFE_NO_PAD.encode(format!("{ledger}:{id}"))
}

/// Decode a cursor back to (ledger, id). Returns a validation error on malformed input.
fn decode_cursor(cursor: &str) -> Result<(i64, Uuid), AppError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    let s = std::str::from_utf8(&bytes)
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    let (ledger_str, id_str) = s
        .split_once(':')
        .ok_or_else(|| AppError::Validation("invalid cursor".to_string()))?;
    let ledger = ledger_str
        .parse::<i64>()
        .map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    if ledger <= 0 {
        return Err(AppError::Validation("invalid cursor".to_string()));
    }
    // Reject astronomically large ledger values (Stellar ledger sequence is a u32)
    if ledger > i64::from(u32::MAX) {
        return Err(AppError::Validation("invalid cursor".to_string()));
    }
    let id =
        Uuid::parse_str(id_str).map_err(|_| AppError::Validation("invalid cursor".to_string()))?;
    if id.get_version() != Some(uuid::Version::Random) {
        return Err(AppError::Validation("invalid cursor".to_string()));
    }
    Ok((ledger, id))
}

fn rows_to_json(
    rows: &[sqlx::postgres::PgRow],
    columns: &[&str],
    enc_key: Option<&[u8; 32]>,
    enc_key_old: Option<&[u8; 32]>,
    compact: bool,
) -> Result<Vec<Value>, AppError> {
    rows_to_json_with_format(
        rows,
        columns,
        enc_key,
        enc_key_old,
        compact,
        crate::scval_format::ScValFormat::Json,
    )
}

/// Like `rows_to_json`, but renders `event_data` (Issue #1064: `value` and
/// `topic` ScVals) in the requested `format` (`native`, `json` or `xdr`).
fn rows_to_json_with_format(
    rows: &[sqlx::postgres::PgRow],
    columns: &[&str],
    enc_key: Option<&[u8; 32]>,
    enc_key_old: Option<&[u8; 32]>,
    compact: bool,
    format: crate::scval_format::ScValFormat,
) -> Result<Vec<Value>, AppError> {
    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let mut event = serde_json::Map::new();
        for &col in columns {
            match col {
                "id" => {
                    event.insert(col.to_string(), json!(row.try_get::<Uuid, _>(col)?));
                }
                "contract_id" => {
                    event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?));
                }
                "event_type" => {
                    event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?));
                }
                "tx_hash" => {
                    event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?));
                }
                "ledger" => {
                    event.insert(col.to_string(), json!(row.try_get::<i64, _>(col)?));
                }
                "timestamp" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<DateTime<Utc>, _>(col)?),
                    );
                }
                "event_data" => {
                    let raw: Value = row.try_get::<Value, _>(col)?;
                    let decrypted = decrypt_event_data(&raw, enc_key, enc_key_old);
                    let rendered = crate::scval_format::render_event_data(&decrypted, format);
                    if compact {
                        event.insert(col.to_string(), compact_event_data(&rendered)?);
                    } else {
                        event.insert(col.to_string(), rendered);
                    }
                }
                "event_data_normalized" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<Option<Value>, _>(col)?),
                    );
                }
                "event_data_decoded" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<Option<Value>, _>(col)?),
                    );
                }
                "ledger_hash" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<Option<String>, _>(col)?),
                    );
                }
                "in_successful_call" => {
                    event.insert(col.to_string(), json!(row.try_get::<bool, _>(col)?));
                }
                "created_at" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<DateTime<Utc>, _>(col)?),
                    );
                }
                "schema_version" => {
                    event.insert(col.to_string(), json!(row.try_get::<i32, _>(col)?));
                }
                "anonymized" => {
                    event.insert(col.to_string(), json!(row.try_get::<bool, _>(col)?));
                }
                "relevance_score" => {
                    event.insert(col.to_string(), json!(row.try_get::<f64, _>(col)?));
                }
                _ => {}
            }
        }
        events.push(Value::Object(event));
    }
    Ok(events)
}
fn resolve_columns<'a>(params: &'a PaginationParams) -> Result<Vec<&'a str>, AppError> {
    params.columns().map_err(|(unknown, allowed)| {
        AppError::Validation(format!(
            "unknown fields: [{}]; valid fields are: [{}]",
            unknown.join(", "),
            allowed.join(", ")
        ))
    })
}

/// Stellar ledger sequences are 32-bit unsigned integers (max 2^32 - 1).
/// Validate that a ledger parameter is within [0, 2^32 - 1] (issue #423).
pub(crate) fn validate_ledger_param(name: &str, value: i64) -> Result<(), AppError> {
    const MAX_LEDGER: i64 = u32::MAX as i64; // 4_294_967_295
    if value < 0 || value > MAX_LEDGER {
        return Err(AppError::Validation(format!(
            "{name} must be in range [0, {MAX_LEDGER}]"
        )));
    }
    Ok(())
}

/// Validate a `contract_id` query/path parameter.
///
/// `pub` (not `pub(crate)`) because `fuzz/fuzz_targets/fuzz_validate_contract_id.rs`
/// links this crate as an external dependency.
pub fn validate_contract_id(contract_id: &str) -> Result<(), AppError> {
    if contract_id.len() != 56 {
        return Err(AppError::Validation(
            "invalid contract_id format".to_string(),
        ));
    }
    if !contract_id.starts_with('C') {
        return Err(AppError::Validation(
            "invalid contract_id format".to_string(),
        ));
    }
    if !contract_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "invalid contract_id format".to_string(),
        ));
    }
    Ok(())
}

/// Validate a `tx_hash` query/path parameter.
///
/// `pub` (not `pub(crate)`) because `fuzz/fuzz_targets/fuzz_validate_tx_hash.rs`
/// links this crate as an external dependency.
pub fn validate_tx_hash(tx_hash: &str) -> Result<(), AppError> {
    if tx_hash.len() != 64 {
        return Err(AppError::Validation("invalid tx_hash format".to_string()));
    }
    // Accept both uppercase and lowercase hex — callers should normalize to lowercase first.
    if !tx_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::Validation("invalid tx_hash format".to_string()));
    }
    Ok(())
}

/// Validate and parse ISO 8601 timestamp string.
fn validate_timestamp(ts: &str) -> Result<DateTime<Utc>, AppError> {
    ts.parse::<DateTime<Utc>>()
        .map_err(|_| AppError::Validation(format!("invalid timestamp format: {}", ts)))
}

/// Query parameters for the email unsubscribe endpoint (Issue #483).
#[derive(serde::Deserialize)]
pub struct UnsubscribeQuery {
    pub token: String,
}

/// Public, unauthenticated endpoint that recipients reach from the
/// "unsubscribe" link in notification emails (Issue #483, CAN-SPAM/GDPR).
/// Marks the token's recipient as opted out and returns a small HTML page.
#[utoipa::path(
    get,
    path = "/unsubscribe",
    tag = "system",
    params(("token" = String, Query, description = "Per-recipient unsubscribe token")),
    responses(
        (status = 200, description = "Unsubscribed (or already unsubscribed)"),
        (status = 404, description = "Unknown unsubscribe token"),
    )
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Query(query): Query<UnsubscribeQuery>,
) -> Response {
    fn html_page(status: StatusCode, title: &str, message: &str) -> Response {
        let body = format!(
            "<!DOCTYPE html><html><head><meta charset=\"utf-8\">\
             <title>{title}</title></head><body style=\"font-family:sans-serif;\
             max-width:32rem;margin:4rem auto;text-align:center;\">\
             <h1>{title}</h1><p>{message}</p></body></html>"
        );
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(body))
            .unwrap_or_else(|e| response_build_error(e))
    }

    // Verify the HMAC-signed, time-limited token (issue #1158).
    // The subject carried in the token is the opaque DB token string that
    // `mark_unsubscribed` looks up; we verify purpose = Unsubscribe before
    // touching the DB.
    let db_token = match crate::email_token::verify_token(
        &query.token,
        Some(&crate::email_token::TokenPurpose::Unsubscribe),
        None,
    ) {
        Ok(claims) => claims.subject,
        Err(_) => {
            // Return 404 (not 400) to avoid leaking why the token failed.
            return html_page(
                StatusCode::NOT_FOUND,
                "Invalid link",
                "This unsubscribe link is not valid.",
            );
        }
    };

    match crate::email::mark_unsubscribed(&state.pool, &db_token).await {
        Ok(true) => html_page(
            StatusCode::OK,
            "Unsubscribed",
            "You have been unsubscribed from Soroban Pulse notifications.",
        ),
        Ok(false) => html_page(
            StatusCode::NOT_FOUND,
            "Invalid link",
            "This unsubscribe link is not valid.",
        ),
        Err(e) => {
            tracing::error!(error = %e, "Failed to process unsubscribe request");
            html_page(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong",
                "We could not process your request. Please try again later.",
            )
        }
    }
}

#[utoipa::path(
    get,
    path = "/v1/status",
    tag = "system",
    responses(
        (status = 200, description = "Indexer operational status", body = serde_json::Value),
    )
)]
pub async fn status(State(state): State<AppState>) -> Json<Value> {
    let current_ledger = state.indexer_state.current_ledger.load(Ordering::Relaxed);
    let latest_ledger = state.indexer_state.latest_ledger.load(Ordering::Relaxed);
    let lag_ledgers = latest_ledger.saturating_sub(current_ledger);
    let uptime_secs = state.indexer_state.uptime_secs();

    let indexer_status = if state.health_state.is_indexer_stalled().is_some() {
        "stalled"
    } else {
        "running"
    };

    let indexer_mode = if state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        "active"
    } else {
        "read_only"
    };

    let indexer_paused = state.indexer_state.is_paused.load(Ordering::Relaxed);

    let total_events: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM events")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    // Query event counts by type
    let events_by_type_rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT event_type, COUNT(*) as count FROM events GROUP BY event_type")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    // Build events_by_type object with all event types (defaulting to 0 if not present)
    let mut events_by_type = serde_json::json!({
        "contract": 0i64,
        "diagnostic": 0i64,
        "system": 0i64,
    });

    for (event_type, count) in events_by_type_rows {
        if let Some(obj) = events_by_type.as_object_mut() {
            obj.insert(event_type, serde_json::json!(count));
        }
    }

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": uptime_secs,
        "current_ledger": current_ledger,
        "latest_ledger": latest_ledger,
        "lag_ledgers": lag_ledgers,
        "total_events": total_events,
        "events_by_type": events_by_type,
        "indexer_status": indexer_status,
        "indexer_mode": indexer_mode,
        "indexer_paused": indexer_paused,
        "rpc_version": crate::rpc_meta::current_version(),
        "gaps": crate::rpc_meta::list_gaps(&state.pool).await,
    }))
}

/// GET /v1/admin/indexer/gaps
pub async fn get_indexer_gaps(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "gaps": crate::rpc_meta::list_gaps(&state.pool).await }))
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct ContractHistoryParams {
    pub bucket: Option<String>,
    pub days: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/stats/history",
    tag = "events",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
        ("bucket" = Option<String>, Query, description = "Aggregation bucket. Currently only 1d is supported"),
        ("days" = Option<i64>, Query, description = "Number of daily buckets to return"),
        ("from" = Option<String>, Query, description = "Start date, YYYY-MM-DD"),
        ("to" = Option<String>, Query, description = "End date, YYYY-MM-DD"),
    ),
    responses(
        (status = 200, description = "Daily contract event and unique transaction history", body = serde_json::Value),
        (status = 400, description = "Invalid contract_id, bucket, or date range", body = ErrorResponse),
    )
)]
pub async fn get_contract_stats_history(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Query(params): Query<ContractHistoryParams>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    let bucket = params.bucket.as_deref().unwrap_or("1d");
    if bucket != "1d" {
        return Err(AppError::Validation("unsupported bucket: only bucket=1d is supported".to_string()));
    }

    let today = Utc::now().date_naive();
    let (start_date, end_date) = if params.from.is_some() || params.to.is_some() {
        let from = params.from.as_deref().ok_or_else(|| AppError::Validation("from is required when to is provided".to_string()))?;
        let to = params.to.as_deref().ok_or_else(|| AppError::Validation("to is required when from is provided".to_string()))?;
        let start = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("from must be YYYY-MM-DD".to_string()))?;
        let end = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("to must be YYYY-MM-DD".to_string()))?;
        (start, end)
    } else {
        let days = params.days.unwrap_or(30);
        if !(1..=366).contains(&days) {
            return Err(AppError::Validation("days must be between 1 and 366".to_string()));
        }
        (today - chrono::Duration::days(days - 1), today)
    };
    if start_date > end_date {
        return Err(AppError::Validation("from must be <= to".to_string()));
    }
    if (end_date - start_date).num_days() + 1 > 366 {
        return Err(AppError::Validation("date range cannot exceed 366 daily buckets".to_string()));
    }

    let start = std::time::Instant::now();
    let rows = sqlx::query(
        r#"
        SELECT d::date AS date,
               COALESCE(s.event_count, 0)::bigint AS event_count,
               COALESCE(s.unique_tx_count, 0)::bigint AS unique_tx_count
        FROM generate_series($2::date, $3::date, interval '1 day') AS d
        LEFT JOIN (
            SELECT timestamp::date AS event_date,
                   COUNT(*) AS event_count,
                   COUNT(DISTINCT tx_hash) AS unique_tx_count
            FROM events
            WHERE contract_id = $1
              AND timestamp::date BETWEEN $2::date AND $3::date
            GROUP BY timestamp::date
        ) s ON s.event_date = d::date
        ORDER BY d::date ASC
        "#,
    )
    .bind(&contract_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_all(&state.read_pool)
    .await?;
    crate::metrics::record_contract_history_query_duration(start.elapsed());

    let data = rows
        .into_iter()
        .map(|row| {
            let date: chrono::NaiveDate = row.try_get("date")?;
            let event_count: i64 = row.try_get("event_count")?;
            let unique_tx_count: i64 = row.try_get("unique_tx_count")?;
            Ok(json!({ "date": date, "event_count": event_count, "unique_tx_count": unique_tx_count }))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    Ok(Json(json!({ "contract_id": contract_id, "bucket": bucket, "data": data })))
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    crate::metrics::update_db_pool_metrics(&state.pool);
    state.prometheus_handle.render()
}

/// Serve the raw OpenAPI JSON spec.
pub async fn openapi_json() -> impl IntoResponse {
    use crate::routes::ApiDoc;
    use utoipa::OpenApi;
    Json(ApiDoc::openapi())
}

/// Serve the branded SorobanPulse Swagger UI page.
///
/// The inline `<style>` block applies SorobanPulse brand colours and full dark
/// mode on top of the stock Swagger UI stylesheet loaded from unpkg.com.
/// The `<script>` block initialises SwaggerUIBundle.
///
/// Both blocks carry a SHA-256 hash that is allow-listed in the `/docs` CSP so
/// `unsafe-inline` is **not** needed.  If you modify either block you must
/// recompute the hash with:
///
/// ```sh
/// printf '%s' 'THE CONTENT' | openssl dgst -sha256 -binary | base64
/// ```
///
/// and update `csp_docs` in `src/middleware/security_headers.rs` accordingly.
pub async fn swagger_ui() -> impl IntoResponse {
    // Inline CSS — SHA-256: t5Nfs8a1PFuEVO00S72ZGB4P65C74f37u8w0VCVsqBw=
    let style = r#":root{--sp-bg:#0f1117;--sp-surface:#1a1d2e;--sp-border:#2d3158;--sp-accent:#7c3aed;--sp-accent-light:#a78bfa;--sp-text:#e2e8f0;--sp-text-muted:#94a3b8;--sp-success:#10b981;--sp-warning:#f59e0b;--sp-danger:#ef4444}
body{background:var(--sp-bg)!important;color:var(--sp-text)!important;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",system-ui,sans-serif}
#swagger-ui .swagger-ui .info{margin:2rem 0}
#swagger-ui .swagger-ui .info .title{color:var(--sp-accent-light)!important;font-size:2rem!important;font-weight:700}
#swagger-ui .swagger-ui .info .description p,#swagger-ui .swagger-ui .info .description li{color:var(--sp-text)!important}
#swagger-ui .swagger-ui .info .description a{color:var(--sp-accent-light)!important}
#swagger-ui .topbar{background:var(--sp-surface)!important;border-bottom:1px solid var(--sp-border)!important;padding:.75rem 1rem}
#swagger-ui .swagger-ui .scheme-container{background:var(--sp-surface)!important;border-bottom:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .opblock-tag{color:var(--sp-text)!important;border-bottom:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .opblock{background:var(--sp-surface)!important;border:1px solid var(--sp-border)!important;border-radius:.5rem!important;margin-bottom:.75rem}
#swagger-ui .swagger-ui .opblock .opblock-summary{border-bottom:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .opblock .opblock-summary-method{border-radius:.25rem!important;font-weight:600}
#swagger-ui .swagger-ui .opblock.opblock-get .opblock-summary-method{background:var(--sp-success)!important}
#swagger-ui .swagger-ui .opblock.opblock-post .opblock-summary-method{background:var(--sp-accent)!important}
#swagger-ui .swagger-ui .opblock.opblock-delete .opblock-summary-method{background:var(--sp-danger)!important}
#swagger-ui .swagger-ui .opblock.opblock-patch .opblock-summary-method{background:var(--sp-warning)!important}
#swagger-ui .swagger-ui .opblock-description-wrapper p,.swagger-ui .markdown p{color:var(--sp-text)!important}
#swagger-ui .swagger-ui section.models{background:var(--sp-surface)!important;border:1px solid var(--sp-border)!important;border-radius:.5rem}
#swagger-ui .swagger-ui section.models h4{color:var(--sp-text)!important}
#swagger-ui .swagger-ui .model-box{background:var(--sp-bg)!important}
#swagger-ui .swagger-ui .parameter__name,#swagger-ui .swagger-ui .parameter__type{color:var(--sp-text-muted)!important}
#swagger-ui .swagger-ui input[type=text],#swagger-ui .swagger-ui textarea{background:var(--sp-bg)!important;border:1px solid var(--sp-border)!important;color:var(--sp-text)!important}
#swagger-ui .swagger-ui select{background:var(--sp-bg)!important;color:var(--sp-text)!important;border:1px solid var(--sp-border)!important}
#swagger-ui .swagger-ui .btn.authorize{background:var(--sp-accent)!important;border-color:var(--sp-accent)!important;color:#fff!important}
#swagger-ui .swagger-ui .btn.execute{background:var(--sp-success)!important;border-color:var(--sp-success)!important;color:#fff!important}
#swagger-ui .swagger-ui .response-col_status{color:var(--sp-text)!important}
#swagger-ui .swagger-ui .response-col_description{color:var(--sp-text-muted)!important}
#swagger-ui .swagger-ui pre.microlight{background:var(--sp-bg)!important;color:var(--sp-accent-light)!important;border:1px solid var(--sp-border)!important;border-radius:.375rem}
#swagger-ui .swagger-ui .highlight-code{background:var(--sp-bg)!important}"#;

    // Inline JS — SHA-256: OlhJ06FtsPaiJ/1A+8VpeJOGKCgqkf63ajgd7nrxk98=
    let script = r##"SwaggerUIBundle({url:"/openapi.json",dom_id:"#swagger-ui",deepLinking:true,presets:[SwaggerUIBundle.presets.apis,SwaggerUIBundle.SwaggerUIStandalonePreset],layout:"BaseLayout",docExpansion:"list",defaultModelsExpandDepth:1})"##;

    // SorobanPulse logo — inline SVG, no external fetch required
    let logo_svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="36" height="36" viewBox="0 0 36 36" fill="none" aria-hidden="true">
  <circle cx="18" cy="18" r="18" fill="#7c3aed"/>
  <path d="M10 18 Q18 8 26 18 Q18 28 10 18Z" fill="#a78bfa"/>
  <circle cx="18" cy="18" r="4" fill="#0f1117"/>
</svg>"##;

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8"/>
  <meta name="viewport" content="width=device-width,initial-scale=1"/>
  <title>SorobanPulse — API Docs</title>
  <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css"/>
  <style>{style}</style>
</head>
<body>
  <header style="display:flex;align-items:center;gap:.75rem;padding:1rem 1.5rem;background:#1a1d2e;border-bottom:1px solid #2d3158;position:sticky;top:0;z-index:100">
    {logo_svg}
    <span style="font-size:1.25rem;font-weight:700;color:#a78bfa;letter-spacing:-.01em">SorobanPulse</span>
    <span style="font-size:.875rem;color:#94a3b8;margin-left:.25rem">API Explorer</span>
    <a href="/openapi.json" style="margin-left:auto;font-size:.8125rem;color:#a78bfa;text-decoration:none;border:1px solid #2d3158;padding:.25rem .625rem;border-radius:.375rem">OpenAPI JSON ↗</a>
  </header>
  <div id="swagger-ui"></div>
  <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
  <script>{script}</script>
</body>
</html>"#,
        style = style,
        logo_svg = logo_svg,
        script = script,
    );

    axum::response::Html(html)
}

/// Converts an `Event` to a JSON object containing only the requested fields.
fn filter_fields(
    event: &models::Event,
    columns: &[&str],
    enc_key: Option<&[u8; 32]>,
    enc_key_old: Option<&[u8; 32]>,
) -> Value {
    let mut map = serde_json::Map::new();
    for &col in columns {
        match col {
            "id" => {
                map.insert(col.to_string(), json!(event.id));
            }
            "contract_id" => {
                map.insert(col.to_string(), json!(event.contract_id));
            }
            "event_type" => {
                map.insert(col.to_string(), json!(event.event_type));
            }
            "tx_hash" => {
                map.insert(col.to_string(), json!(event.tx_hash));
            }
            "ledger" => {
                map.insert(col.to_string(), json!(event.ledger));
            }
            "timestamp" => {
                map.insert(col.to_string(), json!(event.timestamp));
            }
            "event_data" => {
                let decrypted = decrypt_event_data(&event.event_data, enc_key, enc_key_old);
                map.insert(col.to_string(), decrypted);
            }
            "event_data_normalized" => {
                map.insert(col.to_string(), json!(event.event_data_normalized));
            }
            "event_data_decoded" => {
                map.insert(col.to_string(), json!(event.event_data_decoded));
            }
            "ledger_hash" => {
                map.insert(col.to_string(), json!(event.ledger_hash));
            }
            "anonymized" => {
                map.insert(col.to_string(), json!(event.anonymized));
            }
            "in_successful_call" => {
                map.insert(col.to_string(), json!(event.in_successful_call));
            }
            "created_at" => {
                map.insert(col.to_string(), json!(event.created_at));
            }
            "schema_version" => {
                map.insert(col.to_string(), json!(event.schema_version));
            }
            _ => {}
        }
    }
    Value::Object(map)
}

/// Returns true if the client prefers NDJSON via the Accept header.
fn extract_api_key_from_headers(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| headers.get("X-Api-Key").and_then(|h| h.to_str().ok()))
}

fn is_admin_request(headers: &HeaderMap, admin_api_keys: &[secrecy::SecretString]) -> bool {
    extract_api_key_from_headers(headers)
        .map(|provided_key| {
            admin_api_keys
                .iter()
                .any(|expected| expected.expose_secret().as_str() == provided_key)
        })
        .unwrap_or(false)
}

fn accepts_ndjson(headers: &axum::http::HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("application/x-ndjson"))
        .unwrap_or(false)
}

/// Extract client IP from X-Forwarded-For or X-Real-IP headers, falling back to "unknown".
pub(crate) fn extract_client_ip(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_string())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

/// Build a plain JSON response from a `Value`.
fn json_response(body: Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap_or_default()))
        .unwrap_or_else(|e| response_build_error(e))
}

/// Build an NDJSON response: one JSON object per line, no wrapping array.
fn ndjson_response(events: impl Iterator<Item = Value>) -> Response<Body> {
    let mut buf = String::new();
    for ev in events {
        buf.push_str(&serde_json::to_string(&ev).unwrap_or_default());
        buf.push('\n');
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-ndjson")
        .body(Body::from(buf))
        .unwrap_or_else(|e| response_build_error(e))
}

fn xml_response(body: String) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/atom+xml; charset=utf-8")
        .body(Body::from(body))
        .unwrap_or_else(|e| response_build_error(e))
}

fn render_atom_feed(rows: &[sqlx::postgres::PgRow]) -> Result<String, AppError> {
    let updated = rows
        .first()
        .and_then(|row| row.try_get::<DateTime<Utc>, _>("timestamp").ok())
        .unwrap_or_else(Utc::now)
        .to_rfc3339();
    let mut feed = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
         <feed xmlns=\"http://www.w3.org/2005/Atom\">\
         <title>Soroban Pulse Events</title>\
         <id>urn:soroban-pulse:events</id><updated>{}</updated>",
        escape_xml(&updated)
    );
    for row in rows {
        feed.push_str(&render_feed_entry(row)?);
    }
    feed.push_str("</feed>");
    Ok(feed)
}

fn render_feed_entry(row: &sqlx::postgres::PgRow) -> Result<String, AppError> {
    let id: Uuid = row.try_get("id")?;
    let contract_id: String = row.try_get("contract_id")?;
    let event_type: String = row.try_get("event_type")?;
    let tx_hash: String = row.try_get("tx_hash")?;
    let ledger: i64 = row.try_get("ledger")?;
    let timestamp: DateTime<Utc> = row.try_get("timestamp")?;
    let event_data: Value = row.try_get("event_data")?;
    Ok(format!(
        "<entry><title>{} event for {}</title>\
         <id>urn:soroban-pulse:event:{}</id><updated>{}</updated>\
         <summary type=\"html\">ledger {} tx {}</summary>\
         <content type=\"application/json\">{}</content></entry>",
        escape_xml(&event_type),
        escape_xml(&contract_id),
        id,
        escape_xml(&timestamp.to_rfc3339()),
        ledger,
        escape_xml(&tx_hash),
        escape_xml(&event_data.to_string())
    ))
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Escape a single CSV field per RFC 4180.
///
/// A field is wrapped in double-quotes if it contains a comma, double-quote,
/// newline (`\n`), or carriage-return (`\r`). Any double-quote character
/// inside the field is escaped by doubling it (`"` → `""`).
fn csv_escape_field(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        // Escape embedded double-quotes by doubling them, then wrap in quotes.
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_owned()
    }
}

/// Query parameters for the /v1/events/recent endpoint.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct RecentParams {
    pub limit: Option<i64>,
    pub event_type: Option<crate::models::EventType>,
    pub contract_id: Option<String>,
    /// Cursor for pagination (opaque, URL-safe).
    pub cursor: Option<String>,
    /// Not supported — returns 400 if provided.
    pub from_ledger: Option<i64>,
    /// Not supported — returns 400 if provided.
    pub to_ledger: Option<i64>,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct RelatedTxParams {
    pub depth: Option<u8>,
}

fn collect_tx_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if is_tx_ref_key(key) {
                    if let Some(tx_hash) = value.as_str() {
                        out.push(tx_hash.to_string());
                    }
                }
                collect_tx_refs(value, out);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_tx_refs(value, out);
            }
        }
        _ => {}
    }
}

fn is_tx_ref_key(key: &str) -> bool {
    matches!(key, "tx_hash" | "transaction_hash" | "related_tx_hash" | "parent_tx_hash" | "child_tx_hash")
}

#[utoipa::path(
    post,
    path = "/v1/admin/contracts/{contract_id}/abi",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    request_body(content = Value, description = "ABI JSON array", content_type = "application/json"),
    responses(
        (status = 200, description = "ABI registered"),
        (status = 400, description = "Invalid contract_id or ABI", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn register_contract_abi(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(abi): Json<Value>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    // Issue #607: validate ABI structure before persisting.
    if let Err(e) = crate::abi::validate_abi(&abi) {
        crate::metrics::record_abi_validation_failure(&contract_id);
        return Err(AppError::Validation(e));
    }
    sqlx::query(
        "INSERT INTO contract_abis (contract_id, abi, updated_at)
         VALUES ($1, $2, NOW())
         ON CONFLICT (contract_id) DO UPDATE SET abi = EXCLUDED.abi, updated_at = NOW()",
    )
    .bind(&contract_id)
    .bind(&abi)
    .execute(&state.pool)
    .await?;

    // Issue #607: invalidate the in-process cache so the next read fetches fresh data.
    crate::abi::invalidate_abi_cache(&state.abi_cache, &contract_id).await;

    let pool = state.pool.clone();
    let backfill_contract_id = contract_id.clone();
    let backfill_abi = abi.clone();
    tokio::spawn(async move {
        crate::abi::decode_existing_events(pool, backfill_contract_id, backfill_abi).await;
    });

    Ok(Json(
        json!({ "contract_id": contract_id, "status": "registered" }),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/admin/contracts/{contract_id}/abi",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    responses(
        (status = 200, description = "Registered ABI", body = serde_json::Value),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 404, description = "ABI not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_contract_abi(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    let row = sqlx::query("SELECT abi, created_at, updated_at FROM contract_abis WHERE contract_id = $1")
        .bind(&contract_id)
        .fetch_optional(&state.read_pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let abi: Value = row.try_get("abi")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    Ok(Json(json!({ "contract_id": contract_id, "abi": abi, "created_at": created_at, "updated_at": updated_at })))
}

/// Admin: create or update a contract's label/metadata (#1066).
#[utoipa::path(
    post,
    path = "/v1/admin/contracts/{contract_id}/metadata",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    request_body = crate::contract_metadata::UpsertContractMetadata,
    responses(
        (status = 200, description = "Metadata upserted", body = crate::contract_metadata::ContractMetadata),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn upsert_contract_metadata(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(input): Json<crate::contract_metadata::UpsertContractMetadata>,
) -> Result<Json<crate::contract_metadata::ContractMetadata>, AppError> {
    validate_contract_id(&contract_id)?;
    let metadata =
        crate::contract_metadata::upsert_contract_metadata(&state.pool, &contract_id, input)
            .await?;
    Ok(Json(metadata))
}

/// Admin: delete a contract's label/metadata (#1066).
pub async fn delete_contract_metadata(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    let deleted =
        crate::contract_metadata::delete_contract_metadata(&state.pool, &contract_id).await?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "contract_id": contract_id, "status": "deleted" })))
}

/// Public: read a contract's label/metadata (#1066).
#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/metadata",
    tag = "contracts",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    responses(
        (status = 200, description = "Contract metadata", body = crate::contract_metadata::ContractMetadata),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 404, description = "No metadata registered", body = ErrorResponse),
    )
)]
pub async fn get_contract_metadata(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<crate::contract_metadata::ContractMetadata>, AppError> {
    validate_contract_id(&contract_id)?;
    let metadata =
        crate::contract_metadata::get_contract_metadata(&state.read_pool, &contract_id)
            .await?
            .ok_or(AppError::NotFound)?;
    Ok(Json(metadata))
}

/// Admin: bulk import contract labels from a JSON array of
/// `{contract_id, name, description, project_url, source_repo, tags, verified}` (#1066).
pub async fn bulk_import_contract_metadata(
    State(state): State<AppState>,
    Json(entries): Json<Vec<Value>>,
) -> Result<Json<Value>, AppError> {
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let contract_id = entry
            .get("contract_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("missing contract_id".into()))?
            .to_string();
        validate_contract_id(&contract_id)?;
        let input: crate::contract_metadata::UpsertContractMetadata =
            serde_json::from_value(entry)
                .map_err(|e| AppError::Validation(format!("invalid metadata entry: {e}")))?;
        parsed.push((contract_id, input));
    }
    let count = crate::contract_metadata::bulk_import(&state.pool, parsed).await?;
    Ok(Json(json!({ "imported": count })))
}

/// Pause the indexer loop without stopping the HTTP server.
#[utoipa::path(
    post,
    path = "/v1/admin/indexer/pause",
    tag = "admin",
    responses(
        (status = 200, description = "Indexer paused"),
        (status = 403, description = "Not the active indexer", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn pause_indexer(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    if !state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        return Err(AppError::Forbidden(
            "pause endpoint only available on active indexer".to_string(),
        ));
    }
    state.indexer_state.is_paused.store(true, Ordering::Relaxed);
    tracing::info!("Indexer paused via admin API");
    Ok(Json(json!({ "indexer_paused": true })))
}

/// Resume a previously paused indexer loop.
#[utoipa::path(
    post,
    path = "/v1/admin/indexer/resume",
    tag = "admin",
    responses(
        (status = 200, description = "Indexer resumed"),
        (status = 403, description = "Not the active indexer", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn resume_indexer(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    if !state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        return Err(AppError::Forbidden(
            "resume endpoint only available on active indexer".to_string(),
        ));
    }
    state
        .indexer_state
        .is_paused
        .store(false, Ordering::Relaxed);
    tracing::info!("Indexer resumed via admin API");
    Ok(Json(json!({ "indexer_paused": false })))
}

/// Operational cost report with per-component breakdown and a linear forecast
/// (Issue #1264).
///
/// `GET /v1/admin/costs/report`
///
/// Aggregates the hourly `cost_snapshots` rows over the requested window
/// (default: last 24 hours) and returns the cost breakdown by component
/// (`rpc`, `db`, `notification`) together with a forecast for the next window
/// produced by the linear model in `costs::forecast`.
#[utoipa::path(
    get,
    path = "/v1/admin/costs/report",
    tag = "admin",
    params(
        ("hours" = Option<i64>, Query, description = "Lookback window in hours (default 24)"),
        ("forecast_hours" = Option<i64>, Query, description = "Forecast horizon in hours (default 24)"),
    ),
    responses(
        (status = 200, description = "Cost breakdown by component with forecast", body = Value),
        (status = 400, description = "Invalid query parameters", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to generate cost report", body = ErrorResponse),
    )
)]
pub async fn get_cost_report(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let hours = match params.get("hours") {
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|h| *h > 0 && *h <= 24 * 90)
            .ok_or_else(|| {
                AppError::Validation("hours must be an integer between 1 and 2160".to_string())
            })?,
        None => 24,
    };
    let forecast_hours = match params.get("forecast_hours") {
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|h| *h > 0 && *h <= 24 * 90)
            .ok_or_else(|| {
                AppError::Validation(
                    "forecast_hours must be an integer between 1 and 2160".to_string(),
                )
            })?,
        None => 24,
    };

    let report = crate::costs::report::generate_report(&state.read_pool, hours)
        .await
        .map_err(|e| AppError::Internal(format!("failed to generate cost report: {e}")))?;

    let forecast = crate::costs::report::generate_forecast(&state.read_pool, forecast_hours)
        .await
        .map_err(|e| AppError::Internal(format!("failed to generate cost forecast: {e}")))?;

    Ok(Json(json!({
        "window_hours": hours,
        "total_cost": report.total_cost,
        "breakdown": report.by_component,
        "snapshots": report.snapshot_count,
        "forecast": {
            "horizon_hours": forecast_hours,
            "predicted_cost": forecast.predicted_cost,
            "slope": forecast.slope,
            "intercept": forecast.intercept,
            "confidence": forecast.confidence,
        },
    })))
}

/// Get current database connection pool statistics
#[utoipa::path(
    get,
    path = "/v1/admin/pool-config/statistics",
    tag = "admin",
    responses(
        (status = 200, description = "Pool statistics retrieved", body = PoolStatistics),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_pool_statistics(
    State(state): State<AppState>,
) -> Result<Json<crate::models::PoolStatistics>, (StatusCode, Json<Value>)> {
    crate::db_pool::management::get_pool_config(&state.db, state.pool_stats.clone(), state.config.db_max_connections)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Get pool configuration and generate tuning recommendations
#[utoipa::path(
    get,
    path = "/v1/admin/pool-config",
    tag = "admin",
    responses(
        (status = 200, description = "Pool configuration retrieved", body = PoolTuningGuide),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_pool_tuning_guide(
    State(state): State<AppState>,
) -> Result<Json<crate::models::PoolTuningGuide>, (StatusCode, Json<Value>)> {
    let pool_stats = crate::db_pool::management::get_pool_config(&state.db, state.pool_stats.clone(), state.config.db_max_connections)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    let current_config = crate::models::PoolConfig {
        max_connections: state.config.db_max_connections,
        min_connections: state.config.db.min_connections.unwrap_or(1),
        connection_timeout_secs: 30,
        idle_timeout_secs: 600,
        exhaustion_threshold: 0.9,
        sample_interval_secs: 15,
    };

    let guide = crate::db_pool::management::generate_tuning_guide(&pool_stats, &current_config);
    Ok(Json(guide))
}

/// Get comprehensive statistics report for all tables
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/report",
    tag = "admin",
    responses(
        (status = 200, description = "Statistics report for all tables", body = Vec<StatisticsReportItem>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to generate report", body = ErrorResponse),
    )
)]
pub async fn get_statistics_report(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::StatisticsReportItem>>, (StatusCode, Json<Value>)> {
    crate::statistics_management::get_statistics_report(&state.db)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Detect which tables have stale statistics
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/stale",
    tag = "admin",
    responses(
        (status = 200, description = "Stale statistics detection results", body = Vec<StalenessDetection>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to detect stale statistics", body = ErrorResponse),
    )
)]
pub async fn detect_stale_statistics(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::StalenessDetection>>, (StatusCode, Json<Value>)> {
    crate::statistics_management::detect_stale_statistics(&state.db)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Refresh statistics for all tables or a specific table
#[utoipa::path(
    post,
    path = "/v1/admin/statistics/refresh",
    tag = "admin",
    params(
        ("table_name" = Option<String>, Query, description = "Optional table name to refresh only one table")
    ),
    responses(
        (status = 202, description = "Statistics refresh job started"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to refresh statistics", body = ErrorResponse),
    )
)]
pub async fn refresh_statistics(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Issue #1425: validate `table_name` before it reaches the statistics
    // layer. The value is bound as a query parameter there, but rejecting
    // anything that is not a plain identifier (or a known table) up front
    // keeps the admin surface strict and returns a 400 instead of a 500.
    let table_name = match params.get("table_name") {
        Some(raw) => match crate::statistics_management::validate_table_name(raw) {
            Ok(()) => Some(raw.as_str()),
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": e.to_string(),
                        "code": "VALIDATION_ERROR"
                    })),
                ));
            }
        },
        None => None,
    };

    match crate::statistics_management::refresh_table_statistics(&state.db, table_name).await {
        Ok(results) => {
            let summary = if let Some(table) = table_name {
                format!("Refreshed statistics for table: {}", table)
            } else {
                format!("Refreshed statistics for {} tables", results.len())
            };

            Ok((
                StatusCode::ACCEPTED,
                Json(json!({
                    "message": summary,
                    "results_count": results.len(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// Get recent statistics analysis jobs
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/jobs",
    tag = "admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of jobs to return (default 50)")
    ),
    responses(
        (status = 200, description = "Recent analysis jobs", body = Vec<StatisticsAnalysisJobInfo>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to retrieve jobs", body = ErrorResponse),
    )
)]
pub async fn get_analysis_jobs(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<crate::models::StatisticsAnalysisJobInfo>>, (StatusCode, Json<Value>)> {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(50);

    crate::statistics_management::get_recent_analysis_jobs(&state.db, limit)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// #694: Query and return index fragmentation information.
///
/// `GET /v1/admin/indexes/fragmentation`
///
/// Returns a list of all user indexes in the public schema with their
/// estimated bloat ratio, dead/live tuple counts, size, and last vacuum
/// timestamps.  Results are sorted by index size descending.
#[utoipa::path(
    get,
    path = "/v1/admin/indexes/fragmentation",
    tag = "admin",
    responses(
        (status = 200, description = "Index fragmentation report"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn get_index_fragmentation(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::index_monitor::IndexFragmentationInfo>>, (StatusCode, Json<Value>)> {
    crate::index_monitor::query_index_fragmentation(&state.pool)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to query index fragmentation");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Failed to query index fragmentation: {e}"),
                    "code": "INTERNAL_ERROR"
                })),
            )
        })
}

/// #694: Trigger a single REINDEX INDEX CONCURRENTLY operation.
///
/// `POST /v1/admin/indexes/{index_name}/reindex`
///
/// Runs `REINDEX INDEX CONCURRENTLY` on the named index. Returns 202 on
/// success.  The index must be a valid user index in the public schema.
#[utoipa::path(
    post,
    path = "/v1/admin/indexes/{index_name}/reindex",
    tag = "admin",
    params(
        ("index_name" = String, Path, description = "Name of the index to rebuild"),
    ),
    responses(
        (status = 202, description = "REINDEX scheduled successfully"),
        (status = 400, description = "Invalid index name", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "REINDEX failed", body = ErrorResponse),
    )
)]
pub async fn reindex_index(
    State(state): State<AppState>,
    Path(index_name): Path<String>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Basic validation: only allow alphanumeric + underscore index names
    if index_name.is_empty()
        || !index_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid index name",
                "code": "VALIDATION_ERROR"
            })),
        ));
    }

    // Verify the index exists
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_class WHERE relname = $1 AND relkind = 'i')",
    )
    .bind(&index_name)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("Failed to verify index existence: {e}"),
                "code": "INTERNAL_ERROR"
            })),
        )
    })?;

    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("index '{index_name}' not found"),
                "code": "NOT_FOUND"
            })),
        ));
    }

    tracing::info!(index = %index_name, "Admin-triggered REINDEX INDEX CONCURRENTLY");

    let sql = format!("REINDEX INDEX CONCURRENTLY {index_name}");
    sqlx::query(&sql).execute(&state.pool).await.map_err(|e| {
        tracing::error!(index = %index_name, error = %e, "REINDEX failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("REINDEX failed: {e}"),
                "code": "REINDEX_FAILED"
            })),
        )
    })?;

    tracing::info!(index = %index_name, "REINDEX INDEX CONCURRENTLY completed");

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "completed",
            "index": index_name,
            "message": "REINDEX INDEX CONCURRENTLY completed successfully"
        })),
    ))
}

/// Request body for `POST /v1/admin/encryption/rotate` (Issue #1270).
#[derive(Debug, serde::Deserialize)]
pub struct RotateEncryptionKeyRequest {
    /// The new AES-GCM key (base64-encoded 32-byte key).
    pub new_key: String,
}

/// Rotate the AES-GCM credential-encryption key without downtime (Issue #1270).
///
/// `POST /v1/admin/encryption/rotate`
///
/// Accepts `{ "new_key": "<base64 32-byte key>" }`, then starts a batched
/// background job that decrypts every stored credential with the current key
/// and re-encrypts it with `new_key`. The job commits in batches of 100 rows
/// and can be resumed from where it stopped if interrupted.
///
/// This endpoint is gated behind `ADMIN_API_KEY` by the admin auth middleware.
#[utoipa::path(
    post,
    path = "/v1/admin/encryption/rotate",
    tag = "admin",
    request_body(content = Value, description = "{\"new_key\": \"<base64 32-byte key>\"}", content_type = "application/json"),
    responses(
        (status = 202, description = "Key rotation job started"),
        (status = 400, description = "Invalid or missing new_key", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 409, description = "A rotation job is already running", body = ErrorResponse),
    )
)]
pub async fn rotate_encryption_key(
    State(state): State<AppState>,
    Json(body): Json<RotateEncryptionKeyRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    // Validate the new key: must decode to exactly 32 bytes.
    let new_key_bytes = STANDARD
        .decode(body.new_key.trim())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "new_key must be valid base64" })),
            )
        })?;
    if new_key_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "new_key must decode to exactly 32 bytes" })),
        ));
    }
    let mut new_key = [0u8; 32];
    new_key.copy_from_slice(&new_key_bytes);

    // The current key is required to decrypt existing credentials.
    let old_key = state.encryption_key.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "no current encryption key configured (ENCRYPTION_KEY)"
            })),
        )
    })?;

    // Start the batched rotation job (100 rows per transaction).
    let started = crate::reencrypt::start_rotation_job(
        state.pool.clone(),
        new_key,
        old_key,
        100,
        state.reencrypt_state.clone(),
    );

    if !started {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "a key rotation job is already running" })),
        ));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "started",
            "batch_size": 100,
            "message": "key rotation started; poll /v1/admin/encryption/rotate/status for progress"
        })),
    ))
}

/// Report progress of the credential key-rotation job (Issue #1270).
///
/// `GET /v1/admin/encryption/rotate/status`
#[utoipa::path(
    get,
    path = "/v1/admin/encryption/rotate/status",
    tag = "admin",
    responses(
        (status = 200, description = "Rotation job status"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_encryption_rotation_status(
    State(state): State<AppState>,
) -> Json<Value> {
    let (completed, total, running) = state.reencrypt_state.progress();
    Json(json!({
        "running": running,
        "completed": completed,
        "total": total,
        "progress": if total > 0 { completed as f64 / total as f64 } else { 0.0 },
    }))
}

/// Start background re-encryption jobs after a key rotation.
///
/// - Events: migrates `event_data` from `ENCRYPTION_KEY_OLD` to `ENCRYPTION_KEY`
///   (requires the `encryption` feature).
/// - Integration credentials (Issue #1162): re-encrypts every stored token,
///   webhook URL and routing key from `INTEGRATION_ENCRYPTION_KEY_OLD` to
///   `INTEGRATION_ENCRYPTION_KEY`, encrypting any remaining plaintext rows.
#[utoipa::path(
    post,
    path = "/v1/admin/reencrypt",
    tag = "admin",
    responses(
        (status = 202, description = "Re-encryption job(s) started"),
        (status = 400, description = "No old key configured for events or integration credentials", body = ErrorResponse),
        (status = 409, description = "Re-encryption job already running", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn start_reencrypt(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let batch_size = 1000;
    let mut jobs: Vec<&str> = Vec::new();

    if crate::integration_secrets::rotation_configured() {
        if !crate::integration_secrets::start_reencrypt_job(state.pool.clone(), batch_size as i64) {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "integration credential re-encryption job already running" })),
            ));
        }
        jobs.push("integration_credentials");
    }

    #[cfg(feature = "encryption")]
    {
        if let (Some(new_key), Some(old_key)) = (state.encryption_key, state.encryption_key_old) {
            // Create or get the reencrypt state from app state
            // For now, we'll create a new one per request (in production, store in AppState)
            let reencrypt_state = crate::reencrypt::ReencryptState::new();

            if reencrypt_state.is_running() {
                return Err((
                    StatusCode::CONFLICT,
                    Json(json!({ "error": "re-encryption job already running" })),
                ));
            }

            crate::reencrypt::start_reencrypt_job(
                state.pool.clone(),
                new_key,
                old_key,
                batch_size,
                reencrypt_state,
            );
            jobs.push("events");
        }
    }

    if jobs.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "nothing to re-encrypt: configure ENCRYPTION_KEY and ENCRYPTION_KEY_OLD \
                          (events, requires the encryption feature) and/or INTEGRATION_ENCRYPTION_KEY \
                          and INTEGRATION_ENCRYPTION_KEY_OLD (integration credentials)"
            })),
        ));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "message": "re-encryption job started",
            "jobs": jobs,
            "batch_size": batch_size
        })),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/contracts",
    tag = "events",
    params(
        ("page" = Option<i64>, Query, description = "Page number (default 1)"),
        ("limit" = Option<i64>, Query, description = "Items per page (1-100, default 20)"),
        ("sort" = Option<String>, Query, description = "Sort order: event_count_desc, event_count_asc, last_seen_desc (default), first_seen_asc"),
    ),
    responses(
        (status = 200, description = "Paginated list of indexed contract IDs with event counts, ledger info, deployer, WASM hash and label"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 429, description = "Too many requests", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn get_contracts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, AppError> {
    let limit = params.limit();
    let offset = params.offset();

    // Determine sort order
    let sort_clause = match params.sort {
        Some(SortOrder::Asc) => "ORDER BY event_count ASC",
        _ => "ORDER BY last_seen_ledger DESC",
    };

    // Issue #1255: join the deployment registry and the labels registry.
    let rows = sqlx::query_as::<_, ContractSummary>(
        &format!(
            "SELECT s.*, c.deployer, c.wasm_hash, c.deployed_at, m.name AS label FROM ( \
                 SELECT contract_id, COUNT(*) AS event_count, MIN(ledger) AS first_seen_ledger, \
                 MAX(ledger) AS last_seen_ledger, MAX(timestamp) AS last_event_at \
                 FROM events GROUP BY contract_id {} \
                 LIMIT $1 OFFSET $2 \
             ) s \
             LEFT JOIN contracts c ON c.contract_id = s.contract_id \
             LEFT JOIN contract_metadata m ON m.contract_id = s.contract_id \
             {}",
            sort_clause, sort_clause
        ),
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.read_pool)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT contract_id) FROM events")
        .fetch_one(&state.read_pool)
        .await?;

    let result = json!({
        "data": rows,
        "total": total,
        "page": params.page.unwrap_or(1),
        "limit": limit,
    });

    Ok(Json(result))
}

/// In-process TTL cache for per-contract summary data.
static CONTRACT_SUMMARY_CACHE: OnceLock<Mutex<std::collections::HashMap<String, (ContractDetailSummary, std::time::Instant)>>> = OnceLock::new();

fn contract_summary_cache() -> &'static Mutex<std::collections::HashMap<String, (ContractDetailSummary, std::time::Instant)>> {
    CONTRACT_SUMMARY_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// TTL for contract summary cache entries (configurable via env, default 60s).
fn summary_cache_ttl() -> std::time::Duration {
    let secs: u64 = std::env::var("CONTRACT_SUMMARY_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    std::time::Duration::from_secs(secs)
}

/// GET /v1/contracts/:contract_id/summary
///
/// Returns a per-contract summary: total events, first/last event timestamp,
/// event type breakdown, unique tx count, and ledger range.
///
/// Uses the `mv_contract_summary` materialized view as the primary data source.
/// Falls back to a direct query if the view is stale or unavailable.
/// Results are cached in-process with a configurable TTL.
#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/summary",
    tag = "events",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID (56-char, starts with C)"),
    ),
    responses(
        (status = 200, description = "Contract summary", body = crate::models::ContractDetailSummary),
        (status = 400, description = "Invalid contract_id format", body = ErrorResponse),
        (status = 404, description = "Contract not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_contract_summary(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;

    let ttl = summary_cache_ttl();

    // Check in-process cache
    {
        let cache = contract_summary_cache().lock().await;
        if let Some((summary, inserted_at)) = cache.get(&contract_id) {
            if inserted_at.elapsed() < ttl {
                return Ok(Json(serde_json::to_value(summary)?));
            }
        }
    }

    // Try the canonical materialized view first
    let mv_row = sqlx::query(
        "SELECT event_count, first_event_timestamp, latest_event_timestamp, min_ledger, \
         max_ledger, unique_tx_count, contract_events, diagnostic_events, system_events \
         FROM mv_contract_summary WHERE contract_id = $1",
    )
    .bind(&contract_id)
    .fetch_optional(&state.read_pool)
    .await;

    let summary = match mv_row {
        Ok(Some(row)) => {
            ContractDetailSummary {
                contract_id: contract_id.clone(),
                total_events: row.try_get("event_count")?,
                first_event_at: row.try_get("first_event_timestamp")?,
                last_event_at: row.try_get("latest_event_timestamp")?,
                unique_tx_count: row.try_get("unique_tx_count")?,
                ledger_range: LedgerRange {
                    min: row.try_get("min_ledger")?,
                    max: row.try_get("max_ledger")?,
                },
                event_type_breakdown: EventTypeBreakdown {
                    contract: row.try_get("contract_events")?,
                    diagnostic: row.try_get("diagnostic_events")?,
                    system: row.try_get("system_events")?,
                },
                from_cache: true,
            }
        }
        // Materialized view missing or stale — fall back to direct query
        _ => {
            let row = sqlx::query(
                "SELECT \
                    COUNT(*)                                                        AS total_events, \
                    MIN(timestamp)                                                  AS first_event_at, \
                    MAX(timestamp)                                                  AS last_event_at, \
                    MIN(ledger)                                                     AS min_ledger, \
                    MAX(ledger)                                                     AS max_ledger, \
                    COUNT(DISTINCT tx_hash)                                         AS unique_tx_count, \
                    COUNT(*) FILTER (WHERE event_type = 'contract')                 AS contract_events, \
                    COUNT(*) FILTER (WHERE event_type = 'diagnostic')               AS diagnostic_events, \
                    COUNT(*) FILTER (WHERE event_type = 'system')                   AS system_events \
                 FROM events WHERE contract_id = $1",
            )
            .bind(&contract_id)
            .fetch_one(&state.read_pool)
            .await?;

            let total_events: i64 = row.try_get("total_events")?;
            if total_events == 0 {
                return Err(AppError::NotFound);
            }

            ContractDetailSummary {
                contract_id: contract_id.clone(),
                total_events,
                first_event_at: row.try_get("first_event_at")?,
                last_event_at: row.try_get("last_event_at")?,
                unique_tx_count: row.try_get("unique_tx_count")?,
                ledger_range: LedgerRange {
                    min: row.try_get("min_ledger")?,
                    max: row.try_get("max_ledger")?,
                },
                event_type_breakdown: EventTypeBreakdown {
                    contract: row.try_get("contract_events")?,
                    diagnostic: row.try_get("diagnostic_events")?,
                    system: row.try_get("system_events")?,
                },
                from_cache: false,
            }
        }
    };

    // Check for not-found after materialized view path
    if summary.total_events == 0 {
        return Err(AppError::NotFound);
    }

    // Store in cache
    {
        let mut cache = contract_summary_cache().lock().await;
        cache.insert(contract_id, (summary.clone(), std::time::Instant::now()));
    }

    Ok(Json(serde_json::to_value(&summary)?))
}

/// GET /v1/contracts/search?q=prefix
///
/// Returns contract IDs matching the given prefix (minimum 4 characters).
/// Uses a `LIKE 'prefix%'` query against the existing `contract_id` index.
#[utoipa::path(
    get,
    path = "/v1/contracts/search",
    tag = "events",
    params(
        ("q" = String, Query, description = "Contract ID prefix to search for (minimum 4 characters)"),
        ("limit" = Option<i64>, Query, description = "Maximum results to return (1–100, default 20)"),
    ),
    responses(
        (status = 200, description = "Matching contract IDs with event counts", body = Vec<crate::models::ContractSearchResult>),
        (status = 400, description = "Prefix too short (< 4 chars) or missing", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_contracts_search(
    State(state): State<AppState>,
    Query(params): Query<ContractSearchParams>,
) -> Result<Json<Value>, AppError> {
    let prefix = params.q.as_deref().unwrap_or("").trim().to_string();

    if prefix.len() < 4 {
        return Err(AppError::Validation(
            "q must be at least 4 characters to prevent full-table scans".to_string(),
        ));
    }

    // Sanitize: only allow alphanumeric characters in the prefix to prevent injection
    if !prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "q must contain only alphanumeric characters".to_string(),
        ));
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let like_pattern = format!("{}%", prefix);

    let rows = sqlx::query_as::<_, ContractSearchResult>(
        "SELECT contract_id, COUNT(*) AS event_count, MAX(timestamp) AS last_event_at \
         FROM events WHERE contract_id LIKE $1 \
         GROUP BY contract_id ORDER BY event_count DESC LIMIT $2",
    )
    .bind(&like_pattern)
    .bind(limit)
    .fetch_all(&state.read_pool)
    .await?;

    Ok(Json(json!({
        "data": rows,
        "query": prefix,
        "limit": limit,
    })))
}

/// Query the min and max indexed ledger from the events table.
/// Returns `(None, None)` when no events have been indexed yet.
async fn get_indexed_ledger_range(
    pool: &sqlx::PgPool,
) -> Result<(Option<i64>, Option<i64>), AppError> {
    let row = sqlx::query("SELECT MIN(ledger) AS min_ledger, MAX(ledger) AS max_ledger FROM events")
        .fetch_one(pool)
        .await?;
    let min: Option<i64> = row.try_get("min_ledger")?;
    let max: Option<i64> = row.try_get("max_ledger")?;
    Ok((min, max))
}

/// Replay events for a specific ledger range.
///
/// The requested range is validated against the indexed window:
/// - **400** if the range is entirely outside the indexed window (no overlap).
/// - **202** with a `warning` field if the range is only partially indexed.
#[utoipa::path(
    post,
    path = "/v1/admin/replay",
    tag = "admin",
    request_body(content = ReplayRequest, description = "Ledger range to replay", content_type = "application/json"),
    responses(
        (status = 202, description = "Replay job accepted and queued. A `warning` field is included when the requested range is only partially covered by the indexed window."),
        (status = 400, description = "Invalid request parameters, or the requested range is entirely outside the indexed window"),
        (status = 401, description = "Unauthorized - API key required"),
        (status = 403, description = "Forbidden - not the active indexer"),
    )
)]
/// Preview how a Lua transformation script would affect a set of events
/// without writing any changes to the database.
///
/// Applies the same CPU instruction limit, memory limit, and timeout as the
/// production transformer so operator can verify safety before deploying.
#[utoipa::path(
    post,
    path = "/v1/admin/lua/preview",
    tag = "admin",
    request_body = models::LuaPreviewRequest,
    params(
        ("X-Admin-API-Key" = String, Header, description = "Admin API key"),
    ),
    responses(
        (status = 200, description = "Preview results — original and transformed event data side-by-side",
            body = models::LuaPreviewResponse),
        (status = 400, description = "Invalid request — too many event_ids or empty script"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "One or more event_ids not found"),
    )
)]
pub async fn lua_preview(
    State(state): State<AppState>,
    Json(req): Json<models::LuaPreviewRequest>,
) -> Result<Json<models::LuaPreviewResponse>, AppError> {
    const MAX_PREVIEW_EVENTS: usize = 20;

    if req.script.trim().is_empty() {
        return Err(AppError::Validation("script must not be empty".into()));
    }
    if req.event_ids.is_empty() {
        return Err(AppError::Validation("event_ids must not be empty".into()));
    }
    if req.event_ids.len() > MAX_PREVIEW_EVENTS {
        return Err(AppError::Validation(format!(
            "event_ids must contain at most {MAX_PREVIEW_EVENTS} entries"
        )));
    }

    // Fetch the requested events from the DB (read-only)
    let ids: Vec<uuid::Uuid> = req.event_ids.clone();
    let rows = sqlx::query_as::<_, crate::models::Event>(
        "SELECT * FROM events WHERE id = ANY($1) LIMIT 20",
    )
    .bind(&ids)
    .fetch_all(&state.read_pool)
    .await?;

    if rows.is_empty() {
        return Err(AppError::NotFound);
    }

    // Map Event → SorobanEvent for the transformer
    let events: Vec<crate::models::SorobanEvent> = rows
        .into_iter()
        .map(|r| crate::models::SorobanEvent {
            contract_id: r.contract_id,
            event_type: format!("{:?}", r.event_type).to_lowercase(),
            tx_hash: r.tx_hash,
            ledger: r.ledger as u64,
            ledger_closed_at: r.timestamp.to_rfc3339(),
            ledger_hash: r.ledger_hash,
            in_successful_call: r.in_successful_call,
            value: r.event_data.clone(),
            topic: r.event_data
                .get("topic")
                .and_then(|t| t.as_array())
                .cloned(),
        })
        .collect();

    let timeout_ms = state.config.event_transform_timeout_ms;

    let mut results =
        crate::lua_transform::LuaTransformer::preview_events(req.script, events, timeout_ms).await;

    // Re-attach the caller-supplied UUIDs so the response matches the request
    for (item, id) in results.iter_mut().zip(req.event_ids.iter()) {
        item.event_id = *id;
    }

    Ok(Json(models::LuaPreviewResponse { results }))
}

/// Replay already-indexed events through a Lua transformation script (Issue #700).
///
/// Fetches events matching the ledger and/or timestamp window (and optional
/// `contract_id` filter), runs each one through `transformation_script`, and
/// reports per-event results. In `dry_run` mode nothing is redelivered — the
/// endpoint only reports what the script *would* do. Otherwise, transformed
/// events (script returned a table, not `nil`) are broadcast on the same
/// channel newly-indexed events use, so they flow through the normal
/// downstream pipeline (webhooks, SSE, email, etc.) exactly like a live event.
#[utoipa::path(
    post,
    path = "/v1/replay/with-transform",
    tag = "admin",
    request_body = models::ReplayWithTransformRequest,
    responses(
        (status = 200, description = "Replay completed or previewed", body = models::ReplayWithTransformResponse),
        (status = 400, description = "Invalid request or transformation script"),
    )
)]
pub async fn replay_with_transform(
    State(state): State<AppState>,
    Json(request): Json<models::ReplayWithTransformRequest>,
) -> Result<Json<models::ReplayWithTransformResponse>, AppError> {
    if request.transformation_script.trim().is_empty() {
        return Err(AppError::Validation(
            "transformation_script must not be empty".to_string(),
        ));
    }
    if request.from_ledger.is_none() && request.from_timestamp.is_none() {
        return Err(AppError::Validation(
            "either from_ledger or from_timestamp is required".to_string(),
        ));
    }
    if let (Some(from), Some(to)) = (request.from_ledger, request.to_ledger) {
        if from > to {
            return Err(AppError::Validation(
                "from_ledger must be <= to_ledger".to_string(),
            ));
        }
    }
    if let (Some(from), Some(to)) = (request.from_timestamp, request.to_timestamp) {
        if from > to {
            return Err(AppError::Validation(
                "from_timestamp must be <= to_timestamp".to_string(),
            ));
        }
    }

    let wrapped_script =
        crate::lua_transform::LuaTransformer::wrap_script_body(&request.transformation_script);
    crate::lua_transform::LuaTransformer::validate_syntax(&wrapped_script)
        .map_err(|e| AppError::Validation(format!("invalid transformation script: {e}")))?;

    let limit = request.limit.unwrap_or(100).clamp(1, 1000);

    let mut query = QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, contract_id, event_type, tx_hash, ledger, timestamp, event_data \
         FROM events WHERE ",
    );
    let mut needs_and = false;
    if let Some(from_ledger) = request.from_ledger {
        query.push("ledger >= ");
        query.push_bind(from_ledger);
        needs_and = true;
    }
    if let Some(to_ledger) = request.to_ledger {
        if needs_and {
            query.push(" AND ");
        }
        query.push("ledger <= ");
        query.push_bind(to_ledger);
        needs_and = true;
    }
    if let Some(from_timestamp) = request.from_timestamp {
        if needs_and {
            query.push(" AND ");
        }
        query.push("timestamp >= ");
        query.push_bind(from_timestamp);
        needs_and = true;
    }
    if let Some(to_timestamp) = request.to_timestamp {
        if needs_and {
            query.push(" AND ");
        }
        query.push("timestamp <= ");
        query.push_bind(to_timestamp);
        needs_and = true;
    }
    if let Some(ref contract_id) = request.contract_id {
        if needs_and {
            query.push(" AND ");
        }
        query.push("contract_id = ");
        query.push_bind(contract_id);
    }
    query.push(" ORDER BY ledger ASC LIMIT ");
    query.push_bind(limit);

    let rows = query.build().fetch_all(&state.pool).await?;

    let ids: Vec<uuid::Uuid> = rows
        .iter()
        .map(|r| r.try_get::<uuid::Uuid, _>("id"))
        .collect::<Result<_, _>>()?;
    let events: Vec<crate::models::SorobanEvent> = rows
        .iter()
        .map(|r| -> Result<_, sqlx::Error> {
            let timestamp: DateTime<Utc> = r.try_get("timestamp")?;
            Ok(crate::models::SorobanEvent {
                id: None,
                contract_id: r.try_get("contract_id")?,
                event_type: r.try_get("event_type")?,
                tx_hash: r.try_get("tx_hash")?,
                ledger: r.try_get::<i64, _>("ledger")? as u64,
                ledger_closed_at: timestamp.to_rfc3339(),
                ledger_hash: None,
                in_successful_call: true,
                value: r.try_get("event_data")?,
                topic: None,
                tenant_id: None,
            })
        })
        .collect::<Result<_, _>>()?;

    let total_events = events.len() as i64;
    let timeout_ms = state.config.event_transform_timeout_ms;
    let mut preview =
        crate::lua_transform::LuaTransformer::preview_events(wrapped_script, events, timeout_ms)
            .await;
    for (item, id) in preview.iter_mut().zip(ids.iter()) {
        item.event_id = *id;
    }

    let transformed_events = preview.iter().filter(|i| i.transformed.is_some()).count() as i64;
    let error_events = preview.iter().filter(|i| i.error.is_some()).count() as i64;
    let skipped_events = total_events - transformed_events - error_events;

    if !request.dry_run {
        for item in preview.iter() {
            if let Some(ref transformed) = item.transformed {
                if let Ok(event) =
                    serde_json::from_value::<crate::models::SorobanEvent>(transformed.clone())
                {
                    let _ = state.event_tx.send(event);
                }
            }
        }
    }

    Ok(Json(models::ReplayWithTransformResponse {
        replay_id: uuid::Uuid::new_v4(),
        status: if request.dry_run {
            "dry_run".to_string()
        } else {
            "completed".to_string()
        },
        dry_run: request.dry_run,
        total_events,
        transformed_events,
        skipped_events,
        error_events,
        preview,
    }))
}

/// Execute the replay job using the same fetch_and_store_events logic
async fn execute_replay_job(
    pool: sqlx::PgPool,
    rpc_url: &str,
    from_ledger: u64,
    to_ledger: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!(
        from_ledger = from_ledger,
        to_ledger = to_ledger,
        "Starting replay job"
    );

    // Create RPC client
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()?;

    let mut current_ledger = from_ledger;

    while current_ledger <= to_ledger {
        // Fetch events for current ledger range
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": {
                "filters": [],
                "pagination": {"limit": 100},
                "startLedger": current_ledger
            }
        });

        let response = client.post(rpc_url).json(&body).send().await?;

        if !response.status().is_success() {
            return Err(format!("RPC request failed: {}", response.status()).into());
        }

        let rpc_response: crate::models::RpcResponse<crate::models::GetEventsResult> =
            response.json().await?;

        let result = match rpc_response.result {
            Some(r) => r,
            None => {
                if let Some(err) = rpc_response.error {
                    return Err(format!("RPC error: {}", err.message).into());
                } else {
                    return Err("RPC returned no result".into());
                }
            }
        };

        // Store events with ON CONFLICT DO NOTHING for idempotency
        for event in result.events {
            if let Err(e) = store_event_with_idempotency(&pool, &event).await {
                tracing::warn!(error = %e, "Failed to store event during replay");
            }
        }

        // Move to next ledger or break if we've reached the end
        if result.latest_ledger >= current_ledger {
            current_ledger = result.latest_ledger + 1;
        } else {
            break;
        }

        // Add small delay to avoid overwhelming the RPC
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tracing::info!(
        from_ledger = from_ledger,
        to_ledger = to_ledger,
        "Replay job completed"
    );
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// #499 – Notification channel failover
// #500 – Notification analytics dashboard
// #501 – Notification cost tracking
// #502 – Notification channel testing mode
// ─────────────────────────────────────────────────────────────────────────────

/// GET /v1/admin/notifications/dashboard
///
/// Returns a comprehensive analytics summary: totals per window, per-channel delivery
/// success rates, average delivery latency, and the top-10 most-notified contracts.
pub async fn notification_dashboard(
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Total notifications sent in each time window
    let windows: &[(&str, &str)] = &[
        ("last_24h", "NOW() - INTERVAL '24 hours'"),
        ("last_7d",  "NOW() - INTERVAL '7 days'"),
        ("last_30d", "NOW() - INTERVAL '30 days'"),
    ];

    let mut totals = serde_json::Map::new();
    for (label, interval) in windows {
        let sql = format!(
            "SELECT COUNT(*) FROM notification_costs WHERE sent_at >= {interval}"
        );
        let count: i64 = sqlx::query_scalar(&sql)
            .fetch_one(&state.pool)
            .await
            .unwrap_or(0);
        totals.insert(label.to_string(), serde_json::json!(count));
    }

    // Per-channel delivery success rate (based on webhook_failures vs notification_costs)
    let channel_rows = sqlx::query(
        r#"
        SELECT
            nc.id::text               AS channel_id,
            nc.name                   AS channel_name,
            nc.channel_type           AS channel_type,
            COUNT(DISTINCT ncost.id)  AS sent,
            COUNT(DISTINCT wf.id)     AS failed
        FROM notification_channels nc
        LEFT JOIN notification_costs ncost ON ncost.channel_id = nc.id
            AND ncost.sent_at >= NOW() - INTERVAL '30 days'
        LEFT JOIN webhook_failures wf ON wf.url = (nc.config->>'url')
            AND wf.created_at >= NOW() - INTERVAL '30 days'
        GROUP BY nc.id, nc.name, nc.channel_type
        "#
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default();

    let channels: Vec<serde_json::Value> = channel_rows
        .into_iter()
        .map(|r| {
            let sent: i64 = r.try_get("sent").unwrap_or(0);
            let failed: i64 = r.try_get("failed").unwrap_or(0);
            let success_rate = if sent == 0 {
                1.0_f64
            } else {
                (sent - failed).max(0) as f64 / sent as f64
            };
            serde_json::json!({
                "channel_id":   r.try_get::<String, _>("channel_id").unwrap_or_default(),
                "channel_name": r.try_get::<String, _>("channel_name").unwrap_or_default(),
                "channel_type": r.try_get::<String, _>("channel_type").unwrap_or_default(),
                "sent_30d":     sent,
                "failed_30d":   failed,
                "success_rate": (success_rate * 1000.0).round() / 1000.0,
            })
        })
        .collect();

    // Top-10 most-notified contracts (from events table, last 30d)
    let top_contracts = sqlx::query(
        r#"
        SELECT contract_id, COUNT(*) AS event_count
        FROM events
        WHERE created_at >= NOW() - INTERVAL '30 days'
        GROUP BY contract_id
        ORDER BY event_count DESC
        LIMIT 10
        "#
    )
    .fetch_all(&state.pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|r| {
        serde_json::json!({
            "contract_id":  r.try_get::<String, _>("contract_id").unwrap_or_default(),
            "event_count":  r.try_get::<i64, _>("event_count").unwrap_or(0),
        })
    })
    .collect::<Vec<_>>();

    Json(serde_json::json!({
        "totals":       totals,
        "channels":     channels,
        "top_contracts": top_contracts,
    }))
}

/// GET /v1/admin/notifications/costs
///
/// Returns cumulative notification cost breakdown by channel and optional time period.
pub async fn notification_costs(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let period = params.get("period").map(|s| s.as_str()).unwrap_or("30d");
    let interval = match period {
        "24h" => "24 hours",
        "7d"  => "7 days",
        "30d" => "30 days",
        _     => "30 days",
    };

    let sql = format!(
        r#"
        SELECT
            channel_id::text,
            channel_name,
            channel_type,
            COUNT(*)          AS notification_count,
            SUM(cost_cents)   AS total_cost_cents
        FROM notification_costs
        WHERE sent_at >= NOW() - INTERVAL '{interval}'
        GROUP BY channel_id, channel_name, channel_type
        ORDER BY total_cost_cents DESC
        "#
    );

    let rows = sqlx::query(&sql)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

    let breakdown: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|r| {
            let total_cents: i64 = r.try_get("total_cost_cents").unwrap_or(0);
            serde_json::json!({
                "channel_id":          r.try_get::<String, _>("channel_id").unwrap_or_default(),
                "channel_name":        r.try_get::<String, _>("channel_name").unwrap_or_default(),
                "channel_type":        r.try_get::<String, _>("channel_type").unwrap_or_default(),
                "notification_count":  r.try_get::<i64, _>("notification_count").unwrap_or(0),
                "total_cost_usd":      total_cents as f64 / 100.0,
            })
        })
        .collect();

    // Warn if monthly budget exceeded (config field, defaults to 0 = no limit)
    let monthly_budget_usd: f64 = state
        .config
        .notification_monthly_budget_usd
        .unwrap_or(0.0);

    let total_usd: f64 = breakdown
        .iter()
        .filter_map(|b| b["total_cost_usd"].as_f64())
        .sum();

    if monthly_budget_usd > 0.0 && total_usd >= monthly_budget_usd * 0.9 {
        tracing::warn!(
            total_usd = total_usd,
            budget_usd = monthly_budget_usd,
            "Notification cost is approaching or has exceeded the monthly budget"
        );
    }

    Json(serde_json::json!({
        "period":        period,
        "total_cost_usd": (total_usd * 100.0).round() / 100.0,
        "breakdown":     breakdown,
    }))
}

/// POST /v1/admin/notifications/channels/:id/test
///
/// Sends a test notification on the specified channel and returns the result.
pub async fn test_notification_channel(
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
) -> impl IntoResponse {
    let channel_uuid = match channel_id.parse::<uuid::Uuid>() {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid channel id" })),
            ).into_response();
        }
    };

    let row = sqlx::query(
        "SELECT id::text AS channel_id, name, channel_type, config \
         FROM notification_channels WHERE id = $1"
    )
    .bind(channel_uuid)
    .fetch_optional(&state.pool)
    .await;

    let channel = match row {
        Ok(Some(r)) => r,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "channel not found" })),
            ).into_response();
        }
        Err(e) => {
            tracing::error!(error = %e, "DB error looking up notification channel");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "internal error" })),
            ).into_response();
        }
    };

    let channel_name: String = channel.try_get("name").unwrap_or_default();
    let channel_type: String = channel.try_get("channel_type").unwrap_or_default();
    let config: serde_json::Value = channel.try_get("config").unwrap_or(serde_json::json!({}));
    // Issue #1162: sensitive config values are stored encrypted.
    let config = match crate::integration_secrets::open_config(&config) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(error = %e, "Failed to decrypt notification channel credentials");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to decrypt channel credentials" })),
            ).into_response();
        }
    };

    let test_subject = format!("[TEST] Soroban Pulse notification test – channel '{channel_name}'");
    let test_body = format!(
        "[TEST] This is a test notification from Soroban Pulse.\nChannel: {channel_name}\nType: {channel_type}\nSent at: {}",
        chrono::Utc::now().to_rfc3339(),
    );

    let success = match channel_type.as_str() {
        "webhook" => {
            let url = config.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if url.is_empty() {
                false
            } else {
                let client = reqwest::Client::new();
                let payload = serde_json::json!({
                    "subject": test_subject,
                    "body":    test_body,
                    "test":    true,
                });
                client.post(&url)
                    .header("Content-Type", "application/json")
                    .body(payload.to_string())
                    .send()
                    .await
                    .map(|r| r.status().is_success())
                    .unwrap_or(false)
            }
        }
        "email" | "sms" => {
            tracing::info!(
                channel_id = %channel_id,
                channel_type = %channel_type,
                subject = %test_subject,
                "Test notification logged (email/SMS delivery requires live credentials)"
            );
            true
        }
        _ => false,
    };

    crate::metrics::record_notification_test(&channel_type, success);

    let status = if success { StatusCode::OK } else { StatusCode::BAD_GATEWAY };
    (
        status,
        Json(serde_json::json!({
            "channel_id":   channel_id,
            "channel_name": channel_name,
            "channel_type": channel_type,
            "success":      success,
            "subject":      test_subject,
        })),
    ).into_response()
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod temporal_tests {
    use super::*;
    use crate::config::{HealthState, IndexerState};
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use sqlx::PgPool;
    use std::sync::Arc;
    use tower::ServiceExt;

    fn create_test_router(pool: PgPool) -> axum::Router {
        let health_state = Arc::new(HealthState::new(60));
        let indexer_state = Arc::new(IndexerState::new());
        let prometheus_handle = crate::metrics::init_metrics();
        let config = crate::config::Config::default();
        crate::routes::create_router(
            pool,
            Vec::new(),
            &[],
            60,
            health_state,
            indexer_state,
            prometheus_handle,
            2000,
            config,
        )
    }

    // ── parse_relative_duration unit tests ────────────────────────────────────

    #[test]
    fn parse_seconds() {
        let d = parse_relative_duration("30s").unwrap();
        assert_eq!(d, chrono::Duration::seconds(30));
    }

    #[test]
    fn parse_minutes() {
        let d = parse_relative_duration("5m").unwrap();
        assert_eq!(d, chrono::Duration::minutes(5));
    }

    #[test]
    fn parse_hours() {
        let d = parse_relative_duration("24h").unwrap();
        assert_eq!(d, chrono::Duration::hours(24));
    }

    #[test]
    fn parse_days() {
        let d = parse_relative_duration("7d").unwrap();
        assert_eq!(d, chrono::Duration::days(7));
    }

    #[test]
    fn parse_weeks() {
        let d = parse_relative_duration("2w").unwrap();
        assert_eq!(d, chrono::Duration::weeks(2));
    }

    #[test]
    fn parse_invalid_unit_returns_error() {
        assert!(parse_relative_duration("1y").is_err());
    }

    #[test]
    fn parse_missing_unit_returns_error() {
        assert!(parse_relative_duration("42").is_err());
    }

    #[test]
    fn parse_zero_returns_error() {
        assert!(parse_relative_duration("0h").is_err());
    }

    #[test]
    fn parse_negative_returns_error() {
        assert!(parse_relative_duration("-1h").is_err());
    }

    #[test]
    fn parse_empty_returns_error() {
        assert!(parse_relative_duration("").is_err());
    }

    // ── /v1/events/temporal integration tests ─────────────────────────────────

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_missing_since_and_from_returns_400(pool: PgPool) {
        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_since_24h_returns_200_empty(pool: PgPool) {
        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal?since=24h")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["events"], serde_json::json!([]));
        assert_eq!(v["total"], 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_aggregate_returns_buckets(pool: PgPool) {
        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal?since=1d&aggregate=true&window=1h")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        // On an empty DB the buckets array should be empty (no events in last 24h)
        assert!(v["buckets"].is_array());
        assert!(v["events"].is_array());
        assert_eq!(v["events"], serde_json::json!([]));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_since_and_from_timestamp_mutually_exclusive(pool: PgPool) {
        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal?since=1d&from_timestamp=2026-01-01T00:00:00Z")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_invalid_window_returns_400(pool: PgPool) {
        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal?since=1d&aggregate=true&window=2d")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_returns_event_within_window(pool: PgPool) {
        // Insert an event that was created just now (well within 24h window).
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ('CABC', 'contract', 'txabc123temporal', 100, NOW(), '{}'::jsonb)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal?since=24h")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["total"], 1);
        assert_eq!(v["events"].as_array().unwrap().len(), 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn temporal_event_outside_window_not_returned(pool: PgPool) {
        // Insert an event two days ago — outside the 1h window.
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ('CABC', 'contract', 'txold_temporal', 99, NOW() - INTERVAL '2 days', '{}'::jsonb)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let app = create_test_router(pool);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/temporal?since=1h")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["total"], 0);
    }
}

// ============================================================================
// Issue #627: Bloom filter based contract existence check
// ============================================================================

/// Check if a contract has any indexed events using the bloom filter.
///
/// `GET /v1/contracts/exists?id=CABC...`
///
/// Returns whether the contract is likely to exist based on the bloom filter.
/// This is a fast check without a database query.
#[utoipa::path(
    get,
    path = "/v1/contracts/exists",
    tag = "contracts",
    params(
        ("id", description = "Contract ID to check"),
    ),
    responses(
        (status = 200, description = "Contract existence check result", body = ContractExistsResponse),
        (status = 400, description = "Invalid contract ID"),
    )
)]
pub async fn check_contract_exists(
    State(state): State<AppState>,
    Query(params): Query<ContractExistsParams>,
) -> Result<impl IntoResponse, AppError> {
    use models::ContractId;
    
    let contract_id = params.id.trim();
    
    // Validate contract ID format
    ContractId::validate(contract_id)?;
    
    // Check if contract exists in bloom filter
    let exists_in_filter = if let Some(ref filter) = state.indexer_state.bloom_filter {
        filter.contains_contract(contract_id)
    } else {
        // If bloom filter is not available, do a DB lookup
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE contract_id = $1 LIMIT 1"
        )
        .bind(contract_id)
        .fetch_one(&state.pool)
        .await?;
        count > 0
    };
    
    Ok(Json(models::ContractExistsResponse {
        contract_id: contract_id.to_string(),
        exists: exists_in_filter,
        method: if state.indexer_state.bloom_filter.is_some() {
            "bloom_filter".to_string()
        } else {
            "database_query".to_string()
        },
    }))
}

#[derive(serde::Deserialize)]
pub struct ContractExistsParams {
    pub id: String,
}


// ============================================================================
// Issue #631: Dynamic config reloading via API
// ============================================================================

/// Get current configuration (non-secret values).
///
/// `GET /v1/config`
///
/// Returns the current configuration state for the service, excluding secret values.
#[utoipa::path(
    get,
    path = "/v1/config",
    tag = "system",
    responses(
        (status = 200, description = "Current configuration", body = ConfigResponse),
    )
)]
pub async fn get_config(
    State(state): State<AppState>,
) -> impl IntoResponse {
    Json(models::ConfigResponse {
        log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        rate_limit_per_minute: state.config.rate_limit_per_minute,
        sse_keepalive_secs: state.sse_keepalive_interval_ms / 1000,
        health_check_timeout_ms: state.health_check_timeout_ms,
        db_max_connections: state.config.db_max_connections,
        indexer_lag_warn_threshold: state.config.indexer_lag_warn_threshold,
        slow_query_threshold_ms: state.config.slow_query_threshold_ms,
        features: state.config.features.clone(),
    })
}

/// Reload configuration without restarting.
///
/// `POST /v1/admin/config/reload`
///
/// Reloads supported configuration options at runtime:
/// - Log level (RUST_LOG)
/// - Rate limits
/// - SSE keepalive interval
/// - TTL settings
///
/// Requires ADMIN_API_KEY.
#[utoipa::path(
    post,
    path = "/v1/admin/config/reload",
    tag = "admin",
    request_body = ConfigReloadRequest,
    responses(
        (status = 200, description = "Configuration reloaded successfully", body = ConfigReloadResponse),
        (status = 400, description = "Invalid configuration"),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn reload_config(
    State(state): State<AppState>,
    Json(req): Json<models::ConfigReloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    use tracing::info;
    
    // Track which settings were updated for audit logging
    let mut updated_settings = Vec::new();
    
    // Validate new config before applying
    if let Some(ref rate_limit) = req.rate_limit_per_minute {
        if *rate_limit == 0 {
            return Err(AppError::Validation(
                "rate_limit_per_minute must be > 0".to_string(),
            ));
        }
        updated_settings.push(format!("rate_limit_per_minute: {}", rate_limit));
    }

    // Issue #669 / #666: per-key rate limit hot-reload
    if let Some(v) = req.rate_limit_key_per_minute {
        updated_settings.push(format!("rate_limit_key_per_minute: {}", v));
        // 0 means "disable" — set the env var so the next config load picks it up
        if v == 0 {
            std::env::remove_var("RATE_LIMIT_KEY_PER_MINUTE");
        } else {
            std::env::set_var("RATE_LIMIT_KEY_PER_MINUTE", v.to_string());
        }
    }
    if let Some(v) = req.rate_limit_key_per_hour {
        updated_settings.push(format!("rate_limit_key_per_hour: {}", v));
        if v == 0 {
            std::env::remove_var("RATE_LIMIT_KEY_PER_HOUR");
        } else {
            std::env::set_var("RATE_LIMIT_KEY_PER_HOUR", v.to_string());
        }
    }
    if let Some(v) = req.rate_limit_key_per_day {
        updated_settings.push(format!("rate_limit_key_per_day: {}", v));
        if v == 0 {
            std::env::remove_var("RATE_LIMIT_KEY_PER_DAY");
        } else {
            std::env::set_var("RATE_LIMIT_KEY_PER_DAY", v.to_string());
        }
    }
    
    if let Some(ref log_level) = req.log_level {
        // Validate log level is one of: trace, debug, info, warn, error
        if !matches!(log_level.to_lowercase().as_str(), "trace" | "debug" | "info" | "warn" | "error") {
            return Err(AppError::Validation(
                "log_level must be one of: trace, debug, info, warn, error".to_string(),
            ));
        }
        updated_settings.push(format!("log_level: {}", log_level));
        // Note: Setting RUST_LOG at runtime requires re-initializing the tracing subscriber,
        // which we don't do in this implementation. In production, you might use
        // a tracing-reload crate or similar.
        std::env::set_var("RUST_LOG", log_level);
    }
    
    if let Some(ref sse_keepalive) = req.sse_keepalive_secs {
        if *sse_keepalive < 1 || *sse_keepalive > 60 {
            return Err(AppError::Validation(
                "sse_keepalive_secs must be between 1 and 60".to_string(),
            ));
        }
        updated_settings.push(format!("sse_keepalive_secs: {}", sse_keepalive));
    }
    
    // Audit log the configuration change
    if !updated_settings.is_empty() {
        crate::audit_logging::log_event(
            &state.pool,
            "CONFIG_RELOAD",
            Some(&format!("Updated: {}", updated_settings.join(", "))),
        )
        .await
        .ok();
        
        info!(
            updated_fields = ?updated_settings,
            "configuration reloaded dynamically"
        );
    }
    
    Ok(Json(models::ConfigReloadResponse {
        success: true,
        message: format!("Configuration reloaded. Updated {} settings.", updated_settings.len()),
        updated_fields: updated_settings,
    }))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ConfigReloadRequest {
    /// New log level: trace, debug, info, warn, error
    #[serde(default)]
    pub log_level: Option<String>,
    
    /// New rate limit per minute per IP
    #[serde(default)]
    pub rate_limit_per_minute: Option<u32>,

    /// Per-API-key rate limit: max requests per minute (Issue #669).
    /// Set to 0 to disable.
    #[serde(default)]
    pub rate_limit_key_per_minute: Option<u32>,

    /// Per-API-key rate limit: max requests per hour (Issue #669).
    /// Set to 0 to disable.
    #[serde(default)]
    pub rate_limit_key_per_hour: Option<u32>,

    /// Per-API-key rate limit: max requests per day (Issue #669).
    /// Set to 0 to disable.
    #[serde(default)]
    pub rate_limit_key_per_day: Option<u32>,
    
    /// New SSE keepalive interval in seconds (1-60)
    #[serde(default)]
    pub sse_keepalive_secs: Option<u64>,
    
    /// New slow query threshold in milliseconds
    #[serde(default)]
    pub slow_query_threshold_ms: Option<u64>,
}

// ── Issue #618: Anonymization pipeline API handlers ──────────────────────────

use crate::anonymization::AnonymizationRuleRequest;

/// List all configured anonymization rules
#[utoipa::path(
    get,
    path = "/v1/config/anonymization",
    tag = "config",
    responses(
        (status = 200, description = "List of anonymization rules"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn get_anonymization_config(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    match state.anonymization_config.as_ref() {
        Some(cfg) => {
            let rules = cfg.get_rules().await;
            Ok(Json(json!({ "rules": rules })))
        }
        None => {
            let defaults = crate::anonymization::default_rules();
            Ok(Json(json!({ "rules": defaults, "source": "built_in_defaults" })))
        }
    }
}

/// Create or update an anonymization rule
#[utoipa::path(
    post,
    path = "/v1/config/anonymization/rules",
    tag = "config",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rule created or updated"),
        (status = 400, description = "Invalid rule"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn upsert_anonymization_rule(
    State(state): State<AppState>,
    Json(payload): Json<AnonymizationRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = state
        .anonymization_config
        .as_ref()
        .ok_or_else(|| AppError::Internal("Anonymization config not initialized".to_string()))?;

    let rule = cfg
        .upsert_rule(&payload)
        .await
        .map_err(|e| AppError::Validation(e.to_string()))?;

    Ok((StatusCode::OK, Json(json!({ "status": "ok", "rule": rule }))))
}

/// Delete an anonymization rule by name
#[utoipa::path(
    delete,
    path = "/v1/config/anonymization/rules/{name}",
    tag = "config",
    params(
        ("name" = String, Path, description = "Rule name")
    ),
    responses(
        (status = 200, description = "Rule deleted"),
        (status = 404, description = "Rule not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn delete_anonymization_rule(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = state
        .anonymization_config
        .as_ref()
        .ok_or_else(|| AppError::Internal("Anonymization config not initialized".to_string()))?;

    let deleted = cfg
        .delete_rule(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if deleted {
        Ok((StatusCode::OK, Json(json!({ "status": "ok", "deleted": name }))))
    } else {
        Err(AppError::NotFound)
    }
}

// ── Issue #670: GET /v1/admin/audit-logs ─────────────────────────────────────

/// Query audit logs for sensitive operations.
///
/// `GET /v1/admin/audit-logs`
///
/// Requires admin API key. Supports filtering by event_type, resource_type,
/// user_email, severity, success, and date range. Defaults to the last 100
/// entries ordered by most-recent first.
#[utoipa::path(
    get,
    path = "/v1/admin/audit-logs",
    tag = "admin",
    params(
        ("event_type"    = Option<String>, Query, description = "Filter by event type (DELETE, CONFIG_CHANGE, ADMIN_API_CALL, …)"),
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
        ("user_email"    = Option<String>, Query, description = "Filter by user email"),
        ("severity"      = Option<String>, Query, description = "Filter by severity (LOW, INFO, WARNING, CRITICAL)"),
        ("success"       = Option<bool>,   Query, description = "Filter by success flag"),
        ("from"          = Option<String>, Query, description = "ISO-8601 start datetime (inclusive)"),
        ("to"            = Option<String>, Query, description = "ISO-8601 end datetime (inclusive)"),
        ("limit"         = Option<i64>,    Query, description = "Max results to return (default 100, max 1000)"),
        ("offset"        = Option<i64>,    Query, description = "Deprecated: use cursor"),
        ("cursor"        = Option<String>, Query, description = "Opaque keyset cursor from next_cursor"),
        ("include_total" = Option<bool>,   Query, description = "Include exact total (COUNT(*)), default false"),
    ),
    responses(
        (status = 200, description = "Audit log entries"),
        (status = 400, description = "Invalid query parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden – admin key required"),
        (status = 500, description = "Internal server error"),
    ),
    security(("admin_key" = []))
)]
pub async fn get_audit_logs(
    State(state): State<AppState>,
    Query(params): Query<AuditLogQueryParams>,
) -> Result<Json<Value>, AppError> {
    // Clamp limit
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let offset = params.offset.unwrap_or(0).max(0);
    let include_total = params.include_total.unwrap_or(false);
    let cursor_key: Option<(DateTime<Utc>, Uuid)> = params
        .cursor
        .as_deref()
        .map(decode_created_at_cursor)
        .transpose()?;

    // Parse optional datetime bounds
    let from_dt: Option<DateTime<Utc>> = params
        .from
        .as_deref()
        .map(|s| {
            s.parse::<DateTime<Utc>>()
                .map_err(|_| AppError::Validation(format!("invalid 'from' datetime: {s}")))
        })
        .transpose()?;

    let to_dt: Option<DateTime<Utc>> = params
        .to
        .as_deref()
        .map(|s| {
            s.parse::<DateTime<Utc>>()
                .map_err(|_| AppError::Validation(format!("invalid 'to' datetime: {s}")))
        })
        .transpose()?;

    if let (Some(f), Some(t)) = (from_dt, to_dt) {
        if f >= t {
            return Err(AppError::Validation(
                "'from' must be before 'to'".to_string(),
            ));
        }
    }

    // Build WHERE conditions with positional bind params
    let mut conditions: Vec<String> = Vec::new();
    let mut idx: i32 = 1;

    if params.event_type.is_some() {
        conditions.push(format!("event_type = ${idx}"));
        idx += 1;
    }
    if params.resource_type.is_some() {
        conditions.push(format!("resource_type = ${idx}"));
        idx += 1;
    }
    if params.user_email.is_some() {
        conditions.push(format!("user_email = ${idx}"));
        idx += 1;
    }
    if params.severity.is_some() {
        conditions.push(format!("severity = ${idx}"));
        idx += 1;
    }
    if params.success.is_some() {
        conditions.push(format!("success = ${idx}"));
        idx += 1;
    }
    if from_dt.is_some() {
        conditions.push(format!("created_at >= ${idx}"));
        idx += 1;
    }
    if to_dt.is_some() {
        conditions.push(format!("created_at <= ${idx}"));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count total matching rows only when requested (COUNT(*) is costly on large tables)
    let total: Option<i64> = if include_total {
        let count_sql = format!("SELECT COUNT(*) FROM audit_logs {where_clause}");
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
        if let Some(ref v) = params.event_type    { count_q = count_q.bind(v); }
        if let Some(ref v) = params.resource_type { count_q = count_q.bind(v); }
        if let Some(ref v) = params.user_email    { count_q = count_q.bind(v); }
        if let Some(ref v) = params.severity      { count_q = count_q.bind(v); }
        if let Some(v)     = params.success       { count_q = count_q.bind(v); }
        if let Some(v)     = from_dt              { count_q = count_q.bind(v); }
        if let Some(v)     = to_dt                { count_q = count_q.bind(v); }
        Some(count_q.fetch_one(&state.pool).await?)
    } else {
        None
    };

    // Fetch page: keyset cursor when given, offset kept for existing clients
    let mut data_conditions: Vec<String> = conditions.clone();
    let mut data_idx: i32 = idx;
    if cursor_key.is_some() {
        data_conditions.push(format!("(created_at, id) < (${data_idx}, ${})", data_idx + 1));
        data_idx += 2;
    }
    let data_where = if data_conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", data_conditions.join(" AND "))
    };

    let data_sql = format!(
        "SELECT id, event_type, action, resource_type, resource_id, \
         resource_description, request_path, request_method, \
         api_key_hash, user_email, user_id, ip_address::text, user_agent, \
         changes, old_value, new_value, \
         status_code, success, error_message, \
         created_by, severity, created_at, expires_at \
         FROM audit_logs {data_where} \
         ORDER BY created_at DESC, id DESC \
         LIMIT ${data_idx} OFFSET ${}",
        data_idx + 1
    );

    let mut data_q = sqlx::query(&data_sql);
    if let Some(ref v) = params.event_type    { data_q = data_q.bind(v); }
    if let Some(ref v) = params.resource_type { data_q = data_q.bind(v); }
    if let Some(ref v) = params.user_email    { data_q = data_q.bind(v); }
    if let Some(ref v) = params.severity      { data_q = data_q.bind(v); }
    if let Some(v)     = params.success       { data_q = data_q.bind(v); }
    if let Some(v)     = from_dt              { data_q = data_q.bind(v); }
    if let Some(v)     = to_dt                { data_q = data_q.bind(v); }
    if let Some((created_at, id)) = cursor_key { data_q = data_q.bind(created_at).bind(id); }
    data_q = data_q.bind(limit).bind(offset);

    let rows = data_q.fetch_all(&state.pool).await?;

    let next_cursor: Option<String> = if rows.len() as i64 == limit {
        rows.last().and_then(|row| {
            let created_at = row.try_get::<DateTime<Utc>, _>("created_at").ok()?;
            let id = row.try_get::<Uuid, _>("id").ok()?;
            Some(encode_created_at_cursor(created_at, id))
        })
    } else {
        None
    };

    let entries: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":                   row.try_get::<uuid::Uuid, _>("id").ok(),
                "event_type":           row.try_get::<String, _>("event_type").ok(),
                "action":               row.try_get::<String, _>("action").ok(),
                "resource_type":        row.try_get::<String, _>("resource_type").ok(),
                "resource_id":          row.try_get::<Option<String>, _>("resource_id").ok().flatten(),
                "resource_description": row.try_get::<Option<String>, _>("resource_description").ok().flatten(),
                "request_path":         row.try_get::<Option<String>, _>("request_path").ok().flatten(),
                "request_method":       row.try_get::<Option<String>, _>("request_method").ok().flatten(),
                "api_key_hash":         row.try_get::<Option<String>, _>("api_key_hash").ok().flatten(),
                "user_email":           row.try_get::<Option<String>, _>("user_email").ok().flatten(),
                "user_id":              row.try_get::<Option<String>, _>("user_id").ok().flatten(),
                "ip_address":           row.try_get::<Option<String>, _>("ip_address").ok().flatten(),
                "user_agent":           row.try_get::<Option<String>, _>("user_agent").ok().flatten(),
                "changes":              row.try_get::<Option<Value>, _>("changes").ok().flatten(),
                "old_value":            row.try_get::<Option<Value>, _>("old_value").ok().flatten(),
                "new_value":            row.try_get::<Option<Value>, _>("new_value").ok().flatten(),
                "status_code":          row.try_get::<Option<i32>, _>("status_code").ok().flatten(),
                "success":              row.try_get::<bool, _>("success").ok(),
                "error_message":        row.try_get::<Option<String>, _>("error_message").ok().flatten(),
                "created_by":           row.try_get::<Option<String>, _>("created_by").ok().flatten(),
                "severity":             row.try_get::<String, _>("severity").ok(),
                "created_at":           row.try_get::<DateTime<Utc>, _>("created_at").ok(),
                "expires_at":           row.try_get::<Option<DateTime<Utc>>, _>("expires_at").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data":        entries,
        "total":       total,
        "limit":       limit,
        "offset":      offset,
        "next_cursor": next_cursor,
    })))
}

/// Query parameters for the audit log endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct AuditLogQueryParams {
    pub event_type:    Option<String>,
    pub resource_type: Option<String>,
    pub user_email:    Option<String>,
    pub severity:      Option<String>,
    pub success:       Option<bool>,
    pub from:          Option<String>,
    pub to:            Option<String>,
    pub limit:         Option<i64>,
    pub offset:        Option<i64>,
    pub cursor:        Option<String>,
    pub include_total: Option<bool>,
}

// ── Issue #671: GET /v1/analytics/trending ───────────────────────────────────

/// Query parameters for the trending endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct TrendingParams {
    pub window_mins:  Option<i64>,
    pub baseline_hrs: Option<i64>,
    pub contract_id:  Option<String>,
    pub event_type:   Option<String>,
    pub min_zscore:   Option<f64>,
    pub limit:        Option<i64>,
}

// ── Issue #672: GET /v1/analytics/correlations ───────────────────────────────

/// Query parameters for the correlations endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct CorrelationParams {
    pub window_hrs:      Option<i64>,
    pub bucket_mins:     Option<i64>,
    pub min_correlation: Option<f64>,
    pub contract_ids:    Option<String>,
    pub event_type:      Option<String>,
    pub limit:           Option<i64>,
}

// ── Issue #673: Dedicated /v1/export/* endpoints ─────────────────────────────

// ── Issue #673: Export scheduling & history ───────────────────────────────────

/// Schedule an async export job.
///
/// `POST /v1/export/schedule`
///
/// Creates a background export job and returns a job ID that can be polled
/// via `GET /v1/export/jobs/{job_id}`.
#[utoipa::path(
    post,
    path = "/v1/export/schedule",
    tag = "export",
    request_body(content = ScheduleExportRequest, description = "Export job parameters"),
    responses(
        (status = 202, description = "Export job created"),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn schedule_export_job(
    State(state): State<AppState>,
    Json(req): Json<ScheduleExportRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate format
    let fmt = req.format.as_deref().unwrap_or("csv");
    if !matches!(fmt, "csv" | "json" | "jsonl" | "parquet") {
        return Err(AppError::Validation(format!(
            "unsupported format '{fmt}': use csv, json, or parquet"
        )));
    }

    let job_id = Uuid::new_v4();
    let filter_count = [
        &req.contract_id,
        &req.event_type,
    ]
    .iter()
    .filter(|v| v.is_some())
    .count() as i32;

    sqlx::query(
        "INSERT INTO batch_download_jobs (id, status, format, filter_count)
         VALUES ($1, 'pending', $2, $3)",
    )
    .bind(job_id)
    .bind(fmt)
    .bind(filter_count)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "job_id":    job_id,
            "status":    "pending",
            "format":    fmt,
            "message":   "Export job created. Poll GET /v1/export/jobs/{job_id} for status.",
        })),
    ))
}

/// Request body for scheduling an export job.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct ScheduleExportRequest {
    /// Output format: csv, json, or parquet
    pub format:      Option<String>,
    /// Optional contract filter
    pub contract_id: Option<String>,
    /// Optional event type filter
    pub event_type:  Option<String>,
    /// Optional ledger range start
    pub from_ledger: Option<i64>,
    /// Optional ledger range end
    pub to_ledger:   Option<i64>,
}

/// Get status of a scheduled export job.
///
/// `GET /v1/export/jobs/{job_id}`
#[utoipa::path(
    get,
    path = "/v1/export/jobs/{job_id}",
    tag = "export",
    params(
        ("job_id" = String, Path, description = "Export job ID"),
    ),
    responses(
        (status = 200, description = "Job status"),
        (status = 404, description = "Job not found"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn get_export_job_status_db(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = job_id
        .parse::<uuid::Uuid>()
        .map_err(|_| AppError::Validation("invalid job ID".to_string()))?;

    let row = sqlx::query(
        "SELECT id, status, format, filter_count, event_count, \
                created_at, completed_at, error \
         FROM batch_download_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    match row {
        None => Err(AppError::NotFound(format!("export job {job_id} not found"))),
        Some(row) => Ok(Json(json!({
            "job_id":       row.try_get::<uuid::Uuid, _>("id").ok(),
            "status":       row.try_get::<String, _>("status").ok(),
            "format":       row.try_get::<String, _>("format").ok(),
            "filter_count": row.try_get::<i32, _>("filter_count").ok(),
            "event_count":  row.try_get::<i64, _>("event_count").ok(),
            "created_at":   row.try_get::<DateTime<Utc>, _>("created_at").ok(),
            "completed_at": row.try_get::<Option<DateTime<Utc>>, _>("completed_at").ok().flatten(),
            "error":        row.try_get::<Option<String>, _>("error").ok().flatten(),
        }))),
    }
}

/// List export job history.
///
/// `GET /v1/export/history`
///
/// Returns the most recent export jobs (max 100), most-recent first.
#[utoipa::path(
    get,
    path = "/v1/export/history",
    tag = "export",
    params(
        ("limit"  = Option<i64>, Query, description = "Max rows to return (default 20, max 100)"),
        ("offset" = Option<i64>, Query, description = "Deprecated: use cursor"),
        ("cursor" = Option<String>, Query, description = "Opaque keyset cursor from next_cursor"),
        ("include_total" = Option<bool>, Query, description = "Include exact total (COUNT(*)), default false"),
        ("status" = Option<String>, Query, description = "Filter by status: pending, completed, failed"),
    ),
    responses(
        (status = 200, description = "Export job history"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn get_export_history(
    State(state): State<AppState>,
    Query(params): Query<ExportHistoryParams>,
) -> Result<Json<Value>, AppError> {
    let limit  = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);
    let include_total = params.include_total.unwrap_or(false);
    let cursor_key: Option<(DateTime<Utc>, Uuid)> = params
        .cursor
        .as_deref()
        .map(decode_created_at_cursor)
        .transpose()?;

    let mut conditions: Vec<String> = Vec::new();
    let mut idx: i32 = 1;
    if params.status.is_some() {
        conditions.push(format!("status = ${idx}"));
        idx += 1;
    }
    if cursor_key.is_some() {
        conditions.push(format!("(created_at, id) < (${idx}, ${})", idx + 1));
        idx += 2;
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let sql = format!(
        "SELECT id, status, format, filter_count, event_count, created_at, completed_at, error \
         FROM batch_download_jobs {where_clause} \
         ORDER BY created_at DESC, id DESC LIMIT ${idx} OFFSET ${}",
        idx + 1
    );

    let mut q = sqlx::query(&sql);
    if let Some(ref s) = params.status { q = q.bind(s); }
    if let Some((created_at, id)) = cursor_key { q = q.bind(created_at).bind(id); }
    q = q.bind(limit).bind(offset);

    let rows = q.fetch_all(&state.pool).await?;

    // Count only when requested (COUNT(*) is costly on large tables)
    let total: Option<i64> = if include_total {
        let count_where = if params.status.is_some() { "WHERE status = $1" } else { "" };
        let total_sql = format!("SELECT COUNT(*) FROM batch_download_jobs {count_where}");
        let mut tq = sqlx::query_scalar::<_, i64>(&total_sql);
        if let Some(ref s) = params.status { tq = tq.bind(s); }
        Some(tq.fetch_one(&state.pool).await?)
    } else {
        None
    };

    let next_cursor: Option<String> = if rows.len() as i64 == limit {
        rows.last().and_then(|row| {
            let created_at = row.try_get::<DateTime<Utc>, _>("created_at").ok()?;
            let id = row.try_get::<Uuid, _>("id").ok()?;
            Some(encode_created_at_cursor(created_at, id))
        })
    } else {
        None
    };

    let jobs: Vec<Value> = rows
        .iter()
        .map(|row| json!({
            "job_id":       row.try_get::<uuid::Uuid, _>("id").ok(),
            "status":       row.try_get::<String, _>("status").ok(),
            "format":       row.try_get::<String, _>("format").ok(),
            "filter_count": row.try_get::<i32, _>("filter_count").ok(),
            "event_count":  row.try_get::<i64, _>("event_count").ok(),
            "created_at":   row.try_get::<DateTime<Utc>, _>("created_at").ok(),
            "completed_at": row.try_get::<Option<DateTime<Utc>>, _>("completed_at").ok().flatten(),
            "error":        row.try_get::<Option<String>, _>("error").ok().flatten(),
        }))
        .collect();

    Ok(Json(json!({
        "jobs":        jobs,
        "total":       total,
        "limit":       limit,
        "offset":      offset,
        "next_cursor": next_cursor,
    })))
}

/// Query parameters for export history.
#[derive(Debug, serde::Deserialize)]
pub struct ExportHistoryParams {
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
    pub cursor: Option<String>,
    pub include_total: Option<bool>,
}

// ── Issue #586 / #587 / #710: Replication, feature flags, and backup verification tests ──

#[cfg(test)]
mod ops_tests;
