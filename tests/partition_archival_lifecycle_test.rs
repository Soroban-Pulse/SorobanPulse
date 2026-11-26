/// Issue #1449 — Integration tests for partition archival and retention lifecycle
use sqlx::PgPool;
use std::time::{Duration, SystemTime};

#[tokio::test]
async fn test_partition_creation_and_indexing() {
    let pool = setup_test_pool().await;
    
    // Create partition for 2024-01
    let partition_name = create_test_partition(&pool, "2024_01").await;
    assert_eq!(partition_name, "events_2024_01");
    
    // Verify partition exists and is part of parent
    let exists = check_partition_exists(&pool, &partition_name).await;
    assert!(exists, "Partition should be created");
    
    // Verify indexes created on partition
    let indexes = get_partition_indexes(&pool, &partition_name).await;
    assert!(indexes.contains(&"idx_events_2024_01_tenant_id".to_string()));
    assert!(indexes.contains(&"idx_events_2024_01_contract_id".to_string()));
}

#[tokio::test]
async fn test_partition_archival_rename() {
    let pool = setup_test_pool().await;
    
    // 1. Create partition and insert test data
    let partition_name = create_test_partition(&pool, "2024_01").await;
    insert_test_events(&pool, &partition_name, 1000).await;
    
    let event_count = count_events_in_partition(&pool, &partition_name).await;
    assert_eq!(event_count, 1000);
    
    // 2. Archive partition (rename)
    let archive_name = archive_partition(&pool, &partition_name).await;
    assert_eq!(archive_name, "events_archive_2024_01");
    
    // 3. Verify old name no longer exists (as partition)
    let old_exists = check_partition_exists(&pool, &partition_name).await;
    assert!(!old_exists);
    
    // 4. Verify archive table exists and has same data
    let archive_exists = check_table_exists(&pool, &archive_name).await;
    assert!(archive_exists);
    
    let archive_count = count_events_in_table(&pool, &archive_name).await;
    assert_eq!(archive_count, 1000);
}

#[tokio::test]
async fn test_rls_preserved_on_archived_partition() {
    let pool = setup_test_pool().await;
    let tenant_1 = uuid::Uuid::new_v4();
    let tenant_2 = uuid::Uuid::new_v4();
    
    // 1. Create and populate partition
    let partition_name = create_test_partition(&pool, "2024_01").await;
    
    // Insert events for different tenants
    insert_events_for_tenant(&pool, &partition_name, tenant_1, 100).await;
    insert_events_for_tenant(&pool, &partition_name, tenant_2, 100).await;
    
    // 2. Archive partition
    let archive_name = archive_partition(&pool, &partition_name).await;
    
    // 3. Create restricted connection for tenant_1
    let restricted_pool = create_restricted_connection(tenant_1).await;
    
    // 4. Query archived partition as tenant_1 - should see only tenant_1's events
    let events = restricted_pool
        .query_as::<_, (uuid::Uuid,)>(
            &format!("SELECT tenant_id FROM {} WHERE tenant_id = $1", archive_name)
        )
        .bind(tenant_1)
        .fetch_all(&restricted_pool)
        .await
        .expect("Query should succeed");
    
    assert_eq!(events.len(), 100, "Should see only tenant_1's events via RLS");
    
    // 5. Attempt to query other tenant's events - RLS should block
    let other_events = restricted_pool
        .query_as::<_, (uuid::Uuid,)>(
            &format!("SELECT tenant_id FROM {} WHERE tenant_id = $1", archive_name)
        )
        .bind(tenant_2)
        .fetch_all(&restricted_pool)
        .await
        .expect("Query should succeed");
    
    assert_eq!(other_events.len(), 0, "RLS should prevent access to other tenant data");
}

#[tokio::test]
async fn test_partition_retention_and_deletion() {
    let pool = setup_test_pool().await;
    
    // 1. Create and archive partition
    let partition_name = create_test_partition(&pool, "2024_01").await;
    insert_test_events(&pool, &partition_name, 500).await;
    let archive_name = archive_partition(&pool, &partition_name).await;
    
    // Verify archive exists
    let exists = check_table_exists(&pool, &archive_name).await;
    assert!(exists);
    
    // 2. After retention period, delete archive
    delete_old_archive(&pool, &archive_name, 24).await; // retention_months=24
    
    // 3. Verify archive is deleted
    let exists_after = check_table_exists(&pool, &archive_name).await;
    assert!(!exists_after, "Archive should be deleted after retention period");
}

#[tokio::test]
async fn test_materialized_views_work_with_archived_partitions() {
    let pool = setup_test_pool().await;
    
    // 1. Create partition with events
    let partition = create_test_partition(&pool, "2024_01").await;
    insert_test_events(&pool, &partition, 1000).await;
    
    // 2. Create materialized view that queries events
    create_test_materialized_view(&pool, "events_by_contract_daily").await;
    
    // 3. Archive partition
    let archive_name = archive_partition(&pool, &partition).await;
    
    // 4. Refresh materialized view (should include archived data via UNION)
    let refresh_result = refresh_materialized_view(&pool, "events_by_contract_daily").await;
    assert!(refresh_result.is_ok(), "View refresh should work with archived partitions");
    
    // 5. Verify view has data
    let view_rows = query_materialized_view(&pool, "events_by_contract_daily").await;
    assert!(!view_rows.is_empty(), "View should contain data from both active and archived partitions");
}

// Helper functions

async fn setup_test_pool() -> PgPool {
    // Connect to test database
    PgPool::connect(&std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://user:password@localhost/soroban_pulse_test".to_string()
    }))
    .await
    .expect("Failed to create pool")
}

