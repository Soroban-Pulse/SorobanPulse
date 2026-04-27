use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

use soroban_pulse::config::{HealthState, IndexerState};
use soroban_pulse::metrics::init_metrics;
use soroban_pulse::routes::create_router_with_tx;
use soroban_pulse::schema_validator::SchemaValidator;

async fn make_router_with_schema(pool: PgPool, api_key: Option<String>) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    health_state.update_last_poll();
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = init_metrics();
    let api_keys = api_key.into_iter().collect();
    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    
    let schema_validator = Arc::new(SchemaValidator::new(pool.clone()));
    schema_validator.load_schemas().await.unwrap();
    
    let config = soroban_pulse::config::Config {
        database_url: String::new(),
        database_replica_url: None,
        stellar_rpc_url: String::new(),
        rpc_headers: Vec::new(),
        start_ledger: 0,
        port: 3000,
        behind_proxy: false,
        start_ledger_fallback: true,
        indexer_lag_warn_threshold: 1000,
        rpc_connect_timeout_secs: 30,
        rpc_request_timeout_secs: 60,
        api_keys: api_keys.clone(),
        db_max_connections: 10,
        db_min_connections: 2,
        db_idle_timeout_secs: 600,
        db_max_lifetime_secs: 1800,
        db_test_before_acquire: true,
        allowed_origins: vec!["*".to_string()],
        rate_limit_per_minute: 60,
        indexer_stall_timeout_secs: 60,
        db_statement_timeout_ms: 5000,
        indexer_poll_interval_ms: 5000,
        indexer_error_backoff_ms: 10000,
        sse_keepalive_interval_ms: 15000,
        sse_max_connections: 1000,
        environment: soroban_pulse::config::Environment::Development,
        max_body_size_bytes: 1024 * 1024,
        log_sample_rate: 1,
        indexer_event_types: Vec::new(),
        event_data_encryption_key: None,
        event_data_encryption_key_old: None,
        webhook_url: None,
        webhook_secret: None,
        webhook_contract_filter: Vec::new(),
        webhook_max_retries: 3,
        webhook_retry_delay_ms: 1000,
        tls_cert_file: None,
        tls_key_file: None,
        contract_count_cache_size: 1000,
        contract_count_cache_ttl_secs: 300,
        index_check_interval_hours: 24,
    };
    
    create_router_with_tx(
        pool.clone(),
        pool,
        api_keys,
        &["*".to_string()],
        60,
        false,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        15000,
        1000,
        15000,
        None,
        None,
        config,
        Some(schema_validator),
    )
}

#[sqlx::test(migrations = "./migrations")]
async fn register_schema_requires_auth(pool: PgPool) {
    let app = make_router_with_schema(pool, Some("test-key".to_string())).await;

    let schema = json!({
        "type": "object",
        "properties": {
            "value": {
                "type": "object",
                "properties": {
                    "amount": { "type": "number" }
                },
                "required": ["amount"]
            }
        }
    });

    // Without auth
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/contracts/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM/schema")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&json!({"schema": schema})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // With auth
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/contracts/CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM/schema")
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&json!({"schema": schema})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn register_and_get_schema(pool: PgPool) {
    let app = make_router_with_schema(pool, Some("test-key".to_string())).await;

    let contract_id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
    let schema = json!({
        "type": "object",
        "properties": {
            "value": {
                "type": "object",
                "properties": {
                    "amount": { "type": "number" }
                },
                "required": ["amount"]
            }
        }
    });

    // Register schema
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/contracts/{}/schema", contract_id))
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&json!({"schema": schema})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Get schema
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/admin/contracts/{}/schema", contract_id))
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn delete_schema(pool: PgPool) {
    let app = make_router_with_schema(pool.clone(), Some("test-key".to_string())).await;

    let contract_id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
    let schema = json!({
        "type": "object",
        "properties": {
            "value": { "type": "object" }
        }
    });

    // Register schema
    sqlx::query("INSERT INTO contract_schemas (contract_id, schema) VALUES ($1, $2)")
        .bind(contract_id)
        .bind(&schema)
        .execute(&pool)
        .await
        .unwrap();

    // Delete schema
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/admin/contracts/{}/schema", contract_id))
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);

    // Try to get deleted schema
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/v1/admin/contracts/{}/schema", contract_id))
                .header("Authorization", "Bearer test-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_schema_rejected(pool: PgPool) {
    let app = make_router_with_schema(pool, Some("test-key".to_string())).await;

    let contract_id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
    let invalid_schema = json!({
        "type": "invalid_type"  // Invalid JSON Schema
    });

    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/v1/admin/contracts/{}/schema", contract_id))
                .header("Authorization", "Bearer test-key")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_string(&json!({"schema": invalid_schema})).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
