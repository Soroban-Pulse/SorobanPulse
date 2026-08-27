//! Dynamic partition management for the partitioned `events` table.
//!
//! The events table uses monthly RANGE partitioning on `timestamp`.
//! This module provides:
//! - Automatic creation of future partitions
//! - Pruning effectiveness analysis
//! - Hot/cold partition identification
//! - Archival and consolidation
//! - Capacity forecasting
//! - Prometheus metrics

extern crate metrics as m;

use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tokio::sync::watch;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Metadata about a single monthly partition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub table_name: String,
    pub start_date: DateTime<Utc>,
    pub end_date: DateTime<Utc>,
    pub row_count: i64,
    pub size_bytes: i64,
    /// True if this partition has had recent (last 30 days) access.
    pub is_hot: bool,
    pub is_archived: bool,
    pub last_accessed: Option<DateTime<Utc>>,
}

/// Per-partition access statistics from pg_stat_user_tables.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionStats {
    pub partition_name: String,
    pub seq_scan: i64,
    pub idx_scan: i64,
    pub n_live_tup: i64,
    pub n_dead_tup: i64,
    pub last_vacuum: Option<DateTime<Utc>>,
    pub last_analyze: Option<DateTime<Utc>>,
}

/// Summary of partition pruning effectiveness for a time-range query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionPruningReport {
    pub total_partitions: usize,
    pub pruned_partitions: usize,
    pub accessed_partitions: usize,
    pub pruning_effectiveness: f64,
    /// Names of partitions that would be accessed.
    pub accessed_partition_names: Vec<String>,
}

/// Configuration for partition archival policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionArchivalConfig {
    /// Archive partitions older than this many months.
    pub archive_after_months: u32,
    /// Drop archived partitions older than this many months.
    pub delete_after_months: u32,
    /// Prefix for archived partition table names.
    pub archive_table_prefix: String,
    /// When true, only report what would happen — do not execute DDL.
    pub dry_run: bool,
}

/// Configuration for ledger-based partitioning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerPartitionConfig {
    /// Ledger range per partition (e.g., 1_000_000 ledgers per partition).
    pub ledger_range_per_partition: i64,
    /// Enable automatic partition creation for incoming ledgers.
    pub auto_create_partitions: bool,
    /// Maximum number of ledger partitions to keep (older ones get archived).
    pub max_partitions_before_archival: u32,
}

impl Default for LedgerPartitionConfig {
    fn default() -> Self {
        Self {
            ledger_range_per_partition: 1_000_000,
            auto_create_partitions: true,
            max_partitions_before_archival: 12,
        }
    }
}

/// Information about a ledger-based partition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerPartitionInfo {
    pub partition_name: String,
    pub start_ledger: i64,
    pub end_ledger: i64,
    pub row_count: i64,
    pub size_bytes: i64,
    pub is_archived: bool,
}

/// Statistics for ledger partitions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerPartitionStats {
    pub total_partitions: usize,
    pub active_partitions: usize,
    pub archived_partitions: usize,
    pub total_rows: i64,
    pub total_size_bytes: i64,
}

impl Default for PartitionArchivalConfig {
    fn default() -> Self {
        Self {
            archive_after_months: 12,
            delete_after_months: 24,
            archive_table_prefix: "archive_".to_string(),
            dry_run: true,
        }
    }
}

/// Storage growth forecast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapacityForecast {
    pub current_size_bytes: i64,
    pub growth_rate_bytes_per_day: f64,
    pub days_until_threshold: Option<u32>,
    pub threshold_bytes: i64,
    /// (partition_name, estimated_size_bytes) for upcoming months.
    pub forecast_partitions: Vec<(String, i64)>,
}

// ---------------------------------------------------------------------------
// Core partition functions
// ---------------------------------------------------------------------------

