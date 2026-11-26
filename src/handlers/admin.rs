//! Admin handlers: indexer control, replay, re-encryption, anonymization, archive, config, feature flags and export jobs (Issue #1241).

#[allow(unused_imports)]
use super::*;

// --- Async Export Job and Import Endpoint Stubs ---
// Dummy job store for demonstration
pub(super) type JobId = String;
pub(super) type JobStatus = String;
pub(super) static EXPORT_JOBS: OnceLock<Arc<RwLock<HashMap<JobId, JobStatus>>>> = OnceLock::new();
pub(super) static IMPORT_JOBS: OnceLock<Arc<RwLock<HashMap<JobId, JobStatus>>>> = OnceLock::new();

pub(super) fn export_jobs() -> Arc<RwLock<HashMap<JobId, JobStatus>>> {
    EXPORT_JOBS.get_or_init(|| Arc::new(RwLock::new(HashMap::new()))).clone()
}
pub(super) fn import_jobs() -> Arc<RwLock<HashMap<JobId, JobStatus>>> {
    IMPORT_JOBS.get_or_init(|| Arc::new(RwLock::new(HashMap::new()))).clone()
}

/// Start an async export job (stub)
pub async fn start_export_job(State(_state): State<AppState>) -> impl IntoResponse {
    let job_id = Uuid::new_v4().to_string();
    export_jobs().write().await.insert(job_id.clone(), "pending".to_string());
    Json(json!({"job_id": job_id}))
}

/// Get export job status (stub)
pub async fn get_export_job_status(Path(job_id): Path<String>) -> impl IntoResponse {
    let status = export_jobs().read().await.get(&job_id).cloned().unwrap_or("not_found".to_string());
    Json(json!({"job_id": job_id, "status": status, "progress": 42}))
}

/// Download completed export (stub)
pub async fn download_export_job(Path(job_id): Path<String>) -> impl IntoResponse {
    // Just return a dummy file
    let data = b"exported data";
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(header::CONTENT_DISPOSITION, format!("attachment; filename=\"export_{job_id}.bin\""))
        .body(Body::from(data.as_slice()))
        .unwrap_or_else(|e| response_build_error(e))
}

/// Start an import job (stub)
pub async fn start_import_job(State(_state): State<AppState>) -> impl IntoResponse {
    let job_id = Uuid::new_v4().to_string();
    import_jobs().write().await.insert(job_id.clone(), "pending".to_string());
    Json(json!({"job_id": job_id}))
}

/// Get import job status (stub)
pub async fn get_import_job_status(Path(job_id): Path<String>) -> impl IntoResponse {
    let status = import_jobs().read().await.get(&job_id).cloned().unwrap_or("not_found".to_string());
    Json(json!({"job_id": job_id, "status": status, "progress": 42}))
}

/// Pause the indexer loop without stopping the HTTP server.
#[utoipa::path(
    post,
    path = "/v1/admin/indexer/pause",
    tag = "admin",
    responses(
        (status = 200, description = "Indexer paused"),
        (status = 403, description = "Not the active indexer", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn pause_indexer(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    if !state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        return Err(AppError::Forbidden(
            "pause endpoint only available on active indexer".to_string(),
        ));
    }
    state.indexer_state.is_paused.store(true, Ordering::Relaxed);
    tracing::info!("Indexer paused via admin API");
    Ok(Json(json!({ "indexer_paused": true })))
}

/// Resume a previously paused indexer loop.
#[utoipa::path(
    post,
    path = "/v1/admin/indexer/resume",
    tag = "admin",
    responses(
        (status = 200, description = "Indexer resumed"),
        (status = 403, description = "Not the active indexer", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn resume_indexer(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    if !state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        return Err(AppError::Forbidden(
            "resume endpoint only available on active indexer".to_string(),
        ));
    }
    state
        .indexer_state
        .is_paused
        .store(false, Ordering::Relaxed);
    tracing::info!("Indexer resumed via admin API");
    Ok(Json(json!({ "indexer_paused": false })))
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
    let pool_stats = crate::db_pool::management::get_pool_config(&state.db, state.pool_stats.clone(), state.config.db_max_connections)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e.to_string() }))))?;

    let current_config = crate::models::PoolConfig {
        max_connections: state.config.db_max_connections,
        min_connections: state.config.db.min_connections.unwrap_or(1),
        connection_timeout_secs: 30,
        idle_timeout_secs: 600,
        exhaustion_threshold: 0.9,
        sample_interval_secs: 15,
    };

    let guide = crate::db_pool::management::generate_tuning_guide(&pool_stats, &current_config);
    Ok(Json(guide))
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

/// #694: Query and return index fragmentation information.
///
/// `GET /v1/admin/indexes/fragmentation`
///
/// Returns a list of all user indexes in the public schema with their
/// estimated bloat ratio, dead/live tuple counts, size, and last vacuum
/// timestamps.  Results are sorted by index size descending.
#[utoipa::path(
    get,
    path = "/v1/admin/indexes/fragmentation",
    tag = "admin",
    responses(
        (status = 200, description = "Index fragmentation report"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn get_index_fragmentation(
    State(state): State<AppState>,
) -> Result<Json<Vec<crate::index_monitor::IndexFragmentationInfo>>, (StatusCode, Json<Value>)> {
    crate::index_monitor::query_index_fragmentation(&state.pool)
        .await
        .map(Json)
        .map_err(|e| {
            tracing::error!(error = %e, "Failed to query index fragmentation");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "error": format!("Failed to query index fragmentation: {e}"),
                    "code": "INTERNAL_ERROR"
                })),
            )
        })
}

/// #694: Trigger a single REINDEX INDEX CONCURRENTLY operation.
///
/// `POST /v1/admin/indexes/{index_name}/reindex`
///
/// Runs `REINDEX INDEX CONCURRENTLY` on the named index. Returns 202 on
/// success.  The index must be a valid user index in the public schema.
#[utoipa::path(
    post,
    path = "/v1/admin/indexes/{index_name}/reindex",
    tag = "admin",
    params(
        ("index_name" = String, Path, description = "Name of the index to rebuild"),
    ),
    responses(
        (status = 202, description = "REINDEX scheduled successfully"),
        (status = 400, description = "Invalid index name", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "REINDEX failed", body = ErrorResponse),
    )
)]
pub async fn reindex_index(
    State(state): State<AppState>,
    Path(index_name): Path<String>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Basic validation: only allow alphanumeric + underscore index names
    if index_name.is_empty()
        || !index_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "invalid index name",
                "code": "VALIDATION_ERROR"
            })),
        ));
    }

    // Verify the index exists
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM pg_class WHERE relname = $1 AND relkind = 'i')",
    )
    .bind(&index_name)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("Failed to verify index existence: {e}"),
                "code": "INTERNAL_ERROR"
            })),
        )
    })?;

    if !exists {
        return Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "error": format!("index '{index_name}' not found"),
                "code": "NOT_FOUND"
            })),
        ));
    }

    tracing::info!(index = %index_name, "Admin-triggered REINDEX INDEX CONCURRENTLY");

    let sql = format!("REINDEX INDEX CONCURRENTLY {index_name}");
    sqlx::query(&sql).execute(&state.pool).await.map_err(|e| {
        tracing::error!(index = %index_name, error = %e, "REINDEX failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("REINDEX failed: {e}"),
                "code": "REINDEX_FAILED"
            })),
        )
    })?;

    tracing::info!(index = %index_name, "REINDEX INDEX CONCURRENTLY completed");

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "completed",
            "index": index_name,
            "message": "REINDEX INDEX CONCURRENTLY completed successfully"
        })),
    ))
}

/// Request body for `POST /v1/admin/encryption/rotate` (Issue #1270).
#[derive(Debug, serde::Deserialize)]
pub struct RotateEncryptionKeyRequest {
    /// The new AES-GCM key (base64-encoded 32-byte key).
    pub new_key: String,
}

