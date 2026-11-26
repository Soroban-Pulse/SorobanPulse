//! Contract handlers: listing, search, summaries, ABI, metadata and schema registry (Issue #1241).

#[allow(unused_imports)]
use super::*;

pub(super) static CONTRACTS_CACHE: OnceLock<Mutex<Option<CacheEntry>>> = OnceLock::new();

pub(super) fn contracts_cache() -> &'static Mutex<Option<CacheEntry>> {
    CONTRACTS_CACHE.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct ContractHistoryParams {
    pub bucket: Option<String>,
    pub days: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/stats/history",
    tag = "events",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
        ("bucket" = Option<String>, Query, description = "Aggregation bucket. Currently only 1d is supported"),
        ("days" = Option<i64>, Query, description = "Number of daily buckets to return"),
        ("from" = Option<String>, Query, description = "Start date, YYYY-MM-DD"),
        ("to" = Option<String>, Query, description = "End date, YYYY-MM-DD"),
    ),
    responses(
        (status = 200, description = "Daily contract event and unique transaction history", body = serde_json::Value),
        (status = 400, description = "Invalid contract_id, bucket, or date range", body = ErrorResponse),
    )
)]
pub async fn get_contract_stats_history(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Query(params): Query<ContractHistoryParams>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;
    let bucket = params.bucket.as_deref().unwrap_or("1d");
    if bucket != "1d" {
        return Err(AppError::Validation("unsupported bucket: only bucket=1d is supported".to_string()));
    }

    let today = Utc::now().date_naive();
    let (start_date, end_date) = if params.from.is_some() || params.to.is_some() {
        let from = params.from.as_deref().ok_or_else(|| AppError::Validation("from is required when to is provided".to_string()))?;
        let to = params.to.as_deref().ok_or_else(|| AppError::Validation("to is required when from is provided".to_string()))?;
        let start = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("from must be YYYY-MM-DD".to_string()))?;
        let end = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .map_err(|_| AppError::Validation("to must be YYYY-MM-DD".to_string()))?;
        (start, end)
    } else {
        let days = params.days.unwrap_or(30);
        if !(1..=366).contains(&days) {
            return Err(AppError::Validation("days must be between 1 and 366".to_string()));
        }
        (today - chrono::Duration::days(days - 1), today)
    };
    if start_date > end_date {
        return Err(AppError::Validation("from must be <= to".to_string()));
    }
    if (end_date - start_date).num_days() + 1 > 366 {
        return Err(AppError::Validation("date range cannot exceed 366 daily buckets".to_string()));
    }

    let start = std::time::Instant::now();
    let rows = sqlx::query(
        r#"
        SELECT d::date AS date,
               COALESCE(s.event_count, 0)::bigint AS event_count,
               COALESCE(s.unique_tx_count, 0)::bigint AS unique_tx_count
        FROM generate_series($2::date, $3::date, interval '1 day') AS d
        LEFT JOIN (
            SELECT timestamp::date AS event_date,
                   COUNT(*) AS event_count,
                   COUNT(DISTINCT tx_hash) AS unique_tx_count
            FROM events
            WHERE contract_id = $1
              AND timestamp::date BETWEEN $2::date AND $3::date
            GROUP BY timestamp::date
        ) s ON s.event_date = d::date
        ORDER BY d::date ASC
        "#,
    )
    .bind(&contract_id)
    .bind(start_date)
    .bind(end_date)
    .fetch_all(&state.read_pool)
    .await?;
    crate::metrics::record_contract_history_query_duration(start.elapsed());

    let data = rows
        .into_iter()
        .map(|row| {
            let date: chrono::NaiveDate = row.try_get("date")?;
            let event_count: i64 = row.try_get("event_count")?;
            let unique_tx_count: i64 = row.try_get("unique_tx_count")?;
            Ok(json!({ "date": date, "event_count": event_count, "unique_tx_count": unique_tx_count }))
        })
        .collect::<Result<Vec<_>, sqlx::Error>>()?;

    Ok(Json(json!({ "contract_id": contract_id, "bucket": bucket, "data": data })))
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