/// List all known event partitions with size and basic stats.
pub async fn list_partitions(pool: &PgPool) -> Result<Vec<PartitionInfo>, sqlx::Error> {
    // Query pg_inherits + pg_class to enumerate all children of the events table.
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT
             c.relname::text,
             COALESCE(s.n_live_tup, 0)::bigint,
             COALESCE(pg_total_relation_size(c.oid), 0)::bigint
         FROM pg_inherits i
         JOIN pg_class c ON c.oid = i.inhrelid
         JOIN pg_class p ON p.oid = i.inhparent
         LEFT JOIN pg_stat_user_tables s ON s.relname = c.relname
         WHERE p.relname = 'events'
         ORDER BY c.relname",
    )
    .fetch_all(pool)
    .await?;

    // Fetch last-access times from pg_stat_user_tables
    let access_rows: Vec<(String, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT relname::text, last_autovacuum
         FROM pg_stat_user_tables
         WHERE relname LIKE 'events_%'",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let access_map: std::collections::HashMap<String, Option<DateTime<Utc>>> =
        access_rows.into_iter().collect();

    let hot_threshold = Utc::now() - Duration::days(30);

    let partitions = rows
        .into_iter()
        .filter_map(|(name, row_count, size_bytes)| {
            let (start, end) = parse_partition_dates(&name)?;
            let last_accessed = access_map.get(&name).copied().flatten();
            let is_hot = last_accessed
                .map(|t| t > hot_threshold)
                .unwrap_or(false);
            // Check archive prefix
            let is_archived = name.starts_with("archive_");
            Some(PartitionInfo {
                table_name: name,
                start_date: start,
                end_date: end,
                row_count,
                size_bytes,
                is_hot,
                is_archived,
                last_accessed,
            })
        })
        .collect();

    Ok(partitions)
}

/// Create a monthly partition for the given year/month (1-indexed).
pub async fn create_partition(
    pool: &PgPool,
    year: i32,
    month: u32,
) -> Result<String, sqlx::Error> {
    let partition_name = format!("events_{:04}_{:02}", year, month);
    let start_date = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .unwrap_or_else(Utc::now);
    // Advance one month for the end boundary
    let (end_year, end_month) = if month == 12 {
        (year + 1, 1_u32)
    } else {
        (year, month + 1)
    };
    let end_date = Utc.with_ymd_and_hms(end_year, end_month, 1, 0, 0, 0)
        .single()
        .unwrap_or_else(Utc::now);

    let start_str = start_date.format("%Y-%m-%d").to_string();
    let end_str = end_date.format("%Y-%m-%d").to_string();

    // Create partition
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} PARTITION OF events \
         FOR VALUES FROM ('{}') TO ('{}')",
        partition_name, start_str, end_str
    );
    sqlx::query(&create_sql).execute(pool).await?;

    // Create indexes on the new partition
    let index_sqls = [
        format!("CREATE INDEX IF NOT EXISTS idx_{}_contract_id ON {}(contract_id)", partition_name, partition_name),
        format!("CREATE INDEX IF NOT EXISTS idx_{}_ledger ON {}(ledger)", partition_name, partition_name),
        format!("CREATE INDEX IF NOT EXISTS idx_{}_ts ON {}(timestamp DESC)", partition_name, partition_name),
    ];
    for sql in &index_sqls {
        sqlx::query(sql).execute(pool).await?;
    }

    info!(partition = partition_name, "Partition created successfully");
    m::counter!("soroban_pulse_partition_created_total").increment(1);

    Ok(partition_name)
}

/// Create partitions for the next `months_ahead` months starting from now.
pub async fn create_future_partitions(
    pool: &PgPool,
    months_ahead: u32,
) -> Result<Vec<String>, sqlx::Error> {
    let mut created = Vec::new();
    let now = Utc::now();
    let mut year = now.year();
    let mut month = now.month();

    for _ in 0..=months_ahead {
        match create_partition(pool, year, month).await {
            Ok(name) => created.push(name),
            Err(e) => warn!(year, month, error = %e, "Could not create partition"),
        }
        if month == 12 {
            year += 1;
            month = 1;
        } else {
            month += 1;
        }
    }

    info!("Created {} future partitions", created.len());
    Ok(created)
}

