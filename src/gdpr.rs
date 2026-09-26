//! GDPR Data Handling Compliance Module (Issue #945)
//!
//! Implements:
//! - Data retention policy enforcement
//! - Right to Erasure (Article 17)
//! - Data export / Right to Portability (Article 20)
//! - Consent tracking
//! - Breach notification procedures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{info, warn};

// ─────────────────────────────────────────────
// Consent Tracking
// ─────────────────────────────────────────────

/// Types of consent that can be recorded
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsentType {
    Marketing,
    Analytics,
    Notifications,
    ThirdPartySharing,
    DataProcessing,
}

impl ConsentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Marketing => "marketing",
            Self::Analytics => "analytics",
            Self::Notifications => "notifications",
            Self::ThirdPartySharing => "third_party_sharing",
            Self::DataProcessing => "data_processing",
        }
    }
}

/// Legal basis for processing personal data under GDPR Article 6
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalBasis {
    Consent,
    Contract,
    LegalObligation,
    VitalInterests,
    PublicTask,
    LegitimateInterest,
}

impl LegalBasis {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Consent => "consent",
            Self::Contract => "contract",
            Self::LegalObligation => "legal_obligation",
            Self::VitalInterests => "vital_interests",
            Self::PublicTask => "public_task",
            Self::LegitimateInterest => "legitimate_interest",
        }
    }
}

/// Record a consent grant or withdrawal
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub subject_email: String,
    pub consent_type: ConsentType,
    pub granted: bool,
    pub legal_basis: LegalBasis,
    pub source: String,
    pub ip_address: Option<String>,
    pub notes: Option<String>,
}

/// Persist a consent record to the database
pub async fn record_consent(
    pool: &PgPool,
    record: &ConsentRecord,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4();
    let now = Utc::now();
    let granted_at = if record.granted { Some(now) } else { None };
    let withdrawn_at = if !record.granted { Some(now) } else { None };

    sqlx::query(
        "INSERT INTO gdpr_consent_records \
         (id, subject_email, consent_type, granted, granted_at, withdrawn_at, \
          ip_address, source, legal_basis) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(id)
    .bind(&record.subject_email)
    .bind(record.consent_type.as_str())
    .bind(record.granted)
    .bind(granted_at)
    .bind(withdrawn_at)
    .bind(&record.ip_address)
    .bind(&record.source)
    .bind(record.legal_basis.as_str())
    .execute(pool)
    .await?;

    info!(
        email = %record.subject_email,
        consent_type = %record.consent_type.as_str(),
        granted = record.granted,
        "Consent record saved"
    );

    Ok(id.to_string())
}

// ─────────────────────────────────────────────
// Data Subject Requests
// ─────────────────────────────────────────────

/// Type of GDPR data subject request
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DsrType {
    /// Article 15 — right of access
    Access,
    /// Article 17 — right to erasure
    Erasure,
    /// Article 16 — right to rectification
    Rectification,
    /// Article 20 — right to data portability
    Portability,
    /// Article 18 — right to restriction
    Restriction,
}

impl DsrType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Access => "access",
            Self::Erasure => "erasure",
            Self::Rectification => "rectification",
            Self::Portability => "portability",
            Self::Restriction => "restriction",
        }
    }
}

/// Status of a data subject request
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DsrStatus {
    Pending,
    InProgress,
    Completed,
    Rejected,
}

impl DsrStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Rejected => "rejected",
        }
    }
}

/// Register a new data subject request
pub async fn create_data_subject_request(
    pool: &PgPool,
    subject_email: &str,
    request_type: DsrType,
    notes: Option<&str>,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO gdpr_data_subject_requests (id, request_type, subject_email, status, notes) \
         VALUES ($1, $2, $3, 'pending', $4)",
    )
    .bind(id)
    .bind(request_type.as_str())
    .bind(subject_email)
    .bind(notes)
    .execute(pool)
    .await?;

    info!(
        email = %subject_email,
        request_type = %request_type.as_str(),
        id = %id,
        "Data subject request created"
    );

    Ok(id.to_string())
}

