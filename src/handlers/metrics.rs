//! Metrics and operational-status handlers: Prometheus scrape endpoint,
//! service status, connection-pool and planner statistics, SLO
//! reporting, replication status and rate-limit introspection.

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
    crate::pool_management::get_pool_config(&state.db, state.pool_stats.clone(), state.config.db.max_connections)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Get pool configuration and generate tuning recommendations
#[utoipa::path(
    get,
    path = "/v1/admin/pool-config",
    tag = "admin",
    responses(
        (status = 200, description = "Pool configuration retrieved", body = PoolTuningGuide),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_pool_tuning_guide(
    State(state): State<AppState>,
) -> Result<Json<crate::models::PoolTuningGuide>, (StatusCode, Json<Value>)> {
    let pool_stats = crate::pool_management::get_pool_config(&state.db, state.pool_stats.clone(), state.config.db.max_connections)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    let current_config = crate::models::PoolConfig {
        max_connections: state.config.db.max_connections,
        min_connections: state.config.db.min_connections.unwrap_or(1),
        connection_timeout_secs: 30,
        idle_timeout_secs: 600,
        exhaustion_threshold: 0.9,
        sample_interval_secs: 15,
    };

    let guide = crate::pool_management::generate_tuning_guide(&pool_stats, &current_config);
    Ok(Json(guide))
}

/// Get pool health status
#[utoipa::path(
    get,
    path = "/v1/admin/pool-config/health",
    tag = "admin",
    responses(
        (status = 200, description = "Pool health status", body = Value),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_pool_health(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    crate::pool_management::check_pool_health(
        &state.db,
        state.pool_stats.clone(),
        state.config.db.max_connections,
        0.9,
    )
    .await
    .map(Json)
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Get comprehensive statistics report for all tables
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/report",
    tag = "admin",
    responses(
        (status = 200, description = "Statistics report for all tables", body = Vec<StatisticsReportItem>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to generate report", body = ErrorResponse),
    )
)]
pub async fn get_statistics_report(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::StatisticsReportItem>>, (StatusCode, Json<Value>)> {
    crate::statistics_management::get_statistics_report(&state.db)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Detect which tables have stale statistics
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/stale",
    tag = "admin",
    responses(
        (status = 200, description = "Stale statistics detection results", body = Vec<StalenessDetection>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to detect stale statistics", body = ErrorResponse),
    )
)]
pub async fn detect_stale_statistics(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::models::StalenessDetection>>, (StatusCode, Json<Value>)> {
    crate::statistics_management::detect_stale_statistics(&state.db)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))
}

/// Get overall statistics health score
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/health",
    tag = "admin",
    responses(
        (status = 200, description = "Statistics health score (0-100)", body = StatisticsHealth),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to get health score", body = ErrorResponse),
    )
)]
pub async fn get_statistics_health(
    State(state): State<AppState>,
) -> Result<Json<crate::models::StatisticsHealth>, (StatusCode, Json<Value>)> {
    let health_score = crate::statistics_management::get_statistics_health_score(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    let stale = crate::statistics_management::detect_stale_statistics(&state.db)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    let stale_count = stale.iter().filter(|s| s.is_stale).count();
    let total_count = stale.len();

    let status = if health_score >= 80 {
        "healthy"
    } else if health_score >= 60 {
        "degraded"
    } else {
        "critical"
    };

    Ok(Json(crate::models::StatisticsHealth {
        health_score,
        status: status.to_string(),
        stale_tables_count: stale_count as u32,
        total_tables: total_count as u32,
        timestamp: chrono::Utc::now(),
    }))
}

/// Refresh statistics for all tables or a specific table
#[utoipa::path(
    post,
    path = "/v1/admin/statistics/refresh",
    tag = "admin",
    params(
        ("table_name" = Option<String>, Query, description = "Optional table name to refresh only one table")
    ),
    responses(
        (status = 202, description = "Statistics refresh job started"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to refresh statistics", body = ErrorResponse),
    )
)]
pub async fn refresh_statistics(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let table_name = params.get("table_name").map(|s| s.as_str());

    match crate::statistics_management::refresh_table_statistics(&state.db, table_name).await {
        Ok(results) => {
            let summary = if let Some(table) = table_name {
                format!("Refreshed statistics for table: {}", table)
            } else {
                format!("Refreshed statistics for {} tables", results.len())
            };

            Ok((
                StatusCode::ACCEPTED,
                Json(json!({
                    "message": summary,
                    "results_count": results.len(),
                    "timestamp": chrono::Utc::now()
                })),
            ))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// Get recent statistics analysis jobs
#[utoipa::path(
    get,
    path = "/v1/admin/statistics/jobs",
    tag = "admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of jobs to return (default 50)")
    ),
    responses(
        (status = 200, description = "Recent analysis jobs", body = Vec<StatisticsAnalysisJobInfo>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Failed to retrieve jobs", body = ErrorResponse),
    )
)]
pub async fn get_analysis_jobs(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<crate::models::StatisticsAnalysisJobInfo>>, (StatusCode, Json<Value>)> {
    let limit = params
        .get("limit")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(50);

    crate::statistics_management::get_recent_analysis_jobs(&state.db, limit)
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

// ── Issue #817: Adaptive pool tuning handlers ─────────────────────────────

/// GET /v1/admin/pool-config/adaptive
///
/// Returns the latest adaptive tuner snapshot, current runtime config,
/// and advanced pool counters (timeouts, health failures, stale cleaned).
pub async fn get_adaptive_pool_status(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let status = crate::adaptive_pool::AdaptivePoolStatus::from_state(&state.adaptive_pool);
    Ok(Json(serde_json::to_value(&status).unwrap_or_else(|_| json!({"error": "serialization failed"}))))
}

/// PUT /v1/admin/pool-config/adaptive/config
///
/// Hot-reload the adaptive pool configuration at runtime without a restart.
/// Saves the current config to a rollback history (up to 5 versions).
///
/// Body: JSON matching `AdaptivePoolConfig`.
pub async fn update_adaptive_pool_config(
    State(state): State<AppState>,
    Json(new_config): Json<crate::adaptive_pool::AdaptivePoolConfig>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state.adaptive_pool.apply_config(new_config).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e })))
    })?;
    let current = state.adaptive_pool.current_config();
    Ok(Json(json!({
        "status": "applied",
        "config_version": current.config_version,
        "config_variant": current.config_variant,
    })))
}

/// POST /v1/admin/pool-config/adaptive/rollback
///
/// Roll back to the previous adaptive pool configuration version.
pub async fn rollback_adaptive_pool_config(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let rolled_back = state.adaptive_pool.rollback().map_err(|e| {
        (StatusCode::BAD_REQUEST, Json(json!({ "error": e })))
    })?;
    Ok(Json(json!({
        "status": "rolled_back",
        "config_version": rolled_back.config_version,
        "config_variant": rolled_back.config_variant,
    })))
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
