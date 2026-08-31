//! Maps SorobanPulse event fields to warehouse-native column types.

#[derive(Debug, Clone)]
pub struct WarehouseColumn {
    pub name: String,
    pub bigquery_type: &'static str,
    pub snowflake_type: &'static str,
    pub nullable: bool,
}

fn col(name: &str, bq: &'static str, sf: &'static str, nullable: bool) -> WarehouseColumn {
    WarehouseColumn {
        name: name.to_string(),
        bigquery_type: bq,
        snowflake_type: sf,
        nullable,
    }
}

/// Canonical column list for the `events` table, shared by both warehouses.
pub fn event_columns() -> Vec<WarehouseColumn> {
    vec![
        col("event_id", "STRING", "VARCHAR", false),
        col("ledger_sequence", "INT64", "NUMBER", false),
        col("contract_id", "STRING", "VARCHAR", false),
        col("event_type", "STRING", "VARCHAR", false),
        col("topics", "JSON", "VARIANT", true),
        col("payload", "JSON", "VARIANT", true),
        col("ingested_at", "TIMESTAMP", "TIMESTAMP_NTZ", false),
        col("tx_hash", "STRING", "VARCHAR", false),
    ]
}

pub fn event_schema_bigquery() -> serde_json::Value {
    let fields: Vec<serde_json::Value> = event_columns()
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "name": c.name,
                "type": c.bigquery_type,
                "mode": if c.nullable { "NULLABLE" } else { "REQUIRED" },
            })
        })
        .collect();
    serde_json::json!({ "fields": fields })
}

pub fn event_schema_snowflake() -> Vec<WarehouseColumn> {
    event_columns()
}
