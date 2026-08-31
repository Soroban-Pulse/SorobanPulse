//! Webhook request/response logging — Issue #937
//!
//! Stores a bounded, sensitive-data-masked record of every webhook delivery
//! attempt (request and response) for debugging, alongside retention,
//! access-audit, search/filter, and export helpers. See
//! `docs/webhook-logging.md` for the operator-facing reference.

use serde_json::Value;
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

/// Maximum bytes of a request/response body retained per log entry. Larger
/// bodies are truncated and flagged via `request_truncated`/
/// `response_truncated` rather than rejected outright.
pub const MAX_LOGGED_BODY_BYTES: usize = 16 * 1024;

/// Default retention window for `webhook_logs` rows, in days.
pub const DEFAULT_RETENTION_DAYS: i32 = 30;

/// Field names (case-insensitive) whose values are replaced with `"***"`
/// before storage, wherever they appear in a request/response JSON body or
/// header map — covers the common secret/credential shapes used across
/// webhook payloads and headers.
const SENSITIVE_FIELD_NAMES: &[&str] = &[
    "secret",
    "password",
    "token",
    "api_key",
    "apikey",
    "authorization",
    "x-signature-256",
    "x-api-key",
    "private_key",
    "access_token",
    "refresh_token",
    "client_secret",
];

fn is_sensitive_field(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_FIELD_NAMES.iter().any(|f| lower.contains(f))
}

/// Recursively masks sensitive fields in a JSON value (Issue #937: "Add
/// sensitive data masking in logs").
pub fn mask_sensitive(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let masked = map
                .iter()
                .map(|(k, v)| {
                    let masked_v = if is_sensitive_field(k) {
                        Value::String("***".to_string())
                    } else {
                        mask_sensitive(v)
                    };
                    (k.clone(), masked_v)
                })
                .collect();
            Value::Object(masked)
        }
        Value::Array(items) => Value::Array(items.iter().map(mask_sensitive).collect()),
        other => other.clone(),
    }
}

/// Masks a flat header map (name/value pairs) using the same sensitive
/// field list.
pub fn mask_headers(headers: &[(String, String)]) -> Value {
    let obj: serde_json::Map<String, Value> = headers
        .iter()
        .map(|(k, v)| {
            let value = if is_sensitive_field(k) {
                Value::String("***".to_string())
            } else {
                Value::String(v.clone())
            };
            (k.clone(), value)
        })
        .collect();
    Value::Object(obj)
}

/// Truncates `value`'s serialized form to `MAX_LOGGED_BODY_BYTES`, returning
/// `(stored_value, was_truncated)`. Truncation replaces the body with a
/// `{"_truncated": true, "original_size_bytes": N}` marker rather than
/// storing a possibly-invalid partial JSON fragment.
fn bound_body(value: &Value) -> (Value, bool) {
    let serialized = value.to_string();
    if serialized.len() <= MAX_LOGGED_BODY_BYTES {
        return (value.clone(), false);
    }
    (
        serde_json::json!({
            "_truncated": true,
            "original_size_bytes": serialized.len(),
            "preview": serialized.chars().take(512).collect::<String>(),
        }),
        true,
    )
}

/// A single logged webhook request/response exchange.
pub struct WebhookLogEntry<'a> {
    pub url: &'a str,
    pub request_headers: &'a [(String, String)],
    pub request_body: &'a Value,
    pub response_status: Option<i32>,
    pub response_body: Option<&'a Value>,
    pub duration_ms: i64,
    pub contract_id: Option<&'a str>,
    pub event_type: Option<&'a str>,
}