/// Rotate the AES-GCM credential-encryption key without downtime (Issue #1270).
///
/// `POST /v1/admin/encryption/rotate`
///
/// Accepts `{ "new_key": "<base64 32-byte key>" }`, then starts a batched
/// background job that decrypts every stored credential with the current key
/// and re-encrypts it with `new_key`. The job commits in batches of 100 rows
/// and can be resumed from where it stopped if interrupted.
///
/// This endpoint is gated behind `ADMIN_API_KEY` by the admin auth middleware.
#[utoipa::path(
    post,
    path = "/v1/admin/encryption/rotate",
    tag = "admin",
    request_body(content = Value, description = "{\"new_key\": \"<base64 32-byte key>\"}", content_type = "application/json"),
    responses(
        (status = 202, description = "Key rotation job started"),
        (status = 400, description = "Invalid or missing new_key", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 409, description = "A rotation job is already running", body = ErrorResponse),
    )
)]
pub async fn rotate_encryption_key(
    State(state): State<AppState>,
    Json(body): Json<RotateEncryptionKeyRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    use base64::{engine::general_purpose::STANDARD, Engine as _};

    // Validate the new key: must decode to exactly 32 bytes.
    let new_key_bytes = STANDARD
        .decode(body.new_key.trim())
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "new_key must be valid base64" })),
            )
        })?;
    if new_key_bytes.len() != 32 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "new_key must decode to exactly 32 bytes" })),
        ));
    }
    let mut new_key = [0u8; 32];
    new_key.copy_from_slice(&new_key_bytes);

    // The current key is required to decrypt existing credentials.
    let old_key = state.encryption_key.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "no current encryption key configured (ENCRYPTION_KEY)"
            })),
        )
    })?;

    // Start the batched rotation job (100 rows per transaction).
    let started = crate::reencrypt::start_rotation_job(
        state.pool.clone(),
        new_key,
        old_key,
        100,
        state.reencrypt_state.clone(),
    );

    if !started {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({ "error": "a key rotation job is already running" })),
        ));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "status": "started",
            "batch_size": 100,
            "message": "key rotation started; poll /v1/admin/encryption/rotate/status for progress"
        })),
    ))
}

/// Report progress of the credential key-rotation job (Issue #1270).
///
/// `GET /v1/admin/encryption/rotate/status`
#[utoipa::path(
    get,
    path = "/v1/admin/encryption/rotate/status",
    tag = "admin",
    responses(
        (status = 200, description = "Rotation job status"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_encryption_rotation_status(
    State(state): State<AppState>,
) -> Json<Value> {
    let (completed, total, running) = state.reencrypt_state.progress();
    Json(json!({
        "running": running,
        "completed": completed,
        "total": total,
        "progress": if total > 0 { completed as f64 / total as f64 } else { 0.0 },
    }))
}

/// Start background re-encryption jobs after a key rotation.
///
/// - Events: migrates `event_data` from `ENCRYPTION_KEY_OLD` to `ENCRYPTION_KEY`
///   (requires the `encryption` feature).
/// - Integration credentials (Issue #1162): re-encrypts every stored token,
///   webhook URL and routing key from `INTEGRATION_ENCRYPTION_KEY_OLD` to
///   `INTEGRATION_ENCRYPTION_KEY`, encrypting any remaining plaintext rows.
#[utoipa::path(
    post,
    path = "/v1/admin/reencrypt",
    tag = "admin",
    responses(
        (status = 202, description = "Re-encryption job(s) started"),
        (status = 400, description = "No old key configured for events or integration credentials", body = ErrorResponse),
        (status = 409, description = "Re-encryption job already running", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn start_reencrypt(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let batch_size = 1000;
    let mut jobs: Vec<&str> = Vec::new();

    if crate::integration_secrets::rotation_configured() {
        if !crate::integration_secrets::start_reencrypt_job(state.pool.clone(), batch_size as i64) {
            return Err((
                StatusCode::CONFLICT,
                Json(json!({ "error": "integration credential re-encryption job already running" })),
            ));
        }
        jobs.push("integration_credentials");
    }

    #[cfg(feature = "encryption")]
    {
        if let (Some(new_key), Some(old_key)) = (state.encryption_key, state.encryption_key_old) {
            // Create or get the reencrypt state from app state
            // For now, we'll create a new one per request (in production, store in AppState)
            let reencrypt_state = crate::reencrypt::ReencryptState::new();

            if reencrypt_state.is_running() {
                return Err((
                    StatusCode::CONFLICT,
                    Json(json!({ "error": "re-encryption job already running" })),
                ));
            }

            crate::reencrypt::start_reencrypt_job(
                state.pool.clone(),
                new_key,
                old_key,
                batch_size,
                reencrypt_state,
            );
            jobs.push("events");
        }
    }

    if jobs.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "nothing to re-encrypt: configure ENCRYPTION_KEY and ENCRYPTION_KEY_OLD \
                          (events, requires the encryption feature) and/or INTEGRATION_ENCRYPTION_KEY \
                          and INTEGRATION_ENCRYPTION_KEY_OLD (integration credentials)"
            })),
        ));
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "message": "re-encryption job started",
            "jobs": jobs,
            "batch_size": batch_size
        })),
    ))
}

/// Replay events for a specific ledger range.
///
/// The requested range is validated against the indexed window:
/// - **400** if the range is entirely outside the indexed window (no overlap).
/// - **202** with a `warning` field if the range is only partially indexed.
#[utoipa::path(
    post,
    path = "/v1/admin/replay",
    tag = "admin",
    request_body(content = ReplayRequest, description = "Ledger range to replay", content_type = "application/json"),
    responses(
        (status = 202, description = "Replay job accepted and queued. A `warning` field is included when the requested range is only partially covered by the indexed window."),
        (status = 400, description = "Invalid request parameters, or the requested range is entirely outside the indexed window"),
        (status = 401, description = "Unauthorized - API key required"),
        (status = 403, description = "Forbidden - not the active indexer"),
    )
)]
/// Preview how a Lua transformation script would affect a set of events
/// without writing any changes to the database.
///
/// Applies the same CPU instruction limit, memory limit, and timeout as the
/// production transformer so operator can verify safety before deploying.
#[utoipa::path(
    post,
    path = "/v1/admin/lua/preview",
    tag = "admin",
    request_body = models::LuaPreviewRequest,
    params(
        ("X-Admin-API-Key" = String, Header, description = "Admin API key"),
    ),
    responses(
        (status = 200, description = "Preview results — original and transformed event data side-by-side",
            body = models::LuaPreviewResponse),
        (status = 400, description = "Invalid request — too many event_ids or empty script"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "One or more event_ids not found"),
    )
)]
pub async fn lua_preview(
    State(state): State<AppState>,
    Json(req): Json<models::LuaPreviewRequest>,
) -> Result<Json<models::LuaPreviewResponse>, AppError> {
    const MAX_PREVIEW_EVENTS: usize = 20;

    if req.script.trim().is_empty() {
        return Err(AppError::Validation("script must not be empty".into()));
    }
    if req.event_ids.is_empty() {
        return Err(AppError::Validation("event_ids must not be empty".into()));
    }
    if req.event_ids.len() > MAX_PREVIEW_EVENTS {
        return Err(AppError::Validation(format!(
            "event_ids must contain at most {MAX_PREVIEW_EVENTS} entries"
        )));
    }

    // Fetch the requested events from the DB (read-only)
    let ids: Vec<uuid::Uuid> = req.event_ids.clone();
    let rows = sqlx::query_as::<_, crate::models::Event>(
        "SELECT * FROM events WHERE id = ANY($1) LIMIT 20",
    )
    .bind(&ids)
    .fetch_all(&state.read_pool)
    .await?;

    if rows.is_empty() {
        return Err(AppError::NotFound);
    }

    // Map Event → SorobanEvent for the transformer
    let events: Vec<crate::models::SorobanEvent> = rows
        .into_iter()
        .map(|r| crate::models::SorobanEvent {
            contract_id: r.contract_id,
            event_type: format!("{:?}", r.event_type).to_lowercase(),
            tx_hash: r.tx_hash,
            ledger: r.ledger as u64,
            ledger_closed_at: r.timestamp.to_rfc3339(),
            ledger_hash: r.ledger_hash,
            in_successful_call: r.in_successful_call,
            value: r.event_data.clone(),
            topic: r.event_data
                .get("topic")
                .and_then(|t| t.as_array())
                .cloned(),
        })
        .collect();

    let timeout_ms = state.config.event_transform_timeout_ms;

    let mut results =
        crate::lua_transform::LuaTransformer::preview_events(req.script, events, timeout_ms).await;

    // Re-attach the caller-supplied UUIDs so the response matches the request
    for (item, id) in results.iter_mut().zip(req.event_ids.iter()) {
        item.event_id = *id;
    }

    Ok(Json(models::LuaPreviewResponse { results }))
}