/// Determine which partitions would be accessed (i.e. NOT pruned) for a
/// time-range query between `from_ts` and `to_ts`.
pub async fn analyze_partition_pruning(
    pool: &PgPool,
    from_ts: DateTime<Utc>,
    to_ts: DateTime<Utc>,
) -> Result<PartitionPruningReport, sqlx::Error> {
    let partitions = list_partitions(pool).await?;
    let total = partitions.len();
    let mut accessed = Vec::new();

    for p in &partitions {
        // A partition overlaps the query range if start < to_ts AND end > from_ts
        if p.start_date < to_ts && p.end_date > from_ts {
            accessed.push(p.table_name.clone());
        }
    }

    let pruned = total.saturating_sub(accessed.len());
    let effectiveness = if total > 0 {
        pruned as f64 / total as f64
    } else {
        1.0
    };

    m::gauge!("soroban_pulse_partition_pruning_effectiveness")
        .set(effectiveness * 100.0);

    Ok(PartitionPruningReport {
        total_partitions: total,
        pruned_partitions: pruned,
        accessed_partitions: accessed.len(),
        pruning_effectiveness: effectiveness,
        accessed_partition_names: accessed,
    })
}

/// Return per-partition access statistics from pg_stat_user_tables.
pub async fn identify_hot_partitions(
    pool: &PgPool,
) -> Result<Vec<PartitionStats>, sqlx::Error> {
    let rows: Vec<(String, i64, i64, i64, i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> =
        sqlx::query_as(
            "SELECT
                 relname::text,
                 COALESCE(seq_scan, 0)::bigint,
                 COALESCE(idx_scan, 0)::bigint,
                 COALESCE(n_live_tup, 0)::bigint,
                 COALESCE(n_dead_tup, 0)::bigint,
                 last_vacuum,
                 last_analyze
             FROM pg_stat_user_tables
             WHERE relname LIKE 'events_%'
             ORDER BY (COALESCE(seq_scan, 0) + COALESCE(idx_scan, 0)) DESC",
        )
        .fetch_all(pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|(name, seq, idx, live, dead, vac, ana)| PartitionStats {
            partition_name: name,
            seq_scan: seq,
            idx_scan: idx,
            n_live_tup: live,
            n_dead_tup: dead,
            last_vacuum: vac,
            last_analyze: ana,
        })
        .collect())
}

/// Find partitions with no scan activity in the last `inactive_months` months.
pub async fn identify_cold_partitions(
    pool: &PgPool,
    inactive_months: u32,
) -> Result<Vec<PartitionInfo>, sqlx::Error> {
    let threshold = Utc::now() - Duration::days(inactive_months as i64 * 30);
    let partitions = list_partitions(pool).await?;
    Ok(partitions
        .into_iter()
        .filter(|p| {
            p.last_accessed
                .map(|t| t < threshold)
                .unwrap_or(true) // no access record → cold
                && !p.is_archived
        })
        .collect())
}

/// Archive a partition by renaming it to `archive_<original_name>`.
/// Respects `dry_run` in the config.
pub async fn archive_partition(
    pool: &PgPool,
    partition_name: &str,
    config: &PartitionArchivalConfig,
) -> Result<String, sqlx::Error> {
    let archive_name = format!("{}{}", config.archive_table_prefix, partition_name);

    if config.dry_run {
        info!(
            partition = partition_name,
            archive_name,
            "DRY RUN: would rename partition to archive"
        );
        return Ok(format!("DRY RUN: {} → {}", partition_name, archive_name));
    }

    let sql = format!("ALTER TABLE {} RENAME TO {}", partition_name, archive_name);
    sqlx::query(&sql).execute(pool).await?;

    m::counter!("soroban_pulse_archived_partitions_total").increment(1);
    info!(partition = partition_name, archive_name, "Partition archived");
    Ok(format!("Archived {} → {}", partition_name, archive_name))
}

