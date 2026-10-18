//! Administrative handlers: replay, re-encryption, anonymization and
//! masking, contract ABI / metadata / schema registry, indexer control,
//! config reload, audit logs, feature flags, archives, index and
//! compression maintenance, and bulk export / import jobs.

use super::*;

// Dummy job store for demonstration
type JobId = String;
type JobStatus = String;
static EXPORT_JOBS: OnceLock<Arc<RwLock<HashMap<JobId, JobStatus>>>> = OnceLock::new();
static IMPORT_JOBS: OnceLock<Arc<RwLock<HashMap<JobId, JobStatus>>>> = OnceLock::new();

fn export_jobs() -> Arc<RwLock<HashMap<JobId, JobStatus>>> {
    EXPORT_JOBS.get_or_init(|| Arc::new(RwLock::new(HashMap::new()))).clone()
}
fn import_jobs() -> Arc<RwLock<HashMap<JobId, JobStatus>>> {
    IMPORT_JOBS.get_or_init(|| Arc::new(RwLock::new(HashMap::new()))).clone()
}

/// Start an async export job (stub)
pub async fn start_export_job(State(_state): State<AppState>) -> impl IntoResponse {
    let job_id = Uuid::new_v4().to_string();
    export_jobs().write().await.insert(job_id.clone(), "pending".to_string());
    Json(json!({"job_id": job_id}))
}

