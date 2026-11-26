use hmac::Mac;
use reqwest::Client;
use sqlx::Row;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, warn, Span};
use uuid::Uuid;

/// Generic error returned to callers when a webhook URL fails validation.
///
/// Deliberately opaque: it must not echo transport details, resolved
/// addresses, or HTTP status codes, since those would let an attacker probe
/// internal services (SSRF, #1426).
pub const WEBHOOK_URL_VALIDATION_FAILED: &str = "URL failed validation";

/// Validate that `url` is a safe, reachable webhook target.
///
/// Used when creating or updating webhook notification channels (#503).
///
/// The URL is first checked by the shared outbound SSRF guard
/// (`net::outbound::validate_outbound_url`), which rejects non-HTTP(S)
/// schemes, loopback/RFC1918/link-local/metadata addresses (including
/// decimal/octal/hex IP encodings and DNS names resolving to private IPs)
/// and pins the resolved IP to defend against DNS rebinding. Only after that
/// check passes is a request issued, through the hardened outbound client
/// (redirects disabled, timeouts applied).
///
/// Self-hosted deployments that intentionally target private addresses can
/// opt in via the `WEBHOOK_ALLOW_PRIVATE_TARGETS` environment variable (or the
/// `webhook_allow_private_targets` config flag), optionally narrowed to a
/// comma-separated host allowlist in `WEBHOOK_PRIVATE_TARGET_ALLOWLIST`.
///
/// All failures are reported as [`WEBHOOK_URL_VALIDATION_FAILED`] so that the
/// caller cannot distinguish "blocked" from "unreachable" or learn status
/// codes.
pub async fn validate_webhook_url(url: &str) -> Result<(), String> {
    // SSRF guard: reject unsafe destinations before any network request.
    if crate::net::outbound::validate_outbound_url(url).await.is_err() {
        warn!(url = %url, "Webhook URL rejected by outbound SSRF guard");
        return Err(WEBHOOK_URL_VALIDATION_FAILED.to_string());
    }

    // Private-target opt-in for self-hosted deployments. Even when enabled,
    // the allowlist (if configured) must match the URL's host.
    if !private_targets_permitted(url) {
        warn!(url = %url, "Webhook URL targets a private address and is not allowlisted");
        return Err(WEBHOOK_URL_VALIDATION_FAILED.to_string());
    }

    let client = match crate::net::outbound::outbound_client(Duration::from_secs(10)) {
        Ok(client) => client,
        Err(_) => return Err(WEBHOOK_URL_VALIDATION_FAILED.to_string()),
    };

    match client.head(url).send().await {
        Ok(resp) if resp.status().is_success() => Ok(()),
        // Do not leak the status code or transport error to the caller.
        Ok(_) | Err(_) => Err(WEBHOOK_URL_VALIDATION_FAILED.to_string()),
    }
}

/// Returns `true` when the URL's host is permitted despite resolving to a
/// private address.
///
/// Private targets are only permitted when the deployment explicitly opts in
/// via `WEBHOOK_ALLOW_PRIVATE_TARGETS` (truthy). When
/// `WEBHOOK_PRIVATE_TARGET_ALLOWLIST` is set, the URL host must appear in the
/// comma-separated list. Public hosts are always permitted.
fn private_targets_permitted(url: &str) -> bool {
    let host = match reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
    {
        Some(host) => host,
        None => return false,
    };

    // If the host is not a private/loopback/link-local literal, it is a public
    // destination and the SSRF guard has already vetted it.
    if !host_is_private_literal(&host) {
        return true;
    }

    let opt_in = std::env::var("WEBHOOK_ALLOW_PRIVATE_TARGETS")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false);
    if !opt_in {
        return false;
    }

    match std::env::var("WEBHOOK_PRIVATE_TARGET_ALLOWLIST") {
        Ok(list) if !list.trim().is_empty() => list
            .split(',')
            .map(|entry| entry.trim().to_ascii_lowercase())
            .any(|entry| !entry.is_empty() && entry == host),
        _ => true,
    }
}

/// Best-effort check for private/loopback/link-local host literals.
fn host_is_private_literal(host: &str) -> bool {
    let bare = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => {
                v4.is_loopback()
                    || v4.is_private()
                    || v4.is_link_local()
                    || v4.is_unspecified()
                    || v4.is_broadcast()
                    || v4.octets()[0] == 169 && v4.octets()[1] == 254
            }
            std::net::IpAddr::V6(v6) => {
                v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_unique_local()
                    || v6.is_unicast_link_local()
            }
        };
    }
    matches!(host, "localhost" | "localhost.localdomain")
}

use crate::{metrics, models::SorobanEvent, webhook_template::WebhookTemplate};
use serde_json::Value;


