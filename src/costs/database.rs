/// Database cost calculation.
///
/// Pricing is based on configurable per-unit rates for:
/// - Connection hours
/// - Queries executed
/// - Data transfer (GB)
/// - Storage (GB/month → prorated per hour)
/// - IOPS
use super::models::{CostEntry, DatabaseCost, ResourceType};
use chrono::Utc;
use std::collections::HashMap;

/// Pricing rates for database resources (USD).
#[derive(Debug, Clone)]
pub struct DatabaseRates {
    /// Cost per connection-hour.
    pub connection_hour: f64,
    /// Cost per 1,000 queries.
    pub per_thousand_queries: f64,
    /// Cost per GB of data transferred.
    pub data_transfer_gb: f64,
    /// Cost per GB stored per month.
    pub storage_gb_month: f64,
    /// Cost per 1,000 IOPS.
    pub iops_per_thousand: f64,
}

impl Default for DatabaseRates {
    fn default() -> Self {
        Self {
            connection_hour: 0.001,
            per_thousand_queries: 0.005,
            data_transfer_gb: 0.09,
            storage_gb_month: 0.115,
            iops_per_thousand: 0.065,
        }
    }
}

/// Snapshot of observed database usage during a measurement window.
#[derive(Debug, Clone, Default)]
pub struct DatabaseUsage {
    pub active_connections: u32,
    pub max_connections: u32,
    pub query_count: u64,
    pub data_transfer_bytes: u64,
    pub storage_bytes: u64,
    pub iops: u64,
    pub pool_idle: u32,
    pub pool_size: u32,
}

/// Calculate database cost from a usage snapshot and an elapsed window (in hours).
pub fn calculate(usage: &DatabaseUsage, rates: &DatabaseRates, hours: f64) -> DatabaseCost {
    let connection_hours = f64::from(usage.active_connections) * hours;
    let data_transfer_gb = bytes_to_gb(usage.data_transfer_bytes);
    let storage_gb = bytes_to_gb(usage.storage_bytes);
    // Storage is billed monthly; prorate to the measurement window.
    let storage_gb_prorated = storage_gb * (hours / (24.0 * 30.0));

    let connection_cost = connection_hours * rates.connection_hour;
    let query_cost = (usage.query_count as f64 / 1_000.0) * rates.per_thousand_queries;
    let transfer_cost = data_transfer_gb * rates.data_transfer_gb;
    let storage_cost = storage_gb_prorated * rates.storage_gb_month;
    let iops_cost = (usage.iops as f64 / 1_000.0) * rates.iops_per_thousand;

    let total = connection_cost + query_cost + transfer_cost + storage_cost + iops_cost;

    DatabaseCost {
        connection_hours,
        query_count: usage.query_count,
        data_transfer_gb,
        storage_gb,
        iops: usage.iops,
        total_cost: total,
    }
}

/// Wrap a `DatabaseCost` as a `CostEntry` for aggregation.
pub fn to_cost_entry(db_cost: &DatabaseCost, resource_id: &str) -> CostEntry {
    let mut metadata = HashMap::new();
    metadata.insert("connection_hours".into(), format!("{:.4}", db_cost.connection_hours));
    metadata.insert("query_count".into(), db_cost.query_count.to_string());
    metadata.insert("storage_gb".into(), format!("{:.4}", db_cost.storage_gb));
    metadata.insert("data_transfer_gb".into(), format!("{:.4}", db_cost.data_transfer_gb));

    CostEntry {
        resource_id: resource_id.to_string(),
        resource_type: ResourceType::Database,
        cost: db_cost.total_cost,
        timestamp: Utc::now(),
        metadata,
    }
}

fn bytes_to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_usage_yields_zero_cost() {
        let usage = DatabaseUsage::default();
        let result = calculate(&usage, &DatabaseRates::default(), 1.0);
        assert_eq!(result.total_cost, 0.0);
    }

    #[test]
    fn connection_cost_is_proportional_to_hours() {
        let usage = DatabaseUsage { active_connections: 10, ..Default::default() };
        let rates = DatabaseRates::default();
        let one_hour = calculate(&usage, &rates, 1.0);
        let two_hour = calculate(&usage, &rates, 2.0);
        assert!((two_hour.total_cost - 2.0 * one_hour.total_cost).abs() < 1e-10);
    }

    #[test]
    fn query_cost_accumulates() {
        let usage = DatabaseUsage { query_count: 10_000, ..Default::default() };
        let rates = DatabaseRates::default();
        let result = calculate(&usage, &rates, 1.0);
        // 10_000 / 1_000 * 0.005 = 0.05
        assert!((result.total_cost - 0.05).abs() < 1e-10);
    }
}
