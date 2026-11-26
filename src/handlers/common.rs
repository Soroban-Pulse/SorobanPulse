//! Helpers shared by more than one handler domain (row projection, validation, caching).

#[allow(unused_imports)]
use super::*;

/// Compute a lightweight ETag from the last event id, created_at, and total count.
/// Uses SHA-256 truncated to 8 bytes, base64-encoded — no double-serialization needed.
pub(super) const MAX_DATA_PATTERN_LEN: usize = 256;
pub(super) const MAX_REGEX_PATTERN_LEN: usize = 256;

pub(super) fn validate_jsonpath_expr(expr: &str) -> Result<(), AppError> {
    if expr.is_empty() || expr.len() > MAX_DATA_PATTERN_LEN {
        return Err(AppError::Validation(
            "data_pattern expression is invalid or too long".to_string(),
        ));
    }

    let path_re = Regex::new(r"^\$(?:\.[A-Za-z_][A-Za-z0-9_]*|\['[A-Za-z0-9_]+\'])*$")
        .map_err(|e| AppError::Internal(format!("invalid JSONPath validator regex: {e}")))?;
    if !path_re.is_match(expr) {
        return Err(AppError::Validation(
            "data_pattern must be a simple JSONPath expression like $.amount".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_regex_pattern(pattern: &str) -> Result<(), AppError> {
    if pattern.is_empty() || pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(AppError::Validation(
            "pattern is invalid or too complex".to_string(),
        ));
    }
    Regex::new(pattern).map_err(|_| AppError::Validation("pattern is not a valid regular expression".to_string()))?;
    Ok(())
}

/// Fallback for a `Response::builder()` that failed (only possible with an
/// invalid header value, e.g. one derived from user input): log and return 500
/// instead of panicking (issue #1246).
pub(super) fn response_build_error(e: axum::http::Error) -> Response<Body> {
    tracing::error!(error = %e, "Failed to build HTTP response");
    StatusCode::INTERNAL_SERVER_ERROR.into_response()
}

/// Simple in-process cache entry for the contracts list.
pub(super) struct CacheEntry {
    pub(super) data: Value,
    pub(super) expires_at: std::time::Instant,
}

pub(super) fn rows_to_json(
    rows: &[sqlx::postgres::PgRow],
    columns: &[&str],
    enc_key: Option<&[u8; 32]>,
    enc_key_old: Option<&[u8; 32]>,
    compact: bool,
) -> Result<Vec<Value>, AppError> {
    rows_to_json_with_format(
        rows,
        columns,
        enc_key,
        enc_key_old,
        compact,
        crate::scval_format::ScValFormat::Json,
    )
}

/// Like `rows_to_json`, but renders `event_data` (Issue #1064: `value` and
/// `topic` ScVals) in the requested `format` (`native`, `json` or `xdr`).
pub(super) fn rows_to_json_with_format(
    rows: &[sqlx::postgres::PgRow],
    columns: &[&str],
    enc_key: Option<&[u8; 32]>,
    enc_key_old: Option<&[u8; 32]>,
    compact: bool,
    format: crate::scval_format::ScValFormat,
) -> Result<Vec<Value>, AppError> {
    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let mut event = serde_json::Map::new();
        for &col in columns {
            match col {
                "id" => {
                    event.insert(col.to_string(), json!(row.try_get::<Uuid, _>(col)?));
                }
                "contract_id" => {
                    event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?));
                }
                "event_type" => {
                    event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?));
                }
                "tx_hash" => {
                    event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?));
                }
                "ledger" => {
                    event.insert(col.to_string(), json!(row.try_get::<i64, _>(col)?));
                }
                "timestamp" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<DateTime<Utc>, _>(col)?),
                    );
                }
                "event_data" => {
                    let raw: Value = row.try_get::<Value, _>(col)?;
                    let decrypted = decrypt_event_data(&raw, enc_key, enc_key_old);
                    let rendered = crate::scval_format::render_event_data(&decrypted, format);
                    if compact {
                        event.insert(col.to_string(), compact_event_data(&rendered)?);
                    } else {
                        event.insert(col.to_string(), rendered);
                    }
                }
                "event_data_normalized" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<Option<Value>, _>(col)?),
                    );
                }
                "event_data_decoded" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<Option<Value>, _>(col)?),
                    );
                }
                "ledger_hash" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<Option<String>, _>(col)?),
                    );
                }
                "in_successful_call" => {
                    event.insert(col.to_string(), json!(row.try_get::<bool, _>(col)?));
                }
                "created_at" => {
                    event.insert(
                        col.to_string(),
                        json!(row.try_get::<DateTime<Utc>, _>(col)?),
                    );
                }
                "schema_version" => {
                    event.insert(col.to_string(), json!(row.try_get::<i32, _>(col)?));
                }
                "anonymized" => {
                    event.insert(col.to_string(), json!(row.try_get::<bool, _>(col)?));
                }
                "relevance_score" => {
                    event.insert(col.to_string(), json!(row.try_get::<f64, _>(col)?));
                }
                _ => {}
            }
        }
        events.push(Value::Object(event));
    }
    Ok(events)
}

/// Validate a `contract_id` query/path parameter.
///
/// `pub` (not `pub(crate)`) because `fuzz/fuzz_targets/fuzz_validate_contract_id.rs`
/// links this crate as an external dependency.
pub fn validate_contract_id(contract_id: &str) -> Result<(), AppError> {
    if contract_id.len() != 56 {
        return Err(AppError::Validation(
            "invalid contract_id format".to_string(),
        ));
    }
    if !contract_id.starts_with('C') {
        return Err(AppError::Validation(
            "invalid contract_id format".to_string(),
        ));
    }
    if !contract_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "invalid contract_id format".to_string(),
        ));
    }
    Ok(())
}