async fn create_test_partition(pool: &PgPool, partition_id: &str) -> String {
    let table_name = format!("events_{}", partition_id);
    
    // Validate partition ID format (YYYY_MM)
    assert!(partition_id.len() == 7 && partition_id.chars().nth(4) == Some('_'));
    
    // Create partition
    let query = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" PARTITION OF events \
         FOR VALUES FROM ('2024-01-01') TO ('2024-02-01')",
        table_name
    );
    
    sqlx::query(&query)
        .execute(pool)
        .await
        .expect("Failed to create partition");
    
    // Create indexes
    for idx_name in &["tenant_id", "contract_id"] {
        let idx_query = format!(
            "CREATE INDEX IF NOT EXISTS idx_{}_{} ON \"{}\" ({})",
            table_name, idx_name, table_name, idx_name
        );
        sqlx::query(&idx_query)
            .execute(pool)
            .await
            .expect("Failed to create index");
    }
    
    table_name
}

async fn check_partition_exists(pool: &PgPool, partition_name: &str) -> bool {
    let result: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_name = $1 AND table_type = 'BASE TABLE'"
    )
    .bind(partition_name)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
    
    result.0 > 0
}

async fn check_table_exists(pool: &PgPool, table_name: &str) -> bool {
    let result: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables \
         WHERE table_name = $1"
    )
    .bind(table_name)
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
    
    result.0 > 0
}

async fn get_partition_indexes(pool: &PgPool, partition_name: &str) -> Vec<String> {
    let indexes: Vec<(String,)> = sqlx::query_as(
        "SELECT indexname FROM pg_indexes WHERE tablename = $1"
    )
    .bind(partition_name)
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    
    indexes.into_iter().map(|(name,)| name).collect()
}

async fn insert_test_events(pool: &PgPool, partition: &str, count: i32) {
    for i in 0..count {
        sqlx::query(
            &format!(
                "INSERT INTO {} (tenant_id, contract_id, timestamp, event_type, data) \
                 VALUES ($1, $2, $3, $4, $5)",
                partition
            )
        )
        .bind(uuid::Uuid::new_v4())
        .bind(format!("contract-{}", i))
        .bind(chrono::Utc::now())
        .bind("test_event")
        .bind(serde_json::json!({}))
        .execute(pool)
        .await
        .expect("Failed to insert event");
    }
}

async fn insert_events_for_tenant(
    pool: &PgPool,
    partition: &str,
    tenant_id: uuid::Uuid,
    count: i32,
) {
    for i in 0..count {
        sqlx::query(
            &format!(
                "INSERT INTO {} (tenant_id, contract_id, timestamp, event_type, data) \
                 VALUES ($1, $2, $3, $4, $5)",
                partition
            )
        )
        .bind(tenant_id)
        .bind(format!("contract-{}", i))
        .bind(chrono::Utc::now())
        .bind("test_event")
        .bind(serde_json::json!({}))
        .execute(pool)
        .await
        .expect("Failed to insert event");
    }
}

async fn count_events_in_partition(pool: &PgPool, partition: &str) -> i64 {
    let result: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {}", partition))
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
    result.0
}

async fn count_events_in_table(pool: &PgPool, table: &str) -> i64 {
    let result: (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {}", table))
        .fetch_one(pool)
        .await
        .unwrap_or((0,));
    result.0
}

async fn archive_partition(pool: &PgPool, partition_name: &str) -> String {
    let archive_name = partition_name.replace("events_", "events_archive_");
    
    let query = format!("ALTER TABLE \"{}\" RENAME TO \"{}\"", partition_name, archive_name);
    
    sqlx::query(&query)
        .execute(pool)
        .await
        .expect("Failed to archive partition");
    
    archive_name
}

async fn delete_old_archive(pool: &PgPool, archive_name: &str, retention_months: i32) {
    let query = format!("DROP TABLE IF EXISTS \"{}\"", archive_name);
    
    sqlx::query(&query)
        .execute(pool)
        .await
        .expect("Failed to delete archive");
}

async fn create_restricted_connection(tenant_id: uuid::Uuid) -> PgPool {
    // Create connection with RLS context set for specific tenant
    let conn_string = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://user:password@localhost/soroban_pulse_test".to_string()
    });
    
    let pool = PgPool::connect(&conn_string)
        .await
        .expect("Failed to create restricted pool");
    
    // Set RLS context (requires custom role or similar)
    sqlx::query("SET app.current_tenant_id = $1")
        .bind(tenant_id.to_string())
        .execute(&pool)
        .await
        .ok();
    
    pool
}

async fn create_test_materialized_view(pool: &PgPool, view_name: &str) {
    let query = format!(
        "CREATE MATERIALIZED VIEW IF NOT EXISTS {} AS \
         SELECT contract_id, COUNT(*) as event_count FROM events \
         GROUP BY contract_id",
        view_name
    );
    
    sqlx::query(&query)
        .execute(pool)
        .await
        .ok();
}

async fn refresh_materialized_view(pool: &PgPool, view_name: &str) -> Result<(), sqlx::Error> {
    let query = format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {}", view_name);
    sqlx::query(&query).execute(pool).await.map(|_| ())
}

async fn query_materialized_view(pool: &PgPool, view_name: &str) -> Vec<(String,)> {
    sqlx::query_as(&format!("SELECT contract_id FROM {}", view_name))
        .fetch_all(pool)
        .await
        .unwrap_or_default()
}
