use axum::{extract::{Path, Query, State}, Json, response::IntoResponse};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{self, Stream};
use serde_json::{json, Value};
use sqlx::Row;
use std::convert::Infallible;
use std::time::Duration;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use std::sync::atomic::Ordering;
use crate::{error::AppError, models::{PaginationParams, StreamParams}, routes::AppState};

fn validate_contract_id(contract_id: &str) -> Result<(), AppError> {
    if contract_id.len() != 56 {
        return Err(AppError::Validation("invalid contract_id format".to_string()));
    }
    if !contract_id.starts_with('C') {
        return Err(AppError::Validation("invalid contract_id format".to_string()));
    }
    if !contract_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation("invalid contract_id format".to_string()));
    }
    Ok(())
}

fn validate_tx_hash(tx_hash: &str) -> Result<(), AppError> {
    if tx_hash.len() != 64 {
        return Err(AppError::Validation("invalid tx_hash format".to_string()));
    }
    if !tx_hash.chars().all(|c| c.is_ascii_hexdigit() && c.is_lowercase()) {
        return Err(AppError::Validation("invalid tx_hash format".to_string()));
    }
    Ok(())
}

/// Health check endpoint that verifies DB connectivity and indexer status
#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    responses(
        (status = 200, description = "Service is healthy"),
        (status = 503, description = "Service is degraded"),
    )
)]
pub async fn health(State(state): State<AppState>) -> (axum::http::StatusCode, Json<Value>) {
    let mut db_ok = true;
    let mut db_reachable = true;

    // Check DB connectivity with 2-second timeout
    let db_check = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query("SELECT 1")
            .fetch_one(&state.pool)
    );

    match db_check.await {
        Ok(Ok(_)) => {
            // DB is reachable
        }
        Ok(Err(_)) => {
            db_ok = false;
            db_reachable = false;
        }
        Err(_) => {
            // Timeout
            db_ok = false;
            db_reachable = false;
        }
    }

    // Check indexer status
    let indexer_status = if let Some(secs_ago) = state.health_state.is_indexer_stalled() {
        json!({
            "indexer": "stalled",
            "last_poll_secs_ago": secs_ago
        })
    } else {
        json!({"indexer": "ok"})
    };

    // Determine overall status
    let is_degraded = !db_ok || indexer_status.get("indexer").and_then(|v| v.as_str()) == Some("stalled");

    if is_degraded {
        let response = json!({
            "status": "degraded",
            "db": if db_reachable { "ok" } else { "unreachable" }
        });
        // Merge indexer status
        let mut obj = serde_json::to_value(response).unwrap();
        if let Value::Object(ref mut map) = obj {
            if let Value::Object(indexer_map) = indexer_status {
                map.extend(indexer_map);
            }
        }
        (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(obj))
    } else {
        let response = json!({
            "status": "ok",
            "db": "ok",
            "indexer": "ok"
        });
        (axum::http::StatusCode::OK, Json(response))
    }
}

#[utoipa::path(
    get,
    path = "/status",
    tag = "system",
    responses(
        (status = 200, description = "Indexer operational status"),
    )
)]
pub async fn status(State(state): State<AppState>) -> Json<Value> {
    let current_ledger = state.indexer_state.current_ledger.load(Ordering::Relaxed);
    let latest_ledger = state.indexer_state.latest_ledger.load(Ordering::Relaxed);
    let lag_ledgers = latest_ledger.saturating_sub(current_ledger);
    let uptime_secs = state.indexer_state.uptime_secs();

    let indexer_status = if state.health_state.is_indexer_stalled().is_some() {
        "stalled"
    } else {
        "running"
    };

    let total_events: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": uptime_secs,
        "current_ledger": current_ledger,
        "latest_ledger": latest_ledger,
        "lag_ledgers": lag_ledgers,
        "total_events": total_events,
        "indexer_status": indexer_status,
    }))
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    state.prometheus_handle.render()
}

