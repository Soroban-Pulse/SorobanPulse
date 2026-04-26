use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

use soroban_pulse::config::{HealthState, IndexerState};
use soroban_pulse::metrics::init_metrics;
use soroban_pulse::routes::create_router;

fn make_router(pool: PgPool, api_key: Option<String>) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    health_state.update_last_poll();
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = init_metrics();
    let api_keys = api_key.into_iter().collect();
    create_router(pool, api_keys, &[], 60, health_state, indexer_state, prometheus_handle, 15000)
}

// --- Health ---

#[sqlx::test(migrations = "./migrations")]
async fn health_ready_with_live_db_returns_200(pool: PgPool) {
    let app = make_router(pool, None);

    let resp = app
        .oneshot(Request::builder().uri("/healthz/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"], "ok");
    assert_eq!(body["indexer"], "ok");
}

// --- Auth middleware ---

#[sqlx::test(migrations = "./migrations")]
async fn request_without_api_key_returns_401_when_key_configured(pool: PgPool) {
    let app = make_router(pool, Some("secret".to_string()));

    let resp = app
        .oneshot(Request::builder().uri("/v1/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
async fn request_with_bearer_token_passes_auth(pool: PgPool) {
    let app = make_router(pool, Some("secret".to_string()));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header(header::AUTHORIZATION, "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn request_with_x_api_key_header_passes_auth(pool: PgPool) {
    let app = make_router(pool, Some("secret".to_string()));

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events")
                .header("X-Api-Key", "secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
}

#[sqlx::test(migrations = "./migrations")]
async fn health_endpoint_bypasses_auth(pool: PgPool) {
    let app = make_router(pool, Some("secret".to_string()));

    let resp = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_ne!(resp.status(), StatusCode::UNAUTHORIZED);
}

// --- Deprecation headers on unversioned routes ---

#[sqlx::test(migrations = "./migrations")]
async fn unversioned_events_route_returns_deprecation_header(pool: PgPool) {
    let app = make_router(pool, None);

    let resp = app
        .oneshot(Request::builder().uri("/events").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("Deprecation").unwrap(), "true");
    assert!(resp
        .headers()
        .get("Link")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("/v1/events"));
}

// --- Metrics endpoint ---

#[sqlx::test(migrations = "./migrations")]
async fn metrics_endpoint_returns_prometheus_text(pool: PgPool) {
    let app = make_router(pool, None);

    let resp = app
        .oneshot(Request::builder().uri("/metrics").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body =
        String::from_utf8(to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap();
    assert!(body.contains("soroban_pulse"));
}

// --- Issue #185: from_ledger / to_ledger on contract endpoint ---

async fn insert_contract_events(pool: &PgPool, contract_id: &str, ledgers: &[i64]) {
    for (i, &ledger) in ledgers.iter().enumerate() {
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ($1, $2, $3, $4, NOW(), $5)",
        )
        .bind(contract_id)
        .bind("contract")
        .bind(format!("{:0>63}{}", i, ledger))
        .bind(ledger)
        .bind(serde_json::json!({}))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_ledger_range_filters_correctly(pool: PgPool) {
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    insert_contract_events(&pool, contract_id, &[100, 200, 300, 400, 500]).await;

    let app = make_router(pool, None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/events/contract/{}?from_ledger=200&to_ledger=400",
                    contract_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();

    let data = body["data"].as_array().unwrap();
    assert_eq!(data.len(), 3);
    for event in data {
        let ledger = event["ledger"].as_i64().unwrap();
        assert!((200..=400).contains(&ledger));
    }
    assert_eq!(body["from_ledger"], 200);
    assert_eq!(body["to_ledger"], 400);
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_ledger_range_inverted_returns_400(pool: PgPool) {
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    insert_contract_events(&pool, contract_id, &[100]).await;

    let app = make_router(pool, None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/v1/events/contract/{}?from_ledger=500&to_ledger=100",
                    contract_id
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("from_ledger must be <= to_ledger"));
}

#[sqlx::test(migrations = "./migrations")]
async fn contract_without_ledger_range_returns_all_events(pool: PgPool) {
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    insert_contract_events(&pool, contract_id, &[100, 200, 300]).await;

    let app = make_router(pool, None);
    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/contract/{}", contract_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value =
        serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
    assert!(body.get("from_ledger").is_none());
    assert!(body.get("to_ledger").is_none());
}

// --- SSE Streaming ---

#[sqlx::test(migrations = "./migrations")]
async fn sse_contract_stream_invalid_contract_id_returns_400(pool: PgPool) {
    let app = make_router(pool, None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/contract/INVALID/stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn sse_contract_stream_establishes_successfully(pool: PgPool) {
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    let app = make_router(pool, None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/events/contract/{}/stream", contract_id))
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get(header::CONTENT_TYPE).unwrap(), "text/event-stream");
}

#[sqlx::test(migrations = "./migrations")]
async fn sse_deprecated_contract_stream_unversioned_alias_works(pool: PgPool) {
    let contract_id = "C1234567890123456789012345678901234567890123456789012345";
    let app = make_router(pool, None);

    let resp = app
        .oneshot(
            Request::builder()
                .uri(format!("/events/contract/{}/stream", contract_id))
                .header(header::ACCEPT, "text/event-stream")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get("Deprecation").unwrap(), "true");
}
