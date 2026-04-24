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
    create_router(pool, api_key, &[], 60, health_state, indexer_state, prometheus_handle, 1_048_576)
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
