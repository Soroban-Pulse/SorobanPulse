//! Notification delivery receipts (Issue #475).
//!
//! The notification system delivers events to configured channels (webhook,
//! email, …) but, without delivery receipts, operators cannot determine whether
//! a notification was actually delivered. Compliance requirements often mandate
//! proof of delivery for critical alerts.
//!
//! Every delivery attempt is recorded in the `notification_deliveries` table
//! and exposed through the `GET /v1/admin/notifications/deliveries` endpoint.
//!
//! Issue #994 additionally adds a unified delivery *framework* (the
//! [`DeliveryChannel`] trait, [`NotificationError`], [`DeliveryOutcome`], and
//! [`deliver_with_retry`]) to this module — see the "Unified notification
//! delivery framework" section below and `docs/notification-architecture.md`.
//! It is additive to, and independent of, the receipts functionality above:
//! receipts record *that* a delivery happened; the framework governs *how*
//! a delivery attempt (with retries) is driven.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::metrics;

/// Outcome of a single notification delivery attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryStatus {
    Success,
    Failure,
}

impl DeliveryStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            DeliveryStatus::Success => "success",
            DeliveryStatus::Failure => "failure",
        }
    }
}

/// A persisted delivery receipt as returned by the admin query endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeliveryReceipt {
    pub id: Uuid,
    pub channel_type: String,
    pub channel_config_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub status: String,
    pub delivered_at: DateTime<Utc>,
    pub error: Option<String>,
}

/// Record a single delivery attempt in `notification_deliveries` and increment
/// the matching success/failure counter.
///
/// Recording failures must never mask the original delivery error, so a DB
/// error here is logged and swallowed rather than propagated.
pub async fn record_delivery(
    pool: &sqlx::PgPool,
    channel_type: &str,
    channel_config_id: Option<Uuid>,
    event_id: Option<Uuid>,
    status: DeliveryStatus,
    error: Option<&str>,
) {
    match status {
        DeliveryStatus::Success => metrics::record_notification_delivery_success(),
        DeliveryStatus::Failure => metrics::record_notification_delivery_failure(),
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO notification_deliveries \
         (channel_type, channel_config_id, event_id, status, error) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(channel_type)
    .bind(channel_config_id)
    .bind(event_id)
    .bind(status.as_str())
    .bind(error)
    .execute(pool)
    .await
    {
        tracing::error!(error = %e, "Failed to record notification delivery receipt");
    }
}

/// Best-effort resolution of the `events.id` for a delivered event so the
/// receipt can be linked back to the originating event. Returns `None` if the
/// event cannot be found or the lookup fails.
pub async fn resolve_event_id(
    pool: &sqlx::PgPool,
    event: &crate::models::SorobanEvent,
) -> Option<Uuid> {
    sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM events \
         WHERE tx_hash = $1 AND contract_id = $2 AND event_type = $3 \
         LIMIT 1",
    )
    .bind(&event.tx_hash)
    .bind(&event.contract_id)
    .bind(&event.event_type)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

/// Query delivery history, most recent first. Supports optional filtering by
/// channel type and status, and a bounded limit.
pub async fn query_deliveries(
    pool: &sqlx::PgPool,
    channel_type: Option<&str>,
    status: Option<&str>,
    limit: i64,
) -> Result<Vec<DeliveryReceipt>, sqlx::Error> {
    sqlx::query_as::<_, DeliveryReceipt>(
        "SELECT id, channel_type, channel_config_id, event_id, status, delivered_at, error \
         FROM notification_deliveries \
         WHERE ($1::text IS NULL OR channel_type = $1) \
           AND ($2::text IS NULL OR status = $2) \
         ORDER BY delivered_at DESC \
         LIMIT $3",
    )
    .bind(channel_type)
    .bind(status)
    .bind(limit)
    .fetch_all(pool)
    .await
}

/// Aggregated delivery statistics for a single channel+status combination,
/// sourced from the `notification_delivery_stats` view.
///
/// Used by the admin dashboard to give operators a quick overview of
/// notification health across all channels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptStats {
    /// The delivery channel (e.g. "webhook", "email", "sms").
    pub channel_type: String,
    /// Total delivery attempts recorded for this channel.
    pub total_deliveries: i64,
    /// Number of successful attempts.
    pub successful: i64,
    /// Number of failed attempts.
    pub failed: i64,
    /// Fraction of attempts that succeeded, in the range [0.0, 1.0].
    pub success_rate: f64,
    /// Mean delivery latency in milliseconds, or `None` if no latency data
    /// has been recorded.
    pub avg_latency_ms: Option<f64>,
}

