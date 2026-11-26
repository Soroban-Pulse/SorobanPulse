//! Network, ledger-hash and cross-chain correlation handlers (Issue #1241).

#[allow(unused_imports)]
use super::*;

// ── Issue #609: GET /v1/networks ─────────────────────────────────────────────

/// List all registered Soroban networks and their health status.
///
/// `GET /v1/networks`
#[utoipa::path(
    get,
    path = "/v1/networks",
    tag = "system",
    responses(
        (status = 200, description = "List of registered networks"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn list_networks(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let networks = crate::networks::list_networks(&state.pool).await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(json!({
        "count": networks.len(),
        "networks": networks,
    })))
}

// ── Issue #608: GET /v1/ledgers/{ledger}/hash ────────────────────────────────

/// Return the indexed hash for a specific ledger sequence number.
///
/// `GET /v1/ledgers/{ledger}/hash`
#[utoipa::path(
    get,
    path = "/v1/ledgers/{ledger}/hash",
    tag = "events",
    params(
        ("ledger" = u64, Path, description = "Ledger sequence number"),
    ),
    responses(
        (status = 200, description = "Ledger hash"),
        (status = 404, description = "Ledger not found"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn get_ledger_hash(
    State(state): State<AppState>,
    Path(ledger): Path<u64>,
) -> Result<Json<Value>, AppError> {
    match crate::ledger_hashes::get_ledger_hash(&state.pool, ledger).await {
        Ok(Some(hash)) => Ok(Json(json!({ "ledger": ledger, "hash": hash }))),
        Ok(None) => Err(AppError::NotFound),
        Err(e) => Err(AppError::Internal(e.to_string())),
    }
}

/// Verify the hash chain for a ledger range.
///
/// `GET /v1/ledgers/verify-chain?from={from}&to={to}`
#[utoipa::path(
    get,
    path = "/v1/ledgers/verify-chain",
    tag = "events",
    params(
        ("from" = u64, Query, description = "Start ledger (inclusive)"),
        ("to"   = u64, Query, description = "End ledger (inclusive)"),
    ),
    responses(
        (status = 200, description = "Hash chain verification result"),
        (status = 500, description = "Internal server error"),
    )
)]
pub async fn verify_ledger_hash_chain(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let from: u64 = params
        .get("from")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let to: u64 = params
        .get("to")
        .and_then(|v| v.parse().ok())
        .unwrap_or(from);
    let mismatches = crate::ledger_hashes::verify_hash_chain(&state.pool, from, to)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Json(json!({
        "from": from,
        "to": to,
        "mismatches": mismatches,
        "ok": mismatches == 0,
    })))
}

