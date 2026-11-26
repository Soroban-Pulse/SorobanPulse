//! Metrics and operational-status handlers: Prometheus scrape, status, pool/replication/SLO/rate-limit reporting (Issue #1241).

#[allow(unused_imports)]
use super::*;

#[utoipa::path(
    get,
    path = "/v1/status",
    tag = "system",
    responses(
        (status = 200, description = "Indexer operational status", body = serde_json::Value),
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

    let indexer_mode = if state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        "active"
    } else {
        "read_only"
    };

    let indexer_paused = state.indexer_state.is_paused.load(Ordering::Relaxed);

    let total_events: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM events")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);

    // Query event counts by type
    let events_by_type_rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT event_type, COUNT(*) as count FROM events GROUP BY event_type")
            .fetch_all(&state.pool)
            .await
            .unwrap_or_default();

    // Build events_by_type object with all event types (defaulting to 0 if not present)
    let mut events_by_type = serde_json::json!({
        "contract": 0i64,
        "diagnostic": 0i64,
        "system": 0i64,
    });

    for (event_type, count) in events_by_type_rows {
        if let Some(obj) = events_by_type.as_object_mut() {
            obj.insert(event_type, serde_json::json!(count));
        }
    }

    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": uptime_secs,
        "current_ledger": current_ledger,
        "latest_ledger": latest_ledger,
        "lag_ledgers": lag_ledgers,
        "total_events": total_events,
        "events_by_type": events_by_type,
        "indexer_status": indexer_status,
        "indexer_mode": indexer_mode,
        "indexer_paused": indexer_paused,
        "rpc_version": crate::rpc_meta::current_version(),
        "gaps": crate::rpc_meta::list_gaps(&state.pool).await,
    }))
}

/// GET /v1/admin/indexer/gaps
pub async fn get_indexer_gaps(State(state): State<AppState>) -> Json<Value> {
    Json(json!({ "gaps": crate::rpc_meta::list_gaps(&state.pool).await }))
}

pub async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    crate::metrics::update_db_pool_metrics(&state.pool);
    state.prometheus_handle.render()
}

/// Operational cost report with per-component breakdown and a linear forecast
/// (Issue #1264).
///
/// `GET /v1/admin/costs/report`
///
/// Aggregates the hourly `cost_snapshots` rows over the requested window
/// (default: last 24 hours) and returns the cost breakdown by component
/// (`rpc`, `db`, `notification`) together with a forecast for the next window
/// produced by the linear model in `costs::forecast`.
#[utoipa::path(
    get,
    path = "/v1/admin/costs/report",
    tag = "admin",
    params(
        ("hours" = Option<i64>, Query, description = "Lookback window in hours (default 24)"),
        ("forecast_hours" = Option<i64>, Query, description = "Forecast horizon in hours (default 24)"),
    ),
    responses(
        (status = 200, description = "Cost breakdown by component with forecast", body = Value),
        (status = 400, description = "Invalid query parameters", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to generate cost report", body = ErrorResponse),
    )
)]
pub async fn get_cost_report(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let hours = match params.get("hours") {
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|h| *h > 0 && *h <= 24 * 90)
            .ok_or_else(|| {
                AppError::Validation("hours must be an integer between 1 and 2160".to_string())
            })?,
        None => 24,
    };
    let forecast_hours = match params.get("forecast_hours") {
        Some(raw) => raw
            .parse::<i64>()
            .ok()
            .filter(|h| *h > 0 && *h <= 24 * 90)
            .ok_or_else(|| {
                AppError::Validation(
                    "forecast_hours must be an integer between 1 and 2160".to_string(),
                )
            })?,
        None => 24,
    };

    let report = crate::costs::report::generate_report(&state.read_pool, hours)
        .await
        .map_err(|e| AppError::Internal(format!("failed to generate cost report: {e}")))?;

    let forecast = crate::costs::report::generate_forecast(&state.read_pool, forecast_hours)
        .await
        .map_err(|e| AppError::Internal(format!("failed to generate cost forecast: {e}")))?;

    Ok(Json(json!({
        "window_hours": hours,
        "total_cost": report.total_cost,
        "breakdown": report.by_component,
        "snapshots": report.snapshot_count,
        "forecast": {
            "horizon_hours": forecast_hours,
            "predicted_cost": forecast.predicted_cost,
            "slope": forecast.slope,
            "intercept": forecast.intercept,
            "confidence": forecast.confidence,
        },
    })))
}