/// Records one webhook exchange, applying masking and size bounding before
/// storage (Issue #937: "Add request/response storage with size limits").
pub async fn log_exchange(pool: &PgPool, entry: WebhookLogEntry<'_>) -> Result<Uuid, sqlx::Error> {
    let masked_headers = mask_headers(entry.request_headers);
    let masked_request = mask_sensitive(entry.request_body);
    let (bounded_request, request_truncated) = bound_body(&masked_request);

    let (bounded_response, response_truncated) = match entry.response_body {
        Some(body) => {
            let masked = mask_sensitive(body);
            let (bounded, truncated) = bound_body(&masked);
            (Some(bounded), truncated)
        }
        None => (None, false),
    };

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO webhook_logs \
         (id, url, request_headers, request_body, request_truncated, \
          response_status, response_body, response_truncated, duration_ms, \
          contract_id, event_type) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(id)
    .bind(entry.url)
    .bind(masked_headers)
    .bind(bounded_request)
    .bind(request_truncated)
    .bind(entry.response_status)
    .bind(bounded_response)
    .bind(response_truncated)
    .bind(entry.duration_ms)
    .bind(entry.contract_id)
    .bind(entry.event_type)
    .execute(pool)
    .await?;

    Ok(id)
}

/// Deletes `webhook_logs` rows older than `retention_days` (Issue #937:
/// "Create log retention policy"). Returns the number of rows removed.
/// Intended to be run on a periodic scheduler alongside the project's
/// other archival jobs (see `src/archiver.rs`).
pub async fn purge_expired(pool: &PgPool, retention_days: i32) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "DELETE FROM webhook_logs WHERE created_at < NOW() - ($1 || ' days')::interval",
    )
    .bind(retention_days.to_string())
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Filter parameters for [`search`] / [`export`].
#[derive(Default, Debug, Clone)]
pub struct WebhookLogFilter {
    pub url: Option<String>,
    pub contract_id: Option<String>,
    pub response_status: Option<i32>,
    pub since: Option<chrono::DateTime<chrono::Utc>>,
    pub until: Option<chrono::DateTime<chrono::Utc>>,
    pub limit: i64,
    pub offset: i64,
}

impl WebhookLogFilter {
    pub fn new() -> Self {
        Self {
            limit: 100,
            offset: 0,
            ..Default::default()
        }
    }

