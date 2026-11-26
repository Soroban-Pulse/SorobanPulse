//! OpenAPI document for the HTTP API.

#[allow(unused_imports)]
use super::*;

/// OpenAPI spec — all paths are documented via #[utoipa::path] on handlers.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Soroban Pulse API",
        version = "1.0.0",
        description = "## Soroban Pulse API

Real-time indexing and querying of Soroban smart contract events on the Stellar network.

---

### Authentication

Most endpoints are open. When the `API_KEY` environment variable is set, all routes except
`/health` and `/healthz/*` require one of:

- `Authorization: Bearer <API_KEY>` header
- `X-Api-Key: <API_KEY>` header

Administrative endpoints under `/v1/admin/*` require `ADMIN_API_KEY` (independent of `API_KEY`).
A missing key returns **401 Unauthorized**; a wrong key returns **403 Forbidden**.

---

### Rate Limiting

Default: **60 requests / minute per IP** (configurable via `RATE_LIMIT_PER_MINUTE`).
Requests that exceed the limit receive **429 Too Many Requests** with a `Retry-After` header.
Set `RATE_LIMIT_PER_MINUTE=0` to disable rate limiting entirely.

---

### Guides & Resources

- [Developer Onboarding](https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/onboarding.md)
- [API Usage Guide](https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/api-guide.md)
- [SDK Integration Guide](https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/sdk-integration-guide.md)
- [Webhook Verification](https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/webhook-verification.md)
- [Contract Event Schemas](https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/contract-event-schemas.md)"
    ),
    paths(
        crate::job_scheduler::list_jobs,
        crate::account_events::get_account_events,
        crate::sep41::get_sep41_events,
        handlers::health,
        crate::meta::get_meta,
        crate::contract_resource_stats::get_contract_resource_stats,
        crate::api_keys::create_api_key,
        crate::api_keys::list_api_keys,
        crate::api_keys::revoke_api_key,
        handlers::health_live,
        handlers::health_ready,
        handlers::health_postgres,
        handlers::health_rpc,
        handlers::health_external,
        handlers::email_bounce_webhook,
        handlers::get_cross_chain_trace,
        handlers::analyze_causality,
        handlers::get_feature_flag_status,
        handlers::status,
        handlers::get_events,
        handlers::get_events_feed,
        handlers::get_event_stats,
        handlers::get_contract_stats_history,
        handlers::get_events_diff,
        handlers::export_events,
        handlers::get_recent_events,
        handlers::get_events_by_contract,
        handlers::get_events_by_tx,
        handlers::get_related_events_by_tx,
        handlers::get_events_by_ledger_hash,
        handlers::get_events_by_tx_batch,
        handlers::stream_events,
        handlers::stream_events_by_contract,
        handlers::stream_events_multi,
        handlers::ws_events,
        handlers::ws_stream_events,
        handlers::get_contracts,
        handlers::replay_events,
        handlers::replay_with_transform,
        handlers::start_reencrypt,
        handlers::register_contract_abi,
        handlers::get_contract_abi,
        handlers::upsert_contract_metadata,
        handlers::get_contract_metadata,
        crate::contract_tracker::get_contract,
        crate::contract_specs::get_contract_spec,
        handlers::anonymize_event,
        handlers::pause_indexer,
        handlers::resume_indexer,
        handlers::list_archive,
        handlers::query_archive,
        handlers::restore_from_archive,
        handlers::batch_query_events,
        handlers::fulltext_search,
        handlers::events_aggregations,
        handlers::register_contract_schema,
        handlers::get_contract_schema,
        handlers::delete_contract_schema,
        handlers::validate_event_data_against_schema,
        handlers::list_contract_schemas,
        handlers::start_mask_events,
        handlers::get_mask_job_status,
        handlers::get_timeseries,
        handlers::get_contract_event_counts,
        handlers::get_contract_summary,
        handlers::get_contracts_search,
        handlers::check_contract_exists,
        handlers::get_config,
        handlers::reload_config,
        handlers::get_anonymization_config,
        handlers::upsert_anonymization_rule,
        handlers::delete_anonymization_rule,
        handlers::scan_event_for_pii,
        handlers::get_index_fragmentation,
        handlers::reindex_index,
    ),
    components(schemas(
        crate::models::Event,
        crate::contract_metadata::ContractMetadata,
        crate::contract_metadata::UpsertContractMetadata,
        crate::models::EventType,
        crate::models::SortOrder,
        crate::models::PaginationParams,
        crate::models::ContractSummary,
        crate::contract_tracker::TrackedContract,
        crate::models::ContractDetailSummary,
        crate::models::LedgerRange,
        crate::models::EventTypeBreakdown,
        crate::models::ContractSearchResult,
        crate::models::ContractSearchParams,
        crate::models::ContractExistsResponse,
        crate::models::ConfigResponse,
        crate::models::ConfigReloadResponse,
        crate::models::EventStats,
        crate::models::ContractStatEntry,
        crate::models::ReplayRequest,
        crate::models::BatchTxRequest,
        crate::models::ErrorResponse,
        crate::models::DiffParams,
        crate::models::ContractDiff,
        crate::models::DiffResponse,
        crate::models::MaskEventsRequest,
        crate::models::MaskEventsResponse,
        crate::models::MaskJobStatus,
        crate::models::TimeseriesParams,
        crate::models::TimeseriesBucket,
        crate::models::TimeseriesResponse,
        crate::error::ValidationErrorDetail,
        crate::meta::MetaResponse,
        crate::contract_resource_stats::ContractResourceStats,
        crate::contract_resource_stats::ResourceAggregate,
        crate::api_keys::ApiKeyScope,
        crate::api_keys::CreateApiKeyRequest,
        crate::api_keys::CreatedApiKey,
        crate::api_keys::ApiKeyInfo,
    )),
    tags(
        (name = "events", description = "Event indexing endpoints"),
        (name = "system", description = "Health and observability endpoints"),
        (name = "admin", description = "Administrative endpoints"),
    )
)]
pub struct ApiDoc;
