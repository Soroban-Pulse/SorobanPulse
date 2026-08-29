// Issue #885: Conditional request handling (ETag/If-Modified-Since)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Response headers for conditional GET support.
#[derive(Debug, Clone)]
pub struct ConditionalHeaders {
    pub etag: Option<String>,
    pub last_modified: Option<DateTime<Utc>>,
}

impl ConditionalHeaders {
    /// Create a new ConditionalHeaders from components.
    pub fn new(etag: Option<String>, last_modified: Option<DateTime<Utc>>) -> Self {
        Self { etag, last_modified }
    }

    /// Create from a list of events (using last one for timestamp).
    pub fn from_events(events: &[(Uuid, DateTime<Utc>)]) -> Option<Self> {
        events.first().map(|(id, created_at)| {
            let etag = compute_etag_from_event(id, created_at);
            ConditionalHeaders {
                etag: Some(etag),
                last_modified: Some(*created_at),
            }
        })
    }

    /// Create from event count and last event info.
    pub fn from_events_with_count(
        events: &[(Uuid, DateTime<Utc>)],
        total: Option<i64>,
    ) -> Option<Self> {
        events.first().map(|(id, created_at)| {
            let etag = compute_etag_from_event_with_count(id, created_at, total);
            ConditionalHeaders {
                etag: Some(etag),
                last_modified: Some(*created_at),
            }
        })
    }
}

impl fmt::Display for ConditionalHeaders {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ETag: {:?}, Last-Modified: {:?}",
            self.etag, self.last_modified
        )
    }
}

/// Compute ETag from last event ID and creation timestamp.
pub fn compute_etag_from_event(last_id: &Uuid, last_created_at: &DateTime<Utc>) -> String {
    let timestamp_secs = last_created_at.timestamp();
    let timestamp_nanos = last_created_at.timestamp_subsec_nanos();
    let data = format!("{}:{:x}:{:x}", last_id.to_string(), timestamp_secs, timestamp_nanos);
    let hash = blake3::hash(data.as_bytes());
    format!("\"{}\"", hash.to_hex().to_string()[..16].to_string())
}

/// Compute ETag including total count for list endpoints.
pub fn compute_etag_from_event_with_count(
    last_id: &Uuid,
    last_created_at: &DateTime<Utc>,
    total: Option<i64>,
) -> String {
    let timestamp_secs = last_created_at.timestamp();
    let timestamp_nanos = last_created_at.timestamp_subsec_nanos();
    let total_str = total.map(|t| t.to_string()).unwrap_or_default();
    let data = format!(
        "{}:{:x}:{:x}:{}",
        last_id.to_string(),
        timestamp_secs,
        timestamp_nanos,
        total_str
    );
    let hash = blake3::hash(data.as_bytes());
    format!("\"{}\"", hash.to_hex().to_string()[..16].to_string())
}

/// Check if a request should return 304 Not Modified based on ETag or Last-Modified.
pub fn should_return_304(
    headers: &axum::http::HeaderMap,
    etag: &str,
    last_modified: &DateTime<Utc>,
) -> bool {
    // Check If-None-Match (ETag)
    if let Some(inm) = headers.get("if-none-match").and_then(|v| v.to_str().ok()) {
        if inm == etag || inm == "*" {
            return true;
        }
    }

    // Check If-Modified-Since
    if let Some(ims) = headers
        .get("if-modified-since")
        .and_then(|v| v.to_str().ok())
    {
        if let Ok(ims_time) = DateTime::parse_from_rfc2822(ims) {
            let ims_utc = ims_time.with_timezone(&Utc);
            if last_modified <= &ims_utc {
                return true;
            }
        }
    }

    false
}

/// Configuration for ETag caching behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalGetConfig {
    /// Enable ETag caching to avoid recomputation.
    pub enable_etag_cache: bool,
    /// Cache TTL in seconds (default: 3600).
    pub etag_cache_ttl_secs: u64,
    /// Enable If-Modified-Since header support.
    pub enable_if_modified_since: bool,
}

impl Default for ConditionalGetConfig {
    fn default() -> Self {
        Self {
            enable_etag_cache: true,
            etag_cache_ttl_secs: 3600,
            enable_if_modified_since: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_etag_computation() {
        let id = Uuid::nil();
        let time = DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let etag1 = compute_etag_from_event(&id, &time);
        let etag2 = compute_etag_from_event(&id, &time);

        assert_eq!(etag1, etag2, "ETags should be deterministic");
        assert!(etag1.starts_with('"') && etag1.ends_with('"'), "ETag should be quoted");
    }

    #[test]
    fn test_etag_with_count() {
        let id = Uuid::nil();
        let time = DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        let etag1 = compute_etag_from_event_with_count(&id, &time, Some(100));
        let etag2 = compute_etag_from_event_with_count(&id, &time, Some(101));

        assert_ne!(
            etag1, etag2,
            "ETags should differ when count changes"
        );
    }

    #[test]
    fn test_should_return_304_with_etag() {
        let mut headers = axum::http::HeaderMap::new();
        let etag = "\"abc123\"";
        let time = Utc::now();

        // Exact match should return 304
        headers.insert("if-none-match", etag.parse().unwrap());
        assert!(should_return_304(&headers, etag, &time));

        // Different ETag should not return 304
        let mut headers2 = axum::http::HeaderMap::new();
        headers2.insert("if-none-match", "\"different\"".parse().unwrap());
        assert!(!should_return_304(&headers2, etag, &time));
    }

    #[test]
    fn test_should_return_304_with_if_modified_since() {
        let mut headers = axum::http::HeaderMap::new();
        let etag = "\"abc123\"";
        let time = DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);

        // Request with future If-Modified-Since should return 304
        headers.insert(
            "if-modified-since",
            "Thu, 27 Aug 2026 12:30:00 GMT".parse().unwrap(),
        );
        assert!(should_return_304(&headers, etag, &time));

        // Request with past If-Modified-Since should not return 304
        let mut headers2 = axum::http::HeaderMap::new();
        headers2.insert(
            "if-modified-since",
            "Thu, 27 Aug 2026 11:00:00 GMT".parse().unwrap(),
        );
        assert!(!should_return_304(&headers2, etag, &time));
    }
}
