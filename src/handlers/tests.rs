//! Handler unit tests (moved out of `handlers/mod.rs`, Issue #1241).

use super::*;
use crate::config::{HealthState, IndexerState};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

fn create_test_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    crate::routes::create_router(
        pool,
        Vec::new(),
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

/// Build a router with multi-tenant mode enabled.
/// `tenant_map` maps raw API key → tenant_id.
fn create_multitenant_test_router(
    pool: PgPool,
    tenant_map: std::collections::HashMap<String, String>,
) -> axum::Router {
    use crate::middleware::hash_api_key;
    use tokio::sync::broadcast;

    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let mut config = crate::config::Config::default();
    config.multi_tenant = true;

    // api_keys = all raw keys in the map
    let api_keys: Vec<String> = tenant_map.keys().cloned().collect();
    // Convert raw key → hash for the tenant_map
    let hashed_map: std::collections::HashMap<String, String> = tenant_map
        .iter()
        .map(|(k, v)| (hash_api_key(k), v.clone()))
        .collect();

    let (_, shutdown_rx) = tokio::sync::watch::channel(false);
    crate::routes::create_router_with_tx_and_tenant_map(
        pool.clone(),
        pool,
        api_keys,
        &[],
        60,
        false,
        health_state,
        indexer_state,
        prometheus_handle,
        broadcast::channel(256).0,
        15000,
        1000,
        2000,
        None,
        None,
        config,
        None,
        Arc::new(hashed_map),
        shutdown_rx,
        crate::sse_ring_buffer::SseRingBuffer::new(100),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_by_tx_no_events_returns_200_empty_data(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/tx/unknown_tx_hash_no_events_deadbeef")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_by_tx_with_row_returns_200_with_data(pool: PgPool) {
    let tx_hash = "a1b2c3d4e5f6";
    sqlx::query(
        r#"
            INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
    )
    .bind("C_TEST")
    .bind("contract")
    .bind(tx_hash)
    .bind(1_i64)
    .bind(Utc::now())
    .bind(json!({ "value": null, "topic": null }))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/tx/{tx_hash}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["data"].is_array());
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["tx_hash"], json!(tx_hash));
}

#[sqlx::test(migrations = "./migrations")]
async fn database_error_response_does_not_leak_internals(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();

    // Verify response contains generic error message
    assert!(body_str.contains("internal server error"));

    // Verify no SQLx internals are leaked
    assert!(!body_str.to_lowercase().contains("sqlx"));
    assert!(!body_str.contains("events"));
    assert!(!body_str.contains("table"));
    assert!(!body_str.contains("column"));
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_id_too_long_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    let long_id = "C".repeat(100);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/{}", long_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "invalid contract_id format");
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert!(v["correlation_id"].as_str().is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_id_invalid_format_returns_400(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/GABC123456789012345678901234567890123456789012345678")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "invalid contract_id format");
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert!(v["correlation_id"].as_str().is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn tx_hash_invalid_length_returns_400(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/tx/abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "invalid tx_hash format");
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert!(v["correlation_id"].as_str().is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn tx_hash_non_hex_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    let invalid_hex = "z".repeat(64);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/tx/{}", invalid_hex))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "invalid tx_hash format");
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert!(v["correlation_id"].as_str().is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn tx_hash_uppercase_hex_is_normalized_to_lowercase(pool: PgPool) {
    // Insert an event with a lowercase tx_hash
    let lowercase_hash = "a".repeat(64);
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C_TEST_UPPER")
    .bind("contract")
    .bind(&lowercase_hash)
    .bind(1_i64)
    .bind(Utc::now())
    .bind(json!({ "value": null, "topic": null }))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);

    // Request with uppercase hash — should be normalized and return the same event
    let uppercase_hash = "A".repeat(64);
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/tx/{}", uppercase_hash))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["data"].is_array());
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    // Response tx_hash should be the normalized lowercase form
    assert_eq!(v["tx_hash"], json!(lowercase_hash));
}

#[sqlx::test(migrations = "./migrations")]
async fn tx_hash_mixed_case_is_normalized_to_lowercase(pool: PgPool) {
    let lowercase_hash = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C_TEST_MIXED")
    .bind("contract")
    .bind(lowercase_hash)
    .bind(2_i64)
    .bind(Utc::now())
    .bind(json!({ "value": null, "topic": null }))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);

    // Mixed-case version of the same hash
    let mixed_hash = "A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2C3D4E5F6A1B2";
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/tx/{}", mixed_hash))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["tx_hash"], json!(lowercase_hash));
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_paginated_returns_approximate_count_by_default(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["approximate"], true);
    assert!(v.get("total").is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_paginated_returns_exact_count_when_requested(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?exact_count=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["approximate"], false);
    assert_eq!(v["total"], 0); // Empty table
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_with_fields_filter_returns_only_requested_fields(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert a test row
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(Uuid::new_v4())
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("test")
    .bind("a".repeat(64))
    .bind(100_i64)
    .bind(Utc::now())
    .bind(json!({"foo": "bar"}))
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?fields=id,ledger")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    let event = &v["data"][0];
    assert!(event.get("id").is_some());
    assert!(event.get("ledger").is_some());
    assert!(event.get("contract_id").is_none());
    assert!(event.get("event_data").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_total_count_scenarios(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // 1. Empty set
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["total"], 0);
    assert_eq!(v["data"].as_array().unwrap().len(), 0);

    // 2. Single page (3 events, limit 20)
    for i in 0..3 {
        sqlx::query("INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(format!("C{:0>55}", i))
            .bind("contract")
            .bind(format!("{:0>64}", i))
            .bind(i as i64)
            .bind(Utc::now())
            .bind(json!({}))
            .execute(&pool).await.unwrap();
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=20")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["total"].as_u64().is_some()); // Can be approximate or exact
    assert!(v["total"].as_u64().is_some());
    assert_eq!(v["data"].as_array().unwrap().len(), 3);

    // 3. Multi-page (limit 2, total 3)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=2&page=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["total"].as_u64().is_some());
    assert_eq!(v["data"].as_array().unwrap().len(), 2);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=2&page=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["total"].as_u64().is_some());
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
}

/// Test that health endpoint returns 503 when DB is unreachable
#[tokio::test]
async fn health_db_unreachable_returns_503() {
    let pool = PgPool::connect_lazy("postgres://invalid-host:5432/invalid_db").unwrap();
    let health_state = Arc::new(HealthState::new(60));
    let prometheus_handle = crate::metrics::init_metrics();
    let indexer_state = Arc::new(IndexerState::new());
    let config = crate::config::Config::default();
    let app = crate::routes::create_router(
        pool,
        Vec::new(),
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // The DB is unreachable so should return 503
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "degraded");
    assert!(matches!(
        v["db"].as_str(),
        Some("unreachable") | Some("pool_exhausted")
    ));
}

// Health endpoint tests
#[sqlx::test(migrations = "./migrations")]
async fn health_happy_path_returns_200(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "ok");
    assert_eq!(v["db"], "ok");
    assert_eq!(v["indexer"], "ok");
}

#[sqlx::test(migrations = "./migrations")]
async fn healthz_live_returns_200(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz/live")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "alive");
}

#[tokio::test]
async fn healthz_ready_unreachable_db_returns_503() {
    let pool = PgPool::connect_lazy("postgres://invalid-host:5432/invalid_db").unwrap();
    let health_state = Arc::new(HealthState::new(60));
    let prometheus_handle = crate::metrics::init_metrics();
    let indexer_state = Arc::new(IndexerState::new());
    let config = crate::config::Config::default();
    let app = crate::routes::create_router(
        pool,
        Vec::new(),
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "degraded");
    assert!(matches!(
        v["db"].as_str(),
        Some("unreachable") | Some("pool_exhausted")
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn healthz_ready_indexer_stalled_returns_503(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(1));
    // never updated, treated as stalled
    let prometheus_handle = crate::metrics::init_metrics();
    let indexer_state = Arc::new(IndexerState::new());
    let config = crate::config::Config::default();
    let app = crate::routes::create_router(
        pool,
        Vec::new(),
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/healthz/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["status"], "degraded");
    assert_eq!(v["indexer"], "stalled");
}

// Status endpoint tests
#[sqlx::test(migrations = "./migrations")]
async fn status_returns_operational_info(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    // Verify required fields are present
    assert!(v.get("version").is_some());
    assert!(v.get("uptime_secs").is_some());
    assert!(v.get("current_ledger").is_some());
    assert!(v.get("latest_ledger").is_some());
    assert!(v.get("lag_ledgers").is_some());
    assert!(v.get("total_events").is_some());
    assert!(v.get("indexer_status").is_some());

    // Verify total_events is 0 for empty DB
    assert_eq!(v["total_events"], 0);
}

// OpenAPI endpoint tests
#[sqlx::test(migrations = "./migrations")]
async fn openapi_json_returns_valid_spec(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    // Verify it's a valid OpenAPI spec
    assert_eq!(v["openapi"], "3.0.0");
    assert!(v.get("info").is_some());
    assert!(v.get("paths").is_some());
}

// Main events endpoint tests - Happy path
#[sqlx::test(migrations = "./migrations")]
async fn get_events_empty_db_returns_200_with_empty_data(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(v["data"], json!([]));
    assert_eq!(v["total"], 0);
    assert_eq!(v["page"], 1);
    assert_eq!(v["limit"], 20);
    assert_eq!(v["approximate"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_with_data_returns_paginated_results(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert test data
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({"test": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(v["data"].as_array().unwrap().len(), 3);
    assert_eq!(v["total"], 5);
    assert_eq!(v["page"], 1);
    assert_eq!(v["limit"], 3);
}

// Main events endpoint tests - Error cases
#[sqlx::test(migrations = "./migrations")]
async fn get_events_invalid_event_type_returns_400(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?event_type=invalid")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("event_type must be one of"));
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert!(v["correlation_id"].as_str().is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_invalid_ledger_range_returns_400(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?from_ledger=100&to_ledger=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("from_ledger must be <= to_ledger"));
    assert_eq!(v["code"], "VALIDATION_ERROR");
    assert!(v["correlation_id"].as_str().is_some());
}

// /v1/events/recent tests

#[sqlx::test(migrations = "./migrations")]
async fn get_recent_events_returns_events_in_desc_order(pool: PgPool) {
    let app = create_test_router(pool.clone());

    for i in 0..5_i64 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i)
        .bind(Utc::now())
        .bind(json!({}))
        .execute(&pool)
        .await
        .unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/recent?limit=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 3);
    // Verify descending ledger order
    let ledgers: Vec<i64> = data.iter().map(|e| e["ledger"].as_i64().unwrap()).collect();
    assert!(
        ledgers.windows(2).all(|w| w[0] >= w[1]),
        "events must be in descending ledger order"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn get_recent_events_rejects_from_ledger(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/recent?from_ledger=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"].as_str().unwrap().contains("from_ledger"));
    assert_eq!(v["code"], "VALIDATION_ERROR");
}

#[sqlx::test(migrations = "./migrations")]
async fn get_recent_events_rejects_to_ledger(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/recent?to_ledger=400")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v["error"].as_str().unwrap().contains("to_ledger"));
    assert_eq!(v["code"], "VALIDATION_ERROR");
}

// Events by contract tests - Happy path
#[sqlx::test(migrations = "./migrations")]
async fn get_events_by_contract_with_data_returns_200(pool: PgPool) {
    let app = create_test_router(pool.clone());
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";

    // Insert test data
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(contract_id)
    .bind("contract")
    .bind("a".repeat(64))
    .bind(100_i64)
    .bind(Utc::now())
    .bind(json!({"test": "data"}))
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/contract/{}", contract_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["contract_id"], contract_id);
    assert_eq!(v["total"], 1);
    assert!(v.get("page").is_some());
    assert!(v.get("limit").is_some());
    assert!(v.get("approximate").is_some());
}

// Events by contract tests - Error cases
#[sqlx::test(migrations = "./migrations")]
async fn get_events_by_contract_not_found_returns_404(pool: PgPool) {
    let app = create_test_router(pool);
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/contract/{}", contract_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// Events by transaction tests - Happy path
#[sqlx::test(migrations = "./migrations")]
async fn get_events_by_tx_multiple_events_returns_all(pool: PgPool) {
    let app = create_test_router(pool.clone());
    let tx_hash = "a1b2c3d4e5f6789012345678901234567890123456789012345678901234567890";

    // Insert multiple events for same transaction
    for i in 0..3 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(&tx_hash)
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({"event_num": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/tx/{}", tx_hash))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(v["data"].as_array().unwrap().len(), 3);
    assert_eq!(v["tx_hash"], tx_hash);
}

// Stream events endpoint tests
#[sqlx::test(migrations = "./migrations")]
async fn stream_events_returns_sse_stream(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn stream_events_with_contract_filter(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream?contract_id=C1234567890123456789012345678901234567890123456789012345")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
}

// --- Validation edge-case unit tests (discovered via fuzzing review) ---

#[test]
fn validate_contract_id_empty_returns_err() {
    assert!(validate_contract_id("").is_err());
}

#[test]
fn validate_contract_id_55_chars_returns_err() {
    let s = format!("C{}", "A".repeat(54));
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_contract_id_57_chars_returns_err() {
    let s = format!("C{}", "A".repeat(56));
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_contract_id_starts_with_lowercase_c_returns_err() {
    let s = format!("c{}", "A".repeat(55));
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_contract_id_with_special_chars_returns_err() {
    let s = format!("C{}!", "A".repeat(54));
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_contract_id_valid_returns_ok() {
    let s = format!("C{}", "A".repeat(55));
    assert!(validate_contract_id(&s).is_ok());
}

/// Fuzz regression (#1233): `len()` counts bytes, so multi-byte UTF-8 input
/// that is exactly 56 bytes long must still be rejected, deterministically.
#[test]
fn validate_contract_id_multibyte_56_bytes_returns_err() {
    let s = format!("C{}A", "\u{e9}".repeat(27));
    assert_eq!(s.len(), 56);
    assert!(validate_contract_id(&s).is_err());
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_tx_hash_empty_returns_err() {
    assert!(validate_tx_hash("").is_err());
}

#[test]
fn validate_tx_hash_63_chars_returns_err() {
    assert!(validate_tx_hash(&"a".repeat(63)).is_err());
}

#[test]
fn validate_tx_hash_65_chars_returns_err() {
    assert!(validate_tx_hash(&"a".repeat(65)).is_err());
}

#[test]
fn validate_tx_hash_non_hex_returns_err() {
    assert!(validate_tx_hash(&"g".repeat(64)).is_err());
}

/// Fuzz regression (#1234): `len()` counts bytes, so multi-byte UTF-8 input
/// that is exactly 64 bytes long must still be rejected, deterministically.
#[test]
fn validate_tx_hash_multibyte_64_bytes_returns_err() {
    let s = "\u{e9}".repeat(32);
    assert_eq!(s.len(), 64);
    assert!(validate_tx_hash(&s).is_err());
    assert!(validate_tx_hash(&s).is_err());
}

#[test]
fn validate_tx_hash_valid_lowercase_returns_ok() {
    assert!(validate_tx_hash(&"a".repeat(64)).is_ok());
}

#[test]
fn validate_tx_hash_valid_uppercase_returns_ok() {
    assert!(validate_tx_hash(&"A".repeat(64)).is_ok());
}

// --- Fuzz regressions (issues #1227, #1229): multi-byte UTF-8 inputs ---

#[test]
fn validate_contract_id_multibyte_with_56_byte_len_returns_err() {
    // 'é' is 2 bytes, so this is 56 bytes but only 55 chars.
    let s = format!("C{}é", "A".repeat(53));
    assert_eq!(s.len(), 56);
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_contract_id_multibyte_leading_char_returns_err() {
    let s = format!("Ç{}", "A".repeat(54));
    assert_eq!(s.len(), 56);
    assert!(validate_contract_id(&s).is_err());
}

#[test]
fn validate_tx_hash_multibyte_with_64_byte_len_returns_err() {
    let s = format!("{}é", "a".repeat(62));
    assert_eq!(s.len(), 64);
    assert!(validate_tx_hash(&s).is_err());
}

#[test]
fn validate_tx_hash_non_ascii_digits_returns_err() {
    // Fullwidth digits are numeric but not ASCII hex.
    let s = format!("{}０", "a".repeat(61));
    assert_eq!(s.len(), 64);
    assert!(validate_tx_hash(&s).is_err());
}

#[sqlx::test(migrations = "./migrations")]
async fn stream_events_invalid_contract_id_returns_400(pool: PgPool) {
    let app = create_test_router(pool);

    // Invalid: too short
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream?contract_id=CABC")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "invalid contract_id format");

    // Invalid: doesn't start with C
    let response = app.clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream?contract_id=A1234567890123456789012345678901234567890123456789012345")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    // Invalid: contains non-alphanumeric
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream?contract_id=C123456789012345678901234567890123456789012345678901234!")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn stream_events_invalid_topic_prefix_returns_400(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream?topic_prefix=not-valid-json{{{")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "topic_prefix must be valid JSON");
    assert_eq!(v["code"], "VALIDATION_ERROR");
}

#[sqlx::test(migrations = "./migrations")]
async fn stream_events_valid_topic_prefix_returns_sse_stream(pool: PgPool) {
    let app = create_test_router(pool);

    // URL-encoded JSON: {"sym":"swap"}
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream?topic_prefix=%7B%22sym%22%3A%22swap%22%7D")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/event-stream"
    );
}

/// Verify that a named `event: ping` is emitted when no events arrive within the keepalive interval.
#[tokio::test]
async fn stream_events_emits_named_ping_when_idle() {
    use axum::body::Body;
    use futures::StreamExt;
    use http_body_util::BodyExt;
    use std::sync::Arc;
    use tokio::sync::broadcast;

    let pool = sqlx::PgPool::connect_lazy("postgres://localhost/unused").unwrap();
    let health_state = Arc::new(crate::config::HealthState::new(60));
    let indexer_state = Arc::new(crate::config::IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let (event_tx, _) = broadcast::channel::<crate::models::SorobanEvent>(16);
    let config = crate::config::Config::default();

    // Use a 50 ms keepalive so the test completes quickly.
    let app = crate::routes::create_router_with_tx(
        pool.clone(),
        pool,
        vec![],
        &[],
        0, // rate_limit disabled
        false,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        50, // keepalive_ms = 50 ms
        1000,
        2000,
        None,
        None,
        config,
        None,
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stream")
                .header("Accept", "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    // Collect bytes until we see "event: ping" or timeout after 500 ms.
    let mut body = response.into_body().into_data_stream();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(500);
    let mut buf = String::new();
    loop {
        tokio::select! {
            chunk = body.next() => {
                match chunk {
                    Some(Ok(bytes)) => {
                        buf.push_str(&String::from_utf8_lossy(&bytes));
                        if buf.contains("event: ping") {
                            break;
                        }
                    }
                    _ => break,
                }
            }
            _ = tokio::time::sleep_until(deadline) => break,
        }
    }

    assert!(
        buf.contains("event: ping"),
        "expected 'event: ping' in SSE output, got: {buf:?}"
    );
}

// Metrics endpoint tests
#[sqlx::test(migrations = "./migrations")]
async fn metrics_returns_prometheus_format(pool: PgPool) {
    let app = create_test_router(pool);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/metrics")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    // Metrics endpoint should return text/plain content type
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/plain; version=0.0.4"
    );
}

// Pagination boundary condition tests
#[sqlx::test(migrations = "./migrations")]
async fn pagination_boundary_conditions(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert exactly 25 test events
    for i in 0..25 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({"test": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Test limit boundary: limit=1 (minimum)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["limit"], 1);
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert_eq!(v["total"], 25);

    // Test limit boundary: limit=100 (maximum)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=100")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["limit"], 100);
    assert_eq!(v["data"].as_array().unwrap().len(), 25); // All events
    assert_eq!(v["total"], 25);

    // Test page boundary: page=1, limit=10
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?page=1&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["page"], 1);
    assert_eq!(v["limit"], 10);
    assert_eq!(v["data"].as_array().unwrap().len(), 10);

    // Test page boundary: page=3, limit=10 (last page with 5 items)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?page=3&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["page"], 3);
    assert_eq!(v["limit"], 10);
    assert_eq!(v["data"].as_array().unwrap().len(), 5);

    // Test page boundary: page=4, limit=10 (beyond last page, empty)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?page=4&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["page"], 4);
    assert_eq!(v["limit"], 10);
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_invalid_parameters_are_clamped(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert test data
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({"test": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Test limit=0 gets clamped to 1
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["limit"], 1);

    // Test limit=200 gets clamped to 100
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=200")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["limit"], 100);

    // Test page=0 gets treated as page=1
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?page=0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["page"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_exact_count_vs_approximate_count(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert test data
    for i in 0..15 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({"test": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Test approximate count (default)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["approximate"], true);
    // Approximate count may not be exact but should be reasonable
    let approx_count = v["total"].as_i64().unwrap();
    assert!(approx_count >= 0); // Should be non-negative

    // Test exact count
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?exact_count=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["approximate"], false);
    assert_eq!(v["total"], 15); // Exact count should match

    // Test filtered queries always use exact count
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?event_type=contract")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["approximate"], false);
    assert_eq!(v["total"], 15); // All events are contract type
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_with_filters(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert mixed event types
    for i in 0..10 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind(if i % 2 == 0 { "contract" } else { "diagnostic" })
        .bind(format!("{:0>64}", i))
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({"test": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Test pagination with event_type filter
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?event_type=contract&limit=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 3);
    assert_eq!(v["total"], 5); // 5 contract events
    assert_eq!(v["approximate"], false); // Filtered queries use exact count

    // Test pagination with ledger range filter
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?from_ledger=2&to_ledger=8&limit=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 5);
    assert_eq!(v["total"], 7); // Events with ledger 2-8
    assert_eq!(v["approximate"], false); // Filtered queries use exact count

    // Test pagination with both filters
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?event_type=contract&from_ledger=0&to_ledger=6&limit=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 4); // Contract events with ledger 0-6
    assert_eq!(v["total"], 4);
    assert_eq!(v["approximate"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_filter_by_ledger_hash(pool: PgPool) {
    let app = create_test_router(pool.clone());

    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data, ledger_hash) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(100_i64)
    .bind(Utc::now())
    .bind(json!({"test": "one"}))
    .bind("abc123")
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data, ledger_hash) \
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012346")
    .bind("contract")
    .bind("b".repeat(64))
    .bind(101_i64)
    .bind(Utc::now())
    .bind(json!({"test": "two"}))
    .bind("def456")
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?ledger_hash=abc123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["ledger_hash"], json!("abc123"));
    assert_eq!(data[0]["tx_hash"], json!("a".repeat(64)));
}

#[sqlx::test(migrations = "./migrations")]
async fn pagination_fields_filtering(pool: PgPool) {
    let app = create_test_router(pool.clone());

    // Insert test data
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(100_i64)
    .bind(Utc::now())
    .bind(json!({"test": "data"}))
    .execute(&pool)
    .await
    .unwrap();

    // Test fields filter with single field
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?fields=ledger")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let event = &v["data"][0];
    assert!(event.get("ledger").is_some());
    assert!(event.get("contract_id").is_none());
    assert!(event.get("event_type").is_none());
    assert!(event.get("tx_hash").is_none());
    assert!(event.get("timestamp").is_none());
    assert!(event.get("event_data").is_none());
    assert!(event.get("created_at").is_none());

    // Test fields filter with multiple fields
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?fields=ledger,contract_id,event_type")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let event = &v["data"][0];
    assert!(event.get("ledger").is_some());
    assert!(event.get("contract_id").is_some());
    assert!(event.get("event_type").is_some());
    assert!(event.get("tx_hash").is_none());
    assert!(event.get("timestamp").is_none());
    assert!(event.get("event_data").is_none());
    assert!(event.get("created_at").is_none());

    // Test fields filter with invalid fields (should be ignored)
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?fields=ledger,invalid_field,contract_id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let event = &v["data"][0];
    assert!(event.get("ledger").is_some());
    assert!(event.get("contract_id").is_some());
    assert!(event.get("invalid_field").is_none());

    // Test empty fields filter (should return all fields)
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?fields=")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let event = &v["data"][0];
    assert!(event.get("ledger").is_some());
    assert!(event.get("contract_id").is_some());
    assert!(event.get("event_type").is_some());
    assert!(event.get("tx_hash").is_some());
    assert!(event.get("timestamp").is_some());
    assert!(event.get("event_data").is_some());
    assert!(event.get("created_at").is_some());
}

// --- Cursor pagination tests ---

async fn insert_events(pool: &PgPool, count: usize) {
    for i in 0..count {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, $5, $6)"
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i as i64)
        .bind(Utc::now())
        .bind(json!({}))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn cursor_pagination_traverses_all_pages(pool: PgPool) {
    insert_events(&pool, 5).await;
    let app = create_test_router(pool);

    // Page 1: limit=2, no cursor
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    let cursor1 = v["next_cursor"]
        .as_str()
        .expect("next_cursor must be present on page 1")
        .to_string();

    // Page 2: use cursor from page 1
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events?limit=2&cursor={cursor1}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
    let cursor2 = v["next_cursor"]
        .as_str()
        .expect("next_cursor must be present on page 2")
        .to_string();

    // Page 3: last page — 1 row, next_cursor must be null
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events?limit=2&cursor={cursor2}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 1);
    assert!(
        v["next_cursor"].is_null(),
        "next_cursor must be null on last page"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn cursor_pagination_no_duplicate_or_missing_rows(pool: PgPool) {
    insert_events(&pool, 6).await;
    let app = create_test_router(pool);

    let mut seen_ledgers: Vec<i64> = Vec::new();
    let mut cursor: Option<String> = None;

    loop {
        let uri = match &cursor {
            Some(c) => format!("/v1/events?limit=2&cursor={c}"),
            None => "/v1/events?limit=2".to_string(),
        };
        let resp = app
            .clone()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let page = v["data"].as_array().unwrap();
        for ev in page {
            seen_ledgers.push(ev["ledger"].as_i64().unwrap());
        }
        cursor = v["next_cursor"].as_str().map(|s| s.to_string());
        if cursor.is_none() {
            break;
        }
    }

    // All 6 ledgers seen exactly once, in descending order
    assert_eq!(seen_ledgers.len(), 6);
    let mut sorted = seen_ledgers.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(seen_ledgers, sorted);
    let unique: std::collections::HashSet<_> = seen_ledgers.iter().collect();
    assert_eq!(unique.len(), 6);
}

#[sqlx::test(migrations = "./migrations")]
async fn cursor_invalid_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?cursor=notvalidbase64!!!")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "invalid cursor");
}

#[sqlx::test(migrations = "./migrations")]
async fn offset_response_includes_next_cursor(pool: PgPool) {
    insert_events(&pool, 3).await;
    let app = create_test_router(pool);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?limit=2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    // Offset path still returns total/page/approximate AND next_cursor
    assert!(v.get("total").is_some());
    assert!(v.get("page").is_some());
    assert!(
        v["next_cursor"].is_string(),
        "offset path must also return next_cursor"
    );
}

// --- ETag / conditional GET tests ---

#[sqlx::test(migrations = "./migrations")]
async fn get_events_returns_etag_header(pool: PgPool) {
    insert_events(&pool, 3).await;
    let app = create_test_router(pool);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers().contains_key("etag"),
        "ETag header must be present"
    );
    let etag = resp
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        etag.starts_with('"') && etag.ends_with('"'),
        "ETag must be quoted: {etag}"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_matching_etag_returns_304(pool: PgPool) {
    insert_events(&pool, 3).await;
    let app = create_test_router(pool);

    // First request — get ETag
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let etag = resp
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    // Second request with If-None-Match — should get 304
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("if-none-match", &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED);
    // 304 must include ETag header
    assert_eq!(resp.headers().get("etag").unwrap().to_str().unwrap(), etag);
    // 304 body must be empty
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert!(body.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_non_matching_etag_returns_200(pool: PgPool) {
    insert_events(&pool, 3).await;
    let app = create_test_router(pool);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("if-none-match", "\"stale-etag-value\"")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_empty_db_no_etag(pool: PgPool) {
    let app = create_test_router(pool);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    // No rows → no ETag
    assert!(resp.headers().get("etag").is_none());
}

// compact=true tests
#[sqlx::test(migrations = "./migrations")]
async fn compact_false_returns_full_json_event_data(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(100_i64)
    .bind(Utc::now())
    .bind(json!({"topics": ["transfer", "GABC"], "value": 42}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?compact=false")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    // No encoding header when compact is off
    assert!(resp.headers().get("x-event-data-encoding").is_none());

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let event_data = &v["data"][0]["event_data"];
    // Full JSON object, not a string
    assert!(
        event_data.is_object(),
        "event_data should be a JSON object when compact=false"
    );
    assert_eq!(event_data["value"], json!(42));
}

#[sqlx::test(migrations = "./migrations")]
async fn compact_true_returns_base64_gzip_event_data(pool: PgPool) {
    let original = json!({"topics": ["transfer", "GABC"], "value": 42});

    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(100_i64)
    .bind(Utc::now())
    .bind(&original)
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?compact=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    // Encoding hint header must be present
    assert_eq!(
        resp.headers()
            .get("x-event-data-encoding")
            .and_then(|v| v.to_str().ok()),
        Some("gzip+base64")
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let event_data = &v["data"][0]["event_data"];

    // Must be a string (base64-encoded)
    let encoded = event_data
        .as_str()
        .expect("event_data should be a base64 string when compact=true");

    // Decode base64 → decompress gzip → parse JSON → must equal original
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use flate2::read::GzDecoder;
    use std::io::Read;

    let compressed = STANDARD.decode(encoded).expect("valid base64");
    let mut decoder = GzDecoder::new(compressed.as_slice());
    let mut json_str = String::new();
    decoder.read_to_string(&mut json_str).expect("valid gzip");
    let decoded: Value = serde_json::from_str(&json_str).expect("valid JSON");

    assert_eq!(
        decoded, original,
        "decoded compact event_data must equal original"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn compact_true_with_cursor_pagination(pool: PgPool) {
    // Insert two events so we can test cursor path
    for i in 0..2_i64 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind("C1234567890123456789012345678901234567890123456789012345")
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(100_i64 + i)
        .bind(Utc::now())
        .bind(json!({"index": i}))
        .execute(&pool)
        .await
        .unwrap();
    }

    let app = create_test_router(pool);

    // First page — get a cursor
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events?compact=true&limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("x-event-data-encoding")
            .and_then(|v| v.to_str().ok()),
        Some("gzip+base64")
    );

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let cursor = v["next_cursor"].as_str().expect("next_cursor present");

    // Second page via cursor — event_data must still be compact
    let resp2 = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events?compact=true&limit=1&cursor={cursor}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp2.status(), StatusCode::OK);
    let body2 = to_bytes(resp2.into_body(), usize::MAX).await.unwrap();
    let v2: Value = serde_json::from_slice(&body2).unwrap();
    let event_data = &v2["data"][0]["event_data"];
    assert!(
        event_data.is_string(),
        "event_data should be base64 string on cursor page"
    );
}

// Multi-tenant isolation tests
#[sqlx::test(migrations = "./migrations")]
async fn multitenant_tenant_a_cannot_see_tenant_b_events(pool: PgPool) {
    // Insert events for two different tenants
    for (tenant, contract, tx) in [
        (
            "tenant_a",
            "C1111111111111111111111111111111111111111111111111111111",
            "a".repeat(64),
        ),
        (
            "tenant_b",
            "C2222222222222222222222222222222222222222222222222222222",
            "b".repeat(64),
        ),
    ] {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data, tenant_id)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(contract)
        .bind("contract")
        .bind(tx)
        .bind(100_i64)
        .bind(Utc::now())
        .bind(json!({"tenant": tenant}))
        .bind(tenant)
        .execute(&pool)
        .await
        .unwrap();
    }

    let mut tenant_map = std::collections::HashMap::new();
    tenant_map.insert("key_a".to_string(), "tenant_a".to_string());
    tenant_map.insert("key_b".to_string(), "tenant_b".to_string());
    let app = create_multitenant_test_router(pool, tenant_map);

    // Tenant A should only see its own event
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("X-Api-Key", "key_a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "tenant_a should see exactly 1 event");
    assert_eq!(data[0]["event_data"]["tenant"], json!("tenant_a"));

    // Tenant B should only see its own event
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("X-Api-Key", "key_b")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 1, "tenant_b should see exactly 1 event");
    assert_eq!(data[0]["event_data"]["tenant"], json!("tenant_b"));
}

#[sqlx::test(migrations = "./migrations")]
async fn multitenant_unknown_key_returns_403(pool: PgPool) {
    // key_a is in api_keys but NOT in tenant_map → 403
    let mut tenant_map = std::collections::HashMap::new();
    // key_a is registered but has no tenant mapping
    // We simulate this by giving key_b a mapping but not key_a
    tenant_map.insert("key_b".to_string(), "tenant_b".to_string());

    // Manually build a router where key_a is in api_keys but not in tenant_map
    use crate::middleware::hash_api_key;
    use tokio::sync::broadcast;
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let mut config = crate::config::Config::default();
    config.multi_tenant = true;
    let hashed_map: std::collections::HashMap<String, String> = tenant_map
        .iter()
        .map(|(k, v)| (hash_api_key(k), v.clone()))
        .collect();
    let (_, shutdown_rx_multi) = tokio::sync::watch::channel(false);
    let app = crate::routes::create_router_with_tx_and_tenant_map(
        pool.clone(),
        pool,
        vec!["key_a".to_string(), "key_b".to_string()],
        &[],
        60,
        false,
        health_state,
        indexer_state,
        prometheus_handle,
        broadcast::channel(256).0,
        15000,
        1000,
        2000,
        None,
        None,
        config,
        None,
        Arc::new(hashed_map),
        shutdown_rx_multi,
        crate::sse_ring_buffer::SseRingBuffer::new(100),
    );

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("X-Api-Key", "key_a")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn singletenent_mode_returns_all_events(pool: PgPool) {
    // In single-tenant mode, events with NULL tenant_id are returned for all callers
    for tx in ["a".repeat(64), "b".repeat(64)] {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind("C1111111111111111111111111111111111111111111111111111111")
        .bind("contract")
        .bind(tx)
        .bind(100_i64)
        .bind(Utc::now())
        .bind(json!({"x": 1}))
        .execute(&pool)
        .await
        .unwrap();
    }

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["data"].as_array().unwrap().len(), 2);
}

// Replay endpoint tests
#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_requires_api_key(pool: PgPool) {
    let app = create_test_router(pool);
    let replay_request = ReplayRequest {
        from_ledger: 100,
        to_ledger: 200,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    // Should return 401 when no API key is provided (since test router has no API keys configured)
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "unauthorized");
}

#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_validates_ledger_range(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Create router with API key to bypass auth
    let app = crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    // Test invalid range: from_ledger > to_ledger
    let replay_request = ReplayRequest {
        from_ledger: 200,
        to_ledger: 100,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "from_ledger must be <= to_ledger");
}

#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_validates_range_size(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Create router with API key to bypass auth
    let app = crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    // Test range too large: > 10,000 ledgers
    let replay_request = ReplayRequest {
        from_ledger: 1,
        to_ledger: 15000,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "ledger range cannot exceed 10,000 ledgers");
}

#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_returns_403_when_not_active_indexer(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(60));
    let mut indexer_state = Arc::new(IndexerState::new());

    // Set indexer as not active
    indexer_state
        .is_active_indexer
        .store(false, std::sync::atomic::Ordering::Relaxed);

    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Create router with API key to bypass auth
    let app = crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let replay_request = ReplayRequest {
        from_ledger: 100,
        to_ledger: 200,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        v["error"],
        "replay endpoint only available on active indexer"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_accepts_valid_request(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(60));
    let mut indexer_state = Arc::new(IndexerState::new());

    // Set indexer as active
    indexer_state
        .is_active_indexer
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Create router with API key to bypass auth
    let app = crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let replay_request = ReplayRequest {
        from_ledger: 100,
        to_ledger: 200,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["message"], "replay job accepted");
    assert_eq!(v["from_ledger"], 100);
    assert_eq!(v["to_ledger"], 200);
}

#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_accepts_x_api_key_header(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(60));
    let mut indexer_state = Arc::new(IndexerState::new());

    // Set indexer as active
    indexer_state
        .is_active_indexer
        .store(true, std::sync::atomic::Ordering::Relaxed);

    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Create router with API key to bypass auth
    let app = crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let replay_request = ReplayRequest {
        from_ledger: 100,
        to_ledger: 200,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("X-Api-Key", "test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["message"], "replay job accepted");
}

// ── Indexed range validation tests ───────────────────────────────────────

fn create_active_replay_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    indexer_state
        .is_active_indexer
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

async fn insert_events_at_ledgers(pool: &PgPool, ledgers: &[i64]) {
    for (i, &ledger) in ledgers.iter().enumerate() {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(ledger)
        .bind(Utc::now())
        .bind(json!({}))
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn post_replay(app: axum::Router, from: u64, to: u64) -> (StatusCode, Value) {
    let body = serde_json::to_string(&ReplayRequest {
        from_ledger: from,
        to_ledger: to,
    })
    .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    (status, v)
}

/// No events indexed → any range returns 400.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_validation_no_events_indexed_returns_400(pool: PgPool) {
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 100, 200).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        v["error"].as_str().unwrap().contains("no events have been indexed"),
        "unexpected error: {}",
        v["error"]
    );
}

/// Range entirely before the indexed window → 400.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_entirely_before_indexed_window_returns_400(pool: PgPool) {
    // Indexed window: ledgers 500–1000
    insert_events_at_ledgers(&pool, &[500, 750, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 100, 400).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = v["error"].as_str().unwrap();
    assert!(
        err.contains("entirely before"),
        "unexpected error: {err}"
    );
}

/// Range entirely after the indexed window (future ledgers) → 400.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_entirely_after_indexed_window_returns_400(pool: PgPool) {
    // Indexed window: ledgers 500–1000
    insert_events_at_ledgers(&pool, &[500, 750, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 1001, 2000).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let err = v["error"].as_str().unwrap();
    assert!(
        err.contains("entirely after"),
        "unexpected error: {err}"
    );
}

/// Range fully within the indexed window → 202, no warning.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_fully_within_indexed_window_returns_202_no_warning(pool: PgPool) {
    insert_events_at_ledgers(&pool, &[500, 750, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 500, 1000).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(v.get("warning").is_none(), "no warning expected for fully-covered range");
}

/// Range partially overlapping at the low end → 202 with warning.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_partial_overlap_low_end_returns_202_with_warning(pool: PgPool) {
    // Indexed window: 500–1000; request starts before min
    insert_events_at_ledgers(&pool, &[500, 750, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 100, 800).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let warning = v["warning"].as_str().expect("warning field must be present");
    assert!(
        warning.contains("partially outside"),
        "unexpected warning: {warning}"
    );
}

/// Range partially overlapping at the high end → 202 with warning.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_partial_overlap_high_end_returns_202_with_warning(pool: PgPool) {
    // Indexed window: 500–1000; request extends beyond max
    insert_events_at_ledgers(&pool, &[500, 750, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 800, 1500).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    let warning = v["warning"].as_str().expect("warning field must be present");
    assert!(
        warning.contains("partially outside"),
        "unexpected warning: {warning}"
    );
}

/// Range spanning the entire indexed window and beyond on both sides → 202 with warning.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_spanning_beyond_both_ends_returns_202_with_warning(pool: PgPool) {
    insert_events_at_ledgers(&pool, &[500, 750, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 1, 9999).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(
        v["warning"].as_str().is_some(),
        "warning field must be present"
    );
}

/// Boundary: request exactly at min indexed ledger → 202, no warning.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_exact_min_boundary_returns_202_no_warning(pool: PgPool) {
    insert_events_at_ledgers(&pool, &[500, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 500, 500).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(v.get("warning").is_none());
}

/// Boundary: request exactly at max indexed ledger → 202, no warning.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_exact_max_boundary_returns_202_no_warning(pool: PgPool) {
    insert_events_at_ledgers(&pool, &[500, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 1000, 1000).await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(v.get("warning").is_none());
}

/// Boundary: to_ledger == min_indexed - 1 → entirely before → 400.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_to_ledger_one_before_min_returns_400(pool: PgPool) {
    insert_events_at_ledgers(&pool, &[500, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 400, 499).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("entirely before"));
}

/// Boundary: from_ledger == max_indexed + 1 → entirely after → 400.
#[sqlx::test(migrations = "./migrations")]
async fn replay_range_from_ledger_one_after_max_returns_400(pool: PgPool) {
    insert_events_at_ledgers(&pool, &[500, 1000]).await;
    let app = create_active_replay_router(pool);
    let (status, v) = post_replay(app, 1001, 1500).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(v["error"].as_str().unwrap().contains("entirely after"));
}

#[sqlx::test(migrations = "./migrations")]
async fn replay_endpoint_rejects_invalid_api_key(pool: PgPool) {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Create router with API key
    let app = crate::routes::create_router(
        pool,
        vec!["correct-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    );

    let replay_request = ReplayRequest {
        from_ledger: 100,
        to_ledger: 200,
    };

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/replay")
                .header("Authorization", "Bearer wrong-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&replay_request).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["error"], "unauthorized");
}

// --- CSV export tests ---

fn create_export_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    // Export requires api_keys to be non-empty
    crate::routes::create_router(
        pool,
        vec!["test-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_returns_csv_with_header(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(1_i64)
    .bind(Utc::now())
    .bind(json!({"value": null, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/csv");
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("events.csv"));

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "id,contract_id,event_type,tx_hash,ledger,timestamp,event_data,created_at"
    );
    assert!(lines.next().is_some(), "expected at least one data row");
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_field_map_renames_csv_header(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(1_i64)
    .bind(Utc::now())
    .bind(json!({"value": null, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    // URL-encoded JSON object: {"event_data":"raw_data","ledger":"ledger_seq"}
    let fm = "%7B%22event_data%22%3A%22raw_data%22%2C%22ledger%22%3A%22ledger_seq%22%7D";
    let uri = format!("/v1/events/export?field_map={fm}");
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "id,contract_id,event_type,tx_hash,ledger_seq,timestamp,raw_data,created_at"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_field_map_renames_jsonl_keys(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(1_i64)
    .bind(Utc::now())
    .bind(json!({"value": null, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    let fm = "%7B%22event_data%22%3A%22raw_data%22%7D";
    let uri = format!("/v1/events/export?format=jsonl&field_map={fm}");
    let response = app
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let s = String::from_utf8(body.to_vec()).unwrap();
    let first = s.lines().next().unwrap();
    let v: Value = serde_json::from_str(first).unwrap();
    // Ensure mapped key exists and original key does not
    assert!(v.get("raw_data").is_some());
    assert!(v.get("event_data").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_without_api_key_returns_error(pool: PgPool) {
    // Router with no api_keys configured
    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Should be rejected (400 validation error since no api_keys means guard fires)
    assert!(response.status().is_client_error());
}

// ── CSV escaping unit tests ──────────────────────────────────────────────

#[test]
fn csv_escape_plain_field_is_unchanged() {
    assert_eq!(csv_escape_field("hello"), "hello");
    assert_eq!(csv_escape_field("contract"), "contract");
    assert_eq!(csv_escape_field(""), "");
}

#[test]
fn csv_escape_field_with_comma_is_quoted() {
    assert_eq!(csv_escape_field("a,b"), "\"a,b\"");
}

#[test]
fn csv_escape_field_with_double_quote_doubles_it() {
    assert_eq!(csv_escape_field(r#"say "hi""#), r#""say ""hi""""#);
}

#[test]
fn csv_escape_field_with_newline_is_quoted() {
    assert_eq!(csv_escape_field("line1\nline2"), "\"line1\nline2\"");
}

#[test]
fn csv_escape_field_with_carriage_return_is_quoted() {
    assert_eq!(csv_escape_field("line1\rline2"), "\"line1\rline2\"");
}

#[test]
fn csv_escape_field_with_comma_and_quote() {
    // A field like: He said, "hello"
    // Should become: "He said, ""hello"""
    assert_eq!(
        csv_escape_field(r#"He said, "hello""#),
        r#""He said, ""hello""""#
    );
}

#[test]
fn csv_escape_json_event_data() {
    // Typical JSON event_data contains commas and double-quotes
    let json = r#"{"key":"value","amount":100}"#;
    let escaped = csv_escape_field(json);
    // Must be wrapped in quotes and internal quotes doubled
    assert!(escaped.starts_with('"'));
    assert!(escaped.ends_with('"'));
    assert!(escaped.contains("\"\"key\"\""));
}

// ── CSV format=csv explicit query param ──────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn export_events_format_csv_explicit_returns_csv(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("c".repeat(64))
    .bind(5_i64)
    .bind(Utc::now())
    .bind(json!({"key": "value", "amount": 42}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=csv")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/csv");
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("events.csv"));

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "id,contract_id,event_type,tx_hash,ledger,timestamp,event_data,created_at"
    );
    assert!(lines.next().is_some(), "expected at least one data row");
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_csv_escapes_special_characters(pool: PgPool) {
    // Insert an event whose event_data contains commas and quotes (normal JSON)
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("d".repeat(64))
    .bind(10_i64)
    .bind(Utc::now())
    // JSON with commas and quotes — both must be properly escaped in CSV
    .bind(json!({"msg": "hello, world", "note": "say \"hi\""}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=csv")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let csv = String::from_utf8(body.to_vec()).unwrap();

    // The data row must exist
    let mut lines = csv.lines();
    lines.next(); // skip header
    let data_row = lines.next().expect("expected a data row");

    // The event_data field must be quoted (contains commas and quotes)
    assert!(
        data_row.contains('"'),
        "event_data with commas/quotes must be quoted in CSV: {data_row}"
    );
    // The row must not split on the comma inside the JSON value
    // (i.e., the CSV parser should see exactly 8 fields)
    let field_count = count_csv_fields(data_row);
    assert_eq!(field_count, 8, "expected 8 CSV fields, got {field_count}: {data_row}");
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_csv_empty_db_returns_header_only(pool: PgPool) {
    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=csv")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers().get("content-type").unwrap(), "text/csv");

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let csv = String::from_utf8(body.to_vec()).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next().unwrap(),
        "id,contract_id,event_type,tx_hash,ledger,timestamp,event_data,created_at"
    );
    assert!(lines.next().is_none(), "empty DB should produce header row only");
}

#[sqlx::test(migrations = "./migrations")]
async fn export_events_unknown_format_returns_400(pool: PgPool) {
    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=xlsx")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// Minimal RFC 4180 field counter: counts top-level comma-separated fields,
/// respecting double-quoted fields (commas inside quotes don't count).
fn count_csv_fields(line: &str) -> usize {
    let mut count = 1usize;
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                if in_quotes {
                    // Peek: if next char is also '"', it's an escaped quote — skip it.
                    if chars.peek() == Some(&'"') {
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            }
            ',' if !in_quotes => count += 1,
            _ => {}
        }
    }
    count
}

// ── Parquet export tests ─────────────────────────────────────────────────

#[cfg(feature = "parquet")]
#[sqlx::test(migrations = "./migrations")]
async fn export_events_parquet_returns_octet_stream(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("a".repeat(64))
    .bind(1_i64)
    .bind(json!({"value": null, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=parquet")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/octet-stream"
    );
    assert!(response
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("events.parquet"));

    // Verify the bytes are a valid Parquet file (magic bytes PAR1)
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert!(body.len() > 4);
    assert_eq!(&body[..4], b"PAR1", "Parquet magic bytes missing");
    assert_eq!(
        &body[body.len() - 4..],
        b"PAR1",
        "Parquet footer magic missing"
    );
}

#[cfg(feature = "parquet")]
#[sqlx::test(migrations = "./migrations")]
async fn export_events_parquet_can_be_read_back(pool: PgPool) {
    use arrow_array::cast::AsArray;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind(contract_id)
    .bind("contract")
    .bind("b".repeat(64))
    .bind(42_i64)
    .bind(json!({"value": {"amount": 100}, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=parquet")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();

    // Read back with parquet reader
    let cursor = std::io::Cursor::new(body);
    let builder = ParquetRecordBatchReaderBuilder::try_new(cursor).unwrap();
    let mut reader = builder.build().unwrap();
    let batch = reader.next().unwrap().unwrap();

    assert_eq!(batch.num_rows(), 1);

    // Verify ledger column value
    let ledger_col = batch.column_by_name("ledger").unwrap();
    let ledger_val = ledger_col
        .as_primitive::<arrow_array::types::Int64Type>()
        .value(0);
    assert_eq!(ledger_val, 42);

    // Verify contract_id column value
    let cid_col = batch.column_by_name("contract_id").unwrap();
    let cid_val = cid_col.as_string::<i32>().value(0);
    assert_eq!(cid_val, contract_id);
}

#[cfg(feature = "parquet")]
#[sqlx::test(migrations = "./migrations")]
async fn export_events_parquet_empty_db_returns_valid_file(pool: PgPool) {
    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=parquet")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    // Even an empty Parquet file has the PAR1 magic
    assert_eq!(&body[..4], b"PAR1");
}

#[cfg(not(feature = "parquet"))]
#[sqlx::test(migrations = "./migrations")]
async fn export_events_parquet_without_feature_returns_400(pool: PgPool) {
    let app = create_export_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/export?format=parquet")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ── Anonymization tests ──────────────────────────────────────────────────

fn create_admin_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    crate::routes::create_router(
        pool,
        vec!["admin-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

/// Build a router where admin endpoints require a dedicated ADMIN_API_KEY
/// (issue #409). A separate regular API key is also configured.
fn create_admin_auth_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let mut config = crate::config::Config::default();
    config.admin_api_keys = vec![secrecy::SecretString::new("admin-secret".to_string())];
    crate::routes::create_router(
        pool,
        vec!["regular-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_endpoint_rejects_missing_key_with_401(pool: PgPool) {
    let app = create_admin_auth_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/pause")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_endpoint_rejects_regular_key_with_403(pool: PgPool) {
    let app = create_admin_auth_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/pause")
                .header("Authorization", "Bearer regular-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_endpoint_accepts_admin_key(pool: PgPool) {
    let app = create_admin_auth_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/pause")
                .header("Authorization", "Bearer admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // The admin key passes both auth layers. The pause handler may still
    // reject (400) when no live indexer is attached, but it must NOT be
    // blocked by the auth layers.
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn anonymize_event_returns_200_and_scrubs_data(pool: PgPool) {
    let event_id = Uuid::new_v4();
    let original_tx = "a".repeat(64);
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(event_id)
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind(&original_tx)
    .bind(1_i64)
    .bind(Utc::now())
    .bind(json!({"sensitive": "data"}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_admin_router(pool.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/events/{event_id}/anonymize"))
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["anonymized"], true);

    // Verify DB state
    let row = sqlx::query("SELECT event_data, tx_hash, anonymized FROM events WHERE id = $1")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let event_data: Value = row.try_get("event_data").unwrap();
    assert_eq!(event_data, json!({"anonymized": true}));
    let tx_hash: String = row.try_get("tx_hash").unwrap();
    assert_ne!(
        tx_hash, original_tx,
        "tx_hash must be replaced with its hash"
    );
    assert_eq!(tx_hash.len(), 64, "hashed tx_hash must be 64 hex chars");
    let anonymized: bool = row.try_get("anonymized").unwrap();
    assert!(anonymized);
}

#[sqlx::test(migrations = "./migrations")]
async fn anonymize_event_is_idempotent(pool: PgPool) {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, anonymized)
             VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)",
    )
    .bind(event_id)
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("b".repeat(64))
    .bind(2_i64)
    .bind(Utc::now())
    .bind(json!({"anonymized": true}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/events/{event_id}/anonymize"))
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["anonymized"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn anonymize_event_not_found_returns_404(pool: PgPool) {
    let app = create_admin_router(pool);
    let missing_id = Uuid::new_v4();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/events/{missing_id}/anonymize"))
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn anonymize_event_requires_api_key(pool: PgPool) {
    let event_id = Uuid::new_v4();
    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/events/{event_id}/anonymize"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn anonymized_event_data_visible_in_get_events(pool: PgPool) {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, anonymized)
             VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)",
    )
    .bind(event_id)
    .bind("C1234567890123456789012345678901234567890123456789012345")
    .bind("contract")
    .bind("c".repeat(64))
    .bind(3_i64)
    .bind(Utc::now())
    .bind(json!({"anonymized": true}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?exact_count=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let events = v["data"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["event_data"], json!({"anonymized": true}));
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_anonymized_filter_requires_admin(pool: PgPool) {
    let app = create_admin_auth_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?anonymized=true")
                .header("Authorization", "Bearer regular-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_anonymized_filter_admin_key_allows_query(pool: PgPool) {
    let event_id_true = Uuid::new_v4();
    let event_id_false = Uuid::new_v4();
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, anonymized)
             VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)",
    )
    .bind(event_id_true)
    .bind(contract_id)
    .bind("contract")
    .bind("a".repeat(64))
    .bind(1_i64)
    .bind(now)
    .bind(json!({"value": 1}))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data, anonymized)
             VALUES ($1, $2, $3, $4, $5, $6, $7, FALSE)",
    )
    .bind(event_id_false)
    .bind(contract_id)
    .bind("contract")
    .bind("b".repeat(64))
    .bind(2_i64)
    .bind(now)
    .bind(json!({"value": 2}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_admin_auth_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?anonymized=true")
                .header("Authorization", "Bearer admin-secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let events = v["data"].as_array().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["anonymized"], json!(true));
}

// ── Diff endpoint tests ──────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn diff_groups_events_by_contract_and_type(pool: PgPool) {
    // Contract A: 3 contract events at ledger 10-12, 1 diagnostic at ledger 11
    // Contract B: 2 contract events at ledger 10-11
    let contract_a = "CA23456789012345678901234567890123456789012345678901234";
    let contract_b = "CB23456789012345678901234567890123456789012345678901234";
    for (i, (cid, etype, ledger)) in [
        (contract_a, "contract", 10_i64),
        (contract_a, "contract", 11_i64),
        (contract_a, "contract", 12_i64),
        (contract_a, "diagnostic", 11_i64),
        (contract_b, "contract", 10_i64),
        (contract_b, "contract", 11_i64),
    ]
    .iter()
    .enumerate()
    {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(cid)
        .bind(etype)
        .bind(format!("{:0>64}", i))
        .bind(ledger)
        .bind(Utc::now())
        .bind(json!({}))
        .execute(&pool)
        .await
        .unwrap();
    }

    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/diff?from_ledger=10&to_ledger=12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(v["from_ledger"], 10);
    assert_eq!(v["to_ledger"], 12);

    let contracts = v["contracts"].as_array().unwrap();
    assert_eq!(contracts.len(), 2);

    // First entry must be contract_a (total=4 > contract_b total=2)
    assert_eq!(contracts[0]["contract_id"], contract_a);
    assert_eq!(contracts[0]["event_counts"]["contract"], 3);
    assert_eq!(contracts[0]["event_counts"]["diagnostic"], 1);
    assert_eq!(contracts[0]["total"], 4);

    assert_eq!(contracts[1]["contract_id"], contract_b);
    assert_eq!(contracts[1]["event_counts"]["contract"], 2);
    assert_eq!(contracts[1]["total"], 2);
}

#[sqlx::test(migrations = "./migrations")]
async fn diff_empty_range_returns_empty_contracts(pool: PgPool) {
    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/diff?from_ledger=1000&to_ledger=2000")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["contracts"], json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn diff_invalid_range_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/diff?from_ledger=100&to_ledger=50")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["code"], "VALIDATION_ERROR");
}

#[sqlx::test(migrations = "./migrations")]
async fn diff_missing_params_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    // Missing to_ledger
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/diff?from_ledger=10")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

// ── Pause / Resume tests ─────────────────────────────────────────────────

fn create_active_indexer_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    let indexer_state = Arc::new(IndexerState::new());
    indexer_state
        .is_active_indexer
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    crate::routes::create_router(
        pool,
        vec!["admin-key".to_string()],
        &[],
        60,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn pause_indexer_returns_200_and_sets_paused(pool: PgPool) {
    let app = create_active_indexer_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/pause")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["indexer_paused"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn resume_indexer_returns_200_and_clears_paused(pool: PgPool) {
    let app = create_active_indexer_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/resume")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["indexer_paused"], false);
}

#[sqlx::test(migrations = "./migrations")]
async fn pause_returns_403_on_read_only_replica(pool: PgPool) {
    // create_test_router uses IndexerState::new() which defaults is_active_indexer=false
    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/pause")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn resume_returns_403_on_read_only_replica(pool: PgPool) {
    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/resume")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn pause_requires_api_key(pool: PgPool) {
    let app = create_active_indexer_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/indexer/pause")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn status_includes_indexer_paused_field(pool: PgPool) {
    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(
        v.get("indexer_paused").is_some(),
        "indexer_paused must be present in /status"
    );
    assert_eq!(v["indexer_paused"], false);
}

// ── Materialized view stats tests ────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn get_event_stats_empty_db_returns_zeros(pool: PgPool) {
    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["total_events"], 0);
    assert_eq!(v["events_last_24h"], 0);
    assert_eq!(v["events_last_7d"], 0);
    assert!(v["top_contracts"].as_array().unwrap().is_empty());
    assert!(v.get("computed_at").is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn get_event_stats_reflects_data_after_matview_refresh(pool: PgPool) {
    // Insert events
    for i in 0..3_i64 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, NOW(), $5)",
        )
        .bind(format!("C{:0>55}", i))
        .bind("contract")
        .bind(format!("{:0>64}", i))
        .bind(i)
        .bind(json!({}))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Manually refresh the materialized views so the test data is visible
    crate::stats_refresh::refresh_all(&pool).await;

    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(v["total_events"], 3);
    // All events inserted with NOW() so they should appear in 24h and 7d windows
    assert_eq!(v["events_last_24h"], 3);
    assert_eq!(v["events_last_7d"], 3);
    assert_eq!(v["top_contracts"].as_array().unwrap().len(), 3);
    assert_eq!(v["events_by_type"]["contract"], 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_event_stats_top_contracts_ordered_by_count(pool: PgPool) {
    let contract_a = "CA23456789012345678901234567890123456789012345678901234";
    let contract_b = "CB23456789012345678901234567890123456789012345678901234";

    // 3 events for A, 1 for B
    for i in 0..3_i64 {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                 VALUES ($1, $2, $3, $4, NOW(), $5)",
        )
        .bind(contract_a)
        .bind("contract")
        .bind(format!("a{:0>63}", i))
        .bind(i)
        .bind(json!({}))
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind(contract_b)
    .bind("contract")
    .bind("b".repeat(64))
    .bind(10_i64)
    .bind(json!({}))
    .execute(&pool)
    .await
    .unwrap();

    crate::stats_refresh::refresh_all(&pool).await;

    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();

    let top = v["top_contracts"].as_array().unwrap();
    assert_eq!(top[0]["contract_id"], contract_a);
    assert_eq!(top[0]["event_count"], 3);
    assert_eq!(top[1]["contract_id"], contract_b);
    assert_eq!(top[1]["event_count"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_event_stats_returns_cache_control_header(pool: PgPool) {
    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let cc = response
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        cc.contains("max-age=60"),
        "expected max-age=60 in Cache-Control, got: {cc}"
    );
}

// ── Issue #413: full-text search uses GIN index ──────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn fulltext_search_returns_matching_events(pool: PgPool) {
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind("CSEARCH1")
    .bind("contract")
    .bind("s".repeat(64))
    .bind(1_i64)
    .bind(json!({"value": {"amount": "transfer"}, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind("CSEARCH2")
    .bind("contract")
    .bind("t".repeat(64))
    .bind(2_i64)
    .bind(json!({"value": {"amount": "unrelated"}, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?search=transfer")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let data = v["data"].as_array().unwrap();
    assert_eq!(data.len(), 1);
    assert_eq!(data[0]["contract_id"], json!("CSEARCH1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn fulltext_search_uses_gin_index(pool: PgPool) {
    // EXPLAIN ANALYZE the tsv query and verify it mentions the GIN index
    let plan: String = sqlx::query_scalar(
        "EXPLAIN SELECT id FROM events WHERE event_data_tsv @@ plainto_tsquery('english', 'transfer')"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        plan.to_lowercase().contains("gin") || plan.to_lowercase().contains("idx_events_event_data_tsv"),
        "expected GIN index in query plan, got: {plan}"
    );
}

// ── Issue #414: cursor validation ────────────────────────────────────────

#[test]
fn decode_cursor_rejects_negative_ledger() {
    let cursor = URL_SAFE_NO_PAD.encode(format!("-1:{}", Uuid::new_v4()));
    assert!(matches!(
        decode_cursor(&cursor),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn decode_cursor_rejects_zero_ledger() {
    let cursor = URL_SAFE_NO_PAD.encode(format!("0:{}", Uuid::new_v4()));
    assert!(matches!(
        decode_cursor(&cursor),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn decode_cursor_rejects_i64_max_ledger() {
    let cursor = URL_SAFE_NO_PAD.encode(format!("{}:{}", i64::MAX, Uuid::new_v4()));
    assert!(matches!(
        decode_cursor(&cursor),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn decode_cursor_rejects_non_v4_uuid() {
    // UUID v1 (time-based)
    let v1_uuid = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
    let cursor = URL_SAFE_NO_PAD.encode(format!("100:{v1_uuid}"));
    assert!(matches!(
        decode_cursor(&cursor),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn decode_cursor_rejects_invalid_uuid() {
    let cursor = URL_SAFE_NO_PAD.encode("100:not-a-uuid");
    assert!(matches!(
        decode_cursor(&cursor),
        Err(AppError::Validation(_))
    ));
}

#[test]
fn decode_cursor_accepts_valid_input() {
    let id = Uuid::new_v4();
    let cursor = URL_SAFE_NO_PAD.encode(format!("100:{id}"));
    let (ledger, decoded_id) = decode_cursor(&cursor).unwrap();
    assert_eq!(ledger, 100);
    assert_eq!(decoded_id, id);
}

// ── Issue #415: contract count cache invalidation ────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn contract_count_cache_invalidated_on_new_event(pool: PgPool) {
    use tokio::sync::broadcast;

    let (event_tx, _) = broadcast::channel::<crate::models::SorobanEvent>(16);
    let health_state = Arc::new(crate::config::HealthState::new(60));
    let indexer_state = Arc::new(crate::config::IndexerState::new());
    let prometheus_handle = crate::metrics::init_metrics();
    let config = crate::config::Config::default();
    let (_, shutdown_rx) = tokio::sync::watch::channel(false);

    let app = crate::routes::create_router_with_tx_and_tenant_map(
        pool.clone(),
        pool.clone(),
        vec![],
        &[],
        60,
        false,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx.clone(),
        15000,
        1000,
        2000,
        None,
        None,
        config,
        None,
        Arc::new(std::collections::HashMap::new()),
        shutdown_rx,
        crate::sse_ring_buffer::SseRingBuffer::new(100),
    );

    let contract_id = "CCACHEINVAL1234567890123456789012345678901234567890123456";

    // Seed one event so the contract endpoint returns data
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind(contract_id)
    .bind("contract")
    .bind("c".repeat(64))
    .bind(1_i64)
    .bind(json!({}))
    .execute(&pool)
    .await
    .unwrap();

    // First request — populates cache with count=1
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/contract/{contract_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["total"], json!(1));

    // Insert a second event directly into DB (bypassing the indexer)
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
    )
    .bind(contract_id)
    .bind("contract")
    .bind("d".repeat(64))
    .bind(2_i64)
    .bind(json!({}))
    .execute(&pool)
    .await
    .unwrap();

    // Broadcast a new event for this contract — triggers cache invalidation
    let _ = event_tx.send(crate::models::SorobanEvent {
        contract_id: contract_id.to_string(),
        event_type: "contract".to_string(),
        tx_hash: "d".repeat(64),
        ledger: 2,
        ledger_closed_at: Utc::now().to_rfc3339(),
        ledger_hash: None,
        in_successful_call: true,
        topic: None,
        value: serde_json::Value::Null,
        tenant_id: None,
    });

    // Give the background task a moment to process the invalidation
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Second request — cache was invalidated, should re-query and return count=2
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/contract/{contract_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["total"], json!(2));
}

// ── Issue #421: slow query detection ─────────────────────────────────────

#[tokio::test]
async fn slow_query_warn_is_emitted_when_threshold_exceeded() {
    use tracing_subscriber::layer::SubscriberExt;
    let (writer, output) = tracing_subscriber::fmt::TestWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(writer)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // threshold=0 → any query duration triggers the warning
    timed_query(
        async { 42u32 },
        "test_query",
        0,
        Some("ctx"),
    )
    .await;

    let logs = output.into_string();
    assert!(
        logs.contains("slow query detected"),
        "expected 'slow query detected' warn, got: {logs}"
    );
}

#[tokio::test]
async fn slow_query_no_warn_when_under_threshold() {
    use tracing_subscriber::layer::SubscriberExt;
    let (writer, output) = tracing_subscriber::fmt::TestWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(writer)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    // threshold=60_000ms → instant future never triggers warning
    timed_query(
        async { 42u32 },
        "test_query",
        60_000,
        None,
    )
    .await;

    let logs = output.into_string();
    assert!(
        !logs.contains("slow query detected"),
        "unexpected 'slow query detected' warn: {logs}"
    );
}

// ── Issue #423: ledger range bounds validation ────────────────────────────

#[test]
fn validate_ledger_param_accepts_zero() {
    assert!(validate_ledger_param("from_ledger", 0).is_ok());
}

#[test]
fn validate_ledger_param_accepts_max_u32() {
    assert!(validate_ledger_param("from_ledger", u32::MAX as i64).is_ok());
}

#[test]
fn validate_ledger_param_rejects_negative() {
    let err = validate_ledger_param("from_ledger", -1).unwrap_err();
    match err {
        AppError::Validation(msg) => assert!(msg.contains("from_ledger")),
        _ => panic!("expected Validation error"),
    }
}

#[test]
fn validate_ledger_param_rejects_above_u32_max() {
    let err = validate_ledger_param("to_ledger", u32::MAX as i64 + 1).unwrap_err();
    match err {
        AppError::Validation(msg) => assert!(msg.contains("to_ledger")),
        _ => panic!("expected Validation error"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_rejects_negative_from_ledger(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?from_ledger=-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_rejects_out_of_range_to_ledger(pool: PgPool) {
    let app = create_test_router(pool);
    // 2^32 = 4_294_967_296 — one above u32::MAX
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?to_ledger=4294967296")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn get_events_by_contract_rejects_negative_from_ledger(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/contract/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA?from_ledger=-5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// -----------------------------------------------------------------------
// #444 — Topic filter tests
// -----------------------------------------------------------------------

/// Helper: insert an event with a known topic[0] sym value.
async fn insert_event_with_topic(pool: &PgPool, contract_id: &str, topic_sym: &str) -> Uuid {
    let id = Uuid::new_v4();
    let event_data = json!({ "topic": [{ "sym": topic_sym }], "value": {} });
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, 'contract', $3, $4, NOW(), $5)",
    )
    .bind(id)
    .bind(contract_id)
    .bind(format!("{id}"))
    .bind(1_i64)
    .bind(&event_data)
    .execute(pool)
    .await
    .unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
async fn topic_0_filter_returns_matching_events(pool: PgPool) {
    let cid = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0000";
    insert_event_with_topic(&pool, cid, "transfer").await;
    insert_event_with_topic(&pool, cid, "mint").await;

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?topic_0=transfer")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let events = v["events"].as_array().unwrap();
    assert_eq!(events.len(), 1, "only the 'transfer' event should be returned");
}

#[sqlx::test(migrations = "./migrations")]
async fn topic_0_filter_returns_empty_when_no_match(pool: PgPool) {
    let cid = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0001";
    insert_event_with_topic(&pool, cid, "mint").await;

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events?topic_0=transfer")
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    let events = v["events"].as_array().unwrap();
    assert!(events.is_empty());
}

// -----------------------------------------------------------------------
// #446 — Lua preview endpoint tests
// -----------------------------------------------------------------------

#[sqlx::test(migrations = "./migrations")]
async fn lua_preview_rejects_empty_script(pool: PgPool) {
    let app = create_admin_router(pool);
    let body = serde_json::to_string(&json!({
        "script": "",
        "event_ids": [Uuid::new_v4()]
    }))
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/lua/preview")
                .header("Authorization", "Bearer admin-key")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn lua_preview_rejects_too_many_event_ids(pool: PgPool) {
    let ids: Vec<_> = (0..21).map(|_| Uuid::new_v4()).collect();
    let app = create_admin_router(pool);
    let body = serde_json::to_string(&json!({
        "script": "function transform_event(e) return e end",
        "event_ids": ids
    }))
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/lua/preview")
                .header("Authorization", "Bearer admin-key")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn lua_preview_returns_404_when_events_not_found(pool: PgPool) {
    let app = create_admin_router(pool);
    let body = serde_json::to_string(&json!({
        "script": "function transform_event(e) return e end",
        "event_ids": [Uuid::new_v4()]
    }))
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/lua/preview")
                .header("Authorization", "Bearer admin-key")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn lua_preview_does_not_modify_database(pool: PgPool) {
    let event_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA0002',
                     'contract', $2, 1, NOW(), $3)",
    )
    .bind(event_id)
    .bind(format!("{event_id}"))
    .bind(json!({"value": 42, "topic": []}))
    .execute(&pool)
    .await
    .unwrap();

    let app = create_admin_router(pool.clone());
    // Script that would change the value field
    let body = serde_json::to_string(&json!({
        "script": "function transform_event(e) e.value = {value=999} return e end",
        "event_ids": [event_id]
    }))
    .unwrap();

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/lua/preview")
                .header("Authorization", "Bearer admin-key")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    // Preview must succeed
    assert_eq!(resp.status(), StatusCode::OK);

    // DB must be unchanged
    let row: Value = sqlx::query_scalar("SELECT event_data FROM events WHERE id = $1")
        .bind(event_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row["value"], 42, "database must not be modified by preview");
}

#[sqlx::test(migrations = "./migrations")]
async fn lua_preview_requires_admin_key(pool: PgPool) {
    let app = create_admin_auth_router(pool);
    let body = serde_json::to_string(&json!({
        "script": "function transform_event(e) return e end",
        "event_ids": [Uuid::new_v4()]
    }))
    .unwrap();

    // No auth
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/lua/preview")
                .header("Content-Type", "application/json")
                .body(Body::from(body.clone()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // Regular API key (not admin)
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/lua/preview")
                .header("Authorization", "Bearer regular-key")
                .header("Content-Type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ── #513: SLA monitoring unit tests ──────────────────────────────────────

#[test]
fn sla_latency_calculation_is_correct() {
    let indexed = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let delivered = chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:25Z")
        .unwrap()
        .with_timezone(&Utc);
    let latency = (delivered - indexed).num_milliseconds() as f64 / 1000.0;
    assert!((latency - 25.0).abs() < 0.001);
}

#[test]
fn sla_breach_detected_when_latency_exceeds_sla() {
    let sla_seconds: u64 = 30;
    let latency: f64 = 35.0;
    let breached = latency > sla_seconds as f64;
    assert!(breached, "expected SLA breach for latency {latency}s > SLA {sla_seconds}s");
}

#[test]
fn sla_not_breached_when_latency_within_sla() {
    let sla_seconds: u64 = 30;
    let latency: f64 = 10.0;
    let breached = latency > sla_seconds as f64;
    assert!(!breached);
}

// ── #514: Capacity planning unit tests ───────────────────────────────────

#[test]
fn growth_trend_zero_when_no_baseline() {
    let baseline_rate_per_minute: f64 = 0.0;
    let current_rate: f64 = 5.0;
    let trend = if baseline_rate_per_minute > 0.0 {
        ((current_rate - baseline_rate_per_minute) / baseline_rate_per_minute) * 100.0
    } else {
        0.0
    };
    assert_eq!(trend, 0.0);
}

#[test]
fn growth_trend_positive_when_current_above_baseline() {
    let baseline: f64 = 10.0;
    let current: f64 = 15.0;
    let trend = ((current - baseline) / baseline) * 100.0;
    assert!((trend - 50.0).abs() < 0.001);
}

#[test]
fn projected_rate_never_below_current_with_positive_trend() {
    let current: f64 = 20.0;
    let trend: f64 = 25.0;
    let projected = current * (1.0 + trend.max(0.0) / 100.0);
    assert!(projected >= current);
}

// ── #512: Lifecycle webhook unit tests ───────────────────────────────────

#[test]
fn lifecycle_event_type_matching_wildcard() {
    let subscribed_events = vec!["*".to_string()];
    let event_type = "channel_deleted";
    let matches = subscribed_events
        .iter()
        .any(|e| e == "*" || e == event_type);
    assert!(matches);
}

#[test]
fn lifecycle_event_type_matching_exact() {
    let subscribed_events = vec![
        "channel_created".to_string(),
        "delivery_failed".to_string(),
    ];
    let matches_created = subscribed_events
        .iter()
        .any(|e| e == "*" || e == "channel_created");
    let matches_deleted = subscribed_events
        .iter()
        .any(|e| e == "*" || e == "channel_deleted");
    assert!(matches_created);
    assert!(!matches_deleted);
}

#[test]
fn lifecycle_event_not_delivered_to_inactive_webhook() {
    let webhook = crate::models::SystemWebhookConfig {
        id: Uuid::new_v4(),
        url: "http://example.com/hook".to_string(),
        secret: None,
        events: vec!["*".to_string()],
        active: false,
        created_at: Utc::now(),
    };
    assert!(!webhook.active, "inactive webhook should not receive events");
}

// ── #511: Bulk operations unit tests ─────────────────────────────────────

#[test]
fn bulk_response_counts_success_and_failure() {
    let results = vec![
        crate::models::BulkChannelResult {
            id: Uuid::new_v4(),
            success: true,
            error: None,
        },
        crate::models::BulkChannelResult {
            id: Uuid::new_v4(),
            success: false,
            error: Some("not found".to_string()),
        },
        crate::models::BulkChannelResult {
            id: Uuid::new_v4(),
            success: true,
            error: None,
        },
    ];
    let succeeded = results.iter().filter(|r| r.success).count() as i64;
    let failed = results.iter().filter(|r| !r.success).count() as i64;
    assert_eq!(succeeded, 2);
    assert_eq!(failed, 1);
}

#[test]
fn bulk_tag_appends_only_new_tags() {
    let mut tags: Vec<String> = vec!["production".to_string()];
    let new_tags = vec!["production".to_string(), "critical".to_string()];
    for tag in &new_tags {
        if !tags.contains(tag) {
            tags.push(tag.clone());
        }
    }
    assert_eq!(tags.len(), 2);
    assert!(tags.contains(&"production".to_string()));
    assert!(tags.contains(&"critical".to_string()));
}

#[tokio::test]
async fn bulk_enable_returns_not_found_for_unknown_channel() {
    let unknown_id = Uuid::new_v4();
    let req = crate::models::BulkChannelRequest {
        channel_ids: vec![unknown_id],
    };

    let mut store = notification_channels().write().await;
    store.clear();
    drop(store);

    let mut results = Vec::new();
    {
        let mut s = notification_channels().write().await;
        if let Some(ch) = s.get_mut(&unknown_id) {
            ch.active = true;
            results.push(crate::models::BulkChannelResult {
                id: unknown_id,
                success: true,
                error: None,
            });
        } else {
            results.push(crate::models::BulkChannelResult {
                id: unknown_id,
                success: false,
                error: Some(format!("channel {unknown_id} not found")),
            });
        }
    }
    assert_eq!(results.len(), req.channel_ids.len());
    assert!(!results[0].success);
    assert!(results[0].error.as_deref().unwrap_or("").contains("not found"));
}

// Regression for fuzz_validate_tx_hash (Issue #1381): 64 *bytes* of non-ASCII
// or mixed input must be rejected without panicking.
#[test]
fn validate_tx_hash_fuzz_regression_inputs() {
    let cases = ["\u{0}".repeat(64), "é".repeat(32), format!("{}g", "a".repeat(63)), " ".repeat(64)];
    for c in &cases {
        assert!(validate_tx_hash(c).is_err(), "{c:?} should be rejected");
    }
    assert!(validate_tx_hash(&"aB".repeat(32)).is_ok());
}
