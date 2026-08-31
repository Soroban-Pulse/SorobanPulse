//! Data transformation applied to rows before they reach a warehouse.

use serde_json::Value;

/// Applies the default set of warehouse-safe transforms:
/// - flattens nested `topics` arrays into a JSON string for warehouses
///   that don't support native array columns in streaming inserts.
/// - normalizes timestamps to RFC3339 UTC.
/// - drops internal-only fields prefixed with `_`.
pub fn apply_default_transforms(rows: &[Value]) -> Vec<Value> {
    rows.iter().map(transform_row).collect()
}

fn transform_row(row: &Value) -> Value {
    let mut out = row.clone();
    if let Some(obj) = out.as_object_mut() {
        obj.retain(|k, _| !k.starts_with('_'));

        if let Some(topics) = obj.get("topics").cloned() {
            if topics.is_array() {
                obj.insert("topics".to_string(), Value::String(topics.to_string()));
            }
        }

        if let Some(ts) = obj.get("ingested_at").and_then(|v| v.as_str()) {
            obj.insert(
                "ingested_at".to_string(),
                Value::String(normalize_timestamp(ts)),
            );
        }
    }
    out
}

fn normalize_timestamp(raw: &str) -> String {
    if raw.ends_with('Z') || raw.contains('+') {
        raw.to_string()
    } else {
        format!("{raw}Z")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_internal_fields() {
        let row = serde_json::json!({ "event_id": "e1", "_internal": true });
        let out = apply_default_transforms(&[row]);
        assert!(out[0].get("_internal").is_none());
        assert_eq!(out[0]["event_id"], "e1");
    }

    #[test]
    fn normalizes_missing_z_suffix() {
        assert_eq!(normalize_timestamp("2024-01-01T00:00:00"), "2024-01-01T00:00:00Z");
        assert_eq!(normalize_timestamp("2024-01-01T00:00:00Z"), "2024-01-01T00:00:00Z");
    }

    #[test]
    fn flattens_topics_array() {
        let row = serde_json::json!({ "topics": ["a", "b"] });
        let out = apply_default_transforms(&[row]);
        assert!(out[0]["topics"].is_string());
    }
}