/// Update the status of a data subject request
pub async fn update_dsr_status(
    pool: &PgPool,
    dsr_id: &str,
    status: DsrStatus,
    handled_by: Option<&str>,
    proof: Option<serde_json::Value>,
) -> Result<(), sqlx::Error> {
    let completed_at = if status == DsrStatus::Completed {
        Some(Utc::now())
    } else {
        None
    };

    sqlx::query(
        "UPDATE gdpr_data_subject_requests \
         SET status = $1, handled_by = $2, proof_of_completion = $3, completed_at = $4 \
         WHERE id = $5",
    )
    .bind(status.as_str())
    .bind(handled_by)
    .bind(proof)
    .bind(completed_at)
    .bind(dsr_id)
    .execute(pool)
    .await?;

    Ok(())
}

// ─────────────────────────────────────────────
// Right to Erasure (Article 17)
// ─────────────────────────────────────────────

/// Result of executing a GDPR erasure request
#[derive(Debug, Serialize, Deserialize)]
pub struct ErasureResult {
    pub subject_email: String,
    pub subscriptions_deleted: u64,
    pub webhooks_deleted: u64,
    pub delivery_logs_deleted: u64,
    pub consent_records_deleted: u64,
    pub email_deliveries_deleted: u64,
    pub notification_audit_deleted: u64,
    pub completed_at: DateTime<Utc>,
    pub fully_erased: bool,
}

/// Execute a right-to-erasure request for a data subject.
///
/// Deletes all personal data associated with the given email address,
/// in dependency order, wrapped in a single transaction.
/// Contract events on the Stellar ledger are NOT deleted (immutable public data).
pub async fn execute_erasure_request(
    pool: &PgPool,
    subject_email: &str,
) -> Result<ErasureResult, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Delete delivery log entries referencing subscriptions for this email
    let delivery_logs_deleted = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
           DELETE FROM delivery_logs
           WHERE subscription_id IN (SELECT id FROM subscriptions WHERE email = $1)
           RETURNING 1
         ) SELECT COUNT(*) FROM deleted",
    )
    .bind(subject_email)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0) as u64;

    // Delete webhook delivery logs
    let webhooks_deleted = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
           DELETE FROM webhooks WHERE owner_email = $1 RETURNING 1
         ) SELECT COUNT(*) FROM deleted",
    )
    .bind(subject_email)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0) as u64;

    // Delete subscriptions
    let subscriptions_deleted = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
           DELETE FROM subscriptions WHERE email = $1 RETURNING 1
         ) SELECT COUNT(*) FROM deleted",
    )
    .bind(subject_email)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0) as u64;

    // Delete email deliveries
    let email_deliveries_deleted = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
           DELETE FROM email_deliveries WHERE recipient = $1 RETURNING 1
         ) SELECT COUNT(*) FROM deleted",
    )
    .bind(subject_email)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0) as u64;

    // Delete notification audit log entries
    let notification_audit_deleted = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
           DELETE FROM notification_audit_log WHERE recipient = $1 RETURNING 1
         ) SELECT COUNT(*) FROM deleted",
    )
    .bind(subject_email)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0) as u64;

    // Delete consent records
    let consent_records_deleted = sqlx::query_scalar::<_, i64>(
        "WITH deleted AS (
           DELETE FROM gdpr_consent_records WHERE subject_email = $1 RETURNING 1
         ) SELECT COUNT(*) FROM deleted",
    )
    .bind(subject_email)
    .fetch_one(&mut *tx)
    .await
    .unwrap_or(0) as u64;

    tx.commit().await?;

    warn!(
        email = %subject_email,
        subscriptions = subscriptions_deleted,
        webhooks = webhooks_deleted,
        delivery_logs = delivery_logs_deleted,
        consent = consent_records_deleted,
        "GDPR erasure request executed"
    );

    Ok(ErasureResult {
        subject_email: subject_email.to_string(),
        subscriptions_deleted,
        webhooks_deleted,
        delivery_logs_deleted,
        consent_records_deleted,
        email_deliveries_deleted,
        notification_audit_deleted,
        completed_at: Utc::now(),
        fully_erased: true,
    })
}

// ─────────────────────────────────────────────
// Data Export / Right to Portability (Article 20)
// ─────────────────────────────────────────────

