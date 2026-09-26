/// HTTP Caching Headers (Issue: add HTTP caching headers to optimize client-side caching)
///
/// Builds on the ETag/If-Modified-Since primitives in [`crate::conditional_get`]
/// with a full response-header policy: Cache-Control directives, Last-Modified
/// formatting, conditional-request handling (ETag or Last-Modified), cache
/// revalidation, and effectiveness metrics.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::conditional_get::should_return_304;

/// Cache-Control policy for a resource class (e.g. "list events" vs
/// "single event by id" vs "static config").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachePolicy {
    /// Resource identifier used for metrics labeling (e.g. "events.list").
    pub resource: String,
    /// Max age (seconds) a client-side cache may serve this response without
    /// revalidating.
    pub max_age_secs: u64,
    /// If true, allow shared caches (CDN/proxy) to store the response.
    pub public: bool,
    /// If true, require revalidation once max-age has elapsed rather than
    /// silently serving stale content.
    pub must_revalidate: bool,
    /// Optional stale-while-revalidate window (seconds).
    pub stale_while_revalidate_secs: Option<u64>,
}

impl CachePolicy {
    pub fn new(resource: impl Into<String>, max_age_secs: u64) -> Self {
        Self {
            resource: resource.into(),
            max_age_secs,
            public: false,
            must_revalidate: true,
            stale_while_revalidate_secs: None,
        }
    }

    pub fn public(mut self) -> Self {
        self.public = true;
        self
    }

    pub fn with_stale_while_revalidate(mut self, secs: u64) -> Self {
        self.stale_while_revalidate_secs = Some(secs);
        self
    }

    /// Render the `Cache-Control` header value for this policy.
    pub fn cache_control_value(&self) -> String {
        let mut parts = vec![if self.public { "public" } else { "private" }.to_string()];
        parts.push(format!("max-age={}", self.max_age_secs));
        if self.must_revalidate {
            parts.push("must-revalidate".to_string());
        }
        if let Some(swr) = self.stale_while_revalidate_secs {
            parts.push(format!("stale-while-revalidate={}", swr));
        }
        parts.join(", ")
    }

    /// A policy indicating the resource must never be cached (e.g. auth endpoints).
    pub fn no_store(resource: impl Into<String>) -> Self {
        Self {
            resource: resource.into(),
            max_age_secs: 0,
            public: false,
            must_revalidate: true,
            stale_while_revalidate_secs: None,
        }
    }

    fn is_no_store(&self) -> bool {
        self.max_age_secs == 0 && self.stale_while_revalidate_secs.is_none()
    }
}

/// The full set of caching-related response headers for a resource.
#[derive(Debug, Clone)]
pub struct CacheHeaders {
    pub cache_control: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

impl CacheHeaders {
    pub fn as_pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = vec![("Cache-Control", self.cache_control.clone())];
        if let Some(etag) = &self.etag {
            pairs.push(("ETag", etag.clone()));
        }
        if let Some(lm) = &self.last_modified {
            pairs.push(("Last-Modified", lm.clone()));
        }
        pairs
    }
}

/// Format a timestamp as an HTTP-date per RFC 7231 (used for `Last-Modified`).
pub fn format_http_date(dt: &DateTime<Utc>) -> String {
    dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string()
}

/// Build the full set of caching headers for a resource snapshot.
pub fn build_cache_headers(
    policy: &CachePolicy,
    etag: Option<&str>,
    last_modified: Option<&DateTime<Utc>>,
) -> CacheHeaders {
    let cache_control = if policy.is_no_store() {
        "no-store".to_string()
    } else {
        policy.cache_control_value()
    };

    CacheHeaders {
        cache_control,
        etag: etag.map(|e| e.to_string()),
        last_modified: last_modified.map(format_http_date),
    }
}

/// Outcome of evaluating a conditional request against current resource state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RevalidationOutcome {
    /// Resource unchanged; caller should return 304 Not Modified.
    NotModified,
    /// Resource changed (or client sent no conditional headers); caller
    /// should return the full 200 response with fresh cache headers.
    Fresh,
}

/// Evaluate an inbound request's conditional headers (`If-None-Match` /
/// `If-Modified-Since`) against the resource's current ETag/Last-Modified,
/// recording a cache-effectiveness metric either way.
pub fn revalidate(
    resource: &str,
    headers: &axum::http::HeaderMap,
    etag: &str,
    last_modified: &DateTime<Utc>,
) -> RevalidationOutcome {
    let hit = should_return_304(headers, etag, last_modified);
    crate::metrics::record_http_cache_result(resource, hit);
    if hit {
        RevalidationOutcome::NotModified
    } else {
        RevalidationOutcome::Fresh
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CacheEffectivenessSnapshot {
    pub resource: String,
    pub hits: u64,
    pub misses: u64,
}

impl CacheEffectivenessSnapshot {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_time() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-27T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn cache_control_renders_expected_directives() {
        let policy = CachePolicy::new("events.list", 60)
            .public()
            .with_stale_while_revalidate(30);
        assert_eq!(
            policy.cache_control_value(),
            "public, max-age=60, must-revalidate, stale-while-revalidate=30"
        );
    }

    #[test]
    fn no_store_policy_overrides_cache_control() {
        let policy = CachePolicy::no_store("auth.session");
        let headers = build_cache_headers(&policy, None, None);
        assert_eq!(headers.cache_control, "no-store");
    }

    #[test]
    fn http_date_format_matches_rfc7231() {
        let formatted = format_http_date(&sample_time());
        assert_eq!(formatted, "Thu, 27 Aug 2026 12:00:00 GMT");
    }

    #[test]
    fn build_cache_headers_includes_etag_and_last_modified() {
        let policy = CachePolicy::new("events.detail", 120);
        let headers = build_cache_headers(&policy, Some("\"abc\""), Some(&sample_time()));
        assert_eq!(headers.etag.as_deref(), Some("\"abc\""));
        assert_eq!(headers.last_modified.as_deref(), Some("Thu, 27 Aug 2026 12:00:00 GMT"));
        assert!(headers.cache_control.contains("max-age=120"));
    }

    #[test]
    fn revalidate_returns_not_modified_on_etag_match() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("if-none-match", "\"match\"".parse().unwrap());
        let outcome = revalidate("events.detail", &headers, "\"match\"", &sample_time());
        assert_eq!(outcome, RevalidationOutcome::NotModified);
    }

    #[test]
    fn revalidate_returns_fresh_when_no_conditional_headers() {
        let headers = axum::http::HeaderMap::new();
        let outcome = revalidate("events.detail", &headers, "\"etag\"", &sample_time());
        assert_eq!(outcome, RevalidationOutcome::Fresh);
    }

    #[test]
    fn hit_rate_computes_correctly() {
        let snapshot = CacheEffectivenessSnapshot {
            resource: "events.list".to_string(),
            hits: 3,
            misses: 1,
        };
        assert_eq!(snapshot.hit_rate(), 0.75);
    }

    #[test]
    fn hit_rate_is_zero_with_no_traffic() {
        let snapshot = CacheEffectivenessSnapshot {
            resource: "events.list".to_string(),
            hits: 0,
            misses: 0,
        };
        assert_eq!(snapshot.hit_rate(), 0.0);
    }

    #[test]
    fn etag_from_conditional_get_integrates_with_policy() {
        let id = Uuid::nil();
        let time = sample_time();
        let etag = crate::conditional_get::compute_etag_from_event(&id, &time);
        let policy = CachePolicy::new("events.detail", 60);
        let headers = build_cache_headers(&policy, Some(&etag), Some(&time));
        assert_eq!(headers.etag, Some(etag));
    }
}