/// Calculate row-count skew across partitions.
/// Returns (partition_name, row_count, skew_factor) where skew_factor is the
/// deviation from the mean expressed as a ratio (1.0 = average).
pub async fn calculate_partition_skew(
    pool: &PgPool,
) -> Result<Vec<(String, i64, f64)>, sqlx::Error> {
    let partitions = list_partitions(pool).await?;
    if partitions.is_empty() {
        return Ok(Vec::new());
    }

    let mean = partitions.iter().map(|p| p.row_count).sum::<i64>() as f64
        / partitions.len() as f64;

    let max_skew = partitions
        .iter()
        .map(|p| {
            if mean > 0.0 {
                (p.row_count as f64 / mean).abs()
            } else {
                0.0
            }
        })
        .fold(0.0_f64, f64::max);

    m::gauge!("soroban_pulse_partition_skew_max").set(max_skew);

    Ok(partitions
        .iter()
        .map(|p| {
            let skew = if mean > 0.0 {
                p.row_count as f64 / mean
            } else {
                0.0
            };
            (p.table_name.clone(), p.row_count, skew)
        })
        .collect())
}

/// Create a ledger-based partition for the given range.
pub async fn create_ledger_partition(
    pool: &PgPool,
    start_ledger: i64,
    end_ledger: i64,
) -> Result<String, sqlx::Error> {
    let partition_name = format!("events_ledger_{:010}_{:010}", start_ledger, end_ledger);

    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} PARTITION OF events \
         FOR VALUES FROM ({}) TO ({})",
        partition_name, start_ledger, end_ledger
    );
    sqlx::query(&create_sql).execute(pool).await?;

    let index_sqls = [
        format!("CREATE INDEX IF NOT EXISTS idx_{}_ledger ON {}(ledger)", partition_name, partition_name),
        format!("CREATE INDEX IF NOT EXISTS idx_{}_contract_id ON {}(contract_id)", partition_name, partition_name),
    ];
    for sql in &index_sqls {
        sqlx::query(sql).execute(pool).await?;
    }

    info!(partition = partition_name, start_ledger, end_ledger, "Ledger partition created");
    m::counter!("soroban_pulse_ledger_partition_created_total").increment(1);

    Ok(partition_name)
}

/// List all ledger-based partitions.
pub async fn list_ledger_partitions(
    pool: &PgPool,
) -> Result<Vec<LedgerPartitionInfo>, sqlx::Error> {
    // This is a simplified implementation. In a real scenario, you would parse
    // partition constraints to get the actual ledger ranges.
    let rows: Vec<(String, i64, i64)> = sqlx::query_as(
        "SELECT
             c.relname::text,
             COALESCE(s.n_live_tup, 0)::bigint,
             COALESCE(pg_total_relation_size(c.oid), 0)::bigint
         FROM pg_inherits i
         JOIN pg_class c ON c.oid = i.inhrelid
         JOIN pg_class p ON p.oid = i.inhparent
         LEFT JOIN pg_stat_user_tables s ON s.relname = c.relname
         WHERE p.relname = 'events' AND c.relname LIKE 'events_ledger_%'
         ORDER BY c.relname",
    )
    .fetch_all(pool)
    .await?;

    let partitions = rows
        .into_iter()
        .filter_map(|(name, row_count, size_bytes)| {
            let (start, end) = parse_ledger_partition_range(&name)?;
            let is_archived = name.starts_with("archive_");
            Some(LedgerPartitionInfo {
                partition_name: name,
                start_ledger: start,
                end_ledger: end,
                row_count,
                size_bytes,
                is_archived,
            })
        })
        .collect();

    Ok(partitions)
}

/// Automatically create ledger partitions for the next N ledger ranges.
pub async fn create_future_ledger_partitions(
    pool: &PgPool,
    config: &LedgerPartitionConfig,
    num_partitions: u32,
) -> Result<Vec<String>, sqlx::Error> {
    let latest_ledger: (i64,) = sqlx::query_as("SELECT COALESCE(MAX(ledger), 0) FROM events")
        .fetch_one(pool)
        .await?;

    let current_ledger = latest_ledger.0;
    let mut created = Vec::new();

    for i in 0..num_partitions {
        let start_ledger = current_ledger + (i as i64 * config.ledger_range_per_partition);
        let end_ledger = start_ledger + config.ledger_range_per_partition;

        match create_ledger_partition(pool, start_ledger, end_ledger).await {
            Ok(name) => created.push(name),
            Err(e) => warn!(start_ledger, end_ledger, error = %e, "Could not create ledger partition"),
        }
    }

    info!("Created {} future ledger partitions", created.len());
    Ok(created)
}

