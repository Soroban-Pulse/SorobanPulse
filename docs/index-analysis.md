# Index Usage Analysis and Recommendations

## Overview

The index analysis module provides comprehensive monitoring and optimization recommendations for database indexes in SorobanPulse. It automatically detects unused indexes, identifies bloat, and recommends optimization strategies.

## Features

### 1. Index Usage Tracking

The system monitors index scan counts from PostgreSQL's `pg_stat_user_indexes` view to identify:

- **Active Indexes**: Frequently used indexes (scan_count > 0)
- **Unused Indexes**: Never-scanned indexes (scan_count = 0)
- **Duplicate Indexes**: Multiple indexes with identical usage patterns

### 2. Index Fragmentation Analysis

Detects and reports on index bloat using:

- PostgreSQL statistics (dead tuples, live tuples)
- pgstattuple extension (when available)
- Fragmentation ratio calculation

### 3. Automatic Recommendations

Generates actionable recommendations:

- **DROP recommendations** for unused indexes (saves space on writes)
- **REINDEX recommendations** for fragmented indexes (reclaims space)
- **Consolidation suggestions** for duplicate indexes

## Configuration

### Fragmentation Thresholds

```rust
pub struct FragmentationThresholds {
    pub warn_ratio: f64,              // Default: 0.2 (20%)
    pub critical_ratio: f64,          // Default: 0.5 (50%)
    pub auto_reindex: bool,           // Default: false
}
```

### Default Behavior

- Warns on bloat > 20%
- Alerts critically on bloat > 50%
- Manual REINDEX required (auto_reindex disabled by default)

## Usage

### Starting the Index Monitor

```rust
use crate::index_monitor;

let pool = create_pool().await?;
let thresholds = index_monitor::FragmentationThresholds::default();

// Spawn background monitoring (runs every 6 hours)
index_monitor::spawn(pool, 6, shutdown_signal, thresholds);
```

### Generating Analysis Reports

```rust
let report = index_monitor::generate_index_analysis_report(&pool).await?;

// Access report details
println!("Total indexes: {}", report.total_indexes);
println!("Unused: {}", report.unused_indexes.len());
println!("Bloated: {}", report.bloated_indexes.len());
println!("Healthy: {}", report.healthy_indexes.len());

// Get recommendations
for rec in &report.recommendations {
    println!("{}: {} (priority: {})", 
        rec.index_name, 
        rec.description, 
        rec.priority
    );
}
```

## Metrics Exported

| Metric Name | Type | Description |
|---|---|---|
| `soroban_pulse_unused_indexes_total` | Gauge | Count of unused indexes |
| `soroban_pulse_fragmented_indexes_total` | Gauge | Count of fragmented indexes |
| `soroban_pulse_index_scan_count` | Gauge | Per-index scan count (labels: table, index) |
| `soroban_pulse_index_bloat_ratio` | Gauge | Per-index bloat ratio (labels: table, index) |
| `soroban_pulse_index_size_bytes` | Gauge | Per-index size in bytes (labels: table, index) |
| `soroban_pulse_index_dead_tuples` | Gauge | Per-index dead tuple count (labels: table, index) |

## Index Health Scoring

Each index receives a health score (0.0 to 1.0):

- **Score 0.0**: Unused index (never scanned)
- **Score < 0.5**: Bloated or unused
- **Score 0.5-0.8**: Moderately healthy
- **Score > 0.8**: Healthy and active

Calculation factors:
- Scan count (usage frequency)
- Bloat ratio (fragmentation)
- Size (space efficiency)

## Recommendations Priority

### HIGH Priority
- Dropping completely unused indexes (saves write overhead)
- Dropping redundant duplicate indexes

### MEDIUM Priority
- Reindexing bloated indexes (reclaims space)
- Consolidating similar-usage indexes

## Best Practices

1. **Review Unused Indexes Weekly**
   - Check recommendations before dropping
   - Verify no pending queries in slow-query logs

2. **Monitor Bloat Trends**
   - Set up alerts for bloat > 30%
   - Schedule REINDEX during low-traffic windows

3. **Test REINDEX Impact**
   - Run on replicas first
   - Monitor query performance after REINDEX

4. **Consolidate Duplicates**
   - Combine overlapping indexes where possible
   - Remove redundant multi-column indexes

## Example: Handling a Bloated Index

```rust
// Get analysis report
let report = index_monitor::generate_index_analysis_report(&pool).await?;

// Find recommendations for a specific index
for rec in &report.recommendations {
    if rec.index_name == "idx_events_contract_ledger" && rec.recommendation_type == "REINDEX" {
        println!("Potential savings: {} bytes", rec.potential_savings_bytes);
        
        // Execute REINDEX
        sqlx::query("REINDEX INDEX CONCURRENTLY idx_events_contract_ledger")
            .execute(&pool)
            .await?;
    }
}
```

## Troubleshooting

### pgstattuple Extension Not Available

If pgstattuple is not installed, the system falls back to:
- Dead/live tuple ratio estimation
- pg_stat_user_tables statistics

Install with: `CREATE EXTENSION IF NOT EXISTS pgstattuple;`

### High False Positive Rate

Bloat ratio may be inaccurate for:
- Recently created indexes
- Indexes under active modification
- Indexes with VACUUM pending

Run ANALYZE to update statistics: `ANALYZE <table_name>;`

## Related Features

- Query plan caching ([query-plan-tuning.md](query-plan-tuning.md))
- Table partitioning ([table-partitioning.md](table-partitioning.md))
- Schema health checks (index_monitor::run_schema_health_check)
