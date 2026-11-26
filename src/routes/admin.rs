//! Admin and configuration routes (served under `/v1`).

#[allow(unused_imports)]
use super::*;

/// Admin routes registered on the `/v1` router (authenticated by the global auth layer), plus
/// runtime configuration endpoints.
pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/admin/events/bulk",
            axum::routing::post(handlers::bulk_insert_events),
        )
        .route("/config", get(handlers::get_config))
        .route("/rate-limit/status", get(handlers::get_rate_limit_status))
        .route("/admin/config/reload", axum::routing::post(handlers::reload_config))
        .route("/config/anonymization", axum::routing::get(handlers::get_anonymization_config))
        .route("/config/anonymization/rules", axum::routing::post(handlers::upsert_anonymization_rule))
        .route("/config/anonymization/rules/{name}", axum::routing::delete(handlers::delete_anonymization_rule))
        .route("/config/anonymization/scan", axum::routing::post(handlers::scan_event_for_pii))
        .route("/admin/replay", axum::routing::post(handlers::replay_events))
        .route("/replay/with-transform", axum::routing::post(handlers::replay_with_transform))
        .route("/admin/reencrypt", axum::routing::post(handlers::start_reencrypt))
        .route("/admin/mask-events", axum::routing::post(handlers::start_mask_events))
        .route("/admin/mask-events/{job_id}", get(handlers::get_mask_job_status))
        .route("/admin/contracts/{contract_id}/abi", axum::routing::post(handlers::register_contract_abi).get(handlers::get_contract_abi))
        .route("/admin/contracts/{contract_id}/metadata", axum::routing::post(handlers::upsert_contract_metadata).delete(handlers::delete_contract_metadata))
        .route("/admin/contracts/metadata/import", axum::routing::post(handlers::bulk_import_contract_metadata))
        .route("/admin/events/{id}/anonymize", axum::routing::post(handlers::anonymize_event))
        .route("/admin/events/contract/{contract_id}", axum::routing::delete(handlers::delete_contract_events))
        .route("/admin/indexer/pause", axum::routing::post(handlers::pause_indexer))
        .route("/admin/indexer/resume", axum::routing::post(handlers::resume_indexer))
        .route("/admin/contracts/{contract_id}/schema", axum::routing::post(handlers::register_contract_schema).get(handlers::get_contract_schema).delete(handlers::delete_contract_schema))
        .route("/admin/contracts/{contract_id}/validate", axum::routing::post(handlers::validate_event_data_against_schema))
        .route("/admin/schemas", axum::routing::get(handlers::list_contract_schemas))
        // #586: Replica sync monitoring
        .route("/admin/replication/status", axum::routing::get(handlers::get_replication_status))
        // #587: Feature flag management
        .route("/admin/feature-flags", axum::routing::get(handlers::list_feature_flags))
        .route("/admin/feature-flags/audit", axum::routing::get(handlers::get_feature_flag_audit))
        // #694: Index fragmentation monitoring
        .route("/admin/indexes/fragmentation", axum::routing::get(handlers::get_index_fragmentation))
        .route("/admin/indexes/{index_name}/reindex", axum::routing::post(handlers::reindex_index))
        // Issue #610: Compression admin endpoints
        .route("/admin/compression/stats", axum::routing::get(handlers::compression_stats))
        .route("/admin/compression/migrate", axum::routing::post(handlers::start_compression_migration))
        // Issue #632: Feature flag client-side endpoint
        .route("/features", axum::routing::get(handlers::get_feature_flag_status))
        // Issue #670: Audit log query endpoint (admin)
        .route("/admin/audit-logs", axum::routing::get(handlers::get_audit_logs))
        .route("/admin/dsl/compile", axum::routing::post(crate::filter_dsl::compile_dsl_filter))
        .route("/admin/dsl/filters", axum::routing::post(crate::filter_dsl::save_dsl_filter).get(crate::filter_dsl::list_dsl_filters))
}

/// Issue #1258: scoped API key management. Always requires an admin
/// credential, even when ADMIN_API_KEY is unset.
pub(super) fn api_key_routes(admin_auth_state: Arc<middleware::AdminAuthState>) -> Router<AppState> {
    Router::new()
        .route(
            "/admin/api-keys",
            post(crate::api_keys::create_api_key).get(crate::api_keys::list_api_keys),
        )
        .route(
            "/admin/api-keys/{id}",
            axum::routing::delete(crate::api_keys::revoke_api_key),
        )
        .route_layer(axum::middleware::from_fn_with_state(
            admin_auth_state,
            middleware::require_admin_middleware,
        ))
}

