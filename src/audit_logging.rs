/// Audit Logging Module for Sensitive Operations (Issue #568)
///
/// Provides comprehensive audit trail functionality for:
/// - DELETE operations
/// - Configuration changes
/// - Administrative API calls
/// - Authentication changes

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::net::IpAddr;
use tracing::{debug, error};

use crate::client_ip::ClientIp;

/// Severity levels for audit events
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditSeverity {
    Low,
    Info,
    Warning,
    Critical,
}

impl AuditSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "LOW",
            Self::Info => "INFO",
            Self::Warning => "WARNING",
            Self::Critical => "CRITICAL",
        }
    }
}

/// Event types for audit logging
#[derive(Clone, Debug)]
pub enum AuditEventType {
    Delete,
    ConfigChange,
    AdminApiCall,
    AuthChange,
    AccessControlChange,
    DataExport,
    Custom(String),
}

impl AuditEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::ConfigChange => "CONFIG_CHANGE",
            Self::AdminApiCall => "ADMIN_API_CALL",
            Self::AuthChange => "AUTH_CHANGE",
            Self::AccessControlChange => "ACCESS_CONTROL_CHANGE",
            Self::DataExport => "DATA_EXPORT",
            Self::Custom(s) => s,
        }
    }
}

/// Audit log entry
#[derive(Clone, Debug)]
pub struct AuditLogEntry {
    pub event_type: AuditEventType,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub resource_description: Option<String>,
    pub request_path: Option<String>,
    pub request_method: Option<String>,
    pub query_params: Option<Value>,
    pub api_key_hash: Option<String>,
    pub user_email: Option<String>,
    pub user_id: Option<String>,
    pub ip_address: Option<IpAddr>,
    pub user_agent: Option<String>,
    pub changes: Option<Value>,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
    pub status_code: Option<i32>,
    pub success: bool,
    pub error_message: Option<String>,
    pub created_by: Option<String>,
    pub severity: AuditSeverity,
}

impl AuditLogEntry {
    /// Create a new audit log entry
    pub fn new(event_type: AuditEventType, action: impl Into<String>, resource_type: impl Into<String>) -> Self {
        Self {
            event_type,
            action: action.into(),
            resource_type: resource_type.into(),
            resource_id: None,
            resource_description: None,
            request_path: None,
            request_method: None,
            query_params: None,
            api_key_hash: None,
            user_email: None,
            user_id: None,
            ip_address: None,
            user_agent: None,
            changes: None,
            old_value: None,
            new_value: None,
            status_code: None,
            success: true,
            error_message: None,
            created_by: None,
            severity: AuditSeverity::Info,
        }
    }

    /// Set the resource ID
    pub fn with_resource_id(mut self, id: impl Into<String>) -> Self {
        self.resource_id = Some(id.into());
        self
    }

    /// Set the resource description
    pub fn with_resource_description(mut self, desc: impl Into<String>) -> Self {
        self.resource_description = Some(desc.into());
        self
    }

    /// Set the request information
    pub fn with_request(mut self, path: impl Into<String>, method: impl Into<String>) -> Self {
        self.request_path = Some(path.into());
        self.request_method = Some(method.into());
        self
    }

    /// Set the API key (will be hashed)
    pub fn with_api_key(mut self, key: impl AsRef<str>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(key.as_ref().as_bytes());
        self.api_key_hash = Some(format!("{:x}", hasher.finalize()));
        self
    }

    /// Set the user email
    pub fn with_user_email(mut self, email: impl Into<String>) -> Self {
        self.user_email = Some(email.into());
        self
    }

    /// Set the IP address
    pub fn with_ip_address(mut self, ip: IpAddr) -> Self {
        self.ip_address = Some(ip);
        self
    }

    /// Set the IP address from a verified [`ClientIp`] (Issue #1432).
    ///
    /// The IP recorded here is the one resolved by the centralised
    /// `ClientIp` extractor, which only honours forwarding headers when the
    /// TCP peer is a trusted proxy. Callers must never parse
    /// `X-Forwarded-For`/`Forwarded` themselves.
    pub fn with_client_ip(mut self, client_ip: &ClientIp) -> Self {
        self.ip_address = Some(client_ip.ip());
        self
    }