/// Transform webhook payload using a template (Issue #678).
/// Returns the transformed payload or the original event if template is invalid.
pub fn transform_webhook_payload(
    template: &str,
    event: &SorobanEvent,
) -> Result<Value, String> {
    let webhook_template = WebhookTemplate::new(template.to_string());

    // Validate template syntax
    webhook_template.validate()?;

    // Transform the event using the template
    let event_value = serde_json::to_value(event)
        .map_err(|e| format!("Failed to convert event to JSON: {}", e))?;

    webhook_template.transform(&event_value)
}

/// Sign a payload with HMAC-SHA256 and return the hex digest.
///
/// This is the legacy (v1) scheme that signs the body only. It is retained
/// during the deprecation window; new integrations should verify the
/// timestamped v2 signature produced by [`sign_payload_v2`] (Issue #1431).
pub fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac = crate::hmac_util::hmac_sha256(secret.as_bytes());
    mac.update(body);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Compute the v2 signature hex digest: `HMAC-SHA256(secret, "<t>.<body>")`.
///
/// Signing the timestamp together with the body lets receivers reject
/// replayed deliveries by enforcing a tolerance window (Issue #1431).
pub fn sign_payload_v2(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut mac = crate::hmac_util::hmac_sha256(secret.as_bytes());
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Build the v2 signature header value: `t=<unix>,v1=<hex>`.
pub fn signature_v2_header(secret: &str, timestamp: i64, body: &[u8]) -> String {
    format!(
        "t={},v1={}",
        timestamp,
        sign_payload_v2(secret, timestamp, body)
    )
}

/// Generate a unique delivery ID for a webhook delivery (Issue #1431).
pub fn new_delivery_id() -> String {
    Uuid::new_v4().to_string()
}

/// The set of signature headers to attach to a delivery.
///
/// During the deprecation window both the legacy `X-Signature-256` header and
/// the new timestamped `X-SorobanPulse-Signature` header are sent, along with
/// a unique `X-SorobanPulse-Delivery-Id` (Issue #1431).
#[derive(Debug, Clone)]
pub struct SignatureHeaders {
    pub delivery_id: String,
    pub legacy: Option<String>,
    pub v2: Option<String>,
}

impl SignatureHeaders {
    /// Build the headers for a delivery. `secret` is optional; when absent only
    /// the delivery ID is produced.
    pub fn new(secret: Option<&str>, body: &[u8]) -> Self {
        let delivery_id = new_delivery_id();
        match secret {
            Some(secret) => {
                let timestamp = chrono::Utc::now().timestamp();
                Self {
                    delivery_id,
                    legacy: Some(format!("sha256={}", sign_payload(secret, body))),
                    v2: Some(signature_v2_header(secret, timestamp, body)),
                }
            }
            None => Self {
                delivery_id,
                legacy: None,
                v2: None,
            },
        }
    }

    /// Apply the headers to an outgoing request.
    pub fn apply(&self, mut req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req = req.header("X-SorobanPulse-Delivery-Id", &self.delivery_id);
        if let Some(ref legacy) = self.legacy {
            req = req.header("X-Signature-256", legacy);
        }
        if let Some(ref v2) = self.v2 {
            req = req.header("X-SorobanPulse-Signature", v2);
        }
        req
    }

    /// Flatten the headers into `(name, value)` pairs for logging.
    pub fn to_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![(
            "X-SorobanPulse-Delivery-Id".to_string(),
            self.delivery_id.clone(),
        )];
        if let Some(ref legacy) = self.legacy {
            pairs.push(("X-Signature-256".to_string(), legacy.clone()));
        }
        if let Some(ref v2) = self.v2 {
            pairs.push(("X-SorobanPulse-Signature".to_string(), v2.clone()));
        }
        pairs
    }
}

/// Evaluate the notification priority of an event using a JSONPath-style rule (Issue #492).
/// Returns the matched priority string, or `default_priority` if no rule is set or matches.
pub fn evaluate_priority<'a>(
    event: &SorobanEvent,
    rule_path: Option<&str>,
    rule_value: Option<&str>,
    rule_priority: Option<&'a str>,
    default_priority: &'a str,
) -> &'a str {
    if let (Some(path), Some(expected), Some(priority)) = (rule_path, rule_value, rule_priority) {
        let segments: Vec<&str> = path
            .trim_start_matches("$.")
            .split('.')
            .filter(|s| !s.is_empty())
            .collect();

        let mut current = &event.value;
        for segment in &segments {
            match current.get(segment) {
                Some(next) => current = next,
                None => return default_priority,
            }
        }

        if current.as_str() == Some(expected) {
            return priority;
        }
    }
    default_priority
}

/// Deliver a single event to the webhook URL with the default retry policy.
/// On final failure, insert into DLQ.
///
/// The supplied `client` is ignored in favour of the hardened outbound client
/// so that every delivery goes through the SSRF guard (#1426, #1427). The
/// parameter is retained for API compatibility.
pub async fn deliver(
    _client: Client,
    url: String,
    secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
) {
    deliver_with_retry_policy(
        _client,
        url,
        secret,
        event,
        pool,
        &crate::retry_policy::RetryPolicy::webhook_default(),
        "medium".to_string(),
    )
    .await
}