    fn summary(&self) -> String {
        format!(
            "url={:?} contract_id={:?} status={:?} since={:?} until={:?} limit={} offset={}",
            self.url, self.contract_id, self.response_status, self.since, self.until,
            self.limit, self.offset
        )
    }
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct WebhookLogRow {
    pub id: Uuid,
    pub url: String,
    pub request_headers: Option<Value>,
    pub request_body: Option<Value>,
    pub request_truncated: bool,
    pub response_status: Option<i32>,
    pub response_body: Option<Value>,
    pub response_truncated: bool,
    pub duration_ms: Option<i64>,
    pub contract_id: Option<String>,
    pub event_type: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Roles permitted to read webhook logs. Bodies may contain masked but
/// otherwise sensitive customer payload data, so access is restricted to
/// operator-facing roles (Issue #937: "Implement log access controls").
pub fn is_authorized_to_read_logs(role: &str) -> bool {
    matches!(role, "admin" | "operator" | "support")
}

/// Searches `webhook_logs` with the given filter, recording an audit-trail
/// row for who searched and with what filter (Issue #937: "Implement log
/// access controls and audit", "Create log search and filtering").
///
/// Returns `Err` with a message if `accessor_role` is not authorized;
/// otherwise runs the query and always writes an audit row, even when the
/// result set is empty.
pub async fn search(
    pool: &PgPool,
    accessor: &str,
    accessor_role: &str,
    filter: &WebhookLogFilter,
) -> Result<Vec<WebhookLogRow>, String> {
    if !is_authorized_to_read_logs(accessor_role) {
        return Err(format!("role '{accessor_role}' is not authorized to read webhook logs"));
    }

    let rows = sqlx::query_as::<_, WebhookLogRow>(
        "SELECT id, url, request_headers, request_body, request_truncated, \
                response_status, response_body, response_truncated, duration_ms, \
                contract_id, event_type, created_at \
         FROM webhook_logs \
         WHERE ($1::text IS NULL OR url = $1) \
           AND ($2::text IS NULL OR contract_id = $2) \
           AND ($3::int IS NULL OR response_status = $3) \
           AND ($4::timestamptz IS NULL OR created_at >= $4) \
           AND ($5::timestamptz IS NULL OR created_at <= $5) \
         ORDER BY created_at DESC \
         LIMIT $6 OFFSET $7",
    )
    .bind(&filter.url)
    .bind(&filter.contract_id)
    .bind(filter.response_status)
    .bind(filter.since)
    .bind(filter.until)
    .bind(filter.limit)
    .bind(filter.offset)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("webhook log search failed: {e}"))?;

    if let Err(e) = sqlx::query(
        "INSERT INTO webhook_log_access_audit (accessor, action, filter_summary, result_count) \
         VALUES ($1, 'search', $2, $3)",
    )
    .bind(accessor)
    .bind(filter.summary())
    .bind(rows.len() as i32)
    .execute(pool)
    .await
    {
        warn!(error = %e, "Failed to record webhook log access audit entry");
    }

    Ok(rows)
}

/// Exports matching `webhook_logs` rows as newline-delimited JSON (Issue
/// #937: "Implement log export capability"). Subject to the same
/// authorization and audit trail as [`search`].
pub async fn export_ndjson(
    pool: &PgPool,
    accessor: &str,
    accessor_role: &str,
    filter: &WebhookLogFilter,
) -> Result<String, String> {
    let rows = search(pool, accessor, accessor_role, filter).await?;

    if let Err(e) = sqlx::query(
        "INSERT INTO webhook_log_access_audit (accessor, action, filter_summary, result_count) \
         VALUES ($1, 'export', $2, $3)",
    )
    .bind(accessor)
    .bind(filter.summary())
    .bind(rows.len() as i32)
    .execute(pool)
    .await
    {
        warn!(error = %e, "Failed to record webhook log export audit entry");
    }

    let mut out = String::new();
    for row in &rows {
        match serde_json::to_string(row) {
            Ok(line) => {
                out.push_str(&line);
                out.push('\n');
            }
            Err(e) => warn!(error = %e, "Failed to serialize webhook log row for export"),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn masks_sensitive_top_level_fields() {
        let input = json!({"secret": "s3cr3t", "event": "ping"});
        let masked = mask_sensitive(&input);
        assert_eq!(masked["secret"], json!("***"));
        assert_eq!(masked["event"], json!("ping"));
    }

    #[test]
    fn masks_sensitive_nested_fields() {
        let input = json!({"auth": {"api_key": "abc123"}, "ok": true});
        let masked = mask_sensitive(&input);
        assert_eq!(masked["auth"]["api_key"], json!("***"));
        assert_eq!(masked["ok"], json!(true));
    }

    #[test]
    fn masks_sensitive_headers() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer xyz".to_string()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        let masked = mask_headers(&headers);
        assert_eq!(masked["Authorization"], json!("***"));
        assert_eq!(masked["Content-Type"], json!("application/json"));
    }

    #[test]
    fn small_body_is_not_truncated() {
        let value = json!({"a": 1});
        let (stored, truncated) = bound_body(&value);
        assert!(!truncated);
        assert_eq!(stored, value);
    }

    #[test]
    fn oversized_body_is_truncated() {
        let big_string = "x".repeat(MAX_LOGGED_BODY_BYTES + 1);
        let value = json!({"data": big_string});
        let (stored, truncated) = bound_body(&value);
        assert!(truncated);
        assert_eq!(stored["_truncated"], json!(true));
    }

    #[test]
    fn access_control_allows_only_known_roles() {
        assert!(is_authorized_to_read_logs("admin"));
        assert!(is_authorized_to_read_logs("operator"));
        assert!(is_authorized_to_read_logs("support"));
        assert!(!is_authorized_to_read_logs("anonymous"));
        assert!(!is_authorized_to_read_logs(""));
    }
}