/// Get export job status (stub)
pub async fn get_async_export_job_status(Path(job_id): Path<String>) -> impl IntoResponse {
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
        .unwrap()
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

#[utoipa::path(
    post,
    path = "/v1/admin/contracts/{contract_id}/abi",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    request_body(content = Value, description = "ABI JSON array", content_type = "application/json"),
    responses(
        (status = 200, description = "ABI registered"),
        (status = 400, description = "Invalid contract_id or ABI", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn register_contract_abi(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(abi): Json<Value>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    // Issue #607: validate ABI structure before persisting.
    if let Err(e) = crate::abi::validate_abi(&abi) {
        crate::metrics::record_abi_validation_failure(&contract_id);
        return Err(AppError::Validation(e));
    }
    sqlx::query(
        "INSERT INTO contract_abis (contract_id, abi, updated_at)
         VALUES ($1, $2, NOW())
         ON CONFLICT (contract_id) DO UPDATE SET abi = EXCLUDED.abi, updated_at = NOW()",
    )
    .bind(&contract_id)
    .bind(&abi)
    .execute(&state.pool)
    .await?;

    // Issue #607: invalidate the in-process cache so the next read fetches fresh data.
    crate::abi::invalidate_abi_cache(&state.abi_cache, &contract_id).await;

    let pool = state.pool.clone();
    let backfill_contract_id = contract_id.clone();
    let backfill_abi = abi.clone();
    tokio::spawn(async move {
        crate::abi::decode_existing_events(pool, backfill_contract_id, backfill_abi).await;
    });

    Ok(Json(
        json!({ "contract_id": contract_id, "status": "registered" }),
    ))
}

#[utoipa::path(
    get,
    path = "/v1/admin/contracts/{contract_id}/abi",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    responses(
        (status = 200, description = "Registered ABI", body = serde_json::Value),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 404, description = "ABI not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_contract_abi(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    let row = sqlx::query("SELECT abi, created_at, updated_at FROM contract_abis WHERE contract_id = $1")
        .bind(&contract_id)
        .fetch_optional(&state.read_pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let abi: Value = row.try_get("abi")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
    Ok(Json(json!({ "contract_id": contract_id, "abi": abi, "created_at": created_at, "updated_at": updated_at })))
}

/// Admin: create or update a contract's label/metadata (#1066).
#[utoipa::path(
    post,
    path = "/v1/admin/contracts/{contract_id}/metadata",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    request_body = crate::contract_metadata::UpsertContractMetadata,
    responses(
        (status = 200, description = "Metadata upserted", body = crate::contract_metadata::ContractMetadata),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn upsert_contract_metadata(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(input): Json<crate::contract_metadata::UpsertContractMetadata>,
) -> Result<Json<crate::contract_metadata::ContractMetadata>, AppError> {
    validate_contract_id(&contract_id)?;
    let metadata =
        crate::contract_metadata::upsert_contract_metadata(&state.pool, &contract_id, input)
            .await?;
    Ok(Json(metadata))
}

/// Admin: delete a contract's label/metadata (#1066).
pub async fn delete_contract_metadata(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    let deleted =
        crate::contract_metadata::delete_contract_metadata(&state.pool, &contract_id).await?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(Json(json!({ "contract_id": contract_id, "status": "deleted" })))
}

/// Admin: bulk import contract labels from a JSON array of
/// `{contract_id, name, description, project_url, source_repo, tags, verified}` (#1066).
pub async fn bulk_import_contract_metadata(
    State(state): State<AppState>,
    Json(entries): Json<Vec<Value>>,
) -> Result<Json<Value>, AppError> {
    let mut parsed = Vec::with_capacity(entries.len());
    for entry in entries {
        let contract_id = entry
            .get("contract_id")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::Validation("missing contract_id".into()))?
            .to_string();
        validate_contract_id(&contract_id)?;
        let input: crate::contract_metadata::UpsertContractMetadata =
            serde_json::from_value(entry)
                .map_err(|e| AppError::Validation(format!("invalid metadata entry: {e}")))?;
        parsed.push((contract_id, input));
    }
    let count = crate::contract_metadata::bulk_import(&state.pool, parsed).await?;
    Ok(Json(json!({ "imported": count })))
}

/// Anonymize a specific event for GDPR compliance.
/// Replaces event_data with {"anonymized": true} and hashes tx_hash with SHA-256.
/// Idempotent: already-anonymized events return 200 without re-processing.
#[utoipa::path(
    post,
    path = "/v1/admin/events/{id}/anonymize",
    tag = "admin",
    params(
        ("id" = String, Path, description = "Event UUID"),
    ),
    responses(
        (status = 200, description = "Event anonymized"),
        (status = 404, description = "Event not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn anonymize_event(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    use sha2::{Digest, Sha256};

    // Fetch current tx_hash and anonymized flag
    let row = sqlx::query("SELECT tx_hash, anonymized FROM events WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or(AppError::NotFound)?;

    let already_anonymized: bool = row.try_get("anonymized")?;
    if already_anonymized {
        tracing::info!(event_id = %id, "Event already anonymized, skipping");
        return Ok(Json(json!({ "id": id, "anonymized": true })));
    }

    let tx_hash: String = row.try_get("tx_hash")?;
    let hashed_tx = {
        let mut h = Sha256::new();
        h.update(tx_hash.as_bytes());
        format!("{:x}", h.finalize())
    };

    sqlx::query("UPDATE events SET anonymized = TRUE, event_data = $1, tx_hash = $2 WHERE id = $3")
        .bind(json!({"anonymized": true}))
        .bind(&hashed_tx)
        .bind(id)
        .execute(&state.pool)
        .await?;

    tracing::info!(event_id = %id, anonymized_at = %chrono::Utc::now(), "Event anonymized");

    Ok(Json(json!({ "id": id, "anonymized": true })))
}

/// Delete all events for a contract (GDPR right-to-erasure).
/// Optionally anonymize instead of deleting with `anonymize_only=true`.
/// Logs deletion request for audit purposes.
#[utoipa::path(
    delete,
    path = "/v1/admin/events/contract/{contract_id}",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Contract ID"),
        ("anonymize_only" = Option<bool>, Query, description = "If true, anonymize instead of deleting (default: false)"),
    ),
    responses(
        (status = 200, description = "Events deleted or anonymized"),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn delete_contract_events(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;

    let anonymize_only = params
        .get("anonymize_only")
        .and_then(|v| v.parse::<bool>().ok())
        .unwrap_or(false);

    let client_ip = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("unknown");

    if anonymize_only {
        // Anonymize: set anonymized=true and hash tx_hash
        use sha2::{Digest, Sha256};

        let rows = sqlx::query("SELECT id, tx_hash FROM events WHERE contract_id = $1 AND anonymized = FALSE")
            .bind(&contract_id)
            .fetch_all(&state.pool)
            .await?;

        let count = rows.len() as u64;

        for row in rows {
            let id: Uuid = row.try_get("id")?;
            let tx_hash: String = row.try_get("tx_hash")?;
            let hashed_tx = {
                let mut h = Sha256::new();
                h.update(tx_hash.as_bytes());
                format!("{:x}", h.finalize())
            };

            sqlx::query("UPDATE events SET anonymized = TRUE, event_data = $1, tx_hash = $2 WHERE id = $3")
                .bind(json!({"anonymized": true}))
                .bind(&hashed_tx)
                .bind(id)
                .execute(&state.pool)
                .await?;
        }

        tracing::info!(
            contract_id = %contract_id,
            count = count,
            client_ip = client_ip,
            timestamp = %chrono::Utc::now(),
            "Events anonymized for contract (GDPR)"
        );

        crate::metrics::record_events_deleted(count);

        Ok(Json(json!({
            "contract_id": contract_id,
            "action": "anonymized",
            "count": count,
        })))
    } else {
        // Hard delete
        let result = sqlx::query("DELETE FROM events WHERE contract_id = $1")
            .bind(&contract_id)
            .execute(&state.pool)
            .await?;

        let count = result.rows_affected();

        tracing::info!(
            contract_id = %contract_id,
            count = count,
            client_ip = client_ip,
            timestamp = %chrono::Utc::now(),
            "Events deleted for contract (GDPR right-to-erasure)"
        );

        crate::metrics::record_events_deleted(count);

        Ok(Json(json!({
            "contract_id": contract_id,
            "action": "deleted",
            "count": count,
        })))
    }
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "pause endpoint only available on active indexer" })),
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state
        .indexer_state
        .is_active_indexer
        .load(Ordering::Relaxed)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({ "error": "resume endpoint only available on active indexer" })),
        ));
    }
    state
        .indexer_state
        .is_paused
        .store(false, Ordering::Relaxed);
    tracing::info!("Indexer resumed via admin API");
    Ok(Json(json!({ "indexer_paused": false })))
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

