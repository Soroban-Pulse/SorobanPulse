//! Rate-limit response-header middleware — Issue #669
//!
//! This module provides [`rate_limit_headers_middleware`], which injects
//! standard `X-RateLimit-*` headers into every response when per-API-key
//! rate limits are configured.
//!
//! ## Headers injected
//!
//! | Header                         | Description                          |
//! |-------------------------------|--------------------------------------|
//! | `X-RateLimit-Limit-Minute`    | Configured per-minute limit          |
//! | `X-RateLimit-Remaining-Minute`| Remaining requests in the minute     |
//! | `X-RateLimit-Limit-Hour`      | Configured per-hour limit            |
//! | `X-RateLimit-Remaining-Hour`  | Remaining requests in the hour       |
//! | `X-RateLimit-Limit-Day`       | Configured per-day limit             |
//! | `X-RateLimit-Remaining-Day`   | Remaining requests in the day        |
//! | `X-RateLimit-Reset`           | Unix timestamp when window resets    |
//! | `Retry-After`                 | Seconds until reset (on 429 only)    |
//!
//! When no per-key limits are configured (`RATE_LIMIT_KEY_PER_MINUTE`,
//! `RATE_LIMIT_KEY_PER_HOUR`, `RATE_LIMIT_KEY_PER_DAY` all unset) this
//! middleware is a no-op and adds no headers.

use axum::{extract::{Request, State}, middleware::Next, response::Response};

/// Issue #669: X-RateLimit response headers middleware.
///
/// Reads per-API-key rate limit status from the `rate_limit_counters` table
/// and injects standard `X-RateLimit-*` headers into every response:
///
/// - `X-RateLimit-Limit-Minute` / `X-RateLimit-Remaining-Minute` / `X-RateLimit-Reset-Minute`
/// - `X-RateLimit-Limit-Hour`   / `X-RateLimit-Remaining-Hour`   / `X-RateLimit-Reset-Hour`
/// - `X-RateLimit-Limit-Day`    / `X-RateLimit-Remaining-Day`    / `X-RateLimit-Reset-Day`
///
/// When no per-key limits are configured (`RATE_LIMIT_KEY_PER_MINUTE` etc. unset)
/// this middleware is a no-op and adds no headers.
pub async fn rate_limit_headers_middleware(
    State(state): State<crate::routes::AppState>,
    req: Request,
    next: Next,
) -> Response {
    // Only inject headers when at least one per-key window is configured.
    let cfg = &state.config;
    let is_configured = cfg.rate_limit_key_per_minute.is_some()
        || cfg.rate_limit_key_per_hour.is_some()
        || cfg.rate_limit_key_per_day.is_some();

    if !is_configured {
        return next.run(req).await;
    }

    // Extract API key — skip header injection for unauthenticated requests.
    let api_key = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| req.headers().get("X-Api-Key").and_then(|h| h.to_str().ok()))
        .map(|s| s.to_string());

    let mut response = next.run(req).await;

    let Some(key) = api_key else {
        return response;
    };

    let rate_limit_config = crate::rate_limiter::RateLimitConfig::new(
        cfg.rate_limit_key_per_minute,
        cfg.rate_limit_key_per_hour,
        cfg.rate_limit_key_per_day,
    );

    // Fetch status without incrementing the counter (read-only).
    if let Ok(status) =
        crate::rate_limiter::get_rate_limit_status(&state.pool, &key, &rate_limit_config).await
    {
        let headers = response.headers_mut();

        // Per-minute window headers
        if let (Some(limit), Some(remaining)) = (status.limit_minute, status.remaining_minute) {
            let _ = headers.insert(
                "X-RateLimit-Limit-Minute",
                limit.to_string().parse().unwrap(),
            );
            let _ = headers.insert(
                "X-RateLimit-Remaining-Minute",
                remaining.to_string().parse().unwrap(),
            );
        }

        // Per-hour window headers
        if let (Some(limit), Some(remaining)) = (status.limit_hour, status.remaining_hour) {
            let _ = headers.insert(
                "X-RateLimit-Limit-Hour",
                limit.to_string().parse().unwrap(),
            );
            let _ = headers.insert(
                "X-RateLimit-Remaining-Hour",
                remaining.to_string().parse().unwrap(),
            );
        }

        // Per-day window headers
        if let (Some(limit), Some(remaining)) = (status.limit_day, status.remaining_day) {
            let _ = headers.insert(
                "X-RateLimit-Limit-Day",
                limit.to_string().parse().unwrap(),
            );
            let _ = headers.insert(
                "X-RateLimit-Remaining-Day",
                remaining.to_string().parse().unwrap(),
            );
        }

        // Reset timestamp (when currently limited)
        if let Some(reset_at) = status.reset_at {
            let _ = headers.insert(
                "X-RateLimit-Reset",
                reset_at.to_string().parse().unwrap(),
            );
        }

        // Retry-After on 429 responses (seconds until reset)
        if status.is_rate_limited {
            if let Some(reset_at) = status.reset_at {
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let retry_after = (reset_at - now).max(1);
                let _ = headers.insert(
                    "Retry-After",
                    retry_after.to_string().parse().unwrap(),
                );
            }
        }
    }

    response
}
