//! Administrative `/v1/admin/*` routes (replay, re-encryption, anonymization,
//! contract ABI / metadata / schema registry, indexer control, pool and
//! planner statistics, SLOs, backups, alerts, bulk export, maintenance).
//!
//! Every route here is registered exactly once; `super` applies the admin
//! auth layer (issue #409) to this router as a whole.

use axum::{routing::get, Router};

use super::AppState;
use crate::handlers;

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/lua/preview", axum::routing::post(handlers::lua_preview))
        .route("/admin/replay", axum::routing::post(handlers::replay_events))
        .route("/admin/reencrypt", axum::routing::post(handlers::start_reencrypt))
        .route("/admin/token-transfers/backfill", axum::routing::post(crate::token_events::backfill_token_transfers))
        .route("/admin/event-addresses/backfill", axum::routing::post(crate::account_events::backfill_event_addresses))
        .route("/admin/contracts/{contract_id}/abi", axum::routing::post(handlers::register_contract_abi).get(handlers::get_contract_abi))
        .route("/admin/contracts/{contract_id}/metadata", axum::routing::post(handlers::upsert_contract_metadata).delete(handlers::delete_contract_metadata))
        .route("/admin/contracts/metadata/import", axum::routing::post(handlers::bulk_import_contract_metadata))
        .route("/admin/events/{id}/anonymize", axum::routing::post(handlers::anonymize_event))
        .route("/admin/events/contract/{contract_id}", axum::routing::delete(handlers::delete_contract_events))
        .route(
            "/admin/events/bulk",
            axum::routing::post(handlers::bulk_insert_events),
        )
        .route("/admin/indexer/pause", axum::routing::post(handlers::pause_indexer))
        .route("/admin/indexer/resume", axum::routing::post(handlers::resume_indexer))
        .route("/admin/indexer/gaps", axum::routing::get(handlers::get_indexer_gaps))
        .route("/admin/contracts/{contract_id}/schema", axum::routing::post(handlers::register_contract_schema).get(handlers::get_contract_schema).delete(handlers::delete_contract_schema))
        .route("/admin/contracts/{contract_id}/validate", axum::routing::post(handlers::validate_event_data_against_schema))
        .route("/admin/schemas", axum::routing::get(handlers::list_contract_schemas))
        // Admin-gated even though it is not under /admin. Uses `{contract_id}`
        // so it does not conflict with the other `/contracts/{contract_id}/*` routes.
        .route("/contracts/{contract_id}/resources", axum::routing::get(handlers::get_contract_resources))
        .route("/admin/pool-config", axum::routing::get(handlers::get_pool_tuning_guide))
        .route("/admin/pool-config/statistics", axum::routing::get(handlers::get_pool_statistics))
        .route("/admin/pool-config/health", axum::routing::get(handlers::get_pool_health))
        .route("/admin/pool-config/adaptive", axum::routing::get(handlers::get_adaptive_pool_status))
        .route("/admin/pool-config/adaptive/config", axum::routing::put(handlers::update_adaptive_pool_config))
        .route("/admin/pool-config/adaptive/rollback", axum::routing::post(handlers::rollback_adaptive_pool_config))
        .route("/admin/statistics/report", axum::routing::get(handlers::get_statistics_report))
        .route("/admin/statistics/stale", axum::routing::get(handlers::detect_stale_statistics))
        .route("/admin/statistics/health", axum::routing::get(handlers::get_statistics_health))
        .route("/admin/statistics/refresh", axum::routing::post(handlers::refresh_statistics))
        .route("/admin/statistics/jobs", axum::routing::get(handlers::get_analysis_jobs))
        // #696: SLI / SLO dashboard reporting endpoints
        .route("/admin/slo/report", axum::routing::get(handlers::get_slo_report))
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
        // #881: Bulk event export endpoints
        .route("/admin/events/export", axum::routing::post(handlers::start_event_export).get(handlers::list_export_jobs))
        .route("/admin/events/export/{job_id}", axum::routing::get(handlers::get_export_job_status))
        .route("/admin/events/export/{job_id}/download", axum::routing::get(handlers::download_export_file))
        .route("/admin/events/export/cleanup", axum::routing::post(handlers::cleanup_export_files))
        .route("/admin/config/reload", axum::routing::post(handlers::reload_config))
        .route("/admin/mask-events", axum::routing::post(handlers::start_mask_events))
        .route("/admin/mask-events/{job_id}", get(handlers::get_mask_job_status))
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
        // Issue #670: Audit log query endpoint
        .route("/admin/audit-logs", axum::routing::get(handlers::get_audit_logs))
        // Issue #928: Event filtering DSL administration
        .route("/admin/dsl/compile", axum::routing::post(crate::filter_dsl::compile_dsl_filter))
        .route("/admin/dsl/filters", axum::routing::post(crate::filter_dsl::save_dsl_filter).get(crate::filter_dsl::list_dsl_filters))
}