/// Deliver with custom retry policy and priority (Issues #474, #492).
/// The priority is included in the request payload and headers.
/// On success, a notification_acknowledgments record is inserted for escalation
/// tracking (Issue #493).
///
/// All parameters are owned so the resulting future is `'static` and safe to
/// pass to `tokio::spawn`.
///
/// See docs/adr/0003-webhook-retry-strategy.md for the design rationale.
pub async fn deliver_with_retry_policy(
    _client: Client,
    url: String,
    secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
    retry_policy: &crate::retry_policy::RetryPolicy,
    priority: String,
) {
    // Route every delivery through the shared SSRF-safe outbound client
    // (#1426, #1427). The caller-supplied client is intentionally ignored so
    // that no integration can bypass the guard.
    let client = match crate::net::outbound::outbound_client(Duration::from_secs(30)) {
        Ok(client) => client,
        Err(e) => {
            error!(error = %e, url = %url, "Failed to build outbound client for webhook delivery");
            return;
        }
    };

    // Reject unsafe destinations before any request is attempted.
    if crate::net::outbound::validate_outbound_url(&url).await.is_err()
        || !private_targets_permitted(&url)
    {
        error!(url = %url, "Webhook delivery blocked by outbound SSRF guard");
        metrics::record_webhook_failure();
        return;
    }

    // Create a root span for the entire delivery lifecycle
    #[cfg(feature = "full-observability")]
    let root_span = tracing::info_span!(
        "webhook_delivery",
        webhook_url = %url,
        contract_id = %event.contract_id,
        tx_hash = %event.tx_hash,
        priority = %priority,
    );

    #[cfg(not(feature = "full-observability"))]
    let root_span = tracing::info_span!(
        "webhook_delivery",
        webhook_url = %url,
        contract_id = %event.contract_id,
    );

    let _enter = root_span.enter();

    // Check suppression list before attempting delivery (Issue #490)
    if let Some(pool) = pool {
        match sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM suppression_lists \
             WHERE target = $1 AND target_type = 'webhook' \
             AND (expires_at IS NULL OR expires_at > NOW())",
        )
        .bind(&url)
        .fetch_one(pool)
        .await
        {
            Ok(count) if count > 0 => {
                info!(url = %url, "Webhook URL suppressed, skipping delivery");
                crate::metrics::record_notification_suppressed();
                return;
            }
            _ => {}
        }
    }

    if let Some(pool) = pool {
        match endpoint_rate_limited(pool, &url).await {
            Ok(Some(next_retry_at)) => {
                enqueue_endpoint_retry(
                    pool,
                    &url,
                    &event,
                    &secret,
                    "endpoint rate limited",
                    next_retry_at,
                )
                .await;
                return;
            }
            Ok(None) => {}
            Err(e) => warn!(error = %e, url = %url, "Failed to check webhook endpoint rate limit"),
        }
    }

    let mut payload = match serde_json::to_value(&event) {
        Ok(value) => value,
        Err(e) => {
            error!(error = %e, "Failed to serialize event for webhook delivery");
            return;
        }
    };
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("priority".to_string(), serde_json::json!(priority));
    }

    let body = match serde_json::to_vec(&payload) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to re-serialize webhook payload");
            return;
        }
    };

    let signature_headers = SignatureHeaders::new(secret.as_deref(), &body);
    let priority_owned = priority.clone();

    let log_pool = pool.cloned();
    let log_url = url.clone();
    let log_request_headers = {
        let mut headers = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("X-Notification-Priority".to_string(), priority_owned.clone()),
        ];
        headers.extend(signature_headers.to_pairs());
        headers
    };
    let log_request_body = payload.clone();
    let log_contract_id = event.contract_id.clone();
    let log_event_type = event.event_type.to_string();

    let result = retry_policy
        .execute_with_retry(|attempt| {
            let client = client.clone();
            let url = url.clone();
            let body = body.clone();
            let signature_headers = signature_headers.clone();
            let priority_header = priority_owned.clone();
            let log_pool = log_pool.clone();
            let log_url = log_url.clone();
            let log_request_headers = log_request_headers.clone();
            let log_request_body = log_request_body.clone();
            let log_contract_id = log_contract_id.clone();
            let log_event_type = log_event_type.clone();

            async move {
                // Create a child span for this attempt
                #[cfg(feature = "full-observability")]
                let attempt_span = tracing::info_span!(
                    "webhook_attempt",
                    attempt = attempt,
                    parent: &tracing::Span::current(),
                );

                #[cfg(not(feature = "full-observability"))]
                let attempt_span = tracing::info_span!(
                    "webhook_attempt",
                    attempt = attempt,
                );

                let _enter = attempt_span.enter();

                let req = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("X-Notification-Priority", priority_header)
                    .body(body);

                let req = signature_headers.apply(req);

                // Create a span for the HTTP send
                #[cfg(feature = "full-observability")]
                let send_span = tracing::info_span!("webhook_http_send");

                #[cfg(not(feature = "full-observability"))]
                let send_span = tracing::info_span!("webhook_http_send");

                let _send_enter = send_span.enter();

                let started_at = std::time::Instant::now();
                let send_result = req.send().await;
                let duration_ms = started_at.elapsed().as_millis() as i64;
                drop(_send_enter);

                // Record the notification delivery cost for this attempt,
                // attributed to the webhook component (Issue #1264).
                crate::costs::calculator::record_notification_cost(
                    "webhook",
                    duration_ms as u64,
                );

                let outcome = match send_result {
                    Ok(resp) if resp.status().is_success() => {
                        info!(
                            url = %url,
                            contract_id = %event.contract_id,
                            attempt = attempt,
                            priority = %event.event_type,
                            duration_ms = duration_ms,
                            "Webhook delivered successfully"
                        );
                        crate::metrics::record_webhook_delivery_success();
                        let status = resp.status().as_u16() as i32;
                        let body_text = resp.text().await.unwrap_or_default();
                        (Ok(()), Some(status), body_text)
                    }
                    Ok(resp) => {
                        let status = resp.status().as_u16() as i32;
                        let body_text = resp.text().await.unwrap_or_default();
                        let error_msg = format!("HTTP {status}: {body_text}");
                        (Err(error_msg), Some(status), body_text)
                    }
                    Err(e) => (Err(format!("Request error: {}", e)), None, String::new()),
                };

                // Best-effort request/response log, spawned so it never
                // slows down delivery/retry timing (Issue #937).
                if let Some(pool) = log_pool {
                    let response_body: Option<Value> = if outcome.2.is_empty() {
                        None
                    } else {
                        Some(
                            serde_json::from_str(&outcome.2)
                                .unwrap_or_else(|_| serde_json::json!({"raw": outcome.2})),
                        )
                    };
                    let response_status = outcome.1;
                    let request_headers = log_request_headers.clone();
                    let request_body = log_request_body.clone();
                    let contract_id = log_contract_id.clone();
                    let event_type = log_event_type.clone();
                    let url_for_log = log_url.clone();
                    tokio::spawn(async move {
                        if let Err(e) = crate::webhook_logging::log_exchange(
                            &pool,
                            crate::webhook_logging::WebhookLogEntry {
                                url: &url_for_log,
                                request_headers: &request_headers,
                                request_body: &request_body,
                                response_status,
                                response_body: response_body.as_ref(),
                                duration_ms,
                                contract_id: Some(&contract_id),
                                event_type: Some(&event_type),
                            },
                        )
                        .await
                        {
                            warn!(error = %e, "Failed to store webhook request/response log");
                        }
                    });
                }

                outcome.0
            }
        })
        .await;

    match result {
        Ok(()) => {
            if let Some(pool) = pool {
                record_endpoint_success(pool, &url).await;
            }
            // Record the notification for escalation tracking (Issue #493).
            if let Some(pool) = pool {
                let notification_id = Uuid::new_v4();
                if let Err(e) = sqlx::query(
                    "INSERT INTO notification_acknowledgments \
                     (id, channel, event_contract_id, event_type, priority, status) \
                     VALUES ($1, 'webhook', $2, $3, $4, 'pending')",
                )
                .bind(notification_id)
                .bind(&event.contract_id)
                .bind(&event.event_type)
                .bind(priority)
                .execute(pool)
                .await
                {
                    warn!(error = %e, "Failed to record notification for escalation tracking");
                }
            }
        }
        Err(error_msg) => {
            error!(
                url = %url,
                contract_id = %event.contract_id,
                error = %error_msg,
                max_attempts = retry_policy.max_attempts,
                "Webhook delivery failed after all retries"
            );

            #[cfg(feature = "full-observability")]
            metrics::record_webhook_trace_error();

            if let Some(pool) = pool {
                let next_retry = record_endpoint_failure(pool, &url).await;
                let payload_val = serde_json::to_value(&event).unwrap_or(serde_json::json!({}));

                if let Err(e) = sqlx::query(
                    "INSERT INTO webhook_failures \
                     (url, payload, attempts, last_error, next_retry_at) \
                     VALUES ($1, $2, $3, $4, $5)",
                )
                .bind(&url)
                .bind(payload_val)
                .bind(retry_policy.max_attempts as i32)
                .bind(&error_msg)
                .bind(next_retry)
                .execute(pool)
                .await
                {
                    error!(error = %e, "Failed to insert webhook failure into DLQ");
                }

                // Move the exhausted delivery into the webhook DLQ (Issue #1260).
                if let Err(e) = enqueue_dlq(pool, None, &event, &error_msg).await {
                    error!(error = %e, url = %url, "Failed to insert webhook into DLQ");
                }
            }

            metrics::record_webhook_failure();
        }
    }
}