/// Rotate partitions: archive old ones and create new ones as needed.
pub async fn rotate_ledger_partitions(
    pool: &PgPool,
    config: &LedgerPartitionConfig,
) -> Result<(), sqlx::Error> {
    let partitions = list_ledger_partitions(pool).await?;
    let active_partitions: Vec<_> = partitions
        .iter()
        .filter(|p| !p.is_archived)
        .collect();

    if active_partitions.len() as u32 > config.max_partitions_before_archival {
        let to_archive = active_partitions.len() as u32 - config.max_partitions_before_archival;

        let mut sorted = active_partitions.clone();
        sorted.sort_by_key(|p| p.start_ledger);

        for partition in sorted.iter().take(to_archive as usize) {
            let archive_name = format!("archive_{}", partition.partition_name);
            let sql = format!(
                "ALTER TABLE {} RENAME TO {}",
                partition.partition_name, archive_name
            );
            if let Err(e) = sqlx::query(&sql).execute(pool).await {
                warn!(partition = &partition.partition_name, error = %e, "Failed to archive partition");
            }
        }
    }

    if config.auto_create_partitions {
        create_future_ledger_partitions(pool, config, 3).await?;
    }

    Ok(())
}

/// Get comprehensive statistics for ledger partitions.
pub async fn get_ledger_partition_stats(
    pool: &PgPool,
) -> Result<LedgerPartitionStats, sqlx::Error> {
    let partitions = list_ledger_partitions(pool).await?;
    let active = partitions.iter().filter(|p| !p.is_archived).count();
    let archived = partitions.iter().filter(|p| p.is_archived).count();
    let total_rows: i64 = partitions.iter().map(|p| p.row_count).sum();
    let total_size: i64 = partitions.iter().map(|p| p.size_bytes).sum();

    m::gauge!("soroban_pulse_ledger_partitions_total").set(partitions.len() as f64);
    m::gauge!("soroban_pulse_ledger_partitions_active").set(active as f64);
    m::gauge!("soroban_pulse_ledger_partitions_archived").set(archived as f64);
    m::gauge!("soroban_pulse_ledger_partition_total_size_bytes").set(total_size as f64);

    Ok(LedgerPartitionStats {
        total_partitions: partitions.len(),
        active_partitions: active,
        archived_partitions: archived,
        total_rows,
        total_size_bytes: total_size,
    })
}

/// Forecast storage growth over the next `days_ahead` days based on the
/// average size of the most recent partitions.
pub async fn forecast_capacity(
    pool: &PgPool,
    days_ahead: u32,
) -> Result<CapacityForecast, sqlx::Error> {
    let partitions = list_partitions(pool).await?;

    let total_size: i64 = partitions.iter().map(|p| p.size_bytes).sum();
    m::gauge!("soroban_pulse_partition_total_size_bytes").set(total_size as f64);

    // Use the most recent 3 partitions to estimate growth rate
    let recent: Vec<&PartitionInfo> = {
        let mut v: Vec<&PartitionInfo> = partitions.iter().collect();
        v.sort_by(|a, b| b.start_date.cmp(&a.start_date));
        v.truncate(3);
        v
    };

    let avg_monthly_bytes = if recent.is_empty() {
        0.0
    } else {
        recent.iter().map(|p| p.size_bytes as f64).sum::<f64>() / recent.len() as f64
    };
    let growth_rate = avg_monthly_bytes / 30.0; // bytes per day

    // 1 TiB threshold
    let threshold_bytes: i64 = 1024 * 1024 * 1024 * 1024;
    let remaining = threshold_bytes - total_size;
    let days_until = if growth_rate > 0.0 {
        Some((remaining as f64 / growth_rate) as u32)
    } else {
        None
    };

    // Build forecast partitions list
    let mut forecast_partitions = Vec::new();
    let now = Utc::now();
    let mut yr = now.year();
    let mut mo = now.month();
    for _ in 0..days_ahead / 30 + 1 {
        let name = format!("events_{:04}_{:02}", yr, mo);
        forecast_partitions.push((name, avg_monthly_bytes as i64));
        if mo == 12 {
            yr += 1;
            mo = 1;
        } else {
            mo += 1;
        }
    }

    Ok(CapacityForecast {
        current_size_bytes: total_size,
        growth_rate_bytes_per_day: growth_rate,
        days_until_threshold: days_until,
        threshold_bytes,
        forecast_partitions,
    })
}

