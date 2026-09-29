pub struct ContractExistsResponse {
    pub contract_id: String,
    /// true if the contract has indexed events, false otherwise
    pub exists: bool,
    /// "bloom_filter" or "database_query" indicating how the check was performed
    pub method: String,
}

// ── Issue #631: Dynamic config reloading ────────────────────────────────────

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ConfigResponse {
    pub log_level: String,
    pub rate_limit_per_minute: u32,
    pub sse_keepalive_secs: u64,
    pub health_check_timeout_ms: u64,
    pub db_max_connections: u32,
    pub indexer_lag_warn_threshold: u64,
    pub slow_query_threshold_ms: u64,
    pub features: std::collections::HashMap<String, bool>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ConfigReloadResponse {
    pub success: bool,
    pub message: String,
    pub updated_fields: Vec<String>,
}

// ── Issue #692: Connection pool configuration ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PoolConfigRequest {
    /// Maximum number of connections in the pool
    pub max_connections: Option<u32>,
    /// Minimum number of idle connections to maintain
    pub min_connections: Option<u32>,
    /// Connection timeout in seconds
    pub connection_timeout_secs: Option<u64>,
    /// Idle timeout in seconds
    pub idle_timeout_secs: Option<u64>,
    /// Exhaustion threshold (0.0-1.0) for alerts
    pub exhaustion_threshold: Option<f64>,
    /// Sample interval in seconds for pool monitoring
    pub sample_interval_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PoolConfig {
    pub max_connections: u32,
    pub min_connections: u32,
    pub connection_timeout_secs: u64,
    pub idle_timeout_secs: u64,
    pub exhaustion_threshold: f64,
    pub sample_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PoolStatistics {
    /// Current pool size (total connections)
    pub pool_size: u32,
    /// Number of idle connections
    pub idle_connections: u32,
    /// Number of active connections
    pub active_connections: u32,
    /// Maximum allowed connections
    pub max_connections: u32,
    /// Current utilization (0.0-1.0)
    pub utilization: f64,
    /// Peak utilization since startup
    pub peak_utilization: f64,
    /// Number of exhaustion events (≥90% utilization)
    pub exhaustion_events: u64,
    /// Average connection acquire latency in milliseconds
    pub avg_acquire_latency_ms: f64,
    /// Timestamp of this snapshot
    pub snapshot_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PoolConfigResponse {
    pub config: PoolConfig,
    pub success: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PoolTuningGuide {
    pub current_config: PoolConfig,
    pub recommendations: Vec<String>,
    pub reasoning: String,
}

// ── Issue #693: Database statistics auto-analysis ──────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatisticsReportItem {
    pub table_name: String,
    pub last_analyzed: Option<DateTime<Utc>>,
    pub hours_since_analyze: Option<i32>,
    pub is_stale: bool,
    pub row_count: Option<i64>,
    pub table_size_mb: Option<f64>,
    pub recent_jobs_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StatisticsAnalysisJobInfo {
    pub job_id: String,
    pub table_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,
    pub row_count_analyzed: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct StalenessDetection {
    pub table_name: String,
    pub is_stale: bool,
    pub hours_since_analyze: i32,
    pub staleness_threshold_hours: i32,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct StatisticsHealth {
    pub health_score: u32,
    pub status: String,
    pub stale_tables_count: u32,
    pub total_tables: u32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AutoAnalyzeScheduleResponse {
    pub message: String,
    pub tables_scheduled: u32,
    pub timestamp: DateTime<Utc>,
}
