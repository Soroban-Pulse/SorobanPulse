# Database Schema, RLS, and Partitioning Documentation

## #1447 - RLS Integration Tests for Partitioned Events Table

### Current State
- RLS policies in 20260527000001_rls_events.sql (parent table)
- Re-applied for partitions in 20261022000001_rls_events_partitioned.sql
- Partition manager creates runtime partitions: events_YYYY_MM
- Risk: Runtime partitions may not inherit RLS policies

### Test File: tests/partition_rls_integration.rs

```rust
#[tokio::test]
async fn test_rls_on_parent_table_query() {
    // Query through parent table - RLS should apply
    let result = pool.query(
        "SELECT * FROM events WHERE tenant_id = $1",
        &[&OTHER_TENANT_ID],
    ).await;
    
    assert!(result.is_empty(), "Should not see other tenant's events");
}

#[tokio::test]
async fn test_rls_bypass_on_direct_partition_query() {
    // Create partition: events_2024_12
    create_test_partition("2024_12").await;
    
    // Query directly on partition - RLS policy required!
    let result = pool.query(
        "SELECT * FROM events_2024_12 WHERE tenant_id = $1",
        &[&OTHER_TENANT_ID],
    ).await;
    
    assert!(result.is_empty(), "RLS must apply to partitions");
}

#[tokio::test]
async fn test_partition_manager_creates_rls_enabled_partitions() {
    // Let partition manager create a new partition
    partition_manager.create_partition_for_month(2024, 12).await;
    
    // Query as restricted user
    let result = restricted_connection.query(
        "SELECT * FROM events_2024_12 WHERE tenant_id = $1",
        &[&OTHER_TENANT_ID],
    ).await;
    
    assert!(result.is_empty(), "New partitions must have RLS policies");
}
```

### Implementation Checklist
- [ ] Create tests/partition_rls_integration.rs
- [ ] Ensure partition creation applies RLS policies automatically
- [ ] Verify ALTER TABLE for partition archival preserves RLS
- [ ] Add CI job: `cargo test --test partition_rls_integration`
- [ ] Document: RLS policies must be applied before partition goes live

---

## #1449 - Partition Archival and Retention Documentation

### Architecture Overview

```
Lifecycle of events_2024_01 partition:
├─ Creation: CREATE TABLE events_2024_01 PARTITION OF events
├─ Indexing: CREATE INDEX idx_events_2024_01_tenant (tenant_id)
├─ Active Use (1-24 months): Events written, queries run
├─ Aging: Last query timestamp > retention_age
├─ Archival: ALTER TABLE events_2024_01 RENAME TO events_archive_2024_01
├─ Compressed: (Optional) ZSTD compress and move to cold storage
└─ Deleted: DROP TABLE events_archive_2024_01 (after retention period)
```

### Configuration (src/partition_manager.rs)

```rust
pub struct PartitionConfig {
    // Creation: One partition per month
    pub partition_interval: Duration = Duration::days(30),
    
    // Aging threshold: Mark for archival after N months
    pub archival_age_months: u32 = 12,
    
    // Retention: Keep on-line archives for N months
    pub retention_months: u32 = 24,
    
    // Cold storage: Move to external storage after retention
    pub cold_storage_enabled: bool = true,
    
    // RLS: Apply policies to new partitions
    pub rls_enabled: bool = true,
}
```

### Test File: tests/partition_archival_lifecycle.rs

```rust
#[tokio::test]
async fn test_partition_lifecycle() {
    let config = PartitionConfig {
        archival_age_months: 1,
        retention_months: 2,
        ..Default::default()
    };
    
    // 1. Create partition
    partition_manager.create_partition(2024, 1).await;
    assert!(partition_exists("events_2024_01").await);
    
    // 2. Write data
    insert_test_events(1000, "2024_01").await;
    
    // 3. Advance time 1 month → archive
    fast_forward_time(Duration::days(31)).await;
    partition_manager.archive_old_partitions().await;
    assert!(partition_renamed("events_2024_01", "events_archive_2024_01").await);
    
    // 4. Advance time 2 months → delete
    fast_forward_time(Duration::days(31)).await;
    partition_manager.delete_old_archives().await;
    assert!(!partition_exists("events_archive_2024_01").await);
}

#[tokio::test]
async fn test_archive_preserves_rls() {
    // Create and archive partition
    partition_manager.create_partition(2024, 1).await;
    partition_manager.archive_partition("2024_01").await;
    
    // Verify RLS still applies to archived partition
    let result = restricted_connection.query(
        "SELECT * FROM events_archive_2024_01 WHERE tenant_id = $1",
        &[&OTHER_TENANT_ID],
    ).await;
    
    assert!(result.is_empty(), "Archived partition must maintain RLS");
}
```

---

## #1450 - Schema Reference Documentation

### Generated Schema Document: docs/schema-reference.md