/// Public: read a contract's label/metadata (#1066).
#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/metadata",
    tag = "contracts",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID"),
    ),
    responses(
        (status = 200, description = "Contract metadata", body = crate::contract_metadata::ContractMetadata),
        (status = 400, description = "Invalid contract_id", body = ErrorResponse),
        (status = 404, description = "No metadata registered", body = ErrorResponse),
    )
)]
pub async fn get_contract_metadata(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<crate::contract_metadata::ContractMetadata>, AppError> {
    validate_contract_id(&contract_id)?;
    let metadata =
        crate::contract_metadata::get_contract_metadata(&state.read_pool, &contract_id)
            .await?
            .ok_or(AppError::NotFound)?;
    Ok(Json(metadata))
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

#[utoipa::path(
    get,
    path = "/v1/contracts",
    tag = "events",
    params(
        ("page" = Option<i64>, Query, description = "Page number (default 1)"),
        ("limit" = Option<i64>, Query, description = "Items per page (1-100, default 20)"),
        ("sort" = Option<String>, Query, description = "Sort order: event_count_desc, event_count_asc, last_seen_desc (default), first_seen_asc"),
    ),
    responses(
        (status = 200, description = "Paginated list of indexed contract IDs with event counts, ledger info, deployer, WASM hash and label"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 429, description = "Too many requests", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse),
    )
)]
pub async fn get_contracts(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<Value>, AppError> {
    let limit = params.limit();
    let offset = params.offset();

    // Determine sort order
    let sort_clause = match params.sort {
        Some(SortOrder::Asc) => "ORDER BY event_count ASC",
        _ => "ORDER BY last_seen_ledger DESC",
    };

    // Issue #1255: join the deployment registry and the labels registry.
    let rows = sqlx::query_as::<_, ContractSummary>(
        &format!(
            "SELECT s.*, c.deployer, c.wasm_hash, c.deployed_at, m.name AS label FROM ( \
                 SELECT contract_id, COUNT(*) AS event_count, MIN(ledger) AS first_seen_ledger, \
                 MAX(ledger) AS last_seen_ledger, MAX(timestamp) AS last_event_at \
                 FROM events GROUP BY contract_id {} \
                 LIMIT $1 OFFSET $2 \
             ) s \
             LEFT JOIN contracts c ON c.contract_id = s.contract_id \
             LEFT JOIN contract_metadata m ON m.contract_id = s.contract_id \
             {}",
            sort_clause, sort_clause
        ),
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.read_pool)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(DISTINCT contract_id) FROM events")
        .fetch_one(&state.read_pool)
        .await?;

    let result = json!({
        "data": rows,
        "total": total,
        "page": params.page.unwrap_or(1),
        "limit": limit,
    });

    Ok(Json(result))
}

/// In-process TTL cache for per-contract summary data.
pub(super) static CONTRACT_SUMMARY_CACHE: OnceLock<Mutex<std::collections::HashMap<String, (ContractDetailSummary, std::time::Instant)>>> = OnceLock::new();

pub(super) fn contract_summary_cache() -> &'static Mutex<std::collections::HashMap<String, (ContractDetailSummary, std::time::Instant)>> {
    CONTRACT_SUMMARY_CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()))
}