async fn endpoint_rate_limited(
    pool: &sqlx::PgPool,
    url: &str,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, sqlx::Error> {
    let row = sqlx::query(
        "INSERT INTO rate_limit_endpoints (endpoint_url, window_start, window_count) \
         VALUES ($1, NOW(), 1) \
         ON CONFLICT (endpoint_url) DO UPDATE SET \
             window_start = CASE \
                 WHEN rate_limit_endpoints.window_start < NOW() - INTERVAL '1 minute' THEN NOW() \
                 ELSE rate_limit_endpoints.window_start END, \
             window_count = CASE \
                 WHEN rate_limit_endpoints.window_start < NOW() - INTERVAL '1 minute' THEN 1 \
                 ELSE rate_limit_endpoints.window_count + 1 END, \
             updated_at = NOW() \
         RETURNING per_minute_limit, window_count, backoff_until",
    )
    .bind(url)
    .fetch_one(pool)
    .await?;

    let limit: i32 = row.try_get("per_minute_limit")?;
    let count: i32 = row.try_get("window_count")?;
    let backoff_until: Option<chrono::DateTime<chrono::Utc>> = row.try_get("backoff_until")?;
    let now = chrono::Utc::now();
    if let Some(backoff_until) = backoff_until {
        if backoff_until > now {
            return Ok(Some(backoff_until));
        }
    }
    if count > limit {
        return Ok(Some(now + chrono::Duration::seconds(60)));
    }
    Ok(None)
}

async fn record_endpoint_success(pool: &sqlx::PgPool, url: &str) {
    if let Err(e) = sqlx::query(
        "UPDATE rate_limit_endpoints \
         SET consecutive_failures = 0, health_status = 'healthy', \
             backoff_until = NULL, updated_at = NOW() \
         WHERE endpoint_url = $1",
    )
    .bind(url)
    .execute(pool)
    .await
    {
        warn!(error = %e, url = %url, "Failed to record webhook endpoint success");
    }
}

async fn record_endpoint_failure(
    pool: &sqlx::PgPool,
    url: &str,
) -> chrono::DateTime<chrono::Utc> {
    let next_retry = chrono::Utc::now() + chrono::Duration::seconds(60);
    if let Err(e) = sqlx::query(
        "UPDATE rate_limit_endpoints \
         SET consecutive_failures = consecutive_failures + 1, \
             health_status = CASE \
                 WHEN consecutive_failures + 1 >= 3 THEN 'unhealthy' \
                 ELSE 'degraded' END, \
             backoff_until = NOW() + make_interval( \
                 secs => LEAST(900, POWER(2, LEAST(consecutive_failures + 1, 10))::int)), \
             updated_at = NOW() \
         WHERE endpoint_url = $1",
    )
    .bind(url)
    .execute(pool)
    .await
    {
        warn!(error = %e, url = %url, "Failed to record webhook endpoint failure");
    }
    next_retry
}

async fn enqueue_endpoint_retry(
    pool: &sqlx::PgPool,
    url: &str,
    event: &SorobanEvent,
    secret: &Option<String>,
    reason: &str,
    next_retry_at: chrono::DateTime<chrono::Utc>,
) {
    let payload = serde_json::to_value(event).unwrap_or(serde_json::json!({}));
    let secret_hash = secret.as_deref().map(|s| {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(s.as_bytes()))
    });
    if let Err(e) = sqlx::query(
        "INSERT INTO webhook_retry_queue \
         (url, payload, secret_hash, attempt, max_attempts, next_retry_at, last_error, status) \
         VALUES ($1, $2, $3, 0, 5, $4, $5, 'pending')",
    )
    .bind(url)
    .bind(payload)
    .bind(secret_hash)
    .bind(next_retry_at)
    .bind(reason)
    .execute(pool)
    .await
    {
        error!(error = %e, url = %url, "Failed to enqueue endpoint-specific webhook retry");
    }
}