/// Serve the raw OpenAPI JSON spec.
pub async fn openapi_json() -> impl IntoResponse {
    use crate::routes::ApiDoc;
    use utoipa::OpenApi;
    Json(ApiDoc::openapi())
}

/// Stream new events in real time via Server-Sent Events.
#[utoipa::path(
    get,
    path = "/v1/events/stream",
    tag = "events",
    params(
        ("contract_id" = Option<String>, Query, description = "Filter by contract ID"),
    ),
    responses(
        (status = 200, description = "SSE stream of new events (text/event-stream)"),
    )
)]
pub async fn stream_events(
    State(state): State<AppState>,
    Query(params): Query<StreamParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = state.event_tx.subscribe();
    let contract_filter = params.contract_id;

    let s = stream::unfold(rx, move |mut rx| {
        let filter = contract_filter.clone();
        async move {
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        if let Some(ref cid) = filter {
                            if &event.contract_id != cid {
                                continue;
                            }
                        }
                        let data = serde_json::to_string(&event).unwrap_or_default();
                        return Some((Ok(Event::default().data(data)), rx));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        }
    });

    Sse::new(s).keep_alive(KeepAlive::default())
}

#[utoipa::path(
    get,
    path = "/v1/events",
    tag = "events",
    params(
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("limit" = Option<i64>, Query, description = "Results per page, 1–100 (default: 20)"),
        ("exact_count" = Option<bool>, Query, description = "Use exact COUNT(*) instead of approximate"),
        ("event_type" = Option<String>, Query, description = "Filter by event type: contract, diagnostic, system"),
        ("from_ledger" = Option<i64>, Query, description = "Return events at or after this ledger"),
        ("to_ledger" = Option<i64>, Query, description = "Return events at or before this ledger"),
    ),
    responses(
        (status = 200, description = "Paginated list of events"),
        (status = 400, description = "Invalid query parameters"),
    )
)]