/// TTL for contract summary cache entries (configurable via env, default 60s).
pub(super) fn summary_cache_ttl() -> std::time::Duration {
    let secs: u64 = std::env::var("CONTRACT_SUMMARY_CACHE_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);
    std::time::Duration::from_secs(secs)
}

/// GET /v1/contracts/:contract_id/summary
///
/// Returns a per-contract summary: total events, first/last event timestamp,
/// event type breakdown, unique tx count, and ledger range.
///
/// Uses the `mv_contract_summary` materialized view as the primary data source.
/// Falls back to a direct query if the view is stale or unavailable.
/// Results are cached in-process with a configurable TTL.
#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/summary",
    tag = "events",
    params(
        ("contract_id" = String, Path, description = "Stellar contract ID (56-char, starts with C)"),
    ),
    responses(
        (status = 200, description = "Contract summary", body = crate::models::ContractDetailSummary),
        (status = 400, description = "Invalid contract_id format", body = ErrorResponse),
        (status = 404, description = "Contract not found", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_contract_summary(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    validate_contract_id(&contract_id)?;

    let ttl = summary_cache_ttl();

    // Check in-process cache
    {
        let cache = contract_summary_cache().lock().await;
        if let Some((summary, inserted_at)) = cache.get(&contract_id) {
            if inserted_at.elapsed() < ttl {
                return Ok(Json(serde_json::to_value(summary)?));
            }
        }
    }

    // Try the canonical materialized view first
    let mv_row = sqlx::query(
        "SELECT event_count, first_event_timestamp, latest_event_timestamp, min_ledger, \
         max_ledger, unique_tx_count, contract_events, diagnostic_events, system_events \
         FROM mv_contract_summary WHERE contract_id = $1",
    )
    .bind(&contract_id)
    .fetch_optional(&state.read_pool)
    .await;

    let summary = match mv_row {
        Ok(Some(row)) => {
            ContractDetailSummary {
                contract_id: contract_id.clone(),
                total_events: row.try_get("event_count")?,
                first_event_at: row.try_get("first_event_timestamp")?,
                last_event_at: row.try_get("latest_event_timestamp")?,
                unique_tx_count: row.try_get("unique_tx_count")?,
                ledger_range: LedgerRange {
                    min: row.try_get("min_ledger")?,
                    max: row.try_get("max_ledger")?,
                },
                event_type_breakdown: EventTypeBreakdown {
                    contract: row.try_get("contract_events")?,
                    diagnostic: row.try_get("diagnostic_events")?,
                    system: row.try_get("system_events")?,
                },
                from_cache: true,
            }
        }
        // Materialized view missing or stale — fall back to direct query
        _ => {
            let row = sqlx::query(
                "SELECT \
                    COUNT(*)                                                        AS total_events, \
                    MIN(timestamp)                                                  AS first_event_at, \
                    MAX(timestamp)                                                  AS last_event_at, \
                    MIN(ledger)                                                     AS min_ledger, \
                    MAX(ledger)                                                     AS max_ledger, \
                    COUNT(DISTINCT tx_hash)                                         AS unique_tx_count, \
                    COUNT(*) FILTER (WHERE event_type = 'contract')                 AS contract_events, \
                    COUNT(*) FILTER (WHERE event_type = 'diagnostic')               AS diagnostic_events, \
                    COUNT(*) FILTER (WHERE event_type = 'system')                   AS system_events \
                 FROM events WHERE contract_id = $1",
            )
            .bind(&contract_id)
            .fetch_one(&state.read_pool)
            .await?;

            let total_events: i64 = row.try_get("total_events")?;
            if total_events == 0 {
                return Err(AppError::NotFound);
            }

            ContractDetailSummary {
                contract_id: contract_id.clone(),
                total_events,
                first_event_at: row.try_get("first_event_at")?,
                last_event_at: row.try_get("last_event_at")?,
                unique_tx_count: row.try_get("unique_tx_count")?,
                ledger_range: LedgerRange {
                    min: row.try_get("min_ledger")?,
                    max: row.try_get("max_ledger")?,
                },
                event_type_breakdown: EventTypeBreakdown {
                    contract: row.try_get("contract_events")?,
                    diagnostic: row.try_get("diagnostic_events")?,
                    system: row.try_get("system_events")?,
                },
                from_cache: false,
            }
        }
    };

    // Check for not-found after materialized view path
    if summary.total_events == 0 {
        return Err(AppError::NotFound);
    }

    // Store in cache
    {
        let mut cache = contract_summary_cache().lock().await;
        cache.insert(contract_id, (summary.clone(), std::time::Instant::now()));
    }

    Ok(Json(serde_json::to_value(&summary)?))
}

/// GET /v1/contracts/search?q=prefix
///
/// Returns contract IDs matching the given prefix (minimum 4 characters).
/// Uses a `LIKE 'prefix%'` query against the existing `contract_id` index.
#[utoipa::path(
    get,
    path = "/v1/contracts/search",
    tag = "events",
    params(
        ("q" = String, Query, description = "Contract ID prefix to search for (minimum 4 characters)"),
        ("limit" = Option<i64>, Query, description = "Maximum results to return (1–100, default 20)"),
    ),
    responses(
        (status = 200, description = "Matching contract IDs with event counts", body = Vec<crate::models::ContractSearchResult>),
        (status = 400, description = "Prefix too short (< 4 chars) or missing", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
    )
)]
pub async fn get_contracts_search(
    State(state): State<AppState>,
    Query(params): Query<ContractSearchParams>,
) -> Result<Json<Value>, AppError> {
    let prefix = params.q.as_deref().unwrap_or("").trim().to_string();

    if prefix.len() < 4 {
        return Err(AppError::Validation(
            "q must be at least 4 characters to prevent full-table scans".to_string(),
        ));
    }

    // Sanitize: only allow alphanumeric characters in the prefix to prevent injection
    if !prefix.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::Validation(
            "q must contain only alphanumeric characters".to_string(),
        ));
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let like_pattern = format!("{}%", prefix);

    let rows = sqlx::query_as::<_, ContractSearchResult>(
        "SELECT contract_id, COUNT(*) AS event_count, MAX(timestamp) AS last_event_at \
         FROM events WHERE contract_id LIKE $1 \
         GROUP BY contract_id ORDER BY event_count DESC LIMIT $2",
    )
    .bind(&like_pattern)
    .bind(limit)
    .fetch_all(&state.read_pool)
    .await?;

    Ok(Json(json!({
        "data": rows,
        "query": prefix,
        "limit": limit,
    })))
}

/// Query the min and max indexed ledger from the events table.
/// Returns `(None, None)` when no events have been indexed yet.
pub(super) async fn get_indexed_ledger_range(
    pool: &sqlx::PgPool,
) -> Result<(Option<i64>, Option<i64>), AppError> {
    let row = sqlx::query("SELECT MIN(ledger) AS min_ledger, MAX(ledger) AS max_ledger FROM events")
        .fetch_one(pool)
        .await?;
    let min: Option<i64> = row.try_get("min_ledger")?;
    let max: Option<i64> = row.try_get("max_ledger")?;
    Ok((min, max))
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

// ── Issue #607: GET /v1/contracts/{contract_id}/abi (cached) ─────────────────

/// Return the cached ABI for a contract (cache-backed, with hit/miss metrics).
///
/// `GET /v1/contracts/{contract_id}/abi/cached`
#[utoipa::path(
    get,
    path = "/v1/contracts/{contract_id}/abi/cached",
    tag = "events",
    params(
        ("contract_id" = String, Path, description = "Contract ID"),
    ),
    responses(
        (status = 200, description = "Contract ABI"),
        (status = 404, description = "ABI not found"),
    )
)]
pub async fn get_contract_abi_cached(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    match crate::abi::fetch_contract_abi(&state.pool, &state.abi_cache, &contract_id).await {
        Some(abi) => Ok(Json(json!({ "contract_id": contract_id, "abi": abi }))),
        None => Err(AppError::NotFound),
    }
}