/// Extended delivery receipt that includes the new fields added by migration
/// `20260830000003_notification_delivery_receipts.sql`:
/// * `channel_metadata` — arbitrary JSON context captured at delivery time
///   (e.g. HTTP status code, SMS provider response).
/// * `retry_count` — number of prior attempts before this outcome.
/// * `latency_ms` — wall-clock milliseconds from dispatch to final outcome.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeliveryReceiptExtended {
    pub id: Uuid,
    pub channel_type: String,
    pub channel_config_id: Option<Uuid>,
    pub event_id: Option<Uuid>,
    pub status: String,
    pub delivered_at: DateTime<Utc>,
    pub error: Option<String>,
    /// Provider-specific context captured at delivery time.
    pub channel_metadata: Option<serde_json::Value>,
    /// How many prior attempts preceded this record (0 = first attempt).
    pub retry_count: i32,
    /// Round-trip latency from dispatch to final outcome, in milliseconds.
    pub latency_ms: Option<i32>,
}

/// Record a delivery attempt that carries extended metadata introduced by
/// Issue #933 (channel metadata, retry count, latency).
///
/// Failures are swallowed after logging so that a DB hiccup never masks the
/// original delivery error seen by the caller.
pub async fn record_delivery_with_metadata(
    pool: &sqlx::PgPool,
    channel_type: &str,
    channel_config_id: Option<Uuid>,
    event_id: Option<Uuid>,
    status: DeliveryStatus,
    error: Option<&str>,
    channel_metadata: Option<&serde_json::Value>,
    retry_count: i32,
    latency_ms: Option<i32>,
) {
    match status {
        DeliveryStatus::Success => metrics::record_notification_delivery_success(),
        DeliveryStatus::Failure => metrics::record_notification_delivery_failure(),
    }

    if let Err(e) = sqlx::query(
        "INSERT INTO notification_deliveries \
         (channel_type, channel_config_id, event_id, status, error, \
          channel_metadata, retry_count, latency_ms) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(channel_type)
    .bind(channel_config_id)
    .bind(event_id)
    .bind(status.as_str())
    .bind(error)
    .bind(channel_metadata)
    .bind(retry_count)
    .bind(latency_ms)
    .execute(pool)
    .await
    {
        tracing::error!(
            error = %e,
            channel_type = channel_type,
            "Failed to record extended notification delivery receipt"
        );
    }
}

/// Query the `notification_delivery_stats` view and return one [`ReceiptStats`]
/// entry per (channel_type, status) pair, pivoted so each channel_type
/// appears as a single row with separate `successful` / `failed` counts and a
/// pre-computed `success_rate`.
///
/// The view is maintained automatically by the database; no extra aggregation
/// is needed here.
pub async fn get_receipt_stats(
    pool: &sqlx::PgPool,
) -> Result<Vec<ReceiptStats>, sqlx::Error> {
    // We fetch raw (channel_type, status, count, avg_latency_ms) rows from
    // the view and pivot them in Rust so callers get one ReceiptStats per
    // channel_type.
    let rows = sqlx::query(
        "SELECT channel_type, status, count, avg_latency_ms \
         FROM notification_delivery_stats \
         ORDER BY channel_type, status",
    )
    .fetch_all(pool)
    .await?;

    use sqlx::Row as _;
    use std::collections::HashMap;

    // Accumulate per-channel stats from the flattened view rows.
    let mut map: HashMap<String, (i64, i64, Option<f64>)> = HashMap::new();

    for row in &rows {
        let channel_type: String = row.try_get("channel_type")?;
        let status: String = row.try_get("status")?;
        let count: i64 = row.try_get("count")?;
        let avg_latency: Option<f64> = row.try_get("avg_latency_ms")?;

        let entry = map.entry(channel_type).or_insert((0, 0, None));
        match status.as_str() {
            "success" => {
                entry.0 += count;
                // Use the latency from the success rows (most meaningful).
                if entry.2.is_none() {
                    entry.2 = avg_latency;
                }
            }
            "failure" => {
                entry.1 += count;
            }
            _ => {
                // Unknown status — add to totals but don't categorise.
                entry.0 += count;
            }
        }
    }

    let stats = map
        .into_iter()
        .map(|(channel_type, (successful, failed, avg_latency_ms))| {
            let total_deliveries = successful + failed;
            let success_rate = if total_deliveries > 0 {
                successful as f64 / total_deliveries as f64
            } else {
                0.0
            };
            ReceiptStats {
                channel_type,
                total_deliveries,
                successful,
                failed,
                success_rate,
                avg_latency_ms,
            }
        })
        .collect();

    Ok(stats)
}

/// Delete delivery receipts older than `retention_days` days.
///
/// This is intended to be called periodically (e.g. by a nightly cron job or
/// the pruner background task) to keep the `notification_deliveries` table
/// from growing unboundedly.
///
/// Returns the number of rows deleted.
pub async fn purge_old_receipts(
    pool: &sqlx::PgPool,
    retention_days: i64,
) -> Result<u64, sqlx::Error> {
    let cutoff = Utc::now() - chrono::Duration::days(retention_days);

    let result = sqlx::query(
        "DELETE FROM notification_deliveries WHERE delivered_at < $1",
    )
    .bind(cutoff)
    .execute(pool)
    .await?;

    let deleted = result.rows_affected();

    tracing::info!(
        deleted = deleted,
        retention_days = retention_days,
        "Purged old notification delivery receipts"
    );

    Ok(deleted)
}

/// Return all [`DeliveryReceiptExtended`] records linked to the given
/// `event_id`, ordered most-recent first.
///
/// This allows an operator (or an automated checker) to audit exactly which
/// channels received a notification for a particular indexed event, and whether
/// each delivery succeeded.
pub async fn get_receipts_by_event(
    pool: &sqlx::PgPool,
    event_id: Uuid,
) -> Result<Vec<DeliveryReceiptExtended>, sqlx::Error> {
    sqlx::query_as::<_, DeliveryReceiptExtended>(
        "SELECT id, channel_type, channel_config_id, event_id, status, \
                delivered_at, error, channel_metadata, retry_count, latency_ms \
         FROM notification_deliveries \
         WHERE event_id = $1 \
         ORDER BY delivered_at DESC",
    )
    .bind(event_id)
    .fetch_all(pool)
    .await
}

// ============================================================================
// Unified notification delivery framework (Issue #994)
// ============================================================================
//
// Consolidates the retry logic and error handling that were previously
// duplicated across `email.rs`, `sms.rs`, and `push_notification.rs` into a
// single `DeliveryChannel` trait plus a shared `deliver_with_retry` driver.
// See `docs/notification-architecture.md` for the full design and the
// per-channel migration notes.
//
// This is distinct from `crate::notification_channel`, which covers
// webhook-style outbound channels (Discord, Telegram, ...). This section
// covers the three channels named in issue #994: email, SMS, and push.

use crate::retry_policy::RetryPolicy;

/// Consolidated error type for notification delivery across all channels.
///
/// Replaces the ad-hoc `String` / `Box<dyn Error>` error types previously
/// used independently by each channel.
#[derive(Debug, Clone)]
pub enum NotificationError {
    /// A transient failure (network error, upstream 5xx, rate limit). Safe to retry.
    Transient(String),
    /// The target (email address, phone number, device token) is permanently
    /// invalid and retrying will not help (e.g. bounced address, unregistered
    /// push token, malformed phone number).
    InvalidTarget(String),
    /// The channel is misconfigured (missing credentials, invalid config).
    /// Not retryable — requires operator intervention.
    Configuration(String),
}

impl std::fmt::Display for NotificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotificationError::Transient(msg) => write!(f, "transient delivery error: {msg}"),
            NotificationError::InvalidTarget(msg) => write!(f, "invalid delivery target: {msg}"),
            NotificationError::Configuration(msg) => write!(f, "channel misconfigured: {msg}"),
        }
    }
}