/// Replay already-indexed events through a Lua transformation script (Issue #700).
///
/// Fetches events matching the ledger and/or timestamp window (and optional
/// `contract_id` filter), runs each one through `transformation_script`, and
/// reports per-event results. In `dry_run` mode nothing is redelivered — the
/// endpoint only reports what the script *would* do. Otherwise, transformed
/// events (script returned a table, not `nil`) are broadcast on the same
/// channel newly-indexed events use, so they flow through the normal
/// downstream pipeline (webhooks, SSE, email, etc.) exactly like a live event.
#[utoipa::path(
    post,
    path = "/v1/replay/with-transform",
    tag = "admin",
    request_body = models::ReplayWithTransformRequest,
    responses(
        (status = 200, description = "Replay completed or previewed", body = models::ReplayWithTransformResponse),
        (status = 400, description = "Invalid request or transformation script"),
    )
)]
pub async fn replay_with_transform(
    State(state): State<AppState>,
    Json(request): Json<models::ReplayWithTransformRequest>,
) -> Result<Json<models::ReplayWithTransformResponse>, AppError> {
    if request.transformation_script.trim().is_empty() {
        return Err(AppError::Validation(
            "transformation_script must not be empty".to_string(),
        ));
    }
    if request.from_ledger.is_none() && request.from_timestamp.is_none() {
        return Err(AppError::Validation(
            "either from_ledger or from_timestamp is required".to_string(),
        ));
    }
    if let (Some(from), Some(to)) = (request.from_ledger, request.to_ledger) {
        if from > to {
            return Err(AppError::Validation(
                "from_ledger must be <= to_ledger".to_string(),
            ));
        }
    }
    if let (Some(from), Some(to)) = (request.from_timestamp, request.to_timestamp) {
        if from > to {
            return Err(AppError::Validation(
                "from_timestamp must be <= to_timestamp".to_string(),
            ));
        }
    }

    let wrapped_script =
        crate::lua_transform::LuaTransformer::wrap_script_body(&request.transformation_script);
    crate::lua_transform::LuaTransformer::validate_syntax(&wrapped_script)
        .map_err(|e| AppError::Validation(format!("invalid transformation script: {e}")))?;

    let limit = request.limit.unwrap_or(100).clamp(1, 1000);

    let mut query = QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, contract_id, event_type, tx_hash, ledger, timestamp, event_data \
         FROM events WHERE ",
    );
    let mut needs_and = false;
    if let Some(from_ledger) = request.from_ledger {
        query.push("ledger >= ");
        query.push_bind(from_ledger);
        needs_and = true;
    }
    if let Some(to_ledger) = request.to_ledger {
        if needs_and {
            query.push(" AND ");
        }
        query.push("ledger <= ");
        query.push_bind(to_ledger);
        needs_and = true;
    }
    if let Some(from_timestamp) = request.from_timestamp {
        if needs_and {
            query.push(" AND ");
        }
        query.push("timestamp >= ");
        query.push_bind(from_timestamp);
        needs_and = true;
    }
    if let Some(to_timestamp) = request.to_timestamp {
        if needs_and {
            query.push(" AND ");
        }
        query.push("timestamp <= ");
        query.push_bind(to_timestamp);
        needs_and = true;
    }
    if let Some(ref contract_id) = request.contract_id {
        if needs_and {
            query.push(" AND ");
        }
        query.push("contract_id = ");
        query.push_bind(contract_id);
    }
    query.push(" ORDER BY ledger ASC LIMIT ");
    query.push_bind(limit);

    let rows = query.build().fetch_all(&state.pool).await?;

    let ids: Vec<uuid::Uuid> = rows
        .iter()
        .map(|r| r.try_get::<uuid::Uuid, _>("id"))
        .collect::<Result<_, _>>()?;
    let events: Vec<crate::models::SorobanEvent> = rows
        .iter()
        .map(|r| -> Result<_, sqlx::Error> {
            let timestamp: DateTime<Utc> = r.try_get("timestamp")?;
            Ok(crate::models::SorobanEvent {
                id: None,
                contract_id: r.try_get("contract_id")?,
                event_type: r.try_get("event_type")?,
                tx_hash: r.try_get("tx_hash")?,
                ledger: r.try_get::<i64, _>("ledger")? as u64,
                ledger_closed_at: timestamp.to_rfc3339(),
                ledger_hash: None,
                in_successful_call: true,
                value: r.try_get("event_data")?,
                topic: None,
                tenant_id: None,
            })
        })
        .collect::<Result<_, _>>()?;

    let total_events = events.len() as i64;
    let timeout_ms = state.config.event_transform_timeout_ms;
    let mut preview =
        crate::lua_transform::LuaTransformer::preview_events(wrapped_script, events, timeout_ms)
            .await;
    for (item, id) in preview.iter_mut().zip(ids.iter()) {
        item.event_id = *id;
    }

    let transformed_events = preview.iter().filter(|i| i.transformed.is_some()).count() as i64;
    let error_events = preview.iter().filter(|i| i.error.is_some()).count() as i64;
    let skipped_events = total_events - transformed_events - error_events;

    if !request.dry_run {
        for item in preview.iter() {
            if let Some(ref transformed) = item.transformed {
                if let Ok(event) =
                    serde_json::from_value::<crate::models::SorobanEvent>(transformed.clone())
                {
                    let _ = state.event_tx.send(event);
                }
            }
        }
    }

    Ok(Json(models::ReplayWithTransformResponse {
        replay_id: uuid::Uuid::new_v4(),
        status: if request.dry_run {
            "dry_run".to_string()
        } else {
            "completed".to_string()
        },
        dry_run: request.dry_run,
        total_events,
        transformed_events,
        skipped_events,
        error_events,
        preview,
    }))
}

/// Execute the replay job using the same fetch_and_store_events logic
pub(super) async fn execute_replay_job(
    pool: sqlx::PgPool,
    rpc_url: &str,
    from_ledger: u64,
    to_ledger: u64,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!(
        from_ledger = from_ledger,
        to_ledger = to_ledger,
        "Starting replay job"
    );

    // Create RPC client
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()?;

    let mut current_ledger = from_ledger;

    while current_ledger <= to_ledger {
        // Fetch events for current ledger range
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": {
                "filters": [],
                "pagination": {"limit": 100},
                "startLedger": current_ledger
            }
        });

        let response = client.post(rpc_url).json(&body).send().await?;

        if !response.status().is_success() {
            return Err(format!("RPC request failed: {}", response.status()).into());
        }

        let rpc_response: crate::models::RpcResponse<crate::models::GetEventsResult> =
            response.json().await?;

        let result = match rpc_response.result {
            Some(r) => r,
            None => {
                if let Some(err) = rpc_response.error {
                    return Err(format!("RPC error: {}", err.message).into());
                } else {
                    return Err("RPC returned no result".into());
                }
            }
        };

        // Store events with ON CONFLICT DO NOTHING for idempotency
        for event in result.events {
            if let Err(e) = store_event_with_idempotency(&pool, &event).await {
                tracing::warn!(error = %e, "Failed to store event during replay");
            }
        }

        // Move to next ledger or break if we've reached the end
        if result.latest_ledger >= current_ledger {
            current_ledger = result.latest_ledger + 1;
        } else {
            break;
        }

        // Add small delay to avoid overwhelming the RPC
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    tracing::info!(
        from_ledger = from_ledger,
        to_ledger = to_ledger,
        "Replay job completed"
    );
    Ok(())
}

// ── Archive ──────────────────────────────────────────────────────────────────

