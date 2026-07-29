/// GDPR & Data Retention Compliance Reporting (Issue #810)
///
/// Builds auditor-facing compliance reports from the existing audit log
/// trail, and verifies that a data subject's personal data has actually
/// been erased after a GDPR Article 17 request.
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

/// Summary of deletion-related audit activity for a given time window.
#[derive(Debug, Serialize)]
pub struct DeletionAuditReport {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub total_delete_events: i64,
    pub successful_deletions: i64,
    pub failed_deletions: i64,
    pub data_export_events: i64,
}

/// Generate a compliance report covering DELETE and DATA_EXPORT audit
/// events in `[from, to]`, suitable for handing to an auditor.
pub async fn generate_deletion_audit_report(
    pool: &PgPool,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<DeletionAuditReport, sqlx::Error> {
    let total_delete_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE event_type = 'DELETE' AND created_at BETWEEN $1 AND $2",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;

    let successful_deletions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE event_type = 'DELETE' AND success = true AND created_at BETWEEN $1 AND $2",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;

    let data_export_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_logs WHERE event_type = 'DATA_EXPORT' AND created_at BETWEEN $1 AND $2",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;

    Ok(DeletionAuditReport {
        from,
        to,
        total_delete_events,
        successful_deletions,
        failed_deletions: total_delete_events - successful_deletions,
        data_export_events,
    })
}

/// Result of verifying that a GDPR erasure request was fully honored.
#[derive(Debug, Serialize)]
pub struct GdprErasureVerification {
    pub email: String,
    pub remaining_email_deliveries: i64,
    pub remaining_email_delivery_log: i64,
    pub remaining_notification_audit_log: i64,
    pub fully_erased: bool,
}

/// Verify that no personal data remains for `email` after an erasure
/// request, per the "Right to Erasure" procedure in `docs/data-retention.md`.
pub async fn verify_gdpr_erasure(
    pool: &PgPool,
    email: &str,
) -> Result<GdprErasureVerification, sqlx::Error> {
    let remaining_email_deliveries: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM email_deliveries WHERE recipient = $1")
            .bind(email)
            .fetch_one(pool)
            .await?;

    let remaining_email_delivery_log: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM email_delivery_log WHERE email_address = $1")
            .bind(email)
            .fetch_one(pool)
            .await?;

    let remaining_notification_audit_log: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM notification_audit_log WHERE recipient = $1")
            .bind(email)
            .fetch_one(pool)
            .await?;

    let fully_erased = remaining_email_deliveries == 0
        && remaining_email_delivery_log == 0
        && remaining_notification_audit_log == 0;

    Ok(GdprErasureVerification {
        email: email.to_string(),
        remaining_email_deliveries,
        remaining_email_delivery_log,
        remaining_notification_audit_log,
        fully_erased,
    })
}