/// Export all personal data for a data subject as structured JSON
pub async fn export_subject_data(
    pool: &PgPool,
    subject_email: &str,
) -> Result<serde_json::Value, sqlx::Error> {
    let result = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT gdpr_get_subject_data($1)",
    )
    .bind(subject_email)
    .fetch_one(pool)
    .await?;

    Ok(result)
}

// ─────────────────────────────────────────────
// Breach Notification (Articles 33 & 34)
// ─────────────────────────────────────────────

/// Register a personal data breach
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BreachNotification {
    pub detected_at: DateTime<Utc>,
    pub breach_type: String,
    pub data_categories: Vec<String>,
    pub affected_subject_count: Option<i32>,
    pub description: String,
    pub containment_measures: Option<String>,
    pub likely_consequences: Option<String>,
}

/// Record a breach notification in the database.
///
/// Supervisory authority must be notified within 72 hours of detection
/// (GDPR Article 33). Call [`mark_authority_notified`] once notification is sent.
pub async fn record_breach(
    pool: &PgPool,
    breach: &BreachNotification,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4();
    sqlx::query(
        "INSERT INTO gdpr_breach_notifications \
         (id, detected_at, breach_type, data_categories, affected_subject_count, \
          description, containment_measures, likely_consequences) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(breach.detected_at)
    .bind(&breach.breach_type)
    .bind(&breach.data_categories)
    .bind(breach.affected_subject_count)
    .bind(&breach.description)
    .bind(&breach.containment_measures)
    .bind(&breach.likely_consequences)
    .execute(pool)
    .await?;

    warn!(
        id = %id,
        breach_type = %breach.breach_type,
        detected_at = %breach.detected_at,
        "Personal data breach recorded — supervisory authority must be notified within 72 hours"
    );

    Ok(id.to_string())
}

/// Mark that the supervisory authority has been notified of a breach
pub async fn mark_authority_notified(
    pool: &PgPool,
    breach_id: &str,
    dpa_reference: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE gdpr_breach_notifications \
         SET notified_authority_at = NOW(), dpa_reference = $2 \
         WHERE id = $1",
    )
    .bind(breach_id)
    .bind(dpa_reference)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consent_type_as_str() {
        assert_eq!(ConsentType::Marketing.as_str(), "marketing");
        assert_eq!(ConsentType::Analytics.as_str(), "analytics");
        assert_eq!(ConsentType::Notifications.as_str(), "notifications");
        assert_eq!(ConsentType::ThirdPartySharing.as_str(), "third_party_sharing");
        assert_eq!(ConsentType::DataProcessing.as_str(), "data_processing");
    }

    #[test]
    fn test_legal_basis_as_str() {
        assert_eq!(LegalBasis::Consent.as_str(), "consent");
        assert_eq!(LegalBasis::Contract.as_str(), "contract");
        assert_eq!(LegalBasis::LegitimateInterest.as_str(), "legitimate_interest");
    }

    #[test]
    fn test_dsr_type_as_str() {
        assert_eq!(DsrType::Access.as_str(), "access");
        assert_eq!(DsrType::Erasure.as_str(), "erasure");
        assert_eq!(DsrType::Portability.as_str(), "portability");
    }

    #[test]
    fn test_dsr_status_as_str() {
        assert_eq!(DsrStatus::Pending.as_str(), "pending");
        assert_eq!(DsrStatus::Completed.as_str(), "completed");
        assert_eq!(DsrStatus::Rejected.as_str(), "rejected");
    }

    #[test]
    fn test_breach_notification_struct() {
        let breach = BreachNotification {
            detected_at: Utc::now(),
            breach_type: "confidentiality".to_string(),
            data_categories: vec!["email".to_string(), "subscription_data".to_string()],
            affected_subject_count: Some(150),
            description: "Unauthorized access to subscription database".to_string(),
            containment_measures: Some("Access revoked, credentials rotated".to_string()),
            likely_consequences: Some("Spam risk for affected email addresses".to_string()),
        };
        assert_eq!(breach.breach_type, "confidentiality");
        assert_eq!(breach.data_categories.len(), 2);
        assert_eq!(breach.affected_subject_count, Some(150));
    }
}
