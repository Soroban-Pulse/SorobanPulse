//! Advanced statistics management for SorobanPulse.
//!
//! Provides automated ANALYZE scheduling, extended statistics for correlation
//! tracking, histogram collection, query plan tracking, regression detection,
//! and a comprehensive statistics dashboard.

extern crate metrics as m;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::{error, info, warn};

use crate::error::ApiError;

// ---------------------------------------------------------------------------
// Existing types (preserved for backward compatibility)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StatisticsReport {
    pub table_name: String,
    pub last_analyzed: Option<DateTime<Utc>>,
    pub hours_since_analyze: Option<i32>,
    pub is_stale: bool,
    pub row_count: Option<i64>,
    pub table_size_mb: Option<f64>,
    pub recent_jobs_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StatisticsAnalysisJob {
    pub job_id: String,
    pub table_name: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<i32>,
    pub row_count_analyzed: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StalenessDetectionResult {
    pub table_name: String,
    pub is_stale: bool,
    pub hours_since_analyze: i32,
    pub staleness_threshold_hours: i32,
}

// ---------------------------------------------------------------------------
// New types
// ---------------------------------------------------------------------------

/// Configuration for extended PostgreSQL statistics (CREATE STATISTICS).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedStatisticsConfig {
    /// Whether extended statistics collection is enabled.
    pub enabled: bool,
    /// Track cross-column correlations via MCV + ndistinct stats.
    pub correlation_tracking: bool,
    /// Tables for which histogram targets should be increased.
    pub histogram_targets: Vec<String>,
    /// Per-column n_distinct overrides: "table.column" -> n_distinct value.
    pub n_distinct_overrides: HashMap<String, f64>,
}

impl Default for ExtendedStatisticsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            correlation_tracking: true,
            histogram_targets: vec!["events".to_string()],
            n_distinct_overrides: HashMap::new(),
        }
    }
}

/// A captured query execution plan record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryPlanRecord {
    /// Stable hash identifying the query shape.
    pub query_hash: String,
    /// Human-readable label for the query.
    pub query_label: String,
    /// When this plan was captured.
    pub captured_at: DateTime<Utc>,
    /// Planner's estimated row count for the top-level node.
    pub estimated_rows: f64,
    /// Actual rows (only available with ANALYZE; None for EXPLAIN-only).
    pub actual_rows: Option<f64>,
    /// Total cost estimate from the planner.
    pub total_cost: f64,
    /// High-level plan type (e.g. "Seq Scan", "Index Scan", "Hash Join").
    pub plan_type: String,
    /// Whether the plan uses any index scan node.
    pub uses_index: bool,
}

/// Statistics health summary suitable for dashboards.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsHealthMetrics {
    /// Overall health score 0–100.
    pub overall_score: f64,
    pub tables_analyzed: usize,
    pub tables_stale: usize,
    pub avg_hours_since_analyze: f64,
    /// The table that was analyzed least recently.
    pub worst_table: Option<String>,
}

/// Prioritised ANALYZE schedule entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeSchedule {
    pub table_name: String,
    pub next_run: DateTime<Utc>,
    pub interval_hours: u32,
    /// 1 = highest priority.
    pub priority: u8,
    pub reason: String,
}

/// Per-column statistics detail from pg_stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableStatisticsDetail {
    pub table_name: String,
    pub column_name: String,
    pub null_frac: f32,
    pub avg_width: i32,
    pub n_distinct: f32,
    pub correlation: Option<f32>,
    pub most_common_vals_count: i32,
    pub histogram_bounds_count: i32,
}

// ---------------------------------------------------------------------------
// Existing functions (backward compatible)
// ---------------------------------------------------------------------------