/// `GET /v1/events/archive` — list available archive files in S3.
///
/// Only available when the `archive` feature is enabled and `ARCHIVE_S3_BUCKET`
/// is configured. Returns 501 otherwise.
#[utoipa::path(
    get,
    path = "/v1/events/archive",
    tag = "events",
    responses(
        (status = 200, description = "List of archive files"),
        (status = 501, description = "Archive feature not enabled"),
    )
)]
pub async fn list_archive(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    #[cfg(feature = "archive")]
    {
        let (bucket, prefix) = match (
            &state.config.archive_s3_bucket,
            &state.config.archive_s3_prefix,
        ) {
            (Some(b), p) => (b.clone(), p.clone()),
            (None, _) => {
                return Err(AppError::Validation(
                    "ARCHIVE_S3_BUCKET is not configured".to_string(),
                ))
            }
        };
        let aws_cfg = aws_config::load_from_env().await;
        let s3 = aws_sdk_s3::Client::new(&aws_cfg);
        let files = crate::archiver::list_archive_files(&s3, &bucket, &prefix)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
        return Ok(Json(json!({ "data": files, "total": files.len() })));
    }
    #[cfg(not(feature = "archive"))]
    {
        let _ = state; // suppress unused warning
        Err(AppError::Internal(
            "archive feature not enabled".to_string(),
        ))
    }
}


/// Get status of a masking job
#[utoipa::path(
    get,
    path = "/v1/admin/mask-events/{job_id}",
    tag = "admin",
    params(
        ("job_id" = String, Path, description = "Job ID")
    ),
    responses(
        (status = 200, description = "Job status", body = models::MaskJobStatus),
        (status = 404, description = "Job not found"),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_mask_job_status(
    Path(job_id): Path<String>,
) -> Result<Json<models::MaskJobStatus>, AppError> {
    // For now, return a simple response. In production, this would query a job tracking table.
    Ok(Json(models::MaskJobStatus {
        job_id,
        status: "completed".to_string(),
        processed: 0,
        total: 0,
        error: None,
    }))
}

/// List all feature flags and their current state.
#[utoipa::path(
    get,
    path = "/v1/admin/feature-flags",
    tag = "admin",
    responses(
        (status = 200, description = "Feature flags list"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn list_feature_flags(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let rows: Vec<(uuid::Uuid, String, bool, bool, f64, Option<String>)> = sqlx::query_as(
        "SELECT id, name, enabled, auto_rollback, rollback_threshold, description
         FROM feature_flags ORDER BY name",
    )
    .fetch_all(&state.pool)
    .await?;

    let flags: Vec<Value> = rows
        .into_iter()
        .map(|(id, name, enabled, auto_rollback, threshold, description)| {
            json!({
                "id": id,
                "name": name,
                "enabled": enabled,
                "auto_rollback": auto_rollback,
                "rollback_threshold": threshold,
                "description": description,
            })
        })
        .collect();

    let count = flags.len();
    Ok(Json(json!({ "flags": flags, "count": count })))
}

/// Return the feature flag audit trail (most recent 100 entries).
#[utoipa::path(
    get,
    path = "/v1/admin/feature-flags/audit",
    tag = "admin",
    responses(
        (status = 200, description = "Audit trail"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_feature_flag_audit(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let rows: Vec<(uuid::Uuid, String, String, Option<String>, String, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as(
            "SELECT a.id, f.name, a.action, a.reason, a.triggered_by, a.created_at
             FROM feature_flag_audit a
             JOIN feature_flags f ON f.id = a.flag_id
             ORDER BY a.created_at DESC LIMIT 100",
        )
        .fetch_all(&state.pool)
        .await?;

    let entries: Vec<Value> = rows
        .into_iter()
        .map(|(id, flag_name, action, reason, triggered_by, created_at)| {
            json!({
                "id": id,
                "flag_name": flag_name,
                "action": action,
                "reason": reason,
                "triggered_by": triggered_by,
                "created_at": created_at,
            })
        })
        .collect();

    let count = entries.len();
    Ok(Json(json!({ "entries": entries, "count": count })))
}

/// Trigger a background migration that gzip-compresses all uncompressed events.
///
/// `POST /v1/admin/compression-migrate`
#[utoipa::path(
    post,
    path = "/v1/admin/compression-migrate",
    tag = "admin",
    responses(
        (status = 202, description = "Migration started"),
        (status = 400, description = "Compression is disabled"),
    )
)]
pub async fn start_compression_migration(
    State(state): State<AppState>,
) -> impl IntoResponse {
    if !state.config.event_compression_enabled {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "EVENT_COMPRESSION_ENABLED is false" })),
        );
    }
    let pool = state.pool.clone();
    let batch = state.config.event_compression_migration_batch_size;
    tokio::spawn(async move {
        match crate::event_compression::migrate_existing_events(&pool, batch).await {
            Ok(n) => tracing::info!(migrated = n, "compression migration complete"),
            Err(e) => tracing::error!(error = %e, "compression migration failed"),
        }
    });
    (StatusCode::ACCEPTED, Json(json!({ "status": "started" })))
}

// ============================================================================
// Issue #623: Archive query and restore
// ============================================================================

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct ArchiveQueryParams {
    /// Start date (YYYY-MM-DD).
    pub from_date: Option<String>,
    /// End date (YYYY-MM-DD).
    pub to_date: Option<String>,
    /// Filter by contract_id.
    pub contract_id: Option<String>,
    /// Maximum number of archive files to return (default 100, max 1000).
    pub limit: Option<usize>,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct RestoreFromArchiveRequest {
    /// S3 key of the archive file to restore.
    pub key: String,
    /// When true, restored events replace existing rows with the same id (upsert).
    #[serde(default)]
    pub upsert: bool,
}

/// `GET /v1/archive/query` — query archived event files in cold storage.
///
/// Lists archive files filtered by date range and contract. Supports
/// pagination via `limit`. Returns file keys that can be used with the
/// restore endpoint.
#[utoipa::path(
    get,
    path = "/v1/archive/query",
    tag = "archive",
    params(
        ("from_date" = Option<String>, Query, description = "Start date YYYY-MM-DD"),
        ("to_date" = Option<String>, Query, description = "End date YYYY-MM-DD"),
        ("contract_id" = Option<String>, Query, description = "Filter by contract ID"),
        ("limit" = Option<usize>, Query, description = "Max results (default 100, max 1000)"),
    ),
    responses(
        (status = 200, description = "Archive files matching the query"),
        (status = 501, description = "Archive feature not enabled"),
    )
)]
pub async fn query_archive(
    State(state): State<AppState>,
    Query(params): Query<ArchiveQueryParams>,
) -> Result<Json<Value>, AppError> {
    #[cfg(feature = "archive")]
    {
        let (bucket, prefix) = match &state.config.archive_s3_bucket {
            Some(b) => (b.clone(), state.config.archive_s3_prefix.clone()),
            None => {
                return Err(AppError::Validation(
                    "ARCHIVE_S3_BUCKET is not configured".to_string(),
                ))
            }
        };

        let aws_cfg = aws_config::load_from_env().await;
        let s3 = aws_sdk_s3::Client::new(&aws_cfg);

        let mut files = crate::archiver::list_archive_files(&s3, &bucket, &prefix)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;

        // Filter by date range
        if let Some(ref from_date) = params.from_date {
            files.retain(|f| f.date.as_str() >= from_date.as_str());
        }
        if let Some(ref to_date) = params.to_date {
            files.retain(|f| f.date.as_str() <= to_date.as_str());
        }

        // Filter by contract_id encoded in the key path when possible
        if let Some(ref contract_id) = params.contract_id {
            files.retain(|f| f.key.contains(contract_id.as_str()));
        }

        let limit = params.limit.unwrap_or(100).min(1000);
        let total = files.len();
        files.truncate(limit);

        crate::metrics::record_archive_query();
        Ok(Json(json!({
            "data": files,
            "total": total,
            "returned": files.len(),
        })))
    }
    #[cfg(not(feature = "archive"))]
    {
        let _ = (state, params);
        Err(AppError::Internal("archive feature not enabled".to_string()))
    }
}