impl std::error::Error for NotificationError {}

/// Outcome of a single delivery attempt driven through the unified framework.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// The message was accepted for delivery by the upstream provider.
    Delivered,
    /// The target was rejected as permanently invalid (e.g. expired push
    /// token); the caller should stop sending to it, not retry.
    InvalidTarget,
}

/// A notification delivery channel (email, SMS, push, ...).
///
/// Implementors provide a single-attempt `deliver` plus the retry policy
/// that should govern it; `deliver_with_retry` supplies the shared retry
/// loop, metrics, and error classification so channels don't each
/// reimplement it.
#[async_trait::async_trait]
pub trait DeliveryChannel: Send + Sync {
    /// Short, stable identifier used in metrics and logs (e.g. "email").
    fn channel_name(&self) -> &'static str;

    /// The retry policy this channel should be driven with.
    fn retry_policy(&self) -> RetryPolicy;

    /// Attempt one delivery to `target`. Implementations should classify
    /// failures as [`NotificationError::Transient`] (retryable),
    /// [`NotificationError::InvalidTarget`] (not retryable — target is bad),
    /// or [`NotificationError::Configuration`] (not retryable — channel is
    /// misconfigured) rather than returning an opaque error.
    async fn deliver(
        &self,
        target: &str,
        subject: &str,
        body: &str,
    ) -> Result<DeliveryOutcome, NotificationError>;
}

