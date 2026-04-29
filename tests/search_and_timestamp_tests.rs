use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

/// Helper to insert a test event with specific timestamp and event_data
async fn insert_test_event(
    pool: &PgPool,
    contract_id: &str,
    event_data: serde_json::Value,
    timestamp: DateTime<Utc>,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, created_at) 
         VALUES ($1, $2, 'contract', $3, $4, $5, $6, NOW())"
    )
    .bind(id)
    .bind(contract_id)
    .bind(format!("{:064x}", rand::random::<u64>()))
    .bind(rand::random::<i64>() % 1000000)
    .bind(timestamp)
    .bind(event_data)
    .execute(pool)
    .await?;
    Ok(id)
}

#[sqlx::test(migrations = "./migrations")]
async fn test_fulltext_search_finds_matching_events(pool: PgPool) -> sqlx::Result<()> {
    // Insert events with different searchable content
    let ts = Utc::now();
    
    insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"symbol": "USDC", "amount": 1000}),
        ts,
    )
    .await?;

    insert_test_event(
        &pool,
        "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBD2KM",
        json!({"symbol": "XLM", "memo": "payment for services"}),
        ts,
    )
    .await?;

    insert_test_event(
        &pool,
        "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCD2KM",
        json!({"action": "transfer", "recipient": "GDXYZ..."}),
        ts,
    )
    .await?;

    // Search for "USDC" - should find first event
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE event_data_tsv @@ plainto_tsquery('english', $1)"
    )
    .bind("USDC")
    .fetch_all(&pool)
    .await?;
    
    assert_eq!(results.len(), 1, "Should find exactly one event with USDC");

    // Search for "payment" - should find second event
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE event_data_tsv @@ plainto_tsquery('english', $1)"
    )
    .bind("payment")
    .fetch_all(&pool)
    .await?;
    
    assert_eq!(results.len(), 1, "Should find exactly one event with payment");

    // Search for "transfer" - should find third event
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE event_data_tsv @@ plainto_tsquery('english', $1)"
    )
    .bind("transfer")
    .fetch_all(&pool)
    .await?;
    
    assert_eq!(results.len(), 1, "Should find exactly one event with transfer");

    // Search for non-existent term
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE event_data_tsv @@ plainto_tsquery('english', $1)"
    )
    .bind("nonexistent")
    .fetch_all(&pool)
    .await?;
    
    assert_eq!(results.len(), 0, "Should find no events with nonexistent term");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_timestamp_filtering_from_timestamp(pool: PgPool) -> sqlx::Result<()> {
    let base_time = Utc::now();
    let one_hour_ago = base_time - Duration::hours(1);
    let two_hours_ago = base_time - Duration::hours(2);
    let three_hours_ago = base_time - Duration::hours(3);

    // Insert events at different times
    let id1 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "old"}),
        three_hours_ago,
    )
    .await?;

    let id2 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "middle"}),
        two_hours_ago,
    )
    .await?;

    let id3 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "recent"}),
        one_hour_ago,
    )
    .await?;

    // Query events from 2.5 hours ago - should get id2 and id3
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE timestamp >= $1 ORDER BY timestamp ASC"
    )
    .bind(two_hours_ago - Duration::minutes(30))
    .fetch_all(&pool)
    .await?;

    assert!(results.len() >= 2, "Should find at least 2 events");
    assert!(results.iter().any(|(id,)| *id == id2), "Should include id2");
    assert!(results.iter().any(|(id,)| *id == id3), "Should include id3");
    assert!(!results.iter().any(|(id,)| *id == id1), "Should not include id1");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_timestamp_filtering_to_timestamp(pool: PgPool) -> sqlx::Result<()> {
    let base_time = Utc::now();
    let one_hour_ago = base_time - Duration::hours(1);
    let two_hours_ago = base_time - Duration::hours(2);
    let three_hours_ago = base_time - Duration::hours(3);

    // Insert events at different times
    let id1 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "old"}),
        three_hours_ago,
    )
    .await?;

    let id2 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "middle"}),
        two_hours_ago,
    )
    .await?;

    let id3 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "recent"}),
        one_hour_ago,
    )
    .await?;

    // Query events up to 1.5 hours ago - should get id1 and id2
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE timestamp <= $1 ORDER BY timestamp ASC"
    )
    .bind(one_hour_ago - Duration::minutes(30))
    .fetch_all(&pool)
    .await?;

    assert!(results.len() >= 2, "Should find at least 2 events");
    assert!(results.iter().any(|(id,)| *id == id1), "Should include id1");
    assert!(results.iter().any(|(id,)| *id == id2), "Should include id2");
    assert!(!results.iter().any(|(id,)| *id == id3), "Should not include id3");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_timestamp_filtering_range(pool: PgPool) -> sqlx::Result<()> {
    let base_time = Utc::now();
    let one_hour_ago = base_time - Duration::hours(1);
    let two_hours_ago = base_time - Duration::hours(2);
    let three_hours_ago = base_time - Duration::hours(3);
    let four_hours_ago = base_time - Duration::hours(4);

    // Insert events at different times
    let id1 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "very_old"}),
        four_hours_ago,
    )
    .await?;

    let id2 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "old"}),
        three_hours_ago,
    )
    .await?;

    let id3 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "middle"}),
        two_hours_ago,
    )
    .await?;

    let id4 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"event": "recent"}),
        one_hour_ago,
    )
    .await?;

    // Query events between 3 and 1.5 hours ago - should get id2 and id3
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events WHERE timestamp >= $1 AND timestamp <= $2 ORDER BY timestamp ASC"
    )
    .bind(three_hours_ago)
    .bind(one_hour_ago - Duration::minutes(30))
    .fetch_all(&pool)
    .await?;

    assert!(results.len() >= 2, "Should find at least 2 events");
    assert!(results.iter().any(|(id,)| *id == id2), "Should include id2");
    assert!(results.iter().any(|(id,)| *id == id3), "Should include id3");
    assert!(!results.iter().any(|(id,)| *id == id1), "Should not include id1");
    assert!(!results.iter().any(|(id,)| *id == id4), "Should not include id4");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_combined_search_and_timestamp_filters(pool: PgPool) -> sqlx::Result<()> {
    let base_time = Utc::now();
    let one_hour_ago = base_time - Duration::hours(1);
    let two_hours_ago = base_time - Duration::hours(2);

    // Insert events with different content and timestamps
    let id1 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"symbol": "USDC", "action": "transfer"}),
        two_hours_ago,
    )
    .await?;

    let id2 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"symbol": "XLM", "action": "transfer"}),
        one_hour_ago,
    )
    .await?;

    let id3 = insert_test_event(
        &pool,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        json!({"symbol": "USDC", "action": "mint"}),
        one_hour_ago,
    )
    .await?;

    // Search for "USDC" events from the last 1.5 hours - should only get id3
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events 
         WHERE event_data_tsv @@ plainto_tsquery('english', $1) 
         AND timestamp >= $2 
         ORDER BY timestamp ASC"
    )
    .bind("USDC")
    .bind(one_hour_ago - Duration::minutes(30))
    .fetch_all(&pool)
    .await?;

    assert_eq!(results.len(), 1, "Should find exactly one event");
    assert_eq!(results[0].0, id3, "Should find id3");

    // Search for "transfer" events from the last 1.5 hours - should only get id2
    let results: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM events 
         WHERE event_data_tsv @@ plainto_tsquery('english', $1) 
         AND timestamp >= $2 
         ORDER BY timestamp ASC"
    )
    .bind("transfer")
    .bind(one_hour_ago - Duration::minutes(30))
    .fetch_all(&pool)
    .await?;

    assert!(results.len() >= 1, "Should find at least one event");
    assert!(results.iter().any(|(id,)| *id == id2), "Should include id2");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_tsvector_column_exists(pool: PgPool) -> sqlx::Result<()> {
    // Verify the event_data_tsv column exists
    let result: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM information_schema.columns 
            WHERE table_name = 'events' 
            AND column_name = 'event_data_tsv'
        )"
    )
    .fetch_one(&pool)
    .await?;

    assert!(result.0, "event_data_tsv column should exist");

    Ok(())
}

#[sqlx::test(migrations = "./migrations")]
async fn test_tsvector_gin_index_exists(pool: PgPool) -> sqlx::Result<()> {
    // Verify the GIN index on event_data_tsv exists
    let result: (bool,) = sqlx::query_as(
        "SELECT EXISTS (
            SELECT 1 FROM pg_indexes 
            WHERE tablename = 'events' 
            AND indexname = 'idx_events_event_data_tsv'
        )"
    )
    .fetch_one(&pool)
    .await?;

    assert!(result.0, "GIN index on event_data_tsv should exist");

    Ok(())
}