/// Deliver with failover: if the primary URL fails, attempt the failover URL (#499).
/// Returns true if delivered (primary or failover), false if both failed.
pub async fn deliver_with_failover(
    _client: Client,
    primary_url: String,
    primary_secret: Option<String>,
    failover_url: Option<String>,
    failover_secret: Option<String>,
    event: SorobanEvent,
    pool: Option<&sqlx::PgPool>,
    retry_policy: &crate::retry_policy::RetryPolicy,
) -> bool {
    // Use the hardened outbound client for both primary and failover hops
    // (#1426, #1427).
    let client = match crate::net::outbound::outbound_client(Duration::from_secs(30)) {
        Ok(client) => client,
        Err(e) => {
            error!(error = %e, "Failed to build outbound client for webhook failover delivery");
            return false;
        }
    };

    // Reject unsafe primary/failover destinations before any request.
    if crate::net::outbound::validate_outbound_url(&primary_url).await.is_err()
        || !private_targets_permitted(&primary_url)
    {
        error!(url = %primary_url, "Primary webhook blocked by outbound SSRF guard");
        metrics::record_webhook_failure();
        return false;
    }
    if let Some(ref f_url) = failover_url {
        if crate::net::outbound::validate_outbound_url(f_url).await.is_err()
            || !private_targets_permitted(f_url)
        {
            error!(url = %f_url, "Failover webhook blocked by outbound SSRF guard");
            metrics::record_webhook_failure();
            return false;
        }
    }

    let body = match serde_json::to_vec(&event) {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to serialize event for webhook delivery");
            return false;
        }
    };

    let primary_sig = SignatureHeaders::new(primary_secret.as_deref(), &body);

    let primary_result = retry_policy.execute_with_retry(|attempt| {
        let client = client.clone();
        let url = primary_url.clone();
        let body = body.clone();
        let sig = primary_sig.clone();
        async move {
            let req = client
                .post(&url)
                .header("Content-Type", "application/json")
                .body(body);
            let req = sig.apply(req);
            match req.send().await {
                Ok(resp) if resp.status().is_success() => {
                    info!(url = %url, attempt = attempt, "Webhook delivered (primary)");
                    crate::metrics::record_webhook_delivery_success();
                    Ok(())
                }
                Ok(resp) => Err(format!("HTTP {}", resp.status())),
                Err(e) => Err(format!("Request error: {e}")),
            }
        }
    }).await;

    if primary_result.is_ok() {
        return true;
    }

    warn!(
        primary_url = %primary_url,
        "Primary webhook delivery failed, attempting failover"
    );

    // Attempt failover if configured
    if let Some(f_url) = failover_url {
        metrics::record_notification_failover("webhook");
        let f_sig = SignatureHeaders::new(failover_secret.as_deref(), &body);

        let failover_result = retry_policy.execute_with_retry(|attempt| {
            let client = client.clone();
            let url = f_url.clone();
            let body = body.clone();
            let sig = f_sig.clone();
            async move {
                let req = client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body(body);
                let req = sig.apply(req);
                match req.send().await {
                    Ok(resp) if resp.status().is_success() => {
                        info!(url = %url, attempt = attempt, "Webhook delivered (failover)");
                        crate::metrics::record_webhook_delivery_success();
                        Ok(())
                    }
                    Ok(resp) => Err(format!("HTTP {}", resp.status())),
                    Err(e) => Err(format!("Request error: {e}")),
                }
            }
        }).await;

        if failover_result.is_ok() {
            return true;
        }
        error!(failover_url = %f_url, "Failover webhook delivery also failed");
    }

    // Record DLQ and metric
    if let Some(pool) = pool {
        let payload = serde_json::to_value(&event).unwrap_or(serde_json::json!({}));
        let next_retry = chrono::Utc::now() + chrono::Duration::seconds(60);
        if let Err(e) = sqlx::query(
            "INSERT INTO webhook_failures (url, payload, attempts, last_error, next_retry_at)
             VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(&primary_url)
        .bind(payload)
        .bind(retry_policy.max_attempts as i32)
        .bind("Primary and failover both failed")
        .bind(next_retry)
        .execute(pool)
        .await
        {
            error!(error = %e, "Failed to insert webhook failure into DLQ");
        }

        // Move the exhausted delivery into the webhook DLQ (Issue #1260).
        if let Err(e) = enqueue_dlq(pool, None, &event, "Primary and failover both failed").await {
            error!(error = %e, "Failed to insert webhook into DLQ");
        }
    }
    metrics::record_webhook_failure();
    false
}