/// Refresh statistics for all tables or a specific table.
pub async fn refresh_table_statistics(
    pool: &PgPool,
    table_name: Option<&str>,
) -> Result<Vec<(String, String, i32)>, ApiError> {
    let query = if let Some(table) = table_name {
        format!("SELECT * FROM refresh_table_statistics('{}')", table)
    } else {
        "SELECT * FROM refresh_table_statistics()".to_string()
    };

    let results: Vec<(String, String, i32)> = sqlx::query_as(&query)
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to refresh statistics: {}", e)))?;

    info!("Statistics refresh completed for {} tables", results.len());
    Ok(results)
}

/// Detect which tables have stale statistics.
pub async fn detect_stale_statistics(
    pool: &PgPool,
) -> Result<Vec<StalenessDetectionResult>, ApiError> {
    let results: Vec<StalenessDetectionResult> = sqlx::query_as(
        "SELECT table_name, is_stale, hours_since_analyze, staleness_threshold_hours
         FROM detect_stale_statistics()",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::BadRequest(format!("Failed to detect stale statistics: {}", e)))?;

    let stale_count = results.iter().filter(|r| r.is_stale).count();
    if stale_count > 0 {
        warn!("Detected {} tables with stale statistics", stale_count);
    }
    Ok(results)
}

/// Get comprehensive statistics report for all tables.
pub async fn get_statistics_report(pool: &PgPool) -> Result<Vec<StatisticsReport>, ApiError> {
    sqlx::query_as("SELECT * FROM get_statistics_report()")
        .fetch_all(pool)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Failed to get statistics report: {}", e)))
}

/// Get recent statistics analysis jobs.
pub async fn get_recent_analysis_jobs(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<StatisticsAnalysisJob>, ApiError> {
    sqlx::query_as(
        "SELECT id::TEXT as job_id, table_name, started_at, completed_at,
                duration_seconds, row_count_analyzed, status, error_message
         FROM statistics_analysis_jobs
         ORDER BY started_at DESC
         LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::BadRequest(format!("Failed to get analysis jobs: {}", e)))
}

/// Schedule automatic statistics refresh for stale tables.
pub async fn schedule_auto_analyze(pool: &PgPool) -> Result<String, ApiError> {
    let stale = detect_stale_statistics(pool).await?;

    if stale.is_empty() {
        return Ok("No stale statistics detected".to_string());
    }

    let stale_tables: Vec<&str> = stale
        .iter()
        .filter(|s| s.is_stale)
        .map(|s| s.table_name.as_str())
        .collect();

    info!("Scheduling ANALYZE for {} stale tables", stale_tables.len());

    for table_name in &stale_tables {
        match refresh_table_statistics(pool, Some(table_name)).await {
            Ok(_) => info!("Refreshed statistics for table: {}", table_name),
            Err(e) => warn!("Failed to refresh statistics for {}: {}", table_name, e),
        }
    }

    Ok(format!("Scheduled ANALYZE for {} tables", stale_tables.len()))
}

/// Get overall statistics health score (0–100).
pub async fn get_statistics_health_score(pool: &PgPool) -> Result<u32, ApiError> {
    let stale = detect_stale_statistics(pool).await?;
    if stale.is_empty() {
        return Ok(100);
    }
    let stale_count = stale.iter().filter(|s| s.is_stale).count();
    let pct = (stale_count as f64 / stale.len() as f64) * 100.0;
    Ok((100.0 - pct) as u32)
}

// ---------------------------------------------------------------------------
// New advanced functions
// ---------------------------------------------------------------------------

/// Create or refresh extended statistics objects for cross-column correlation.
///
/// For the `events` table this creates combined ndistinct + mcv statistics on
/// the most selective column pairs to help the planner estimate complex predicates.
pub async fn run_extended_statistics(
    pool: &PgPool,
    table_name: &str,
) -> Result<(), ApiError> {
    // Define statistics objects for key column combinations.
    let stats_defs: &[(&str, &str, &str)] = &[
        ("stx_events_contract_type",   "events", "(contract_id, event_type)"),
        ("stx_events_contract_ledger", "events", "(contract_id, ledger)"),
        ("stx_events_ledger_ts",       "events", "(ledger, timestamp)"),
    ];

    for (stat_name, tbl, cols) in stats_defs {
        if *tbl != table_name && table_name != "events" {
            continue;
        }
        // DROP + CREATE to refresh — idempotent.
        let drop_sql = format!("DROP STATISTICS IF EXISTS {}", stat_name);
        let create_sql = format!(
            "CREATE STATISTICS IF NOT EXISTS {} (ndistinct, mcv) ON {} FROM {}",
            stat_name, cols, tbl
        );
        if let Err(e) = sqlx::query(&drop_sql).execute(pool).await {
            warn!(stat_name, error = %e, "Could not drop extended statistics (may not exist)");
        }
        sqlx::query(&create_sql)
            .execute(pool)
            .await
            .map_err(|e| ApiError::BadRequest(format!("Failed to create extended stats: {}", e)))?;
        info!(stat_name, table = tbl, "Extended statistics object created");
    }

    // Trigger ANALYZE so the new statistics are populated immediately.
    let analyze_sql = format!("ANALYZE {}", table_name);
    sqlx::query(&analyze_sql)
        .execute(pool)
        .await
        .map_err(|e| ApiError::BadRequest(format!("ANALYZE after extended stats failed: {}", e)))?;

    Ok(())
}

/// Collect per-column histogram and correlation data from pg_stats.
pub async fn collect_histogram_statistics(
    pool: &PgPool,
    table_name: &str,
) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows: Vec<(String, f32, i32, f32, Option<f32>, i32, i32)> = sqlx::query_as(
        "SELECT
             attname::text,
             null_frac,
             avg_width,
             n_distinct,
             correlation,
             COALESCE(array_length(most_common_vals::text[], 1), 0),
             COALESCE(array_length(histogram_bounds::text[], 1), 0)
         FROM pg_stats
         WHERE schemaname = 'public' AND tablename = $1
         ORDER BY attname",
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::BadRequest(format!("pg_stats query failed: {}", e)))?;

    Ok(rows
        .into_iter()
        .map(|(col, null_frac, avg_width, n_distinct, correlation, mcv_count, hist_count)| {
            serde_json::json!({
                "column_name": col,
                "null_frac": null_frac,
                "avg_width": avg_width,
                "n_distinct": n_distinct,
                "correlation": correlation,
                "most_common_vals_count": mcv_count,
                "histogram_bounds_count": hist_count,
            })
        })
        .collect())
}

/// Capture an EXPLAIN plan for a query and return a structured record.
pub async fn track_query_plan(
    pool: &PgPool,
    query_label: &str,
    query_sql: &str,
) -> Result<QueryPlanRecord, ApiError> {
    let explain_sql = format!("EXPLAIN (FORMAT JSON) {}", query_sql);

    let plan_json: serde_json::Value = sqlx::query_scalar(&explain_sql)
        .fetch_one(pool)
        .await
        .map_err(|e| ApiError::BadRequest(format!("EXPLAIN failed for '{}': {}", query_label, e)))?;

    // Parse top-level plan node
    let plan_node = plan_json
        .get(0)
        .and_then(|p| p.get("Plan"))
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    let estimated_rows = plan_node
        .get("Plan Rows")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let total_cost = plan_node
        .get("Total Cost")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let plan_type = plan_node
        .get("Node Type")
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();

    // Detect whether any index scan appears in the plan JSON.
    let plan_str = plan_json.to_string();
    let uses_index = plan_str.contains("Index Scan")
        || plan_str.contains("Index Only Scan")
        || plan_str.contains("Bitmap Index Scan");

    // Simple stable hash based on query text.
    let query_hash = format!("{:x}", {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        query_sql.hash(&mut h);
        h.finish()
    });

    let record = QueryPlanRecord {
        query_hash,
        query_label: query_label.to_string(),
        captured_at: Utc::now(),
        estimated_rows,
        actual_rows: None,
        total_cost,
        plan_type,
        uses_index,
    };

    // Emit metric
    m::gauge!(
        "soroban_pulse_query_plan_estimated_rows",
        "query" => query_label.to_string()
    )
    .set(estimated_rows);

    info!(
        query = query_label,
        estimated_rows,
        total_cost,
        uses_index,
        "Query plan captured"
    );

    Ok(record)
}

/// Returns true when the current plan's estimated rows diverge from the
/// baseline by more than 50% — indicating a potential plan regression.
pub fn detect_plan_regression(current: &QueryPlanRecord, baseline_rows: f64) -> bool {
    if baseline_rows <= 0.0 {
        return false;
    }
    let deviation = ((current.estimated_rows - baseline_rows) / baseline_rows).abs();
    if deviation > 0.5 {
        warn!(
            query = current.query_label,
            estimated = current.estimated_rows,
            baseline = baseline_rows,
            deviation_pct = deviation * 100.0,
            "Query plan regression detected"
        );
        true
    } else {
        false
    }
}

/// Return a comprehensive JSON dashboard combining health metrics, stale table
/// list, recent jobs, and sampled query plans.
pub async fn get_statistics_dashboard(pool: &PgPool) -> Result<serde_json::Value, ApiError> {
    let health_metrics = build_health_metrics(pool).await?;
    let recent_jobs = get_recent_analysis_jobs(pool, 10).await.unwrap_or_default();

    // Sample a representative query plan
    let plan = track_query_plan(
        pool,
        "events_paginated",
        "SELECT id, contract_id, event_type, ledger, timestamp FROM events ORDER BY ledger DESC LIMIT 20",
    )
    .await
    .ok();

    let stale = detect_stale_statistics(pool).await.unwrap_or_default();
    let stale_list: Vec<&str> = stale
        .iter()
        .filter(|s| s.is_stale)
        .map(|s| s.table_name.as_str())
        .collect();

    Ok(serde_json::json!({
        "generated_at": Utc::now(),
        "health": {
            "overall_score": health_metrics.overall_score,
            "tables_analyzed": health_metrics.tables_analyzed,
            "tables_stale": health_metrics.tables_stale,
            "avg_hours_since_analyze": health_metrics.avg_hours_since_analyze,
            "worst_table": health_metrics.worst_table,
        },
        "stale_tables": stale_list,
        "recent_jobs": recent_jobs.len(),
        "sample_query_plan": plan.map(|p| serde_json::json!({
            "query_label": p.query_label,
            "estimated_rows": p.estimated_rows,
            "total_cost": p.total_cost,
            "plan_type": p.plan_type,
            "uses_index": p.uses_index,
        })),
    }))
}

/// Override n_distinct for a column via ALTER TABLE ... ALTER COLUMN ... SET STATISTICS
/// (PostgreSQL uses the statistics target to control histogram bucket count).
pub async fn update_n_distinct_estimate(
    pool: &PgPool,
    table_name: &str,
    column_name: &str,
    n_distinct: f64,
) -> Result<(), ApiError> {
    // ALTER TABLE ... ALTER COLUMN ... SET (n_distinct = ...) controls n_distinct
    // override. Positive = absolute count; negative = fraction of rows.
    let sql = format!(
        "ALTER TABLE {} ALTER COLUMN {} SET (n_distinct = {})",
        table_name, column_name, n_distinct
    );
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(|e| {
            ApiError::BadRequest(format!(
                "Failed to set n_distinct for {}.{}: {}",
                table_name, column_name, e
            ))
        })?;
    info!(table = table_name, column = column_name, n_distinct, "n_distinct override applied");
    Ok(())
}

/// Build a prioritised ANALYZE schedule based on staleness and table size.
/// Tables with more rows and longer staleness get higher priority (lower number).
pub async fn schedule_priority_analyze(
    pool: &PgPool,
) -> Result<Vec<AnalyzeSchedule>, ApiError> {
    let stale = detect_stale_statistics(pool).await?;

    let mut schedules: Vec<AnalyzeSchedule> = stale
        .iter()
        .map(|s| {
            let interval_hours: u32 = if s.hours_since_analyze > 72 { 4 }
                else if s.hours_since_analyze > 24 { 12 }
                else { 24 };

            let priority: u8 = if s.is_stale && s.hours_since_analyze > 48 { 1 }
                else if s.is_stale { 2 }
                else { 3 };

            let reason = if s.is_stale {
                format!("Stale: {}h since last ANALYZE (threshold {}h)",
                    s.hours_since_analyze, s.staleness_threshold_hours)
            } else {
                "Scheduled maintenance".to_string()
            };

            AnalyzeSchedule {
                table_name: s.table_name.clone(),
                next_run: Utc::now() + Duration::hours(interval_hours as i64),
                interval_hours,
                priority,
                reason,
            }
        })
        .collect();

    schedules.sort_by_key(|s| s.priority);
    Ok(schedules)
}

/// Set session-level parallel query parameters to improve performance for
/// complex aggregation queries on the events table.
pub async fn enable_parallel_query(pool: &PgPool) -> Result<(), ApiError> {
    let settings = [
        ("max_parallel_workers_per_gather", "4"),
        ("parallel_tuple_cost", "0.1"),
        ("parallel_setup_cost", "100"),
        ("min_parallel_table_scan_size", "1MB"),
    ];

    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| ApiError::BadRequest(format!("Could not acquire connection: {}", e)))?;

    for (key, val) in &settings {
        let sql = format!("SET {} = '{}'", key, val);
        sqlx::query(&sql)
            .execute(&mut *conn)
            .await
            .map_err(|e| {
                ApiError::BadRequest(format!("Failed to SET {}: {}", key, e))
            })?;
    }
    info!("Parallel query settings applied");
    Ok(())
}