/// Get current database connection pool statistics
#[utoipa::path(
    get,
    path = "/v1/admin/pool-config/statistics",
    tag = "admin",
    responses(
        (status = 200, description = "Pool statistics retrieved", body = PoolStatistics),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_pool_statistics(
    State(state): State<AppState>,
) -> Result<Json<crate::models::PoolStatistics>, (StatusCode, Json<Value>)> {
    crate::db_pool::management::get_pool_config(&state.db, state.pool_stats.clone(), state.config.db_max_connections)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Return current PostgreSQL streaming replication status.
#[utoipa::path(
    get,
    path = "/v1/admin/replication/status",
    tag = "admin",
    responses(
        (status = 200, description = "Replication status"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_replication_status(State(state): State<AppState>) -> Json<Value> {
    let replicas = crate::replica_monitor::query_replication_status(&state.pool).await;
    Json(json!({
        "replica_count": replicas.len(),
        "replicas": replicas,
    }))
}

// ── Issue #610: GET /v1/admin/compression-stats ──────────────────────────────

/// Return compression statistics and trigger a backfill migration when enabled.
///
/// `GET /v1/admin/compression-stats`
#[utoipa::path(
    get,
    path = "/v1/admin/compression-stats",
    tag = "admin",
    responses(
        (status = 200, description = "Compression statistics"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn compression_stats(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    // query_scalar! gives compile-time SQL verification via the .sqlx offline cache.
    let total: i64 = sqlx::query_scalar!("SELECT COUNT(*) FROM events")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    let compressed: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM events WHERE event_data_compressed IS NOT NULL")
            .fetch_one(&state.pool)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    let enabled = state.config.event_compression_enabled;
    Ok(Json(json!({
        "enabled": enabled,
        "total_events": total,
        "compressed_events": compressed,
        "uncompressed_events": total - compressed,
        "coverage_pct": if total > 0 { (compressed as f64 / total as f64) * 100.0 } else { 0.0 },
    })))
}

// ── Issue #696: SLI / SLO admin reporting endpoint ──────────────────────────

/// Return the current SLI / SLO aggregate report.
///
/// The report is generated in-memory by `crate::slo_tracker` from the rolling
/// sample buffers. It is the JSON counterpart of the Prometheus gauges
/// `soroban_pulse_slo_completion_ratio`, `soroban_pulse_slo_error_budget_remaining`,
/// and `soroban_pulse_slo_burn_rate` published by the background evaluator.
#[utoipa::path(
    get,
    path = "/v1/admin/slo/report",
    tag = "admin",
    responses(
        (status = 200, description = "Current SLO report for every tracked SLO"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 503, description = "SLO tracker not initialized"),
    )
)]
pub async fn get_slo_report() -> Result<Json<Value>, AppError> {
    let tracker = crate::slo_tracker::current().ok_or_else(|| {
        AppError::Internal(
            "SLO tracker not initialized — server is still starting up".to_string(),
        )
    })?;

    let report = {
        let guard = tracker.read().await;
        guard.generate_report()
    };
    Ok(Json(serde_json::to_value(&report).unwrap_or_else(|e| {
        json!({
            "error": "failed to serialize SLO report",
            "details": e.to_string(),
        })
    })))
}

/// Record an SLI sample against a named SLO. Used by internal callers and
/// by integration tests; not part of the documented admin surface but exposed
/// for symmetry with `get_slo_report`.
#[utoipa::path(
    post,
    path = "/v1/admin/slo/sample",
    tag = "admin",
    responses(
        (status = 202, description = "Sample accepted"),
        (status = 400, description = "Invalid request"),
        (status = 503, description = "SLO tracker not initialized"),
    )
)]
pub async fn record_slo_sample(
    Json(payload): Json<serde_json::Value>,
) -> Result<axum::http::StatusCode, AppError> {
    let slo_name = payload
        .get("slo")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("missing 'slo' field".to_string()))?;
    let value = payload
        .get("value")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| AppError::Validation("missing numeric 'value' field".to_string()))?;

    let tracker = crate::slo_tracker::current().ok_or_else(|| {
        AppError::Internal(
            "SLO tracker not initialized — server is still starting up".to_string(),
        )
    })?;

    crate::slo_tracker::record_sli_sample(&tracker, slo_name, value).await;
    Ok(axum::http::StatusCode::ACCEPTED)
}

// ── Issue #669: Per-API-Key Rate Limit Status Endpoint ──────────────────────

/// Get rate limit status for the authenticated API key.
///
/// Returns the current rate limit usage and remaining quota for the minute,
/// hour, and day sliding windows (when per-key limits are configured).
///
/// This endpoint does **not** increment the rate limit counter (read-only status check).
///
/// Requires authentication (API_KEY or X-Api-Key header).
#[utoipa::path(
    get,
    path = "/v1/rate-limit/status",
    tag = "system",
    responses(
        (status = 200, description = "Rate limit status for the authenticated API key"),
        (status = 401, description = "Unauthorized — API key required"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn get_rate_limit_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, AppError> {
    // Extract API key from headers (same logic as middleware::extract_api_key)
    let api_key = headers
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| headers.get("X-Api-Key").and_then(|h| h.to_str().ok()))
        .ok_or_else(|| AppError::Forbidden("API key required for rate limit status".to_string()))?;

    // Build RateLimitConfig from Config fields
    let rate_limit_config = crate::rate_limiter::RateLimitConfig::new(
        state.config.rate_limit_key_per_minute,
        state.config.rate_limit_key_per_hour,
        state.config.rate_limit_key_per_day,
        state.config.rate_limit_key_per_month,
    );

    // Get status (no counter increment)
    let status = crate::rate_limiter::get_rate_limit_status(
        &state.pool,
        api_key,
        &rate_limit_config,
    )
    .await
    .map_err(|e| AppError::Internal(format!("Failed to check rate limit status: {}", e)))?;

    Ok(Json(serde_json::to_value(&status).unwrap()))
}