/// Insert an exhausted webhook delivery into the `webhook_dlq` table and
/// refresh the DLQ depth gauge (Issue #1260).
pub async fn enqueue_dlq(
    pool: &sqlx::PgPool,
    subscription_id: Option<Uuid>,
    event: &SorobanEvent,
    last_error: &str,
) -> Result<Uuid, sqlx::Error> {
    let payload = serde_json::to_value(event).unwrap_or(serde_json::json!({}));
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO webhook_dlq \
         (subscription_id, event_id, payload, last_error, failed_at) \
         VALUES ($1, $2, $3, $4, NOW()) \
         RETURNING id",
    )
    .bind(subscription_id)
    .bind(&event.tx_hash)
    .bind(payload)
    .bind(last_error)
    .fetch_one(pool)
    .await?;

    refresh_dlq_depth(pool).await;
    Ok(id)
}

/// Recompute the `soroban_pulse_webhook_dlq_depth` gauge from the table.
pub async fn refresh_dlq_depth(pool: &sqlx::PgPool) {
    match sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM webhook_dlq WHERE replayed_at IS NULL",
    )
    .fetch_one(pool)
    .await
    {
        Ok(depth) => metrics::set_webhook_dlq_depth(depth),
        Err(e) => warn!(error = %e, "Failed to compute webhook DLQ depth"),
    }
}

