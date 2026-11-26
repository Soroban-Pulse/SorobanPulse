//! Issue #586 / #587 / #710: Replication, feature flags, and backup verification tests.

use super::*;
use crate::config::{HealthState, IndexerState};
use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

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

#[sqlx::test(migrations = "./migrations")]
async fn replication_status_returns_200(pool: PgPool) {
    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/replication/status")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("replica_count").is_some());
    assert!(v.get("replicas").is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn feature_flags_list_returns_200(pool: PgPool) {
    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/feature-flags")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("flags").is_some());
    assert!(v.get("count").is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn feature_flag_audit_returns_200(pool: PgPool) {
    let app = create_admin_router(pool);
    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/admin/feature-flags/audit")
                .header("Authorization", "Bearer admin-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let v: Value = serde_json::from_slice(&body).unwrap();
    assert!(v.get("entries").is_some());
    assert!(v.get("count").is_some());
}