/// Drive a [`DeliveryChannel`] through its retry policy, recording the
/// consolidated delivery-success/failure metrics uniformly across all
/// channels (issue #994; previously each channel recorded its own ad-hoc
/// metrics on success/failure).
///
/// Only [`NotificationError::Transient`] is retried; `InvalidTarget` and
/// `Configuration` fail fast since retrying cannot help either case. This is
/// why this driver does not reuse `RetryPolicy::execute_with_retry` directly
/// — that helper retries on any `Err`, with no way to opt individual error
/// variants out of the retry loop.
pub async fn deliver_with_retry(
    channel: &dyn DeliveryChannel,
    target: &str,
    subject: &str,
    body: &str,
) -> Result<DeliveryOutcome, NotificationError> {
    let policy = channel.retry_policy();
    let start = std::time::Instant::now();

    let mut result = Err(NotificationError::Transient("no attempts made".to_string()));

    for attempt in 1..=policy.max_attempts {
        match channel.deliver(target, subject, body).await {
            Ok(outcome) => {
                result = Ok(outcome);
                break;
            }
            Err(NotificationError::Transient(msg)) => {
                result = Err(NotificationError::Transient(msg));
                if attempt < policy.max_attempts {
                    tokio::time::sleep(policy.calculate_backoff(attempt)).await;
                }
            }
            Err(other) => {
                // Not retryable — fail fast rather than burning through
                // the rest of the retry budget.
                result = Err(other);
                break;
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    crate::metrics::record_notification_delivery_latency(channel.channel_name(), elapsed);

    match &result {
        Ok(_) => crate::metrics::record_notification_delivery_success(),
        Err(_) => crate::metrics::record_notification_delivery_failure(),
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    // Issue #994 checklist item "Write comprehensive delivery tests" was
    // intentionally NOT done for the DeliveryChannel/deliver_with_retry
    // additions below — this change was implemented under an explicit
    // "implement only, do not test" instruction. Do not treat the absence of
    // tests for that part of this module as an oversight; add them (mock
    // `DeliveryChannel` impls covering Transient/InvalidTarget/Configuration
    // and retry exhaustion) before relying on the framework in production.
    // The tests below predate issue #994 and cover the delivery-receipts
    // functionality only.

    #[test]
    fn delivery_status_serializes_to_expected_strings() {
        assert_eq!(DeliveryStatus::Success.as_str(), "success");
        assert_eq!(DeliveryStatus::Failure.as_str(), "failure");
    }

    #[sqlx::test]
    async fn record_delivery_persists_a_success_receipt(pool: PgPool) {
        let event_id = Uuid::new_v4();
        record_delivery(
            &pool,
            "webhook",
            None,
            Some(event_id),
            DeliveryStatus::Success,
            None,
        )
        .await;

        let rows = query_deliveries(&pool, None, None, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].channel_type, "webhook");
        assert_eq!(rows[0].status, "success");
        assert_eq!(rows[0].event_id, Some(event_id));
        assert!(rows[0].error.is_none());
    }

    #[sqlx::test]
    async fn record_delivery_persists_a_failure_with_error(pool: PgPool) {
        record_delivery(
            &pool,
            "webhook",
            None,
            None,
            DeliveryStatus::Failure,
            Some("HTTP 500: boom"),
        )
        .await;

        let rows = query_deliveries(&pool, None, None, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, "failure");
        assert_eq!(rows[0].error.as_deref(), Some("HTTP 500: boom"));
    }

    #[sqlx::test]
    async fn query_deliveries_filters_by_channel_and_status(pool: PgPool) {
        record_delivery(&pool, "webhook", None, None, DeliveryStatus::Success, None).await;
        record_delivery(&pool, "email", None, None, DeliveryStatus::Failure, Some("smtp")).await;

        let only_email = query_deliveries(&pool, Some("email"), None, 100).await.unwrap();
        assert_eq!(only_email.len(), 1);
        assert_eq!(only_email[0].channel_type, "email");

        let only_failures = query_deliveries(&pool, None, Some("failure"), 100).await.unwrap();
        assert_eq!(only_failures.len(), 1);
        assert_eq!(only_failures[0].status, "failure");

        let all = query_deliveries(&pool, None, None, 100).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    // ------------------------------------------------------------------
    // Tests for Issue #933 additions
    // ------------------------------------------------------------------

    #[test]
    fn receipt_stats_success_rate_zero_when_no_deliveries() {
        let stats = ReceiptStats {
            channel_type: "sms".to_string(),
            total_deliveries: 0,
            successful: 0,
            failed: 0,
            success_rate: 0.0,
            avg_latency_ms: None,
        };
        assert_eq!(stats.success_rate, 0.0);
        assert_eq!(stats.total_deliveries, 0);
    }

    #[test]
    fn receipt_stats_success_rate_full_success() {
        let total = 10i64;
        let successful = 10i64;
        let failed = 0i64;
        let success_rate = if total > 0 {
            successful as f64 / total as f64
        } else {
            0.0
        };
        assert!((success_rate - 1.0).abs() < f64::EPSILON);
        let stats = ReceiptStats {
            channel_type: "webhook".to_string(),
            total_deliveries: total,
            successful,
            failed,
            success_rate,
            avg_latency_ms: Some(42.5),
        };
        assert_eq!(stats.successful, 10);
        assert_eq!(stats.failed, 0);
        assert!(stats.avg_latency_ms.is_some());
    }

    #[test]
    fn delivery_receipt_extended_fields_are_optional() {
        let receipt = DeliveryReceiptExtended {
            id: Uuid::new_v4(),
            channel_type: "sms".to_string(),
            channel_config_id: None,
            event_id: None,
            status: "success".to_string(),
            delivered_at: Utc::now(),
            error: None,
            channel_metadata: None,
            retry_count: 0,
            latency_ms: None,
        };
        assert!(receipt.channel_metadata.is_none());
        assert_eq!(receipt.retry_count, 0);
        assert!(receipt.latency_ms.is_none());
    }

    #[test]
    fn delivery_receipt_extended_captures_metadata() {
        let meta = serde_json::json!({ "http_status": 200, "provider": "twilio" });
        let receipt = DeliveryReceiptExtended {
            id: Uuid::new_v4(),
            channel_type: "sms".to_string(),
            channel_config_id: None,
            event_id: Some(Uuid::new_v4()),
            status: "success".to_string(),
            delivered_at: Utc::now(),
            error: None,
            channel_metadata: Some(meta.clone()),
            retry_count: 2,
            latency_ms: Some(350),
        };
        assert_eq!(receipt.retry_count, 2);
        assert_eq!(receipt.latency_ms, Some(350));
        assert_eq!(receipt.channel_metadata.unwrap()["http_status"], 200);
    }
}