/// Get cross-chain trace for a transaction
/// Issue #682: Implement cross-chain event correlation
#[utoipa::path(
    get,
    path = "/v1/cross-chain/trace/{tx_hash}",
    tag = "cross-chain",
    params(
        ("tx_hash" = String, Path, description = "Transaction hash to trace")
    ),
    responses(
        (status = 200, description = "Cross-chain trace", body = serde_json::Value),
        (status = 404, description = "No trace found"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn get_cross_chain_trace(
    State(state): State<AppState>,
    Path(tx_hash): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Query all events related to this transaction
    let events = sqlx::query_as::<_, (String, String, String, String, String, i32, String)>(
        "SELECT id, contract_id, event_type, tx_hash, topic, ledger, ledger_close_time FROM events WHERE tx_hash = $1 ORDER BY ledger"
    )
    .bind(&tx_hash)
    .fetch_all(&state.read_pool)
    .await
    .map_err(|_| AppError::NotFound)?;

    if events.is_empty() {
        return Err(AppError::NotFound);
    }

    // Build cross-chain trace
    let root_tx = crate::cross_chain_correlation::TransactionId::new("soroban-mainnet", tx_hash.clone());
    let mut builder = crate::cross_chain_correlation::CrossChainTraceBuilder::new(root_tx);

    for (event_id, contract_id, event_type, _, _, ledger, ledger_close_time) in events {
        let trace_event = crate::cross_chain_correlation::TraceEvent {
            event_id,
            chain: "soroban-mainnet".to_string(),
            contract_id,
            event_type,
            tx_hash: tx_hash.clone(),
            ledger: ledger as u64,
            ledger_close_time: ledger_close_time.parse().unwrap_or_else(|_| chrono::Utc::now()),
            depth: 0,
            confidence: 1.0,
        };
        builder = builder.add_event(trace_event);
    }

    let trace = builder.build().ok_or(AppError::NotFound)?;

    Ok(Json(json!({
        "id": trace.id,
        "root_transaction": {
            "chain": trace.root_transaction.chain,
            "tx_hash": trace.root_transaction.tx_hash
        },
        "events_count": trace.events.len(),
        "correlations_count": trace.correlations.len(),
        "chain_sequence": trace.chain_sequence,
        "overall_confidence": trace.overall_confidence,
        "created_at": trace.created_at,
        "events": trace.events.iter().map(|e| json!({
            "event_id": e.event_id,
            "chain": e.chain,
            "contract_id": e.contract_id,
            "event_type": e.event_type,
            "ledger": e.ledger,
            "confidence": e.confidence
        })).collect::<Vec<_>>()
    })))
}

/// Get causality analysis between two events
/// Issue #682: Implement cross-chain event correlation
#[utoipa::path(
    get,
    path = "/v1/cross-chain/causality",
    tag = "cross-chain",
    params(
        ("event1" = String, Query, description = "First event ID"),
        ("event2" = String, Query, description = "Second event ID")
    ),
    responses(
        (status = 200, description = "Causality analysis", body = serde_json::Value),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    security(("api_key" = []))
)]
pub async fn analyze_causality(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let event1_id = params.get("event1")
        .ok_or(AppError::BadRequest("event1 parameter required".to_string()))?;
    let event2_id = params.get("event2")
        .ok_or(AppError::BadRequest("event2 parameter required".to_string()))?;

    // Fetch both events
    let event1 = sqlx::query_as::<_, (String, String, String, String, i32, String)>(
        "SELECT id, contract_id, event_type, tx_hash, ledger, ledger_close_time FROM events WHERE id = $1"
    )
    .bind(event1_id)
    .fetch_optional(&state.read_pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let event2 = sqlx::query_as::<_, (String, String, String, String, i32, String)>(
        "SELECT id, contract_id, event_type, tx_hash, ledger, ledger_close_time FROM events WHERE id = $1"
    )
    .bind(event2_id)
    .fetch_optional(&state.read_pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let engine = crate::cross_chain_correlation::CorrelationEngine::new();

    let trace1 = crate::cross_chain_correlation::TraceEvent {
        event_id: event1.0,
        chain: "soroban-mainnet".to_string(),
        contract_id: event1.1,
        event_type: event1.2,
        tx_hash: event1.3,
        ledger: event1.4 as u64,
        ledger_close_time: event1.5.parse().unwrap_or_else(|_| chrono::Utc::now()),
        depth: 0,
        confidence: 1.0,
    };

    let trace2 = crate::cross_chain_correlation::TraceEvent {
        event_id: event2.0,
        chain: "soroban-mainnet".to_string(),
        contract_id: event2.1,
        event_type: event2.2,
        tx_hash: event2.3,
        ledger: event2.4 as u64,
        ledger_close_time: event2.5.parse().unwrap_or_else(|_| chrono::Utc::now()),
        depth: 1,
        confidence: 1.0,
    };

    let similarity = engine.calculate_similarity(&trace1, &trace2);
    let causality = engine.detect_causality(&trace1, &trace2);

    Ok(Json(json!({
        "event1_id": event1_id,
        "event2_id": event2_id,
        "similarity_score": similarity,
        "causality": causality.map(|c| format!("{:?}", c)),
        "related": causality.is_some()
    })))
}