    /// Set the User-Agent header
    pub fn with_user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }

    /// Set the changes (what changed and how)
    pub fn with_changes(mut self, changes: Value) -> Self {
        self.changes = Some(changes);
        self
    }

    /// Set old and new values (for before/after comparison)
    pub fn with_values(mut self, old: Option<Value>, new: Option<Value>) -> Self {
        self.old_value = old;
        self.new_value = new;
        self
    }

    /// Set the response status code
    pub fn with_status_code(mut self, code: i32) -> Self {
        self.status_code = Some(code);
        self
    }

    /// Mark as failed
    pub fn with_failure(mut self, message: impl Into<String>) -> Self {
        self.success = false;
        self.error_message = Some(message.into());
        self
    }

    /// Set the severity level
    pub fn with_severity(mut self, severity: AuditSeverity) -> Self {
        self.severity = severity;
        self
    }

    /// Set who created this log entry
    pub fn with_created_by(mut self, creator: impl Into<String>) -> Self {
        self.created_by = Some(creator.into());
        self
    }
}

/// Log an audit entry to the database
pub async fn log_audit(pool: &PgPool, entry: &AuditLogEntry) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::new_v4();

    sqlx::query(
        "INSERT INTO audit_logs (
            id, event_type, action, resource_type, resource_id, resource_description,
            request_path, request_method, query_params,
            api_key_hash, user_email, user_id, ip_address, user_agent,
            changes, old_value, new_value,
            status_code, success, error_message,
            created_by, severity
        ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, $9,
            $10, $11, $12, $13, $14,
            $15, $16, $17,
            $18, $19, $20,
            $21, $22
        )",
    )
    .bind(id)
    .bind(entry.event_type.as_str())
    .bind(&entry.action)
    .bind(&entry.resource_type)
    .bind(&entry.resource_id)
    .bind(&entry.resource_description)
    .bind(&entry.request_path)
    .bind(&entry.request_method)
    .bind(&entry.query_params)
    .bind(&entry.api_key_hash)
    .bind(&entry.user_email)
    .bind(&entry.user_id)
    .bind(&entry.ip_address.map(|ip| ip.to_string()))
    .bind(&entry.user_agent)
    .bind(&entry.changes)
    .bind(&entry.old_value)
    .bind(&entry.new_value)
    .bind(entry.status_code)
    .bind(entry.success)
    .bind(&entry.error_message)
    .bind(&entry.created_by)
    .bind(entry.severity.as_str())
    .execute(pool)
    .await?;

    debug!(
        event_type = entry.event_type.as_str(),
        action = %entry.action,
        resource = %entry.resource_type,
        id = %id,
        "Audit log entry created"
    );

    Ok(id.to_string())
}

/// Query audit logs with optional filters
#[derive(Clone, Debug, Default)]
pub struct AuditLogQuery {
    pub event_type: Option<String>,
    pub resource_type: Option<String>,
    pub user_email: Option<String>,
    pub api_key_hash: Option<String>,
    pub severity: Option<String>,
    pub success_only: Option<bool>,
    pub from_date: Option<DateTime<Utc>>,
    pub to_date: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl AuditLogQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_event_type(mut self, event_type: impl Into<String>) -> Self {
        self.event_type = Some(event_type.into());
        self
    }

    pub fn with_resource_type(mut self, resource_type: impl Into<String>) -> Self {
        self.resource_type = Some(resource_type.into());
        self
    }

    pub fn with_user_email(mut self, email: impl Into<String>) -> Self {
        self.user_email = Some(email.into());
        self
    }

    pub fn with_severity(mut self, severity: impl Into<String>) -> Self {
        self.severity = Some(severity.into());
        self
    }

    pub fn with_date_range(mut self, from: DateTime<Utc>, to: DateTime<Utc>) -> Self {
        self.from_date = Some(from);
        self.to_date = Some(to);
        self
    }

    pub fn with_pagination(mut self, limit: i64, offset: i64) -> Self {
        self.limit = Some(limit);
        self.offset = Some(offset);
        self
    }
}

