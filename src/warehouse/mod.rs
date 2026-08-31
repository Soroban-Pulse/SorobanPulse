//! Data warehouse export integrations (BigQuery, Snowflake).
//!
//! This module extends the existing `parquet_export` pipeline so that
//! exported event batches can be loaded directly into external analytics
//! warehouses instead of (or in addition to) local/S3 parquet files.

pub mod bigquery;
pub mod snowflake;
pub mod schema_mapping;
pub mod incremental;
pub mod transform;

use std::fmt;

/// Supported destination warehouses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarehouseKind {
    BigQuery,
    Snowflake,
}

impl fmt::Display for WarehouseKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WarehouseKind::BigQuery => write!(f, "bigquery"),
            WarehouseKind::Snowflake => write!(f, "snowflake"),
        }
    }
}

/// Configuration shared by all warehouse exporters.
#[derive(Debug, Clone)]
pub struct WarehouseExportConfig {
    pub kind: WarehouseKind,
    pub dataset_or_schema: String,
    pub table: String,
    /// If true, only rows newer than the last watermark are exported.
    pub incremental: bool,
    /// Column used to track incremental progress (usually `ingested_at`).
    pub watermark_column: String,
    /// Max rows per batch sent to the warehouse API.
    pub batch_size: usize,
}

impl Default for WarehouseExportConfig {
    fn default() -> Self {
        Self {
            kind: WarehouseKind::BigQuery,
            dataset_or_schema: "soroban_pulse".to_string(),
            table: "events".to_string(),
            incremental: true,
            watermark_column: "ingested_at".to_string(),
            batch_size: 5_000,
        }
    }
}

/// Result of a single warehouse export run.
#[derive(Debug, Clone, Default)]
pub struct WarehouseExportResult {
    pub rows_exported: u64,
    pub batches: u32,
    pub last_watermark: Option<String>,
    pub errors: Vec<String>,
}

/// Trait implemented by each warehouse-specific client.
pub trait WarehouseExporter {
    /// Export a slice of already-serialized rows (as JSON objects) to the
    /// warehouse table described by `config`.
    fn export_rows(
        &self,
        config: &WarehouseExportConfig,
        rows: &[serde_json::Value],
    ) -> Result<WarehouseExportResult, WarehouseError>;

    /// Ensure the destination table exists and matches the expected schema,
    /// creating or altering it if necessary.
    fn ensure_schema(&self, config: &WarehouseExportConfig) -> Result<(), WarehouseError>;
}

#[derive(Debug, Clone)]
pub enum WarehouseError {
    Auth(String),
    Schema(String),
    Network(String),
    Serialization(String),
}

impl fmt::Display for WarehouseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WarehouseError::Auth(m) => write!(f, "warehouse auth error: {m}"),
            WarehouseError::Schema(m) => write!(f, "warehouse schema error: {m}"),
            WarehouseError::Network(m) => write!(f, "warehouse network error: {m}"),
            WarehouseError::Serialization(m) => write!(f, "warehouse serialization error: {m}"),
        }
    }
}

impl std::error::Error for WarehouseError {}

/// Entry point used by `parquet_export` to fan out a batch of rows to a
/// configured warehouse destination after (or instead of) writing parquet.
pub fn export_to_warehouse(
    exporter: &dyn WarehouseExporter,
    config: &WarehouseExportConfig,
    rows: &[serde_json::Value],
) -> Result<WarehouseExportResult, WarehouseError> {
    exporter.ensure_schema(config)?;

    let transformed = transform::apply_default_transforms(rows);

    if config.incremental {
        incremental::export_incremental(exporter, config, &transformed)
    } else {
        exporter.export_rows(config, &transformed)
    }
}