/// Run ANALYZE on all event partitions.
pub async fn refresh_partition_statistics(pool: &PgPool) -> Result<(), sqlx::Error> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT relname::text FROM pg_inherits i
         JOIN pg_class c ON c.oid = i.inhrelid
         JOIN pg_class p ON p.oid = i.inhparent
         WHERE p.relname = 'events'",
    )
    .fetch_all(pool)
    .await?;

    for (name,) in rows {
        let sql = format!("ANALYZE {}", name);
        if let Err(e) = sqlx::query(&sql).execute(pool).await {
            warn!(partition = name, error = %e, "ANALYZE failed on partition");
        } else {
            debug!(partition = name, "Partition statistics refreshed");
        }
    }
    Ok(())
}

/// Return a comprehensive JSON dashboard for the partition layer.
pub async fn get_partition_dashboard(pool: &PgPool) -> Result<serde_json::Value, sqlx::Error> {
    let partitions = list_partitions(pool).await?;
    let hot = identify_hot_partitions(pool).await.unwrap_or_default();
    let cold = identify_cold_partitions(pool, 3).await.unwrap_or_default();
    let skew = calculate_partition_skew(pool).await.unwrap_or_default();
    let forecast = forecast_capacity(pool, 90).await.ok();

    let total_size: i64 = partitions.iter().map(|p| p.size_bytes).sum();
    let total_rows: i64 = partitions.iter().map(|p| p.row_count).sum();

    m::gauge!("soroban_pulse_partition_count").set(partitions.len() as f64);
    m::gauge!("soroban_pulse_hot_partitions_count")
        .set(hot.iter().filter(|h| h.seq_scan + h.idx_scan > 0).count() as f64);

    Ok(serde_json::json!({
        "generated_at": Utc::now(),
        "summary": {
            "total_partitions": partitions.len(),
            "hot_partitions": hot.iter().filter(|h| h.seq_scan + h.idx_scan > 0).count(),
            "cold_partitions": cold.len(),
            "total_size_bytes": total_size,
            "total_rows": total_rows,
        },
        "partitions": partitions.iter().map(|p| serde_json::json!({
            "name": p.table_name,
            "start_date": p.start_date,
            "end_date": p.end_date,
            "row_count": p.row_count,
            "size_bytes": p.size_bytes,
            "is_hot": p.is_hot,
            "is_archived": p.is_archived,
        })).collect::<Vec<_>>(),
        "skew": skew.iter().map(|(n, r, s)| serde_json::json!({
            "partition": n, "row_count": r, "skew_factor": s
        })).collect::<Vec<_>>(),
        "forecast": forecast.map(|f| serde_json::json!({
            "current_size_bytes": f.current_size_bytes,
            "growth_rate_bytes_per_day": f.growth_rate_bytes_per_day,
            "days_until_1tib": f.days_until_threshold,
        })),
    }))
}

