//! Temporal query handler tests (moved out of `handlers/mod.rs`, Issue #1241).

use super::*;
use crate::config::{HealthState, IndexerState};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
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

// ── parse_relative_duration unit tests ────────────────────────────────────

#[test]
fn parse_seconds() {
    let d = parse_relative_duration("30s").unwrap();
    assert_eq!(d, chrono::Duration::seconds(30));
}

#[test]
fn parse_minutes() {
    let d = parse_relative_duration("5m").unwrap();
    assert_eq!(d, chrono::Duration::minutes(5));
}

#[test]
fn parse_hours() {
    let d = parse_relative_duration("24h").unwrap();
    assert_eq!(d, chrono::Duration::hours(24));
}

#[test]
fn parse_days() {
    let d = parse_relative_duration("7d").unwrap();
    assert_eq!(d, chrono::Duration::days(7));
}

#[test]
fn parse_weeks() {
    let d = parse_relative_duration("2w").unwrap();
    assert_eq!(d, chrono::Duration::weeks(2));
}

#[test]
fn parse_invalid_unit_returns_error() {
    assert!(parse_relative_duration("1y").is_err());
}

#[test]
fn parse_missing_unit_returns_error() {
    assert!(parse_relative_duration("42").is_err());
}

#[test]
fn parse_zero_returns_error() {
    assert!(parse_relative_duration("0h").is_err());
}

#[test]
fn parse_negative_returns_error() {
    assert!(parse_relative_duration("-1h").is_err());
}

#[test]
fn parse_empty_returns_error() {
    assert!(parse_relative_duration("").is_err());
}

// ── /v1/events/temporal integration tests ─────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn temporal_missing_since_and_from_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_since_24h_returns_200_empty(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal?since=24h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["events"], serde_json::json!([]));
    assert_eq!(v["total"], 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_aggregate_returns_buckets(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal?since=1d&aggregate=true&window=1h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    // On an empty DB the buckets array should be empty (no events in last 24h)
    assert!(v["buckets"].is_array());
    assert!(v["events"].is_array());
    assert_eq!(v["events"], serde_json::json!([]));
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_since_and_from_timestamp_mutually_exclusive(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal?since=1d&from_timestamp=2026-01-01T00:00:00Z")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_invalid_window_returns_400(pool: PgPool) {
    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal?since=1d&aggregate=true&window=2d")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_returns_event_within_window(pool: PgPool) {
    // Insert an event that was created just now (well within 24h window).
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ('CABC', 'contract', 'txabc123temporal', 100, NOW(), '{}'::jsonb)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal?since=24h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["total"], 1);
    assert_eq!(v["events"].as_array().unwrap().len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn temporal_event_outside_window_not_returned(pool: PgPool) {
    // Insert an event two days ago — outside the 1h window.
    sqlx::query(
        "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
             VALUES ('CABC', 'contract', 'txold_temporal', 99, NOW() - INTERVAL '2 days', '{}'::jsonb)",
    )
    .execute(&pool)
    .await
    .unwrap();

    let app = create_test_router(pool);
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/v1/events/temporal?since=1h")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(v["total"], 0);
}
