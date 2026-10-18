//! Public `/v1` read and query API: events, streams, contracts, ledgers,
//! analytics, exports, batch operations, and runtime config introspection.

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};

use super::AppState;
use crate::{error::AppError, handlers, models::Event};

pub async fn get_events_with_params(
    State(state): State<AppState>,
    Query(params): Query<crate::models::PaginationParams>,
) -> Result<Json<crate::models::Paginated<Event>>, AppError> {
    let response = handlers::get_events(
        State(state),
        Query(params),
        axum::http::HeaderMap::new(),
        axum::http::Extensions::new(),
    )
    .await?;

    let payload = response.into_body();
    let bytes = axum::body::to_bytes(payload, usize::MAX)
        .await
        .map_err(|e| crate::error::AppError::Internal(format!("failed to decode events response: {}", e)))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| crate::error::AppError::Internal(format!("failed to parse events response: {}", e)))?;
    let records: Vec<Event> = serde_json::from_value(value["events"].clone())
        .map_err(|e| crate::error::AppError::Internal(format!("failed to decode event records: {}", e)))?;
    Ok(Json(crate::models::Paginated {
        data: records,
        page: value["page"].as_i64().unwrap_or(1),
        limit: value["limit"].as_i64().unwrap_or(20),
        total: value["total"].as_i64().unwrap_or(0),
        has_more: value["has_more"].as_bool().unwrap_or(false),
    }))
}

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/events", get(handlers::get_events))
        .route("/events/feed.rss", get(handlers::get_events_feed))
        .route("/events/stats", get(handlers::get_event_stats))
        .route("/contracts/{contract_id}/stats/history", get(handlers::get_contract_stats_history))
        .route("/events/diff", get(handlers::get_events_diff))
        .route("/events/export", get(handlers::export_events))
        .route("/events/timeseries", get(handlers::get_timeseries))
        .route("/events/temporal", get(handlers::get_temporal_events))
        .route("/events/recent", get(handlers::get_recent_events))
        .route("/events/stream", get(handlers::stream_events))
        .route("/events/stream/multi", get(handlers::stream_events_multi))
        .route("/events/ws", get(handlers::ws_stream_events))
        .route(
            "/events/contract/{contract_id}",
            get(handlers::get_events_by_contract),
        )
        .route(
            "/events/contract/{contract_id}/stream",
            get(handlers::stream_events_by_contract),
        )
        .route(
            "/events/tx/batch",
            axum::routing::post(handlers::get_events_by_tx_batch),
        )
        .route("/events/tx/{tx_hash}/related", get(handlers::get_related_events_by_tx))
        .route("/events/tx/{tx_hash}", get(handlers::get_events_by_tx))
        .route(
            "/events/ledger-hash/{hash}",
            get(handlers::get_events_by_ledger_hash),
        )
        .route("/contracts", get(handlers::get_contracts))
        .route("/contracts/search", get(handlers::get_contracts_search))
        .route("/contracts/exists", get(handlers::check_contract_exists))
        .route("/config", get(handlers::get_config))
        .route("/rate-limit/status", get(handlers::get_rate_limit_status))
        .route("/config/anonymization", axum::routing::get(handlers::get_anonymization_config))
        .route("/config/anonymization/rules", axum::routing::post(handlers::upsert_anonymization_rule))
        .route("/config/anonymization/rules/{name}", axum::routing::delete(handlers::delete_anonymization_rule))
        .route("/config/anonymization/scan", axum::routing::post(handlers::scan_event_for_pii))
        .route("/cross-chain/trace/{tx_hash}", get(handlers::get_cross_chain_trace))
        .route("/cross-chain/causality", get(handlers::analyze_causality))
        .route("/contracts/{contract_id}/metadata", get(handlers::get_contract_metadata))
        .route("/contracts/{contract_id}/summary", get(handlers::get_contract_summary))
        .route("/contracts/{contract_id}/event-counts", get(handlers::get_contract_event_counts))
        .route("/replay/with-transform", axum::routing::post(handlers::replay_with_transform))
        // Issue #609: Multi-chain networks
        .route("/networks", axum::routing::get(handlers::list_networks))
        // Issue #608: Ledger hash endpoints
        .route("/ledgers/{ledger}/hash", axum::routing::get(handlers::get_ledger_hash))
        .route("/ledgers/verify-chain", axum::routing::get(handlers::verify_ledger_hash_chain))
        .route("/tokens/{contract_id}/transfers", axum::routing::get(crate::token_events::get_token_transfers))
        .route("/accounts/{address}/events", axum::routing::get(crate::account_events::get_account_events))
        .route("/contracts/{contract_id}/versions", axum::routing::get(crate::contract_versions::get_contract_versions))
        // Issue #607: Cached ABI endpoint
        .route("/contracts/{contract_id}/abi/cached", axum::routing::get(handlers::get_contract_abi_cached))
        // Issue #632: Feature flag client-side endpoint
        .route("/features", axum::routing::get(handlers::get_feature_flag_status))
        // Issue #671: Trending analytics
        .route("/analytics/trending", axum::routing::get(handlers::get_trending_events))
        // Issue #672: Event correlation analysis
        .route("/analytics/correlations", axum::routing::get(handlers::get_event_correlations))
        // Issue #673: Dedicated export endpoints
        .route("/export/csv", axum::routing::get(handlers::export_events_csv))
        .route("/export/json", axum::routing::get(handlers::export_events_json))
        .route("/export/parquet", axum::routing::get(handlers::export_events_parquet))
        .route("/export/schedule", axum::routing::post(handlers::schedule_export_job))
        .route("/export/jobs/{job_id}", axum::routing::get(handlers::get_export_job_status_db))
        .route("/export/history", axum::routing::get(handlers::get_export_history))
        // Push-preload endpoints: read-only schema/ABI lookup for HTTP/2 push
        // and client-side preloading. Gated by enable_push_preload at the
        // handler level so the routes are always registered (returning 501 when
        // the flag is off) to keep the routing table stable.
        .route(
            "/push/{contract_id}/schema",
            axum::routing::get(crate::push_preload::get_push_schema),
        )
        .route(
            "/push/{contract_id}/abi",
            axum::routing::get(crate::push_preload::get_push_abi),
        )
        // Issue #931: Batch event operations
        .route("/events/batch/retrieve", axum::routing::post(crate::batch_operations::batch_retrieve_events))
        .route("/events/batch/delete", axum::routing::post(crate::batch_operations::batch_delete_events))
        .route("/events/batch/tag", axum::routing::post(crate::batch_operations::batch_tag_events))
        .route("/events/batch/subscriptions", axum::routing::post(crate::batch_operations::batch_update_subscriptions))
        .route("/events/batch/transform", axum::routing::post(crate::batch_operations::batch_transform_events))
        .route("/events/batch/progress/{job_id}", axum::routing::get(crate::batch_operations::get_batch_progress))
        // Issue #929: Real-time event stream statistics
        .route("/stats/stream", axum::routing::get(crate::stream_statistics::get_stream_stats))
        .route("/stats/stream/throughput", axum::routing::get(crate::stream_statistics::get_stream_throughput))
        .route("/stats/stream/{contract_id}", axum::routing::get(crate::stream_statistics::get_contract_stream_stats))
        // Issue #928: Event filtering DSL
        .route("/events/filter", axum::routing::post(crate::filter_dsl::get_events_with_dsl))
}