// ============================================================================
// Issue #627: Bloom filter based contract existence check
// ============================================================================

/// Check if a contract has any indexed events using the bloom filter.
///
/// `GET /v1/contracts/exists?id=CABC...`
///
/// Returns whether the contract is likely to exist based on the bloom filter.
/// This is a fast check without a database query.
#[utoipa::path(
    get,
    path = "/v1/contracts/exists",
    tag = "contracts",
    params(
        ("id", description = "Contract ID to check"),
    ),
    responses(
        (status = 200, description = "Contract existence check result", body = ContractExistsResponse),
        (status = 400, description = "Invalid contract ID"),
    )
)]
pub async fn check_contract_exists(
    State(state): State<AppState>,
    Query(params): Query<ContractExistsParams>,
) -> Result<impl IntoResponse, AppError> {
    use models::ContractId;
    
    let contract_id = params.id.trim();
    
    // Validate contract ID format
    ContractId::validate(contract_id)?;
    
    // Check if contract exists in bloom filter
    let exists_in_filter = if let Some(ref filter) = state.indexer_state.bloom_filter {
        filter.contains_contract(contract_id)
    } else {
        // If bloom filter is not available, do a DB lookup
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events WHERE contract_id = $1 LIMIT 1"
        )
        .bind(contract_id)
        .fetch_one(&state.pool)
        .await?;
        count > 0
    };
    
    Ok(Json(models::ContractExistsResponse {
        contract_id: contract_id.to_string(),
        exists: exists_in_filter,
        method: if state.indexer_state.bloom_filter.is_some() {
            "bloom_filter".to_string()
        } else {
            "database_query".to_string()
        },
    }))
}

#[derive(serde::Deserialize)]
pub struct ContractExistsParams {
    pub id: String,
}
