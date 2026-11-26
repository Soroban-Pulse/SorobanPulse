//! Subscription handlers: creation, unsubscribe links and per-IP subscription/SSE limits (Issue #1240).

#[allow(unused_imports)]
use super::*;

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
    pub(super) hits: dashmap::DashMap<String, Vec<std::time::Instant>>,
    pub(super) limit: usize,
    pub(super) window: std::time::Duration,
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
    pub(super) active: dashmap::DashMap<String, usize>,
    pub(super) limit: usize,
}

/// RAII guard that releases an SSE connection slot on drop.
pub struct SseConnectionGuard {
    pub(super) limiter: Arc<SseConnectionLimiter>,
    pub(super) ip: String,
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
pub(super) fn client_ip_from_headers(headers: &HeaderMap) -> String {
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
pub(super) fn too_many_requests(retry_after_secs: u64, message: &str) -> Response {
    let mut response = Response::new(Body::from(
        json!({ "error": message, "retry_after": retry_after_secs }).to_string(),
    ));
    *response.status_mut() = StatusCode::TOO_MANY_REQUESTS;
    let headers = response.headers_mut();
    headers.insert(header::RETRY_AFTER, header::HeaderValue::from(retry_after_secs));
    headers.insert(header::CONTENT_TYPE, header::HeaderValue::from_static("application/json"));
    response
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

/// Register a webhook subscription (Issue #1268: per-IP rate limited to 5/min).
///
/// `POST /v1/subscriptions`
pub async fn create_subscription(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let ip = client_ip_from_headers(&headers);
    if let Err(retry_after) = state.subscription_rate_limiter.check(&ip) {
        crate::metrics::record_subscription_rate_limited();
        tracing::warn!(client_ip = %ip, "subscription registration rate limited");
        return too_many_requests(retry_after, "too many subscription requests");
    }

    match crate::webhooks::create_subscription(&state.pool, payload).await {
        Ok(subscription) => (StatusCode::CREATED, Json(subscription)).into_response(),
        Err(e) => e.into_response(),
    }
}