/// Re-enqueue a single DLQ row into the normal delivery queue and mark it
/// replayed (Issue #1260). Returns `Ok(false)` if the row does not exist or
/// has already been replayed.
pub async fn replay_dlq_entry(pool: &sqlx::PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        "SELECT payload, last_error FROM webhook_dlq \
         WHERE id = $1 AND replayed_at IS NULL",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(false);
    };

    let payload: Value = row.try_get("payload")?;
    let last_error: Option<String> = row.try_get("last_error").ok();
    let event: SorobanEvent = match serde_json::from_value(payload) {
        Ok(event) => event,
        Err(e) => {
            warn!(error = %e, dlq_id = %id, "Failed to deserialize DLQ payload");
            return Ok(false);
        }
    };

    // Re-enter the normal delivery queue.
    enqueue_endpoint_retry(
        pool,
        &event.contract_id,
        &event,
        &None,
        last_error.as_deref().unwrap_or("replayed from DLQ"),
        chrono::Utc::now(),
    )
    .await;

    sqlx::query("UPDATE webhook_dlq SET replayed_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    refresh_dlq_depth(pool).await;
    Ok(true)
}

/// Bulk replay of DLQ entries, optionally filtered by `subscription_id`
/// (Issue #1260). Returns the number of entries replayed.
pub async fn replay_dlq_bulk(
    pool: &sqlx::PgPool,
    subscription_id: Option<Uuid>,
) -> Result<u64, sqlx::Error> {
    let ids: Vec<Uuid> = match subscription_id {
        Some(sub_id) => {
            sqlx::query_scalar(
                "SELECT id FROM webhook_dlq \
                 WHERE replayed_at IS NULL AND subscription_id = $1",
            )
            .bind(sub_id)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_scalar("SELECT id FROM webhook_dlq WHERE replayed_at IS NULL")
                .fetch_all(pool)
                .await?
        }
    };

    let mut replayed = 0u64;
    for id in ids {
        match replay_dlq_entry(pool, id).await {
            Ok(true) => replayed += 1,
            Ok(false) => {}
            Err(e) => warn!(error = %e, dlq_id = %id, "Failed to replay DLQ entry"),
        }
    }

    refresh_dlq_depth(pool).await;
    Ok(replayed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn mock_event(contract_id: &str) -> SorobanEvent {
        SorobanEvent {
            contract_id: contract_id.to_string(),
            event_type: "contract".to_string(),
            tx_hash: "abc123".to_string(),
            ledger: 100,
            ledger_closed_at: "2026-06-25T00:00:00Z".to_string(),
            ledger_hash: None,
            in_successful_call: true,
            value: json!({"amount": "100", "action": "transfer"}),
            topic: None,
            tenant_id: None,
        }
    }

    #[test]
    fn test_sign_payload_produces_consistent_hex() {
        let sig1 = sign_payload("mysecret", b"hello world");
        let sig2 = sign_payload("mysecret", b"hello world");
        assert_eq!(sig1, sig2);
        assert_eq!(sig1.len(), 64);
    }

    #[test]
    fn test_sign_payload_different_secrets_differ() {
        let sig1 = sign_payload("secret1", b"payload");
        let sig2 = sign_payload("secret2", b"payload");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_payload_known_value() {
        let sig = sign_payload("key", b"test");
        assert_eq!(
            sig,
            "02afb56304902c656fcb737cdd03de6205bb6d401da2812efd9b2d36a08af159"
        );
    }

    /// Load the shared cross-language webhook signature fixture and verify
    /// every vector against the server's signing implementation
    /// (Issue #1413). This ties the fixture to `sign_payload` so drift in
    /// either the fixture or the signing scheme is caught.
    #[test]
    fn test_signature_vectors_fixture_matches_signing_implementation() {
        let fixture_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/webhook_signature_vectors.json");
        let raw = std::fs::read_to_string(&fixture_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", fixture_path.display()));
        let fixture: serde_json::Value = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("{} is not valid JSON: {e}", fixture_path.display()));

        let vectors = fixture
            .get("vectors")
            .and_then(|v| v.as_array())
            .expect("fixture must contain a `vectors` array");
        assert!(!vectors.is_empty(), "fixture must contain at least one vector");

        for vector in vectors {
            let name = vector
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("<unnamed>");
            let secret = vector
                .get("secret")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("vector {name} missing `secret`"));
            let timestamp = vector
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("vector {name} missing `timestamp`"));
            let payload = vector
                .get("payload")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("vector {name} missing `payload`"));
            let expected = vector
                .get("signature")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("vector {name} missing `signature`"));

            let signed = format!("{timestamp}.{payload}");
            let actual = sign_payload(secret, signed.as_bytes());
            assert_eq!(
                actual, expected,
                "signature mismatch for vector `{name}` (signed payload: {signed:?})"
            );
        }
    }

    #[test]
    fn test_sign_payload_v2_covers_timestamp() {
        let body = b"{\"a\":1}";
        let sig_a = sign_payload_v2("secret", 1_700_000_000, body);
        let sig_b = sign_payload_v2("secret", 1_700_000_001, body);
        assert_ne!(sig_a, sig_b);
        assert_eq!(sig_a.len(), 64);
    }

    #[test]
    fn test_sign_payload_v2_matches_manual_hmac() {
        let body = b"hello";
        let ts = 1_700_000_000i64;
        let mut mac = crate::hmac_util::hmac_sha256(b"secret");
        mac.update(format!("{ts}.", ).as_bytes());
        mac.update(body);
        let expected = hex::encode(mac.finalize().into_bytes());
        assert_eq!(sign_payload_v2("secret", ts, body), expected);
    }

    #[test]
    fn test_signature_v2_header_format() {
        let header = signature_v2_header("secret", 1_700_000_000, b"body");
        assert!(header.starts_with("t=1700000000,v1="));
        let hex_part = header.split("v1=").nth(1).unwrap();
        assert_eq!(hex_part.len(), 64);
    }

    #[test]
    fn test_signature_headers_include_delivery_id_and_both_signatures() {
        let headers = SignatureHeaders::new(Some("secret"), b"body");
        assert!(!headers.delivery_id.is_empty());
        assert!(headers.legacy.is_some());
        assert!(headers.v2.is_some());
        let pairs = headers.to_pairs();
        assert!(pairs.iter().any(|(k, _)| k == "X-SorobanPulse-Delivery-Id"));
        assert!(pairs.iter().any(|(k, _)| k == "X-Signature-256"));
        assert!(pairs.iter().any(|(k, _)| k == "X-SorobanPulse-Signature"));
    }

    #[test]
    fn test_signature_headers_without_secret_only_delivery_id() {
        let headers = SignatureHeaders::new(None, b"body");
        assert!(!headers.delivery_id.is_empty());
        assert!(headers.legacy.is_none());
        assert!(headers.v2.is_none());
        assert_eq!(headers.to_pairs().len(), 1);
    }

    #[test]
    fn test_delivery_ids_are_unique() {
        assert_ne!(new_delivery_id(), new_delivery_id());
    }

    #[test]
    fn test_evaluate_priority_no_rule_returns_default() {
        let event = mock_event("C1");
        let p = evaluate_priority(&event, None, None, None, "medium");
        assert_eq!(p, "medium");
    }

    #[test]
    fn test_evaluate_priority_rule_matches() {
        let event = mock_event("C1");
        // event.value = {"amount": "100", "action": "transfer"}
        let p = evaluate_priority(
            &event,
            Some("$.action"),
            Some("transfer"),
            Some("critical"),
            "medium",
        );
        assert_eq!(p, "critical");
    }

    #[test]
    fn test_evaluate_priority_rule_no_match_returns_default() {
        let event = mock_event("C1");
        let p = evaluate_priority(
            &event,
            Some("$.action"),
            Some("mint"),
            Some("critical"),
            "low",
        );
        assert_eq!(p, "low");
    }

    #[test]
    fn test_evaluate_priority_missing_path_returns_default() {
        let event = mock_event("C1");
        let p = evaluate_priority(
            &event,
            Some("$.nonexistent.nested"),
            Some("value"),
            Some("high"),
            "medium",
        );
        assert_eq!(p, "medium");
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_loopback_without_request() {
        let err = validate_webhook_url("http://127.0.0.1:5432/hook")
            .await
            .unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_metadata_ip() {
        let err = validate_webhook_url("http://169.254.169.254/latest/meta-data/")
            .await
            .unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_rfc1918() {
        let err = validate_webhook_url("http://10.0.0.5/hook").await.unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_ipv6_loopback() {
        let err = validate_webhook_url("http://[::1]:8080/hook")
            .await
            .unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_decimal_ip_encoding() {
        // 2130706433 == 127.0.0.1
        let err = validate_webhook_url("http://2130706433/hook").await.unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_octal_ip_encoding() {
        // 0177.0.0.1 == 127.0.0.1
        let err = validate_webhook_url("http://0177.0.0.1/hook").await.unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_localhost_name() {
        let err = validate_webhook_url("http://localhost:6379/hook")
            .await
            .unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[tokio::test]
    async fn test_validate_webhook_url_rejects_non_http_scheme() {
        let err = validate_webhook_url("file:///etc/passwd").await.unwrap_err();
        assert_eq!(err, WEBHOOK_URL_VALIDATION_FAILED);
    }

    #[test]
    fn test_private_targets_permitted_public_host() {
        assert!(private_targets_permitted("https://hooks.example.com/hook"));
    }

    #[test]
    fn test_private_targets_permitted_private_host_without_opt_in() {
        // Ensure the opt-in env var is not set for this test.
        std::env::remove_var("WEBHOOK_ALLOW_PRIVATE_TARGETS");
        assert!(!private_targets_permitted("http://10.1.2.3/hook"));
    }
}