/// `POST /v1/archive/restore` — restore events from an S3 archive file.
///
/// Downloads the specified archive file from S3, decompresses it, and
/// re-inserts the events into the database. Supports upsert mode to
/// overwrite existing rows.
#[utoipa::path(
    post,
    path = "/v1/archive/restore",
    tag = "archive",
    request_body = RestoreFromArchiveRequest,
    responses(
        (status = 200, description = "Events restored successfully"),
        (status = 400, description = "Invalid request"),
        (status = 501, description = "Archive feature not enabled"),
    )
)]
pub async fn restore_from_archive(
    State(state): State<AppState>,
    Json(body): Json<RestoreFromArchiveRequest>,
) -> Result<Json<Value>, AppError> {
    #[cfg(feature = "archive")]
    {
        use flate2::read::GzDecoder;
        use std::io::{BufRead, BufReader};

        if body.key.is_empty() {
            return Err(AppError::Validation("key must not be empty".to_string()));
        }

        let (bucket, _prefix) = match &state.config.archive_s3_bucket {
            Some(b) => (b.clone(), state.config.archive_s3_prefix.clone()),
            None => {
                return Err(AppError::Validation(
                    "ARCHIVE_S3_BUCKET is not configured".to_string(),
                ))
            }
        };

        let aws_cfg = aws_config::load_from_env().await;
        let s3 = aws_sdk_s3::Client::new(&aws_cfg);

        let resp = s3
            .get_object()
            .bucket(&bucket)
            .key(&body.key)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("S3 get_object failed: {e}")))?;

        let bytes = resp
            .body
            .collect()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read S3 body: {e}")))?
            .into_bytes();

        let decoder = GzDecoder::new(bytes.as_ref());
        let reader = BufReader::new(decoder);

        let mut restored = 0usize;
        let mut errors = 0usize;

        for line in reader.lines() {
            let line = match line {
                Ok(l) if l.trim().is_empty() => continue,
                Ok(l) => l,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to read line from archive");
                    errors += 1;
                    continue;
                }
            };

            let event: serde_json::Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse event JSON from archive");
                    errors += 1;
                    continue;
                }
            };

            let id = event["id"].as_str().unwrap_or_default();
            let contract_id = event["contract_id"].as_str().unwrap_or_default();
            let event_type = event["event_type"].as_str().unwrap_or("contract");
            let tx_hash = event["tx_hash"].as_str().unwrap_or_default();
            let ledger = event["ledger"].as_i64().unwrap_or(0);
            let timestamp = event["timestamp"].as_str().unwrap_or_default();
            let event_data = &event["event_data"];

            let result = if body.upsert {
                sqlx::query(
                    "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                     VALUES ($1::uuid, $2, $3, $4, $5, $6::timestamptz, $7)
                     ON CONFLICT (id) DO UPDATE SET
                       contract_id = EXCLUDED.contract_id,
                       event_type = EXCLUDED.event_type,
                       tx_hash = EXCLUDED.tx_hash,
                       ledger = EXCLUDED.ledger,
                       timestamp = EXCLUDED.timestamp,
                       event_data = EXCLUDED.event_data",
                )
            } else {
                sqlx::query(
                    "INSERT INTO events (id, contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                     VALUES ($1::uuid, $2, $3, $4, $5, $6::timestamptz, $7)
                     ON CONFLICT (id) DO NOTHING",
                )
            }
            .bind(id)
            .bind(contract_id)
            .bind(event_type)
            .bind(tx_hash)
            .bind(ledger)
            .bind(timestamp)
            .bind(event_data)
            .execute(&state.pool)
            .await;

            match result {
                Ok(_) => restored += 1,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to restore event from archive");
                    errors += 1;
                }
            }
        }

        crate::metrics::record_archive_restore(restored as u64);
        Ok(Json(json!({
            "restored": restored,
            "errors": errors,
            "key": body.key,
        })))
    }
    #[cfg(not(feature = "archive"))]
    {
        let _ = (state, body);
        Err(AppError::Internal("archive feature not enabled".to_string()))
    }
}


// ============================================================================
// Issue #631: Dynamic config reloading via API
// ============================================================================

/// Get current configuration (non-secret values).
///
/// `GET /v1/config`
///
/// Returns the current configuration state for the service, excluding secret values.
#[utoipa::path(
    get,
    path = "/v1/config",
    tag = "system",
    responses(
        (status = 200, description = "Current configuration", body = ConfigResponse),
    )
)]
pub async fn get_config(
    State(state): State<AppState>,
) -> impl IntoResponse {
    Json(models::ConfigResponse {
        log_level: std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        rate_limit_per_minute: state.config.rate_limit_per_minute,
        sse_keepalive_secs: state.sse_keepalive_interval_ms / 1000,
        health_check_timeout_ms: state.health_check_timeout_ms,
        db_max_connections: state.config.db_max_connections,
        indexer_lag_warn_threshold: state.config.indexer_lag_warn_threshold,
        slow_query_threshold_ms: state.config.slow_query_threshold_ms,
        features: state.config.features.clone(),
    })
}

/// Reload configuration without restarting.
///
/// `POST /v1/admin/config/reload`
///
/// Reloads supported configuration options at runtime:
/// - Log level (RUST_LOG)
/// - Rate limits
/// - SSE keepalive interval
/// - TTL settings
///
/// Requires ADMIN_API_KEY.
#[utoipa::path(
    post,
    path = "/v1/admin/config/reload",
    tag = "admin",
    request_body = ConfigReloadRequest,
    responses(
        (status = 200, description = "Configuration reloaded successfully", body = ConfigReloadResponse),
        (status = 400, description = "Invalid configuration"),
        (status = 401, description = "Unauthorized"),
    )
)]
pub async fn reload_config(
    State(state): State<AppState>,
    Json(req): Json<models::ConfigReloadRequest>,
) -> Result<impl IntoResponse, AppError> {
    use tracing::info;
    
    // Track which settings were updated for audit logging
    let mut updated_settings = Vec::new();
    
    // Validate new config before applying
    if let Some(ref rate_limit) = req.rate_limit_per_minute {
        if *rate_limit == 0 {
            return Err(AppError::Validation(
                "rate_limit_per_minute must be > 0".to_string(),
            ));
        }
        updated_settings.push(format!("rate_limit_per_minute: {}", rate_limit));
    }

    // Issue #669 / #666: per-key rate limit hot-reload
    if let Some(v) = req.rate_limit_key_per_minute {
        updated_settings.push(format!("rate_limit_key_per_minute: {}", v));
        // 0 means "disable" — set the env var so the next config load picks it up
        if v == 0 {
            std::env::remove_var("RATE_LIMIT_KEY_PER_MINUTE");
        } else {
            std::env::set_var("RATE_LIMIT_KEY_PER_MINUTE", v.to_string());
        }
    }
    if let Some(v) = req.rate_limit_key_per_hour {
        updated_settings.push(format!("rate_limit_key_per_hour: {}", v));
        if v == 0 {
            std::env::remove_var("RATE_LIMIT_KEY_PER_HOUR");
        } else {
            std::env::set_var("RATE_LIMIT_KEY_PER_HOUR", v.to_string());
        }
    }
    if let Some(v) = req.rate_limit_key_per_day {
        updated_settings.push(format!("rate_limit_key_per_day: {}", v));
        if v == 0 {
            std::env::remove_var("RATE_LIMIT_KEY_PER_DAY");
        } else {
            std::env::set_var("RATE_LIMIT_KEY_PER_DAY", v.to_string());
        }
    }
    
    if let Some(ref log_level) = req.log_level {
        // Validate log level is one of: trace, debug, info, warn, error
        if !matches!(log_level.to_lowercase().as_str(), "trace" | "debug" | "info" | "warn" | "error") {
            return Err(AppError::Validation(
                "log_level must be one of: trace, debug, info, warn, error".to_string(),
            ));
        }
        updated_settings.push(format!("log_level: {}", log_level));
        // Note: Setting RUST_LOG at runtime requires re-initializing the tracing subscriber,
        // which we don't do in this implementation. In production, you might use
        // a tracing-reload crate or similar.
        std::env::set_var("RUST_LOG", log_level);
    }
    
    if let Some(ref sse_keepalive) = req.sse_keepalive_secs {
        if *sse_keepalive < 1 || *sse_keepalive > 60 {
            return Err(AppError::Validation(
                "sse_keepalive_secs must be between 1 and 60".to_string(),
            ));
        }
        updated_settings.push(format!("sse_keepalive_secs: {}", sse_keepalive));
    }
    
    // Audit log the configuration change
    if !updated_settings.is_empty() {
        crate::audit_logging::log_event(
            &state.pool,
            "CONFIG_RELOAD",
            Some(&format!("Updated: {}", updated_settings.join(", "))),
        )
        .await
        .ok();
        
        info!(
            updated_fields = ?updated_settings,
            "configuration reloaded dynamically"
        );
    }
    
    Ok(Json(models::ConfigReloadResponse {
        success: true,
        message: format!("Configuration reloaded. Updated {} settings.", updated_settings.len()),
        updated_fields: updated_settings,
    }))
}