```markdown
# SorobanPulse Database Schema Reference

## Events Table
```
events (parent table, partitioned by month)
├─ event_id: BIGSERIAL PRIMARY KEY
├─ tenant_id: UUID NOT NULL (RLS POLICY)
├─ contract_id: VARCHAR(56) NOT NULL
├─ timestamp: TIMESTAMP WITH TIME ZONE NOT NULL
├─ event_type: VARCHAR(50) NOT NULL
├─ data: JSONB
└─ Partitions: events_YYYY_MM (monthly)
   └─ Indexes: idx_events_YYYY_MM_tenant, idx_events_YYYY_MM_contract
```

## Multi-Tenant Tables
```
subscriptions
├─ id: UUID PRIMARY KEY
├─ tenant_id: UUID NOT NULL (RLS POLICY)
├─ webhook_url: VARCHAR NOT NULL
└─ status: VARCHAR (active, paused, deleted)

audit_logs
├─ id: BIGSERIAL PRIMARY KEY
├─ tenant_id: UUID NOT NULL (RLS POLICY)
├─ user_id: UUID
├─ action: VARCHAR
├─ table_name: VARCHAR
└─ timestamp: TIMESTAMP
```

## Shared/Unpartitioned Tables
```
tenants (SaaS configuration)
├─ id: UUID PRIMARY KEY
├─ name: VARCHAR NOT NULL
├─ storage_quota_gb: FLOAT
└─ status: VARCHAR

api_keys (Multi-tenant, indexed)
├─ id: UUID PRIMARY KEY
├─ tenant_id: UUID NOT NULL (RLS POLICY)
├─ key_hash: VARCHAR NOT NULL (unique)
└─ expires_at: TIMESTAMP

materialized_views (Aggregations)
├─ events_by_contract_daily
├─ tenant_usage_summary
└─ Refresh strategy: CONCURRENTLY
```

## RLS Policies Applied
```
Policy: Enable for all users
  - Restrict SELECT/UPDATE/DELETE to tenant_id = current_user_id
  - Applied to: events (parent + partitions), subscriptions, audit_logs, api_keys

Policy: Super-user bypass
  - Admins can query across tenants
  - Applied to: tenant_id IS NULL or admin role check
```

## Partitioning Strategy
```
events table:
├─ Partition by RANGE (timestamp)
├─ Monthly intervals (events_2024_01, events_2024_02, ...)
├─ Auto-creation via partition_manager
├─ Archival: Rename to events_archive_YYYY_MM after 12 months
├─ Retention: Keep archives for 24 months
└─ Index per partition: idx_tenant_id, idx_contract_id
```
```

### Schema Generation Script: scripts/generate-schema-docs.sh

```bash
#!/bin/bash
# Generate schema reference from migrations and current database

echo "# SorobanPulse Database Schema" > docs/schema-reference.md
echo "" >> docs/schema-reference.md

# List tables
psql -d soroban_pulse -c "
  SELECT tablename FROM pg_tables 
  WHERE schemaname = 'public' 
  ORDER BY tablename;
" | while read table; do
  echo "## $table" >> docs/schema-reference.md
  
  # Show columns
  psql -d soroban_pulse -c "
    SELECT column_name, data_type, is_nullable 
    FROM information_schema.columns 
    WHERE table_name='$table'
  " >> docs/schema-reference.md
done

# Generate ER diagram (requires pgmodeler or similar)
pg_dump --schema-only soroban_pulse | schemaspy -t pgsql -o docs/schema-diagram/

echo "Schema documentation generated to docs/schema-reference.md"
```

---

## Implementation Checklist

### #1447 - RLS Tests
- [ ] Create tests/partition_rls_integration.rs
- [ ] Test RLS on parent table queries
- [ ] Test RLS on direct partition queries
- [ ] Test RLS applies to runtime partitions
- [ ] Test RLS on archived partitions
- [ ] Add to CI: `test-rls-partitions` job

### #1448 - SQL Injection Prevention
- [ ] Create src/sql_helpers.rs with validation functions
- [ ] Fix src/db.rs:31 - statement_timeout
- [ ] Fix src/db_pool/mod.rs:91 - pool timeout
- [ ] Fix src/stats_refresh.rs:53 - view names
- [ ] Fix src/partition_manager.rs - partition names
- [ ] Add tests: test_validate_partition_id, test_escape_identifier
- [ ] Add clippy lint for unsafe SQL patterns

### #1449 - Partition Archival Tests
- [ ] Create tests/partition_archival_lifecycle.rs
- [ ] Test partition creation → archival → deletion lifecycle
- [ ] Test RLS preservation during archival
- [ ] Test materialized views reference archived partitions
- [ ] Document archival in docs/data-retention.md
- [ ] Document configuration in docs/partition-management.md

### #1450 - Schema Documentation
- [ ] Generate docs/schema-reference.md
- [ ] Create scripts/generate-schema-docs.sh
- [ ] Document all tables and RLS policies
- [ ] Create ERD (pgmodeler or similar)
- [ ] Add to CI: Validate schema docs on migration
- [ ] Update architecture.md with schema overview