/// Admin-only routes gated by a dedicated admin auth layer (issue #409).
///
/// **Not mounted.** Before the routes split (#1242) this router was built
/// inside `create_router_with_tx_and_tenant_map` but never merged into the
/// app, so none of these paths are served today. Routes that were also
/// registered on the `/v1` router were removed from here so that no route is
/// registered twice. Mounting this router is a deliberate API change and is
/// left to a follow-up.
#[allow(dead_code)]
pub(super) fn admin_gated_routes(admin_auth_state: Arc<middleware::AdminAuthState>) -> Router<AppState> {
    Router::new()
        .route("/admin/lua/preview", axum::routing::post(handlers::lua_preview))
        .route("/admin/token-transfers/backfill", axum::routing::post(crate::token_events::backfill_token_transfers))
        .route("/admin/event-addresses/backfill", axum::routing::post(crate::account_events::backfill_event_addresses))
        .route("/admin/sep41/backfill", axum::routing::post(crate::sep41::backfill_sep41))
        .route("/admin/pool-config", axum::routing::get(handlers::get_pool_tuning_guide))
        .route("/admin/pool-config/statistics", axum::routing::get(handlers::get_pool_statistics))
        .route("/admin/pool-config/health", axum::routing::get(handlers::get_pool_health))
        // Issue #1345: pg_stat_statements slow-query digest
        .route("/admin/slow-queries", axum::routing::get(handlers::get_slow_queries))
        .route("/contracts/{id}/resources", axum::routing::get(handlers::get_contract_resources))
        .route("/admin/indexer/gaps", axum::routing::get(handlers::get_indexer_gaps))
        .route("/admin/pool-config/adaptive", axum::routing::get(handlers::get_adaptive_pool_status))
        .route("/admin/pool-config/adaptive/config", axum::routing::put(handlers::update_adaptive_pool_config))
        .route("/admin/pool-config/adaptive/rollback", axum::routing::post(handlers::rollback_adaptive_pool_config))
        .route("/admin/statistics/report", axum::routing::get(handlers::get_statistics_report))
        .route("/admin/statistics/stale", axum::routing::get(handlers::detect_stale_statistics))
        .route("/admin/statistics/health", axum::routing::get(handlers::get_statistics_health))
        .route("/admin/statistics/refresh", axum::routing::post(handlers::refresh_statistics))
        .route("/admin/statistics/jobs", axum::routing::get(handlers::get_analysis_jobs))
        // #696: SLI / SLO dashboard reporting endpoints (admin-gated)
        .route("/admin/slo/report", axum::routing::get(handlers::get_slo_report))
        .route("/admin/jobs", axum::routing::get(crate::job_scheduler::list_jobs))
        .route("/admin/slo/sample", axum::routing::post(handlers::record_slo_sample))
        // #894: Backup verification
        .route("/admin/backup/verification/report", axum::routing::get(handlers::get_backup_verification_report))
        .route("/admin/backup/verification/trigger", axum::routing::post(handlers::trigger_backup_verification))
        // #897: Alert silence management
        .route(
            "/admin/alerts/silences",
            axum::routing::post(handlers::create_alert_silence)
                .get(handlers::get_alert_silences),
        )
        .route("/admin/alerts/silences/{silence_id}", axum::routing::delete(handlers::delete_alert_silence))
        // #839: Push notification delivery analytics
        .route("/admin/push/analytics", axum::routing::get(crate::push_notification::get_push_analytics))
        // #879: Webhook circuit breaker admin endpoints
        .route("/admin/webhook/circuit-breaker", axum::routing::get(handlers::get_circuit_breaker_stats))
        .route("/admin/webhook/circuit-breaker/{endpoint}", axum::routing::get(handlers::get_endpoint_circuit_breaker_stats))
        .route("/admin/webhook/circuit-breaker/{endpoint}/reset", axum::routing::post(handlers::reset_circuit_breaker))
        // #881: Bulk event export endpoints
        .route("/admin/events/export", axum::routing::post(handlers::start_event_export).get(handlers::list_export_jobs))
        .route("/admin/events/export/{job_id}", axum::routing::get(handlers::get_export_job_status))
        .route("/admin/events/export/{job_id}/download", axum::routing::get(handlers::download_export_file))
        .route("/admin/events/export/cleanup", axum::routing::post(handlers::cleanup_export_files))
        .route_layer(axum::middleware::from_fn_with_state(
            admin_auth_state,
            middleware::admin_auth_middleware,
        ))
}