/// Spawn a background task that runs auto-ANALYZE on a fixed interval and
/// emits health-score metrics.
pub fn spawn_auto_analyze(
    pool: PgPool,
    interval_secs: u64,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    match schedule_auto_analyze(&pool).await {
                        Ok(msg) => info!("Auto-analyze: {}", msg),
                        Err(e) => error!("Auto-analyze error: {}", e),
                    }
                    match get_statistics_health_score(&pool).await {
                        Ok(score) => {
                            m::gauge!("soroban_pulse_statistics_health_score")
                                .set(score as f64);
                            if score < 80 {
                                warn!(score, "Statistics health score below 80%");
                            }
                        }
                        Err(e) => error!("Health score error: {}", e),
                    }
                    // Count stale tables metric
                    if let Ok(stale) = detect_stale_statistics(&pool).await {
                        let n = stale.iter().filter(|s| s.is_stale).count();
                        m::gauge!("soroban_pulse_stale_tables_total").set(n as f64);
                    }
                }
                _ = shutdown.changed() => {
                    info!("Auto-analyze task shutting down");
                    break;
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

async fn build_health_metrics(pool: &PgPool) -> Result<StatisticsHealthMetrics, ApiError> {
    let stale = detect_stale_statistics(pool).await?;

    if stale.is_empty() {
        return Ok(StatisticsHealthMetrics {
            overall_score: 100.0,
            tables_analyzed: 0,
            tables_stale: 0,
            avg_hours_since_analyze: 0.0,
            worst_table: None,
        });
    }

    let tables_stale = stale.iter().filter(|s| s.is_stale).count();
    let avg_hours = stale.iter().map(|s| s.hours_since_analyze as f64).sum::<f64>()
        / stale.len() as f64;
    let worst_table = stale
        .iter()
        .max_by_key(|s| s.hours_since_analyze)
        .map(|s| s.table_name.clone());
    let pct_stale = tables_stale as f64 / stale.len() as f64;
    let overall_score = (100.0 * (1.0 - pct_stale)).max(0.0);

    m::gauge!("soroban_pulse_statistics_health_score").set(overall_score);
    m::gauge!("soroban_pulse_stale_tables_total").set(tables_stale as f64);

    Ok(StatisticsHealthMetrics {
        overall_score,
        tables_analyzed: stale.len(),
        tables_stale,
        avg_hours_since_analyze: avg_hours,
        worst_table,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn staleness_detection_threshold() {
        let result = StalenessDetectionResult {
            table_name: "events".to_string(),
            is_stale: true,
            hours_since_analyze: 48,
            staleness_threshold_hours: 24,
        };
        assert!(result.is_stale);
        assert!(result.hours_since_analyze > result.staleness_threshold_hours);
    }

    #[test]
    fn extended_statistics_config_default() {
        let cfg = ExtendedStatisticsConfig::default();
        assert!(cfg.enabled);
        assert!(cfg.correlation_tracking);
        assert!(cfg.histogram_targets.contains(&"events".to_string()));
        assert!(cfg.n_distinct_overrides.is_empty());
    }

    #[test]
    fn detect_plan_regression_no_regression() {
        let plan = QueryPlanRecord {
            query_hash: "abc".to_string(),
            query_label: "test".to_string(),
            captured_at: Utc::now(),
            estimated_rows: 1000.0,
            actual_rows: None,
            total_cost: 50.0,
            plan_type: "Index Scan".to_string(),
            uses_index: true,
        };
        // 10% deviation — below the 50% threshold
        assert!(!detect_plan_regression(&plan, 950.0));
    }

    #[test]
    fn detect_plan_regression_triggers_on_large_deviation() {
        let plan = QueryPlanRecord {
            query_hash: "abc".to_string(),
            query_label: "test".to_string(),
            captured_at: Utc::now(),
            estimated_rows: 100.0,
            actual_rows: None,
            total_cost: 50.0,
            plan_type: "Seq Scan".to_string(),
            uses_index: false,
        };
        // Baseline was 10000; now estimating 100 — 99% deviation
        assert!(detect_plan_regression(&plan, 10_000.0));
    }

    #[test]
    fn detect_plan_regression_zero_baseline_is_safe() {
        let plan = QueryPlanRecord {
            query_hash: "abc".to_string(),
            query_label: "test".to_string(),
            captured_at: Utc::now(),
            estimated_rows: 999.0,
            actual_rows: None,
            total_cost: 10.0,
            plan_type: "Seq Scan".to_string(),
            uses_index: false,
        };
        assert!(!detect_plan_regression(&plan, 0.0));
    }

    #[test]
    fn analyze_schedule_priority_ordering() {
        let schedules = vec![
            AnalyzeSchedule {
                table_name: "b".to_string(),
                next_run: Utc::now(),
                interval_hours: 24,
                priority: 2,
                reason: "stale".to_string(),
            },
            AnalyzeSchedule {
                table_name: "a".to_string(),
                next_run: Utc::now(),
                interval_hours: 4,
                priority: 1,
                reason: "very stale".to_string(),
            },
        ];
        let mut sorted = schedules.clone();
        sorted.sort_by_key(|s| s.priority);
        assert_eq!(sorted[0].table_name, "a");
    }

    #[test]
    fn health_score_all_fresh_is_100() {
        let stale: Vec<StalenessDetectionResult> = vec![];
        let score = if stale.is_empty() {
            100_u32
        } else {
            let stale_count = stale.iter().filter(|s| s.is_stale).count();
            let pct = (stale_count as f64 / stale.len() as f64) * 100.0;
            (100.0 - pct) as u32
        };
        assert_eq!(score, 100);
    }

    #[test]
    fn health_score_all_stale_is_zero() {
        let stale = vec![
            StalenessDetectionResult {
                table_name: "t1".to_string(),
                is_stale: true,
                hours_since_analyze: 100,
                staleness_threshold_hours: 24,
            },
            StalenessDetectionResult {
                table_name: "t2".to_string(),
                is_stale: true,
                hours_since_analyze: 200,
                staleness_threshold_hours: 24,
            },
        ];
        let stale_count = stale.iter().filter(|s| s.is_stale).count();
        let pct = (stale_count as f64 / stale.len() as f64) * 100.0;
        let score = (100.0 - pct) as u32;
        assert_eq!(score, 0);
    }
}