/// Query audit logs from the database
pub async fn query_audit_logs(
    pool: &PgPool,
    query: &AuditLogQuery,
) -> Result<Vec<serde_json::Value>, sqlx::Error> {
    let mut sql = "SELECT * FROM audit_logs WHERE 1=1".to_string();
    let mut params: Vec<String> = Vec::new();

    if let Some(event_type) = &query.event_type {
        sql.push_str(" AND event_type = $1");
        params.push(event_type.clone());
    }

    if let Some(resource_type) = &query.resource_type {
        let param_idx = params.len() + 1;
        sql.push_str(&format!(" AND resource_type = ${}", param_idx));
        params.push(resource_type.clone());
    }

    if let Some(user_email) = &query.user_email {
        let param_idx = params.len() + 1;
        sql.push_str(&format!(" AND user_email = ${}", param_idx));
        params.push(user_email.clone());
    }

    if let Some(severity) = &query.severity {
        let param_idx = params.len() + 1;
        sql.push_str(&format!(" AND severity = ${}", param_idx));
        params.push(severity.clone());
    }

    if query.success_only == Some(true) {
        sql.push_str(" AND success = true");
    }

    if let Some(from_date) = query.from_date {
        let param_idx = params.len() + 1;
        sql.push_str(&format!(" AND created_at >= ${}", param_idx));
        // For now, this is a simplified approach
    }

    sql.push_str(" ORDER BY created_at DESC");

    if let Some(limit) = query.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    } else {
        sql.push_str(" LIMIT 100"); // Default limit
    }

    if let Some(offset) = query.offset {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    let rows = sqlx::query_as::<_, serde_json::Value>(&sql)
        .fetch_all(pool)
        .await?;

    Ok(rows)
}

/// Convenience helper: log a simple event by action name and optional detail.
///
/// This is a lightweight wrapper around [`log_audit`] for callsites that only
/// need to record an action string without building a full [`AuditLogEntry`].
/// Failures are silently ignored (the result is only used for diagnostics) so
/// callers should not rely on the return value for control flow.
pub async fn log_event(pool: &PgPool, action: &str, detail: Option<&str>) -> Result<String, sqlx::Error> {
    let entry = AuditLogEntry::new(AuditEventType::AdminApiCall, action, "system")
        .with_resource_description(detail.unwrap_or(action));
    log_audit(pool, &entry).await
}

/// Clean up expired audit logs (run periodically)
pub async fn cleanup_expired_audit_logs(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("SELECT cleanup_expired_audit_logs()")
        .execute(pool)
        .await?;

    debug!(
        rows_deleted = result.rows_affected(),
        "Cleaned up expired audit logs"
    );

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_log_entry_builder() {
        let entry = AuditLogEntry::new(AuditEventType::Delete, "DELETE_SCHEMA", "schema")
            .with_resource_id("schema_123")
            .with_severity(AuditSeverity::Critical);

        assert_eq!(entry.action, "DELETE_SCHEMA");
        assert_eq!(entry.resource_type, "schema");
        assert_eq!(entry.resource_id, Some("schema_123".to_string()));
        assert_eq!(entry.severity, AuditSeverity::Critical);
    }

    #[test]
    fn test_severity_as_str() {
        assert_eq!(AuditSeverity::Low.as_str(), "LOW");
        assert_eq!(AuditSeverity::Critical.as_str(), "CRITICAL");
    }

    #[test]
    fn test_api_key_hashing() {
        let entry1 = AuditLogEntry::new(AuditEventType::Delete, "TEST", "test")
            .with_api_key("my_secret_key");
        let entry2 = AuditLogEntry::new(AuditEventType::Delete, "TEST", "test")
            .with_api_key("my_secret_key");

        assert_eq!(entry1.api_key_hash, entry2.api_key_hash);
    }

    #[test]
    fn test_with_client_ip_records_resolved_ip() {
        let client_ip = ClientIp::from_peer("203.0.113.7".parse().unwrap());
        let entry = AuditLogEntry::new(AuditEventType::Delete, "DELETE_SCHEMA", "schema")
            .with_client_ip(&client_ip);

        assert_eq!(entry.ip_address, Some("203.0.113.7".parse().unwrap()));
    }
}