/// Spawn a background task that ensures future partitions exist and
/// refreshes statistics on a fixed interval.
pub fn spawn(
    pool: PgPool,
    interval_secs: u64,
    months_ahead: u32,
    mut shutdown: watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut ticker =
            tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    debug!("Partition manager: creating future partitions");
                    match create_future_partitions(&pool, months_ahead).await {
                        Ok(created) if !created.is_empty() =>
                            info!("Created partitions: {:?}", created),
                        Ok(_) => debug!("All future partitions already exist"),
                        Err(e) => error!("Failed to create future partitions: {}", e),
                    }
                    if let Err(e) = refresh_partition_statistics(&pool).await {
                        warn!("Partition stats refresh failed: {}", e);
                    }
                }
                _ = shutdown.changed() => {
                    info!("Partition manager shutting down");
                    break;
                }
            }
        }
    });
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Parse start and end ledger numbers from a partition name like `events_ledger_0000000000_1000000000`.
fn parse_ledger_partition_range(name: &str) -> Option<(i64, i64)> {
    let stripped = name.strip_prefix("archive_").unwrap_or(name);
    let suffix = stripped.strip_prefix("events_ledger_")?;
    let parts: Vec<&str> = suffix.splitn(2, '_').collect();
    if parts.len() != 2 {
        return None;
    }
    let start: i64 = parts[0].parse().ok()?;
    let end: i64 = parts[1].parse().ok()?;
    Some((start, end))
}

