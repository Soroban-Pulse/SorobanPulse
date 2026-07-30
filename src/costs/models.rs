use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEntry {
    pub resource_id: String,
    pub resource_type: ResourceType,
    pub cost: f64,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Database,
    Compute,
    Storage,
    Network,
    Memory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseCost {
    pub connection_hours: f64,
    pub query_count: u64,
    pub data_transfer_gb: f64,
    pub storage_gb: f64,
    pub iops: u64,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComputeCost {
    pub cpu_hours: f64,
    pub vcpu_count: u32,
    pub total_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostBreakdown {
    pub timestamp: DateTime<Utc>,
    pub database: f64,
    pub compute: f64,
    pub storage: f64,
    pub network: f64,
    pub total: f64,
    pub details: HashMap<String, CostEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostReport {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub breakdown: CostBreakdown,
    pub by_resource_type: HashMap<ResourceType, f64>,
    pub top_resources: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostForecast {
    pub forecast_date: DateTime<Utc>,
    pub predicted_daily_cost: f64,
    pub predicted_monthly_cost: f64,
    pub confidence_interval: (f64, f64),
    pub trend: CostTrend,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostTrend {
    Increasing,
    Stable,
    Decreasing,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostMetrics {
    pub total_cost: f64,
    pub cost_per_event: f64,
    pub cost_per_request: f64,
    pub hourly_rate: f64,
    pub daily_average: f64,
}
