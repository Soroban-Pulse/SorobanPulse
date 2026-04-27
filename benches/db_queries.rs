//! Database query benchmarks.
//!
//! These benchmarks measure real PostgreSQL query performance against a
//! pre-seeded dataset of 10,000 events. They serve as regression tests for
//! query performance and provide baseline numbers for evaluating the impact
//! of schema changes, index additions, or query rewrites.
//!
//! # Running
//!
//! ```bash
//! cargo bench --bench db_queries
//! ```
//!
//! Requires a running PostgreSQL instance. Set DATABASE_URL in the environment
//! or in a `.env` file before running.
//!
//! # Scenarios
//!
//! - `get_events_no_filter`      — GET /v1/events with no filters (page 1, limit 20)
//! - `get_events_ledger_range`   — GET /v1/events with from_ledger / to_ledger filters
//! - `get_events_exact_count`    — GET /v1/events with exact_count=true (COUNT(*))
//! - `get_events_by_contract`    — GET /v1/events/contract/:id for a busy contract

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sqlx::postgres::PgPoolOptions;
use tokio::runtime::Runtime;

const SEED_EVENTS: usize = 10_000;
/// Number of events belonging to the "hot" contract used in the contract benchmark.
const HOT_CONTRACT_EVENTS: usize = 500;
const HOT_CONTRACT_ID: &str = "CBENCHMARKHOTCONTRACT0000000000000000000000000000000000";

/// Build a Tokio runtime for driving async code inside Criterion.
fn rt() -> Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime")
}

/// Connect to the database and return a pool.
async fn connect() -> sqlx::PgPool {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set to run db_queries benchmarks");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&url)
        .await
        .expect("failed to connect to database")
}

/// Seed the database with SEED_EVENTS rows if not already present.
/// Uses a sentinel contract ID to detect whether seeding has already run.
async fn seed(pool: &sqlx::PgPool) {
    // Run migrations so the schema exists.
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .expect("migrations failed");

    let existing: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM events WHERE contract_id = $1",
    )
    .bind(HOT_CONTRACT_ID)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if existing >= HOT_CONTRACT_EVENTS as i64 {
        // Already seeded — skip to keep bench runs fast.
        return;
    }

    // Insert HOT_CONTRACT_EVENTS events for the hot contract spread across ledgers 1..=500.
    for i in 0..HOT_CONTRACT_EVENTS {
        let ledger = (i % 500 + 1) as i64;
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)
             ON CONFLICT (tx_hash, contract_id, event_type) DO NOTHING",
        )
        .bind(HOT_CONTRACT_ID)
        .bind("contract")
        .bind(format!("bench_hot_{i:0>60}"))
        .bind(ledger)
        .bind(serde_json::json!({"bench": true}))
        .execute(pool)
        .await
        .expect("failed to insert hot contract event");
    }

    // Insert the remaining events spread across many contracts and ledgers 1..=1000.
    let remaining = SEED_EVENTS - HOT_CONTRACT_EVENTS;
    for i in 0..remaining {
        let ledger = (i % 1000 + 1) as i64;
        let contract = format!("C{:0>55}", i % 200);
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)
             ON CONFLICT (tx_hash, contract_id, event_type) DO NOTHING",
        )
        .bind(&contract)
        .bind("contract")
        .bind(format!("bench_bulk_{i:0>60}"))
        .bind(ledger)
        .bind(serde_json::json!({"bench": true}))
        .execute(pool)
        .await
        .expect("failed to insert bulk event");
    }
}

fn bench_get_events_no_filter(c: &mut Criterion) {
    let rt = rt();
    let pool = rt.block_on(connect());
    rt.block_on(seed(&pool));

    c.bench_function("db/get_events_no_filter", |b| {
        b.iter(|| {
            rt.block_on(async {
                let rows = sqlx::query(
                    "SELECT id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, created_at \
                     FROM events ORDER BY ledger DESC, id DESC LIMIT $1 OFFSET $2",
                )
                .bind(black_box(20_i64))
                .bind(black_box(0_i64))
                .fetch_all(&pool)
                .await
                .expect("query failed");
                black_box(rows.len())
            })
        });
    });
}

fn bench_get_events_ledger_range(c: &mut Criterion) {
    let rt = rt();
    let pool = rt.block_on(connect());
    rt.block_on(seed(&pool));

    c.bench_function("db/get_events_ledger_range", |b| {
        b.iter(|| {
            rt.block_on(async {
                let rows = sqlx::query(
                    "SELECT id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, created_at \
                     FROM events WHERE ledger >= $1 AND ledger <= $2 \
                     ORDER BY ledger DESC, id DESC LIMIT $3 OFFSET $4",
                )
                .bind(black_box(200_i64))
                .bind(black_box(400_i64))
                .bind(black_box(20_i64))
                .bind(black_box(0_i64))
                .fetch_all(&pool)
                .await
                .expect("query failed");
                black_box(rows.len())
            })
        });
    });
}

fn bench_get_events_exact_count(c: &mut Criterion) {
    let rt = rt();
    let pool = rt.block_on(connect());
    rt.block_on(seed(&pool));

    c.bench_function("db/get_events_exact_count", |b| {
        b.iter(|| {
            rt.block_on(async {
                let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
                    .fetch_one(&pool)
                    .await
                    .expect("count query failed");
                black_box(count)
            })
        });
    });
}

fn bench_get_events_by_contract(c: &mut Criterion) {
    let rt = rt();
    let pool = rt.block_on(connect());
    rt.block_on(seed(&pool));

    c.bench_function("db/get_events_by_contract", |b| {
        b.iter(|| {
            rt.block_on(async {
                let rows = sqlx::query(
                    "SELECT id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, created_at \
                     FROM events WHERE contract_id = $1 ORDER BY ledger DESC LIMIT $2 OFFSET $3",
                )
                .bind(black_box(HOT_CONTRACT_ID))
                .bind(black_box(20_i64))
                .bind(black_box(0_i64))
                .fetch_all(&pool)
                .await
                .expect("query failed");
                black_box(rows.len())
            })
        });
    });
}

criterion_group!(
    db_benches,
    bench_get_events_no_filter,
    bench_get_events_ledger_range,
    bench_get_events_exact_count,
    bench_get_events_by_contract,
);
criterion_main!(db_benches);
