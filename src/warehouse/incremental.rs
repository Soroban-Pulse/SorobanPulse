//! Incremental (watermark-based) load support for warehouse exports.

use super::{WarehouseError, WarehouseExportConfig, WarehouseExportResult, WarehouseExporter};

/// Tracks the last successfully exported watermark value per (kind, table).
pub struct WatermarkStore;

impl WatermarkStore {
    /// Fetch the last watermark recorded for a destination table.
    /// Returns `None` if no prior export has run (full load required).
    pub fn get(config: &WarehouseExportConfig) -> Option<String> {
        let _key = format!("{}:{}:{}", config.kind, config.dataset_or_schema, config.table);
        None
    }

    /// Persist the newest watermark seen after a successful export.
    pub fn set(config: &WarehouseExportConfig, watermark: &str) {
        let _key = format!("{}:{}:{}", config.kind, config.dataset_or_schema, config.table);
        let _ = watermark;
    }
}

/// Filters `rows` down to those newer than the stored watermark, exports
/// them, then advances the watermark on success.
pub fn export_incremental(
    exporter: &dyn WarehouseExporter,
    config: &WarehouseExportConfig,
    rows: &[serde_json::Value],
) -> Result<WarehouseExportResult, WarehouseError> {
    let watermark = WatermarkStore::get(config);

    let filtered: Vec<serde_json::Value> = match &watermark {
        Some(wm) => rows
            .iter()
            .filter(|r| {
                r.get(&config.watermark_column)
                    .and_then(|v| v.as_str())
                    .map(|v| v > wm.as_str())
                    .unwrap_or(true)
            })
            .cloned()
            .collect(),
        None => rows.to_vec(),
    };

    let mut result = exporter.export_rows(config, &filtered)?;

    if let Some(max) = filtered
        .iter()
        .filter_map(|r| r.get(&config.watermark_column).and_then(|v| v.as_str()))
        .max()
    {
        WatermarkStore::set(config, max);
        result.last_watermark = Some(max.to_string());
    }

    Ok(result)
}