/// Parse year and month from a partition name like `events_2026_07`.
fn parse_partition_dates(name: &str) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    // Strip optional "archive_" prefix
    let stripped = name.strip_prefix("archive_").unwrap_or(name);
    // Expected suffix: events_YYYY_MM
    let suffix = stripped.strip_prefix("events_")?;
    let parts: Vec<&str> = suffix.splitn(2, '_').collect();
    if parts.len() != 2 {
        return None;
    }
    let year: i32 = parts[0].parse().ok()?;
    let month: u32 = parts[1].parse().ok()?;
    if month < 1 || month > 12 {
        return None;
    }
    let start = Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0).single()?;
    let (end_year, end_month) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let end = Utc.with_ymd_and_hms(end_year, end_month, 1, 0, 0, 0).single()?;
    Some((start, end))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_partition_dates_valid() {
        let (start, end) = parse_partition_dates("events_2026_07").unwrap();
        assert_eq!(start.year(), 2026);
        assert_eq!(start.month(), 7);
        assert_eq!(end.year(), 2026);
        assert_eq!(end.month(), 8);
    }

    #[test]
    fn parse_partition_dates_december_rolls_year() {
        let (start, end) = parse_partition_dates("events_2025_12").unwrap();
        assert_eq!(start.year(), 2025);
        assert_eq!(start.month(), 12);
        assert_eq!(end.year(), 2026);
        assert_eq!(end.month(), 1);
    }

    #[test]
    fn parse_partition_dates_archive_prefix() {
        let result = parse_partition_dates("archive_events_2024_01");
        assert!(result.is_some());
        let (start, _) = result.unwrap();
        assert_eq!(start.year(), 2024);
        assert_eq!(start.month(), 1);
    }

    #[test]
    fn parse_partition_dates_invalid_returns_none() {
        assert!(parse_partition_dates("events").is_none());
        assert!(parse_partition_dates("events_notayear_xx").is_none());
        assert!(parse_partition_dates("events_2026_13").is_none()); // month 13 invalid
    }

    #[test]
    fn partition_pruning_report_effectiveness() {
        // Simulate 10 partitions, 8 pruned → 20% accessed
        let report = PartitionPruningReport {
            total_partitions: 10,
            pruned_partitions: 8,
            accessed_partitions: 2,
            pruning_effectiveness: 0.8,
            accessed_partition_names: vec![
                "events_2026_06".to_string(),
                "events_2026_07".to_string(),
            ],
        };
        assert_eq!(report.pruning_effectiveness, 0.8);
        assert_eq!(report.accessed_partition_names.len(), 2);
    }

    #[test]
    fn archival_config_default_is_dry_run() {
        let cfg = PartitionArchivalConfig::default();
        assert!(cfg.dry_run);
        assert_eq!(cfg.archive_after_months, 12);
        assert_eq!(cfg.archive_table_prefix, "archive_");
    }

    #[test]
    fn skew_factor_mean_partition_is_one() {
        // If all partitions have equal row count, skew factor = 1.0 for each
        let counts = vec![1000i64, 1000, 1000];
        let mean = counts.iter().sum::<i64>() as f64 / counts.len() as f64;
        let skews: Vec<f64> = counts.iter().map(|c| *c as f64 / mean).collect();
        for s in skews {
            assert!((s - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn pruning_no_partitions_returns_full_effectiveness() {
        let total = 0_usize;
        let accessed = 0_usize;
        let effectiveness = if total > 0 {
            (total - accessed) as f64 / total as f64
        } else {
            1.0
        };
        assert_eq!(effectiveness, 1.0);
    }

    #[test]
    fn capacity_forecast_growth_rate() {
        // avg monthly = 1 GiB, daily ≈ 34 MiB
        let avg_monthly: f64 = 1024.0 * 1024.0 * 1024.0;
        let daily = avg_monthly / 30.0;
        assert!((daily - 34_952_533.0).abs() < 1.0);
    }

    #[test]
    fn create_partition_name_format() {
        let year = 2026;
        let month = 7_u32;
        let name = format!("events_{:04}_{:02}", year, month);
        assert_eq!(name, "events_2026_07");
    }

    #[test]
    fn archive_name_uses_prefix() {
        let cfg = PartitionArchivalConfig::default();
        let name = format!("{}events_2024_01", cfg.archive_table_prefix);
        assert_eq!(name, "archive_events_2024_01");
    }

    #[test]
    fn parse_ledger_partition_range_valid() {
        let (start, end) = parse_ledger_partition_range("events_ledger_0000000000_1000000000").unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 1_000_000_000);
    }

    #[test]
    fn parse_ledger_partition_range_with_archive_prefix() {
        let (start, end) = parse_ledger_partition_range("archive_events_ledger_1000000000_2000000000").unwrap();
        assert_eq!(start, 1_000_000_000);
        assert_eq!(end, 2_000_000_000);
    }

    #[test]
    fn parse_ledger_partition_range_invalid_returns_none() {
        assert!(parse_ledger_partition_range("events_ledger_invalid").is_none());
        assert!(parse_ledger_partition_range("events_2026_07").is_none());
    }

    #[test]
    fn ledger_partition_config_default() {
        let cfg = LedgerPartitionConfig::default();
        assert_eq!(cfg.ledger_range_per_partition, 1_000_000);
        assert!(cfg.auto_create_partitions);
        assert_eq!(cfg.max_partitions_before_archival, 12);
    }

    #[test]
    fn ledger_partition_info_serialization() {
        let info = LedgerPartitionInfo {
            partition_name: "events_ledger_0000000000_1000000000".to_string(),
            start_ledger: 0,
            end_ledger: 1_000_000_000,
            row_count: 50_000,
            size_bytes: 1024 * 1024 * 100,
            is_archived: false,
        };
        let json = serde_json::to_string(&info).unwrap();
        let parsed: LedgerPartitionInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.start_ledger, 0);
        assert_eq!(parsed.end_ledger, 1_000_000_000);
    }

    #[test]
    fn ledger_partition_stats_calculation() {
        let partitions = vec![
            LedgerPartitionInfo {
                partition_name: "p1".to_string(),
                start_ledger: 0,
                end_ledger: 1_000_000,
                row_count: 10_000,
                size_bytes: 1024,
                is_archived: false,
            },
            LedgerPartitionInfo {
                partition_name: "p2".to_string(),
                start_ledger: 1_000_000,
                end_ledger: 2_000_000,
                row_count: 10_000,
                size_bytes: 1024,
                is_archived: true,
            },
        ];

        let total_rows: i64 = partitions.iter().map(|p| p.row_count).sum();
        let active = partitions.iter().filter(|p| !p.is_archived).count();
        let archived = partitions.iter().filter(|p| p.is_archived).count();

        assert_eq!(total_rows, 20_000);
        assert_eq!(active, 1);
        assert_eq!(archived, 1);
    }
}