/// Query the min and max indexed ledger from the events table.
/// Returns `(None, None)` when no events have been indexed yet.
async fn get_indexed_ledger_range(
    pool: &sqlx::PgPool,
) -> Result<(Option<i64>, Option<i64>), AppError> {
    let row = sqlx::query("SELECT MIN(ledger) AS min_ledger, MAX(ledger) AS max_ledger FROM events")
        .fetch_one(pool)
        .await?;
    let min: Option<i64> = row.try_get("min_ledger")?;
    let max: Option<i64> = row.try_get("max_ledger")?;
    Ok((min, max))
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

pub async fn replay_events(
    State(state): State<AppState>,
    Json(request): Json<models::ReplayRequest>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    // Validate ledger range
    if request.from_ledger > request.to_ledger {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "from_ledger must be <= to_ledger"
            })),
        ));
    }

    // Validate range size (max 10,000 ledgers)
    let range_size = request.to_ledger.saturating_sub(request.from_ledger) + 1;
    if range_size > 10_000 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "ledger range cannot exceed 10,000 ledgers"
            })),
        ));
    }

    // Check if this replica is the active indexer (holds the advisory lock)
    let is_active = state
        .indexer_state
        .is_active_indexer
        .load(std::sync::atomic::Ordering::Relaxed);
    if !is_active {
        return Err((
            StatusCode::FORBIDDEN,
            Json(json!({
                "error": "replay endpoint only available on active indexer"
            })),
        ));
    }

    // Validate the requested range against the indexed window.
    let from = request.from_ledger as i64;
    let to = request.to_ledger as i64;

    let warning: Option<String> =
        match get_indexed_ledger_range(&state.pool).await.map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })? {
            (None, None) => {
                // No events indexed at all — any range is entirely outside.
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "error": "requested range is entirely outside the indexed window: no events have been indexed yet"
                    })),
                ));
            }
            (Some(min_indexed), Some(max_indexed)) => {
                // Entirely before the indexed window.
                if to < min_indexed {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": format!(
                                "requested range [{from}, {to}] is entirely before the indexed window [{min_indexed}, {max_indexed}]"
                            )
                        })),
                    ));
                }
                // Entirely after the indexed window (future ledgers).
                if from > max_indexed {
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(json!({
                            "error": format!(
                                "requested range [{from}, {to}] is entirely after the indexed window [{min_indexed}, {max_indexed}]"
                            )
                        })),
                    ));
                }
                // Partial overlap — warn the caller.
                if from < min_indexed || to > max_indexed {
                    Some(format!(
                        "requested range [{from}, {to}] is partially outside the indexed window [{min_indexed}, {max_indexed}]; only ledgers [{}, {}] will be replayed",
                        from.max(min_indexed),
                        to.min(max_indexed),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };

    // Record the replay job metric
    crate::metrics::record_replay_job();

    // Spawn background task to handle the replay
    let pool = state.pool.clone();
    let rpc_url = state.config.stellar_rpc_url.clone();
    let from_ledger = request.from_ledger;
    let to_ledger = request.to_ledger;

    tokio::spawn(async move {
        if let Err(e) = execute_replay_job(pool, &rpc_url, from_ledger, to_ledger).await {
            tracing::error!(error = %e, "Replay job failed");
        }
    });

    let mut body = json!({
        "message": "replay job accepted",
        "from_ledger": request.from_ledger,
        "to_ledger": request.to_ledger
    });
    if let Some(w) = warning {
        body["warning"] = json!(w);
    }

    Ok((StatusCode::ACCEPTED, Json(body)))
}

/// Execute the replay job using the same fetch_and_store_events logic
async fn execute_replay_job(
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

/// Store event with idempotency using ON CONFLICT DO NOTHING
async fn store_event_with_idempotency(
    pool: &sqlx::PgPool,
    event: &crate::models::SorobanEvent,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    let ledger = i64::try_from(event.ledger)?;
    let timestamp = DateTime::parse_from_rfc3339(&event.ledger_closed_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))?;

    let event_data = serde_json::json!({
        "value": event.value,
        "topic": event.topic
    });

    let contract_id = event.contract_id.clone();
    let event_type = event.event_type.clone();
    let tx_hash = event.tx_hash.clone();

    let rows_affected = crate::error::with_deadlock_retry(3, || {
        let pool = pool.clone();
        let contract_id = contract_id.clone();
        let event_type = event_type.clone();
        let tx_hash = tx_hash.clone();
        let event_data = event_data.clone();
        async move {
            sqlx::query(
                r#"
                INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (tx_hash, contract_id, event_type) DO NOTHING
                "#,
            )
            .bind(contract_id)
            .bind(event_type)
            .bind(tx_hash)
            .bind(ledger)
            .bind(timestamp)
            .bind(event_data)
            .execute(&pool)
            .await
            .map(|r| r.rows_affected())
        }
    })
    .await
    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

    Ok(rows_affected)
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

// ============================================================================
// Schema Management Endpoints
// ============================================================================

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct RegisterSchemaRequest {
    /// JSON Schema definition (Draft 7)
    pub schema: Value,
    /// Optional human-readable description of this schema (issue #617 versioning)
    pub description: Option<String>,
}

/// Register or update a JSON Schema for a contract
#[utoipa::path(
    post,
    path = "/v1/admin/contracts/{contract_id}/schema",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Contract ID")
    ),
    request_body = RegisterSchemaRequest,
    responses(
        (status = 200, description = "Schema registered successfully"),
        (status = 400, description = "Invalid schema or contract ID"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn register_contract_schema(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(payload): Json<RegisterSchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    validate_contract_id(&contract_id)?;

    let validator = state
        .schema_validator
        .as_ref()
        .ok_or_else(|| AppError::Internal("Schema validator not initialized".to_string()))?;

    validator
        .register_schema_described(
            &contract_id,
            &payload.schema,
            payload.description.as_deref(),
        )
        .await
        .map_err(|e| AppError::Validation(format!("Invalid schema: {}", e)))?;

    // Return the current version after registration
    let info = validator.get_schema_info(&contract_id).await;
    Ok((
        StatusCode::OK,
        Json(json!({
            "status": "ok",
            "message": "Schema registered successfully",
            "contract_id": contract_id,
            "version": info.map(|i| i.version).unwrap_or(1),
        })),
    ))
}

/// List all registered JSON Schemas (metadata only, no schema body)
#[utoipa::path(
    get,
    path = "/v1/admin/schemas",
    tag = "admin",
    responses(
        (status = 200, description = "List of registered schemas"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn list_contract_schemas(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let validator = state
        .schema_validator
        .as_ref()
        .ok_or_else(|| AppError::Internal("Schema validator not initialized".to_string()))?;

    let schemas = validator
        .list_schemas()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok((StatusCode::OK, Json(json!({ "schemas": schemas }))))
}

/// Get the JSON Schema for a contract
#[utoipa::path(
    get,
    path = "/v1/admin/contracts/{contract_id}/schema",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Contract ID")
    ),
    responses(
        (status = 200, description = "Schema retrieved successfully"),
        (status = 404, description = "No schema registered for this contract"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn get_contract_schema(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    validate_contract_id(&contract_id)?;

    let validator = state
        .schema_validator
        .as_ref()
        .ok_or_else(|| AppError::Internal("Schema validator not initialized".to_string()))?;

    let schema = validator
        .get_schema(&contract_id)
        .await
        .ok_or(AppError::NotFound)?;

    Ok((StatusCode::OK, Json(json!({ "schema": schema }))))
}

/// Delete the JSON Schema for a contract
#[utoipa::path(
    delete,
    path = "/v1/admin/contracts/{contract_id}/schema",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Contract ID")
    ),
    responses(
        (status = 200, description = "Schema deleted successfully"),
        (status = 404, description = "No schema registered for this contract"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn delete_contract_schema(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    validate_contract_id(&contract_id)?;

    let validator = state
        .schema_validator
        .as_ref()
        .ok_or_else(|| AppError::Internal("Schema validator not initialized".to_string()))?;

    let deleted = validator
        .delete_schema(&contract_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if deleted {
        Ok((
            StatusCode::OK,
            Json(json!({
                "status": "ok",
                "message": "Schema deleted successfully"
            })),
        ))
    } else {
        Err(AppError::NotFound)
    }
}

/// Validate event data against a contract's JSON Schema
#[utoipa::path(
    post,
    path = "/v1/admin/contracts/{contract_id}/validate",
    tag = "admin",
    params(
        ("contract_id" = String, Path, description = "Contract ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Validation result"),
        (status = 400, description = "Validation failed with error details"),
        (status = 404, description = "No schema registered for this contract"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn validate_event_data_against_schema(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Json(event_data): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    validate_contract_id(&contract_id)?;

    let validator = state.schema_validator
        .as_ref()
        .ok_or_else(|| AppError::Internal("Schema validator not initialized".to_string()))?;

    match validator.validate_event_data(&contract_id, &event_data).await {
        None => Err(AppError::NotFound),
        Some((true, _)) => Ok((
            StatusCode::OK,
            Json(json!({
                "valid": true,
                "message": "Event data is valid"
            }))
        )),
        Some((false, errors)) => {
            let error_msg = format!("Event data validation failed with {} error(s)", errors.len());
            Err(AppError::ValidationWithDetails(error_msg, errors))
        }
    }
}


/// Start a background masking job for event data
#[utoipa::path(
    post,
    path = "/v1/admin/mask-events",
    tag = "admin",
    request_body = models::MaskEventsRequest,
    responses(
        (status = 200, description = "Masking job started", body = models::MaskEventsResponse),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn start_mask_events(
    State(state): State<AppState>,
    Json(req): Json<models::MaskEventsRequest>,
) -> Result<Json<models::MaskEventsResponse>, AppError> {
    let job_id = Uuid::new_v4().to_string();
    
    let pool = state.pool.clone();
    let contract_ids = req.contract_ids.clone();
    let job_id_clone = job_id.clone();
    
    tokio::spawn(async move {
        let _ = mask_events_background(&pool, contract_ids, &job_id_clone).await;
    });
    
    Ok(Json(models::MaskEventsResponse {
        job_id,
        status: "pending".to_string(),
    }))
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

async fn mask_events_background(
    pool: &sqlx::PgPool,
    contract_ids: Option<Vec<String>>,
    _job_id: &str,
) -> Result<(), AppError> {
    let mut conditions: Vec<String> = vec!["event_data IS NOT NULL".to_string()];
    let mut bind_idx = 1;
    
    if let Some(ref ids) = contract_ids {
        if !ids.is_empty() {
            let placeholders = ids.iter().enumerate()
                .map(|(i, _)| format!("${}", bind_idx + i))
                .collect::<Vec<_>>()
                .join(",");
            conditions.push(format!("contract_id IN ({})", placeholders));
            bind_idx += ids.len() as i32;
        }
    }
    
    let where_clause = conditions.join(" AND ");
    let query_str = format!(
        "SELECT id, event_data FROM events WHERE {} ORDER BY id LIMIT 1000",
        where_clause
    );
    
    let mut q = sqlx::query(&query_str);
    if let Some(ref ids) = contract_ids {
        for id in ids {
            q = q.bind(id);
        }
    }
    
    let rows = q.fetch_all(pool).await?;
    
    for row in rows {
        let id: Uuid = row.try_get("id")?;
        let event_data: serde_json::Value = row.try_get("event_data")?;
        let masked_data = mask_event_data(&event_data);
        
        sqlx::query("UPDATE events SET event_data = $1 WHERE id = $2")
            .bind(&masked_data)
            .bind(id)
            .execute(pool)
            .await?;
    }
    
    Ok(())
}

fn mask_event_data(data: &serde_json::Value) -> serde_json::Value {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    
    fn deterministic_hash(value: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }
    
    match data {
        serde_json::Value::Object(obj) => {
            let mut new_obj = serde_json::Map::new();
            for (key, value) in obj {
                let masked_value = mask_event_data(value);
                let key_lower = key.to_lowercase();
                
                if key_lower.contains("address") || key_lower.contains("account") {
                    if let serde_json::Value::String(s) = &masked_value {
                        let hash = deterministic_hash(s);
                        new_obj.insert(key.clone(), serde_json::Value::String(format!("G{:x}", hash)[..56].to_string()));
                        continue;
                    }
                }
                new_obj.insert(key.clone(), masked_value);
            }
            serde_json::Value::Object(new_obj)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(mask_event_data).collect())
        }
        _ => data.clone(),
    }
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

/// Scan a single event's data for PII (dry-run, does not modify the event)
#[utoipa::path(
    post,
    path = "/v1/config/anonymization/scan",
    tag = "config",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "PII scan result"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn scan_event_for_pii(
    State(state): State<AppState>,
    Json(event_data): Json<serde_json::Value>,
) -> Result<impl IntoResponse, AppError> {
    let detection_rules = match state.anonymization_config.as_ref() {
        Some(cfg) => cfg.compile_detection_rules().await,
        None => {
            crate::anonymization::default_rules()
                .iter()
                .filter(|r| r.enabled)
                .filter_map(|r| Regex::new(&r.pattern).ok().map(|re| (r.name.clone(), re)))
                .collect()
        }
    };

    let result = crate::anonymization::scan_for_pii(&event_data, &detection_rules);
    let detections: Vec<serde_json::Value> = result
        .detections
        .iter()
        .map(|d| json!({ "field": d.field_path, "rule": d.rule_name }))
        .collect();

    Ok(Json(json!({
        "pii_detected": !detections.is_empty(),
        "detections": detections,
    })))
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

// ── Issue #586 / #587 / #710: Replication, feature flags, and backup verification tests ──

#[cfg(test)]
mod ops_tests {
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
}

// ── Issue #881: Bulk Event Export Endpoints ────────────────────────────────

/// POST /v1/admin/events/export
/// Start a new bulk event export with specified format and compression
pub async fn start_event_export(
    State(state): State<AppState>,
    Json(request): Json<crate::bulk_export::ExportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let job_id = state.bulk_export_manager.start_export(&state.pool, request).await
        .map_err(|e| AppError::Internal(e))?;

    Ok(Json(json!({
        "job_id": job_id,
        "status": "pending",
    })))
}

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