pub async fn get_events(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, AppError> {
    // Validate event_type
    if let Some(ref et) = params.event_type {
        if !["contract", "diagnostic", "system"].contains(&et.as_str()) {
            return Err(AppError::Validation(
                "event_type must be one of: contract, diagnostic, system".to_string(),
            ));
        }
    }

    // Validate ledger range
    if let (Some(from), Some(to)) = (params.from_ledger, params.to_ledger) {
        if from > to {
            return Err(AppError::Validation(
                "from_ledger must be <= to_ledger".to_string(),
            ));
        }
    }

    let limit = params.limit();
    let offset = params.offset();
    let exact = params.exact_count.unwrap_or(false);
    let columns = params.columns();

    // Build WHERE clause dynamically
    let mut conditions: Vec<String> = Vec::new();
    let mut bind_idx: i32 = 1;

    if params.event_type.is_some() {
        conditions.push(format!("event_type = ${bind_idx}"));
        bind_idx += 1;
    }
    if params.from_ledger.is_some() {
        conditions.push(format!("ledger >= ${bind_idx}"));
        bind_idx += 1;
    }
    if params.to_ledger.is_some() {
        conditions.push(format!("ledger <= ${bind_idx}"));
        bind_idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let query_str = format!(
        "SELECT {} FROM events {} ORDER BY ledger DESC LIMIT ${} OFFSET ${}",
        columns.join(", "),
        where_clause,
        bind_idx,
        bind_idx + 1,
    );

    let mut q = sqlx::query(&query_str);
    if let Some(ref et) = params.event_type { q = q.bind(et); }
    if let Some(fl) = params.from_ledger { q = q.bind(fl); }
    if let Some(tl) = params.to_ledger { q = q.bind(tl); }
    q = q.bind(limit).bind(offset);

    let rows = q.fetch_all(&state.pool).await?;

    let mut events = Vec::new();
    for row in rows {
        let mut event = serde_json::Map::new();
        for &col in &columns {
            match col {
                "id" => { event.insert(col.to_string(), json!(row.try_get::<Uuid, _>(col)?)); }
                "contract_id" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "event_type" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "tx_hash" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "ledger" => { event.insert(col.to_string(), json!(row.try_get::<i64, _>(col)?)); }
                "timestamp" => { event.insert(col.to_string(), json!(row.try_get::<DateTime<Utc>, _>(col)?)); }
                "event_data" => { event.insert(col.to_string(), row.try_get::<Value, _>(col)?); }
                "created_at" => { event.insert(col.to_string(), json!(row.try_get::<DateTime<Utc>, _>(col)?)); }
                _ => {}
            }
        }
        events.push(Value::Object(event));
    }

    let (total, approximate): (i64, bool) = if exact {
        let count_str = format!("SELECT COUNT(*) FROM events {}", where_clause);
        let mut cq = sqlx::query_scalar::<_, i64>(&count_str);
        if let Some(ref et) = params.event_type { cq = cq.bind(et); }
        if let Some(fl) = params.from_ledger { cq = cq.bind(fl); }
        if let Some(tl) = params.to_ledger { cq = cq.bind(tl); }
        let count = cq.fetch_one(&state.pool).await?;
        (count, false)
    } else if where_clause.is_empty() {
        let count: i64 = sqlx::query_scalar(
            "SELECT reltuples::bigint FROM pg_class WHERE relname = 'events'",
        )
        .fetch_one(&state.pool)
        .await?;
        (count, true)
    } else {
        // Filtered queries always use exact count — approximate stats don't apply to subsets
        let count_str = format!("SELECT COUNT(*) FROM events {}", where_clause);
        let mut cq = sqlx::query_scalar::<_, i64>(&count_str);
        if let Some(ref et) = params.event_type { cq = cq.bind(et); }
        if let Some(fl) = params.from_ledger { cq = cq.bind(fl); }
        if let Some(tl) = params.to_ledger { cq = cq.bind(tl); }
        let count = cq.fetch_one(&state.pool).await?;
        (count, false)
    };

    Ok(Json(json!({
        "data": events,
        "total": total,
        "page": params.page.unwrap_or(1),
        "limit": limit,
        "approximate": approximate
    })))
}

#[utoipa::path(
    get,
    path = "/v1/events/contract/{contract_id}",
    tag = "events",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID (56-char, starts with C)"),
    ),
    responses(
        (status = 200, description = "Events for the given contract"),
        (status = 400, description = "Invalid contract_id format"),
        (status = 404, description = "No events found for contract"),
    )
)]
pub async fn get_events_by_contract(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    
    let limit = params.limit();
    let offset = params.offset();
    let columns = params.columns();

    let query_str = format!(
        "SELECT {} FROM events WHERE contract_id = $1 ORDER BY ledger DESC LIMIT $2 OFFSET $3",
        columns.join(", ")
    );

    let rows = sqlx::query(&query_str)
        .bind(&contract_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?;

    if rows.is_empty() {
        return Err(AppError::NotFound);
    }

    let mut events = Vec::new();
    for row in rows {
        let mut event = serde_json::Map::new();
        for &col in &columns {
            match col {
                "id" => { event.insert(col.to_string(), json!(row.try_get::<Uuid, _>(col)?)); }
                "contract_id" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "event_type" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "tx_hash" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "ledger" => { event.insert(col.to_string(), json!(row.try_get::<i64, _>(col)?)); }
                "timestamp" => { event.insert(col.to_string(), json!(row.try_get::<DateTime<Utc>, _>(col)?)); }
                "event_data" => { event.insert(col.to_string(), row.try_get::<Value, _>(col)?); }
                "created_at" => { event.insert(col.to_string(), json!(row.try_get::<DateTime<Utc>, _>(col)?)); }
                _ => {}
            }
        }
        events.push(Value::Object(event));
    }

    Ok(Json(json!({ "data": events, "contract_id": contract_id })))
}

#[utoipa::path(
    get,
    path = "/v1/events/tx/{tx_hash}",
    tag = "events",
    params(
        ("tx_hash" = String, Path, description = "Transaction hash (64 lowercase hex chars)"),
    ),
    responses(
        (status = 200, description = "Events for the given transaction (empty array if none)"),
        (status = 400, description = "Invalid tx_hash format"),
    )
)]
pub async fn get_events_by_tx(
    State(state): State<AppState>,
    Path(tx_hash): Path<String>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, AppError> {
    validate_tx_hash(&tx_hash)?;
    
    let columns = params.columns();
    let query_str = format!(
        "SELECT {} FROM events WHERE tx_hash = $1 ORDER BY ledger DESC",
        columns.join(", ")
    );

    let rows = sqlx::query(&query_str)
        .bind(&tx_hash)
        .fetch_all(&state.pool)
        .await?;

    let mut events = Vec::new();
    for row in rows {
        let mut event = serde_json::Map::new();
        for &col in &columns {
            match col {
                "id" => { event.insert(col.to_string(), json!(row.try_get::<Uuid, _>(col)?)); }
                "contract_id" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "event_type" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "tx_hash" => { event.insert(col.to_string(), json!(row.try_get::<String, _>(col)?)); }
                "ledger" => { event.insert(col.to_string(), json!(row.try_get::<i64, _>(col)?)); }
                "timestamp" => { event.insert(col.to_string(), json!(row.try_get::<DateTime<Utc>, _>(col)?)); }
                "event_data" => { event.insert(col.to_string(), row.try_get::<Value, _>(col)?); }
                "created_at" => { event.insert(col.to_string(), json!(row.try_get::<DateTime<Utc>, _>(col)?)); }
                _ => {}
            }
        }
        events.push(Value::Object(event));
    }

    Ok(Json(json!({ "data": events, "tx_hash": tx_hash })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use sqlx::PgPool;
    use std::sync::Arc;
    use tower::ServiceExt;
    use crate::config::{HealthState, IndexerState};

    fn create_test_router(pool: PgPool) -> axum::Router {
        let health_state = Arc::new(HealthState::new(60));
        let indexer_state = Arc::new(IndexerState::new());
        let prometheus_handle = crate::metrics::init_metrics();
        crate::routes::create_router(pool, None, &[], 60, health_state, indexer_state, prometheus_handle)
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
    async fn get_events_by_contract_no_events_returns_200_empty_data(pool: PgPool) {
        let app = create_test_router(pool);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events/contract/unknown_contract_no_events_deadbeef")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["data"], json!([]));
        assert_eq!(v["contract_id"], json!("unknown_contract_no_events_deadbeef"));
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
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn tx_hash_uppercase_hex_returns_400(pool: PgPool) {
        let app = create_test_router(pool);
        let uppercase_hex = "A".repeat(64);

        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/events/tx/{}", uppercase_hex))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["error"], "invalid tx_hash format");
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events").body(Body::empty()).unwrap())
            .await.unwrap();
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

        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?limit=20").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["total"].as_u64().is_some()); // Can be approximate or exact
        assert!(v["total"].as_u64().is_some());
        assert_eq!(v["data"].as_array().unwrap().len(), 3);

        // 3. Multi-page (limit 2, total 3)
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?limit=2&page=1").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert!(v["total"].as_u64().is_some());
        assert_eq!(v["data"].as_array().unwrap().len(), 2);

        let response = app
            .oneshot(Request::builder().uri("/v1/events?limit=2&page=2").body(Body::empty()).unwrap())
            .await.unwrap();
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
        let app = crate::routes::create_router(pool, None, &[], 60, health_state, indexer_state, prometheus_handle);

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
        assert_eq!(v["db"], "unreachable");
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
        assert!(v["error"].as_str().unwrap().contains("event_type must be one of"));
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
        assert!(v["error"].as_str().unwrap().contains("from_ledger must be <= to_ledger"));
    }

    // Events by contract tests - Happy path
    #[sqlx::test(migrations = "./migrations")]
    async fn get_events_by_contract_with_data_returns_200(pool: PgPool) {
        let app = create_test_router(pool.clone());
        let contract_id = "C1234567890123456789012345678901234567890123456789012345";
        
        // Insert test data
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
             VALUES ($1, $2, $3, $4, $5, $6)"
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?limit=1").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["limit"], 1);
        assert_eq!(v["data"].as_array().unwrap().len(), 1);
        assert_eq!(v["total"], 25);

        // Test limit boundary: limit=100 (maximum)
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?limit=100").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["limit"], 100);
        assert_eq!(v["data"].as_array().unwrap().len(), 25); // All events
        assert_eq!(v["total"], 25);

        // Test page boundary: page=1, limit=10
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?page=1&limit=10").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["page"], 1);
        assert_eq!(v["limit"], 10);
        assert_eq!(v["data"].as_array().unwrap().len(), 10);

        // Test page boundary: page=3, limit=10 (last page with 5 items)
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?page=3&limit=10").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["page"], 3);
        assert_eq!(v["limit"], 10);
        assert_eq!(v["data"].as_array().unwrap().len(), 5);

        // Test page boundary: page=4, limit=10 (beyond last page, empty)
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?page=4&limit=10").body(Body::empty()).unwrap())
            .await.unwrap();
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?limit=0").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["limit"], 1);

        // Test limit=200 gets clamped to 100
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?limit=200").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["limit"], 100);

        // Test page=0 gets treated as page=1
        let response = app
            .oneshot(Request::builder().uri("/v1/events?page=0").body(Body::empty()).unwrap())
            .await.unwrap();
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["approximate"], true);
        // Approximate count may not be exact but should be reasonable
        let approx_count = v["total"].as_i64().unwrap();
        assert!(approx_count >= 0); // Should be non-negative

        // Test exact count
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?exact_count=true").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["approximate"], false);
        assert_eq!(v["total"], 15); // Exact count should match

        // Test filtered queries always use exact count
        let response = app
            .oneshot(Request::builder().uri("/v1/events?event_type=contract").body(Body::empty()).unwrap())
            .await.unwrap();
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?event_type=contract&limit=3").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["data"].as_array().unwrap().len(), 3);
        assert_eq!(v["total"], 5); // 5 contract events
        assert_eq!(v["approximate"], false); // Filtered queries use exact count

        // Test pagination with ledger range filter
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?from_ledger=2&to_ledger=8&limit=5").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["data"].as_array().unwrap().len(), 5);
        assert_eq!(v["total"], 7); // Events with ledger 2-8
        assert_eq!(v["approximate"], false); // Filtered queries use exact count

        // Test pagination with both filters
        let response = app
            .oneshot(Request::builder().uri("/v1/events?event_type=contract&from_ledger=0&to_ledger=6&limit=10").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["data"].as_array().unwrap().len(), 4); // Contract events with ledger 0-6
        assert_eq!(v["total"], 4);
        assert_eq!(v["approximate"], false);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn pagination_fields_filtering(pool: PgPool) {
        let app = create_test_router(pool.clone());
        
        // Insert test data
        sqlx::query(
            "INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data) 
             VALUES ($1, $2, $3, $4, $5, $6)"
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?fields=ledger").body(Body::empty()).unwrap())
            .await.unwrap();
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
        let response = app.clone()
            .oneshot(Request::builder().uri("/v1/events?fields=ledger,contract_id,event_type").body(Body::empty()).unwrap())
            .await.unwrap();
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
            .oneshot(Request::builder().uri("/v1/events?fields=ledger,invalid_field,contract_id").body(Body::empty()).unwrap())
            .await.unwrap();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        let event = &v["data"][0];
        assert!(event.get("ledger").is_some());
        assert!(event.get("contract_id").is_some());
        assert!(event.get("invalid_field").is_none());

        // Test empty fields filter (should return all fields)
        let response = app
            .oneshot(Request::builder().uri("/v1/events?fields=").body(Body::empty()).unwrap())
            .await.unwrap();
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
}