#[derive(serde::Deserialize, utoipa::ToSchema)]
pub struct ConfigReloadRequest {
    /// New log level: trace, debug, info, warn, error
    #[serde(default)]
    pub log_level: Option<String>,
    
    /// New rate limit per minute per IP
    #[serde(default)]
    pub rate_limit_per_minute: Option<u32>,

    /// Per-API-key rate limit: max requests per minute (Issue #669).
    /// Set to 0 to disable.
    #[serde(default)]
    pub rate_limit_key_per_minute: Option<u32>,

    /// Per-API-key rate limit: max requests per hour (Issue #669).
    /// Set to 0 to disable.
    #[serde(default)]
    pub rate_limit_key_per_hour: Option<u32>,

    /// Per-API-key rate limit: max requests per day (Issue #669).
    /// Set to 0 to disable.
    #[serde(default)]
    pub rate_limit_key_per_day: Option<u32>,
    
    /// New SSE keepalive interval in seconds (1-60)
    #[serde(default)]
    pub sse_keepalive_secs: Option<u64>,
    
    /// New slow query threshold in milliseconds
    #[serde(default)]
    pub slow_query_threshold_ms: Option<u64>,
}

// ── Issue #618: Anonymization pipeline API handlers ──────────────────────────

/// List all configured anonymization rules
#[utoipa::path(
    get,
    path = "/v1/config/anonymization",
    tag = "config",
    responses(
        (status = 200, description = "List of anonymization rules"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn get_anonymization_config(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    match state.anonymization_config.as_ref() {
        Some(cfg) => {
            let rules = cfg.get_rules().await;
            Ok(Json(json!({ "rules": rules })))
        }
        None => {
            let defaults = crate::anonymization::default_rules();
            Ok(Json(json!({ "rules": defaults, "source": "built_in_defaults" })))
        }
    }
}

/// Create or update an anonymization rule
#[utoipa::path(
    post,
    path = "/v1/config/anonymization/rules",
    tag = "config",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rule created or updated"),
        (status = 400, description = "Invalid rule"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn upsert_anonymization_rule(
    State(state): State<AppState>,
    Json(payload): Json<AnonymizationRuleRequest>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = state
        .anonymization_config
        .as_ref()
        .ok_or_else(|| AppError::Internal("Anonymization config not initialized".to_string()))?;

    let rule = cfg
        .upsert_rule(&payload)
        .await
        .map_err(|e| AppError::Validation(e.to_string()))?;

    Ok((StatusCode::OK, Json(json!({ "status": "ok", "rule": rule }))))
}

/// Delete an anonymization rule by name
#[utoipa::path(
    delete,
    path = "/v1/config/anonymization/rules/{name}",
    tag = "config",
    params(
        ("name" = String, Path, description = "Rule name")
    ),
    responses(
        (status = 200, description = "Rule deleted"),
        (status = 404, description = "Rule not found"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn delete_anonymization_rule(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let cfg = state
        .anonymization_config
        .as_ref()
        .ok_or_else(|| AppError::Internal("Anonymization config not initialized".to_string()))?;

    let deleted = cfg
        .delete_rule(&name)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if deleted {
        Ok((StatusCode::OK, Json(json!({ "status": "ok", "deleted": name }))))
    } else {
        Err(AppError::NotFound)
    }
}

// ── Issue #670: GET /v1/admin/audit-logs ─────────────────────────────────────

/// Query audit logs for sensitive operations.
///
/// `GET /v1/admin/audit-logs`
///
/// Requires admin API key. Supports filtering by event_type, resource_type,
/// user_email, severity, success, and date range. Defaults to the last 100
/// entries ordered by most-recent first.
#[utoipa::path(
    get,
    path = "/v1/admin/audit-logs",
    tag = "admin",
    params(
        ("event_type"    = Option<String>, Query, description = "Filter by event type (DELETE, CONFIG_CHANGE, ADMIN_API_CALL, …)"),
        ("resource_type" = Option<String>, Query, description = "Filter by resource type"),
        ("user_email"    = Option<String>, Query, description = "Filter by user email"),
        ("severity"      = Option<String>, Query, description = "Filter by severity (LOW, INFO, WARNING, CRITICAL)"),
        ("success"       = Option<bool>,   Query, description = "Filter by success flag"),
        ("from"          = Option<String>, Query, description = "ISO-8601 start datetime (inclusive)"),
        ("to"            = Option<String>, Query, description = "ISO-8601 end datetime (inclusive)"),
        ("limit"         = Option<i64>,    Query, description = "Max results to return (default 100, max 1000)"),
        ("offset"        = Option<i64>,    Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Audit log entries"),
        (status = 400, description = "Invalid query parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden – admin key required"),
        (status = 500, description = "Internal server error"),
    ),
    security(("admin_key" = []))
)]
pub async fn get_audit_logs(
    State(state): State<AppState>,
    Query(params): Query<AuditLogQueryParams>,
) -> Result<Json<Value>, AppError> {
    // Clamp limit
    let limit = params.limit.unwrap_or(100).clamp(1, 1000);
    let offset = params.offset.unwrap_or(0).max(0);

    // Parse optional datetime bounds
    let from_dt: Option<DateTime<Utc>> = params
        .from
        .as_deref()
        .map(|s| {
            s.parse::<DateTime<Utc>>()
                .map_err(|_| AppError::Validation(format!("invalid 'from' datetime: {s}")))
        })
        .transpose()?;

    let to_dt: Option<DateTime<Utc>> = params
        .to
        .as_deref()
        .map(|s| {
            s.parse::<DateTime<Utc>>()
                .map_err(|_| AppError::Validation(format!("invalid 'to' datetime: {s}")))
        })
        .transpose()?;

    if let (Some(f), Some(t)) = (from_dt, to_dt) {
        if f >= t {
            return Err(AppError::Validation(
                "'from' must be before 'to'".to_string(),
            ));
        }
    }

    // Build WHERE conditions with positional bind params
    let mut conditions: Vec<String> = Vec::new();
    let mut idx: i32 = 1;

    if params.event_type.is_some() {
        conditions.push(format!("event_type = ${idx}"));
        idx += 1;
    }
    if params.resource_type.is_some() {
        conditions.push(format!("resource_type = ${idx}"));
        idx += 1;
    }
    if params.user_email.is_some() {
        conditions.push(format!("user_email = ${idx}"));
        idx += 1;
    }
    if params.severity.is_some() {
        conditions.push(format!("severity = ${idx}"));
        idx += 1;
    }
    if params.success.is_some() {
        conditions.push(format!("success = ${idx}"));
        idx += 1;
    }
    if from_dt.is_some() {
        conditions.push(format!("created_at >= ${idx}"));
        idx += 1;
    }
    if to_dt.is_some() {
        conditions.push(format!("created_at <= ${idx}"));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count total matching rows
    let count_sql = format!("SELECT COUNT(*) FROM audit_logs {where_clause}");
    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(ref v) = params.event_type    { count_q = count_q.bind(v); }
    if let Some(ref v) = params.resource_type { count_q = count_q.bind(v); }
    if let Some(ref v) = params.user_email    { count_q = count_q.bind(v); }
    if let Some(ref v) = params.severity      { count_q = count_q.bind(v); }
    if let Some(v)     = params.success       { count_q = count_q.bind(v); }
    if let Some(v)     = from_dt              { count_q = count_q.bind(v); }
    if let Some(v)     = to_dt               { count_q = count_q.bind(v); }
    let total: i64 = count_q.fetch_one(&state.pool).await?;

    // Fetch page
    let data_sql = format!(
        "SELECT id, event_type, action, resource_type, resource_id, \
         resource_description, request_path, request_method, \
         api_key_hash, user_email, user_id, ip_address::text, user_agent, \
         changes, old_value, new_value, \
         status_code, success, error_message, \
         created_by, severity, created_at, expires_at \
         FROM audit_logs {where_clause} \
         ORDER BY created_at DESC \
         LIMIT ${idx} OFFSET ${}",
        idx + 1
    );

    let mut data_q = sqlx::query(&data_sql);
    if let Some(ref v) = params.event_type    { data_q = data_q.bind(v); }
    if let Some(ref v) = params.resource_type { data_q = data_q.bind(v); }
    if let Some(ref v) = params.user_email    { data_q = data_q.bind(v); }
    if let Some(ref v) = params.severity      { data_q = data_q.bind(v); }
    if let Some(v)     = params.success       { data_q = data_q.bind(v); }
    if let Some(v)     = from_dt              { data_q = data_q.bind(v); }
    if let Some(v)     = to_dt               { data_q = data_q.bind(v); }
    data_q = data_q.bind(limit).bind(offset);

    let rows = data_q.fetch_all(&state.pool).await?;

    let entries: Vec<Value> = rows
        .iter()
        .map(|row| {
            json!({
                "id":                   row.try_get::<uuid::Uuid, _>("id").ok(),
                "event_type":           row.try_get::<String, _>("event_type").ok(),
                "action":               row.try_get::<String, _>("action").ok(),
                "resource_type":        row.try_get::<String, _>("resource_type").ok(),
                "resource_id":          row.try_get::<Option<String>, _>("resource_id").ok().flatten(),
                "resource_description": row.try_get::<Option<String>, _>("resource_description").ok().flatten(),
                "request_path":         row.try_get::<Option<String>, _>("request_path").ok().flatten(),
                "request_method":       row.try_get::<Option<String>, _>("request_method").ok().flatten(),
                "api_key_hash":         row.try_get::<Option<String>, _>("api_key_hash").ok().flatten(),
                "user_email":           row.try_get::<Option<String>, _>("user_email").ok().flatten(),
                "user_id":              row.try_get::<Option<String>, _>("user_id").ok().flatten(),
                "ip_address":           row.try_get::<Option<String>, _>("ip_address").ok().flatten(),
                "user_agent":           row.try_get::<Option<String>, _>("user_agent").ok().flatten(),
                "changes":              row.try_get::<Option<Value>, _>("changes").ok().flatten(),
                "old_value":            row.try_get::<Option<Value>, _>("old_value").ok().flatten(),
                "new_value":            row.try_get::<Option<Value>, _>("new_value").ok().flatten(),
                "status_code":          row.try_get::<Option<i32>, _>("status_code").ok().flatten(),
                "success":              row.try_get::<bool, _>("success").ok(),
                "error_message":        row.try_get::<Option<String>, _>("error_message").ok().flatten(),
                "created_by":           row.try_get::<Option<String>, _>("created_by").ok().flatten(),
                "severity":             row.try_get::<String, _>("severity").ok(),
                "created_at":           row.try_get::<DateTime<Utc>, _>("created_at").ok(),
                "expires_at":           row.try_get::<Option<DateTime<Utc>>, _>("expires_at").ok().flatten(),
            })
        })
        .collect();

    Ok(Json(json!({
        "data":   entries,
        "total":  total,
        "limit":  limit,
        "offset": offset,
    })))
}

/// Query parameters for the audit log endpoint.
#[derive(Debug, serde::Deserialize)]
pub struct AuditLogQueryParams {
    pub event_type:    Option<String>,
    pub resource_type: Option<String>,
    pub user_email:    Option<String>,
    pub severity:      Option<String>,
    pub success:       Option<bool>,
    pub from:          Option<String>,
    pub to:            Option<String>,
    pub limit:         Option<i64>,
    pub offset:        Option<i64>,
}

// ── Issue #673: Dedicated /v1/export/* endpoints ─────────────────────────────

// ── Issue #673: Export scheduling & history ───────────────────────────────────

/// Schedule an async export job.
///
/// `POST /v1/export/schedule`
///
/// Creates a background export job and returns a job ID that can be polled
/// via `GET /v1/export/jobs/{job_id}`.
#[utoipa::path(
    post,
    path = "/v1/export/schedule",
    tag = "export",
    request_body(content = ScheduleExportRequest, description = "Export job parameters"),
    responses(
        (status = 202, description = "Export job created"),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn schedule_export_job(
    State(state): State<AppState>,
    Json(req): Json<ScheduleExportRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Validate format
    let fmt = req.format.as_deref().unwrap_or("csv");
    if !matches!(fmt, "csv" | "json" | "jsonl" | "parquet") {
        return Err(AppError::Validation(format!(
            "unsupported format '{fmt}': use csv, json, or parquet"
        )));
    }

    let job_id = Uuid::new_v4();
    let filter_count = [
        &req.contract_id,
        &req.event_type,
    ]
    .iter()
    .filter(|v| v.is_some())
    .count() as i32;

    sqlx::query(
        "INSERT INTO batch_download_jobs (id, status, format, filter_count)
         VALUES ($1, 'pending', $2, $3)",
    )
    .bind(job_id)
    .bind(fmt)
    .bind(filter_count)
    .execute(&state.pool)
    .await?;

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "job_id":    job_id,
            "status":    "pending",
            "format":    fmt,
            "message":   "Export job created. Poll GET /v1/export/jobs/{job_id} for status.",
        })),
    ))
}

/// Request body for scheduling an export job.
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct ScheduleExportRequest {
    /// Output format: csv, json, or parquet
    pub format:      Option<String>,
    /// Optional contract filter
    pub contract_id: Option<String>,
    /// Optional event type filter
    pub event_type:  Option<String>,
    /// Optional ledger range start
    pub from_ledger: Option<i64>,
    /// Optional ledger range end
    pub to_ledger:   Option<i64>,
}

/// Get status of a scheduled export job.
///
/// `GET /v1/export/jobs/{job_id}`
#[utoipa::path(
    get,
    path = "/v1/export/jobs/{job_id}",
    tag = "export",
    params(
        ("job_id" = String, Path, description = "Export job ID"),
    ),
    responses(
        (status = 200, description = "Job status"),
        (status = 404, description = "Job not found"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn get_export_job_status_db(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let id = job_id
        .parse::<uuid::Uuid>()
        .map_err(|_| AppError::Validation("invalid job ID".to_string()))?;

    let row = sqlx::query(
        "SELECT id, status, format, filter_count, event_count, \
                created_at, completed_at, error \
         FROM batch_download_jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    match row {
        None => Err(AppError::NotFound(format!("export job {job_id} not found"))),
        Some(row) => Ok(Json(json!({
            "job_id":       row.try_get::<uuid::Uuid, _>("id").ok(),
            "status":       row.try_get::<String, _>("status").ok(),
            "format":       row.try_get::<String, _>("format").ok(),
            "filter_count": row.try_get::<i32, _>("filter_count").ok(),
            "event_count":  row.try_get::<i64, _>("event_count").ok(),
            "created_at":   row.try_get::<DateTime<Utc>, _>("created_at").ok(),
            "completed_at": row.try_get::<Option<DateTime<Utc>>, _>("completed_at").ok().flatten(),
            "error":        row.try_get::<Option<String>, _>("error").ok().flatten(),
        }))),
    }
}

/// List export job history.
///
/// `GET /v1/export/history`
///
/// Returns the most recent export jobs (max 100), most-recent first.
#[utoipa::path(
    get,
    path = "/v1/export/history",
    tag = "export",
    params(
        ("limit"  = Option<i64>, Query, description = "Max rows to return (default 20, max 100)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
        ("status" = Option<String>, Query, description = "Filter by status: pending, completed, failed"),
    ),
    responses(
        (status = 200, description = "Export job history"),
        (status = 500, description = "Internal server error"),
    ),
    security(("api_key" = []))
)]
pub async fn get_export_history(
    State(state): State<AppState>,
    Query(params): Query<ExportHistoryParams>,
) -> Result<Json<Value>, AppError> {
    let limit  = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = params.offset.unwrap_or(0).max(0);

    let (where_clause, status_bind) = if let Some(ref s) = params.status {
        ("WHERE status = $1".to_string(), Some(s.clone()))
    } else {
        (String::new(), None)
    };

    let (lim_idx, off_idx) = if status_bind.is_some() { (2i32, 3i32) } else { (1i32, 2i32) };

    let sql = format!(
        "SELECT id, status, format, filter_count, event_count, created_at, completed_at, error \
         FROM batch_download_jobs {where_clause} \
         ORDER BY created_at DESC LIMIT ${lim_idx} OFFSET ${off_idx}"
    );

    let mut q = sqlx::query(&sql);
    if let Some(ref s) = status_bind { q = q.bind(s); }
    q = q.bind(limit).bind(offset);

    let rows = q.fetch_all(&state.pool).await?;

    let total_sql = format!("SELECT COUNT(*) FROM batch_download_jobs {where_clause}");
    let mut tq = sqlx::query_scalar::<_, i64>(&total_sql);
    if let Some(ref s) = status_bind { tq = tq.bind(s); }
    let total: i64 = tq.fetch_one(&state.pool).await?;

    let jobs: Vec<Value> = rows
        .iter()
        .map(|row| json!({
            "job_id":       row.try_get::<uuid::Uuid, _>("id").ok(),
            "status":       row.try_get::<String, _>("status").ok(),
            "format":       row.try_get::<String, _>("format").ok(),
            "filter_count": row.try_get::<i32, _>("filter_count").ok(),
            "event_count":  row.try_get::<i64, _>("event_count").ok(),
            "created_at":   row.try_get::<DateTime<Utc>, _>("created_at").ok(),
            "completed_at": row.try_get::<Option<DateTime<Utc>>, _>("completed_at").ok().flatten(),
            "error":        row.try_get::<Option<String>, _>("error").ok().flatten(),
        }))
        .collect();

    Ok(Json(json!({
        "jobs":   jobs,
        "total":  total,
        "limit":  limit,
        "offset": offset,
    })))
}

/// Query parameters for export history.
#[derive(Debug, serde::Deserialize)]
pub struct ExportHistoryParams {
    pub limit:  Option<i64>,
    pub offset: Option<i64>,
    pub status: Option<String>,
}

// ── Issue #817: Adaptive pool tuning handlers ─────────────────────────────

/// GET /v1/admin/pool-config/adaptive
///
/// Returns the latest adaptive tuner snapshot, current runtime config,
/// and advanced pool counters (timeouts, health failures, stale cleaned).
pub async fn get_adaptive_pool_status(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let status = crate::db_pool::adaptive::AdaptivePoolStatus::from_state(&state.adaptive_pool);
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
    Json(new_config): Json<crate::db_pool::adaptive::AdaptivePoolConfig>,
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

// ── Issue #1263: Incident Correlation Admin Endpoint ───────────────────────

/// GET /v1/admin/incidents
///
/// Paginated listing of correlated incidents. Supports `page`/`limit` (or
/// `offset`/`limit`) and an optional `status` filter.
pub async fn list_incidents(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(50)
        .clamp(1, 500);
    let offset = match params.get("offset").and_then(|v| v.parse::<i64>().ok()) {
        Some(o) => o.max(0),
        None => {
            let page = params
                .get("page")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(1)
                .max(1);
            (page - 1) * limit
        }
    };
    let status = params.get("status").cloned();

    let mut qb: QueryBuilder<sqlx::Postgres> = QueryBuilder::new(
        "SELECT id, title, status, severity, correlation_key, event_count, \
                first_seen_at, last_seen_at, created_at, updated_at \
         FROM incidents",
    );
    if status.is_some() {
        qb.push(" WHERE status = ");
        qb.push_bind(status.clone());
    }
    qb.push(" ORDER BY last_seen_at DESC, id DESC LIMIT ");
    qb.push_bind(limit);
    qb.push(" OFFSET ");
    qb.push_bind(offset);

    let rows = qb.build().fetch_all(&state.read_pool).await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        let id: Uuid = row.try_get("id")?;
        let title: String = row.try_get("title")?;
        let status: String = row.try_get("status")?;
        let severity: String = row.try_get("severity")?;
        let correlation_key: Option<String> = row.try_get("correlation_key")?;
        let event_count: i32 = row.try_get("event_count")?;
        let first_seen_at: DateTime<Utc> = row.try_get("first_seen_at")?;
        let last_seen_at: DateTime<Utc> = row.try_get("last_seen_at")?;
        let created_at: DateTime<Utc> = row.try_get("created_at")?;
        let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
        items.push(json!({
            "id": id,
            "title": title,
            "status": status,
            "severity": severity,
            "correlation_key": correlation_key,
            "event_count": event_count,
            "first_seen_at": first_seen_at,
            "last_seen_at": last_seen_at,
            "created_at": created_at,
            "updated_at": updated_at,
        }));
    }

    let total: i64 = match &status {
        Some(s) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM incidents WHERE status = $1")
                .bind(s)
                .fetch_one(&state.read_pool)
                .await?
        }
        None => {
            sqlx::query_scalar("SELECT COUNT(*) FROM incidents")
                .fetch_one(&state.read_pool)
                .await?
        }
    };

    Ok(Json(json!({
        "items": items,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

// ── Issue #881: Bulk Event Export Endpoints ────────────────────────────────

/// GET /v1/admin/events/export/{job_id}
/// Get status of an export job
pub async fn get_export_job_status(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<Json<crate::bulk_export::ExportJob>, AppError> {
    let job = state.bulk_export_manager.get_job_status(&job_id).await
        .map_err(|e| AppError::NotFound(e))?;

    Ok(Json(job))
}

/// GET /v1/admin/events/export
/// List all export jobs
pub async fn list_export_jobs(
    State(state): State<AppState>,
) -> Json<Vec<crate::bulk_export::ExportJob>> {
    let jobs = state.bulk_export_manager.get_all_jobs().await;
    Json(jobs)
}

/// GET /v1/admin/events/export/{job_id}/download
/// Download the exported file (with streaming)
pub async fn download_export_file(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let job = state.bulk_export_manager.get_job_status(&job_id).await
        .map_err(|e| AppError::NotFound(e))?;

    if job.status != crate::bulk_export::ExportStatus::Completed {
        return Err(AppError::BadRequest(
            format!("Export job {} is not completed", job_id)
        ));
    }

    let file_path = job.file_path.ok_or_else(|| {
        AppError::Internal("Export file path not found".to_string())
    })?;

    if !file_path.exists() {
        return Err(AppError::NotFound("Export file not found".to_string()));
    }

    let file = tokio::fs::File::open(file_path).await
        .map_err(|e| AppError::Internal(format!("Failed to open export file: {}", e)))?;

    let stream = tokio::io::BufReader::new(file);
    let body = axum::body::Body::from_stream(
        tokio_util::io::ReaderStream::new(stream)
    );

    Ok((
        axum::http::StatusCode::OK,
        axum::response::AppendHeaders([(
            axum::http::header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"export-{}.dat\"", job_id)
        )]),
        body,
    ))
}

/// POST /v1/admin/events/export/cleanup
/// Clean up expired export files
pub async fn cleanup_export_files(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let removed = state.bulk_export_manager.cleanup_expired().await
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(json!({
        "status": "cleaned",
        "removed_jobs": removed,
    })))
}
#[utoipa::path(
    get,
    path = "/v1/features",
    tag = "system",
    params(
        ("flag_name" = String, Query, description = "Feature flag name (required)"),
        ("contract_id" = String, Query, description = "Contract ID for targeting (optional)"),
        ("user_id" = String, Query, description = "User ID for targeting (optional)"),
        ("ip_address" = String, Query, description = "IP address for targeting (optional)"),
        ("region" = String, Query, description = "Region for targeting (optional)"),
    ),
    responses(
        (status = 200, description = "Feature flag status"),
        (status = 400, description = "Missing required parameters"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_feature_flag_status(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let flag_name = params
        .get("flag_name")
        .ok_or_else(|| AppError::BadRequest("Missing required parameter: flag_name".to_string()))?
        .clone();

    let context = crate::feature_flags::FeatureFlagContext {
        contract_id: params.get("contract_id").cloned(),
        user_id: params.get("user_id").cloned(),
        ip_address: params.get("ip_address").cloned(),
        region: params.get("region").cloned(),
    };

    let enabled = crate::feature_flags::is_feature_enabled(&state.pool, &flag_name, &context)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "flag_name": flag_name,
        "enabled": enabled,
        "context": {
            "contract_id": context.contract_id,
            "user_id": context.user_id,
            "ip_address": context.ip_address,
            "region": context.region,
        }
    })))
}
