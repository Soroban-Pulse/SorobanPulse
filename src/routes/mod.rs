use axum::extract::MatchedPath;
use axum::http::{HeaderValue, Method, Request};
use axum::{body::Body, routing::{get, post}, Router};
use dashmap::DashMap;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::PgPool;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tower_governor::{
    errors::GovernorError,
    governor::GovernorConfigBuilder,
    key_extractor::{KeyExtractor, PeerIpKeyExtractor, SmartIpKeyExtractor},
    GovernorLayer,
};

/// Rate-limit key extractor for multi-tenant mode.
/// Uses the raw API key from Authorization or X-Api-Key as the bucket key so
/// each tenant gets an independent quota regardless of IP address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    type Key = String;

    fn extract<T>(&self, req: &axum::http::Request<T>) -> Result<String, GovernorError> {
        let bearer = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.strip_prefix("Bearer "))
            .map(|s| s.to_string());
        let x_api_key = req
            .headers()
            .get("X-Api-Key")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());
        bearer.or(x_api_key).ok_or(GovernorError::UnableToExtractKey)
    }
}

use tower_http::{
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};
use utoipa::OpenApi;
use uuid::Uuid;

use crate::{
    aggregation, config::{Config, HealthState, IndexerState},
    handlers, metrics, middleware,
    models::SorobanEvent,
    saved_queries,
};

mod admin;
mod deprecated;
mod events;
mod subscriptions;
mod webhooks;

pub use events::get_events_with_params;

type ContractCountCache = moka::future::Cache<String, i64>;

#[derive(Clone, Default)]
struct UuidMakeRequestId;

impl MakeRequestId for UuidMakeRequestId {
    fn make_request_id<B>(&mut self, _request: &Request<B>) -> Option<RequestId> {
        let id = Uuid::new_v4().to_string().parse().ok()?;
        Some(RequestId::new(id))
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    /// Read pool: points to replica when DATABASE_REPLICA_URL is set, otherwise same as pool.
    pub read_pool: PgPool,
    pub health_state: Arc<HealthState>,
    pub indexer_state: Arc<IndexerState>,
    pub prometheus_handle: PrometheusHandle,
    pub event_tx: broadcast::Sender<SorobanEvent>,
    pub sse_keepalive_interval_ms: u64,
    pub sse_connections: Arc<AtomicUsize>,
    pub sse_max_connections: usize,
    pub health_check_timeout_ms: u64,
    pub encryption_key: Option<[u8; 32]>,
    pub encryption_key_old: Option<[u8; 32]>,
    pub contract_count_cache: ContractCountCache,
    pub config: crate::config::Config,
    pub schema_validator: Option<Arc<crate::schema_validator::SchemaValidator>>,
    /// key_hash → tenant_id; populated only when multi_tenant is enabled.
    pub tenant_map: Arc<std::collections::HashMap<String, String>>,
    /// Cache for stats results (Issue #404)
    pub stats_cache: moka::future::Cache<String, serde_json::Value>,
    /// Shutdown signal for SSE streams (Issue #405)
    pub shutdown_rx: tokio::sync::watch::Receiver<bool>,
    /// Per-IP SSE connection counts (Issue #453)
    pub sse_connections_per_ip: Arc<DashMap<String, usize>>,
    /// SHA-256 hex of SUPER_ADMIN_API_KEY, grants unrestricted channel access (#508).
    pub super_admin_key_hash: Option<String>,
    /// Issue #607: In-process ABI cache (LRU-bounded, 24 h TTL by default).
    pub abi_cache: crate::abi::AbiCache,
    /// In-memory SSE event replay ring buffer (10 k events, FIFO eviction).
    pub sse_ring_buffer: std::sync::Arc<crate::sse_ring_buffer::SseRingBuffer>,
    /// TTL-bounded query result cache for materialized-view backed queries.
    pub query_result_cache: std::sync::Arc<moka::future::Cache<String, serde_json::Value>>,
    /// Issue #618: Anonymization rules configuration.
    pub anonymization_config: Option<Arc<crate::anonymization::AnonymizationConfig>>,
    /// Issue #817: Connection pool stats tracker (shared with pool monitor).
    pub pool_stats: Arc<crate::connection_pool::PoolStats>,
    /// Issue #817: Adaptive pool tuner — hot-reloadable config, snapshots, counters.
    pub adaptive_pool: Arc<crate::adaptive_pool::AdaptiveTunerState>,
    /// Issue #817: Alias so handlers can use `state.db` (same as `state.pool`).
    pub db: sqlx::PgPool,
    /// Issue #879: Circuit breaker manager for webhook endpoints.
    pub circuit_breaker_manager: crate::webhook_circuit_breaker::CircuitBreakerManager,
    /// Issue #881: Bulk export manager for event export jobs.
    pub bulk_export_manager: crate::bulk_export::BulkExportManager,
    /// Deployment role for this instance — used by the readiness handler to
    /// apply role-specific readiness semantics.
    pub role: crate::config::Role,
}

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
        handlers::health,
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
    )),
    tags(
        (name = "events", description = "Event indexing endpoints"),
        (name = "system", description = "Health and observability endpoints"),
        (name = "admin", description = "Administrative endpoints"),
    )
)]
pub struct ApiDoc;

pub fn create_router(
    pool: PgPool,
    api_keys: Vec<String>,
    allowed_origins: &[String],
    rate_limit_per_minute: u32,
    health_state: Arc<HealthState>,
    indexer_state: Arc<IndexerState>,
    prometheus_handle: PrometheusHandle,
    health_check_timeout_ms: u64,
    config: crate::config::Config,
) -> Router {
    create_router_with_tx(
        pool.clone(),
        pool,
        api_keys,
        allowed_origins,
        rate_limit_per_minute,
        false,
        health_state,
        indexer_state,
        prometheus_handle,
        broadcast::channel(256).0,
        15000,
        1000,
        health_check_timeout_ms,
        None,
        None,
        config,
        None,
    )
}

/// Load the key_hash → tenant_id mapping from the `api_key_tenants` table.
/// Called once at startup when `MULTI_TENANT=true`.
pub async fn load_tenant_map(
    pool: &sqlx::PgPool,
) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT key_hash, tenant_id FROM api_key_tenants")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

pub fn create_router_with_tx(
    pool: PgPool,
    read_pool: PgPool,
    api_keys: Vec<String>,
    allowed_origins: &[String],
    rate_limit_per_minute: u32,
    behind_proxy: bool,
    health_state: Arc<HealthState>,
    indexer_state: Arc<IndexerState>,
    prometheus_handle: PrometheusHandle,
    event_tx: broadcast::Sender<SorobanEvent>,
    sse_keepalive_interval_ms: u64,
    sse_max_connections: usize,
    health_check_timeout_ms: u64,
    encryption_key: Option<[u8; 32]>,
    encryption_key_old: Option<[u8; 32]>,
    config: crate::config::Config,
    schema_validator: Option<Arc<crate::schema_validator::SchemaValidator>>,
) -> Router {
    let (_, shutdown_rx) = tokio::sync::watch::channel(false);
    create_router_with_tx_and_tenant_map(
        pool,
        read_pool,
        api_keys,
        allowed_origins,
        rate_limit_per_minute,
        behind_proxy,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        sse_keepalive_interval_ms,
        sse_max_connections,
        health_check_timeout_ms,
        encryption_key,
        encryption_key_old,
        config.clone(),
        schema_validator,
        Arc::new(std::collections::HashMap::new()),
        shutdown_rx,
        crate::sse_ring_buffer::SseRingBuffer::new(config.sse_ring_buffer_capacity),
    )
}

/// Full router constructor that accepts a pre-loaded tenant map.
/// Use this in `main` when `MULTI_TENANT=true` after calling `load_tenant_map`.
pub fn create_router_with_tx_and_tenant_map(
    pool: PgPool,
    read_pool: PgPool,
    api_keys: Vec<String>,
    allowed_origins: &[String],
    rate_limit_per_minute: u32,
    behind_proxy: bool,
    health_state: Arc<HealthState>,
    indexer_state: Arc<IndexerState>,
    prometheus_handle: PrometheusHandle,
    event_tx: broadcast::Sender<SorobanEvent>,
    sse_keepalive_interval_ms: u64,
    sse_max_connections: usize,
    health_check_timeout_ms: u64,
    encryption_key: Option<[u8; 32]>,
    encryption_key_old: Option<[u8; 32]>,
    config: crate::config::Config,
    schema_validator: Option<Arc<crate::schema_validator::SchemaValidator>>,
    tenant_map: Arc<std::collections::HashMap<String, String>>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    sse_ring_buffer: std::sync::Arc<crate::sse_ring_buffer::SseRingBuffer>,
) -> Router {
    let cors = build_cors(allowed_origins);

    // Admin keys are independent of the regular API keys (issue #409).
    let admin_api_keys: Vec<String> = {
        use secrecy::ExposeSecret;
        config
            .admin_api_keys
            .iter()
            .map(|k| k.expose_secret().to_string())
            .collect()
    };

    let auth_state = Arc::new(middleware::AuthState {
        api_keys,
        admin_api_keys: admin_api_keys.clone(),
        tenant_map: Arc::clone(&tenant_map),
        multi_tenant: config.multi_tenant,
    });
    let admin_auth_state = Arc::new(middleware::AdminAuthState { admin_api_keys });
    // Push-preload feature flag state — built early so it can be passed to
    // the route_layer middleware before AppState is assembled.
    let push_preload_state = crate::push_preload::PushPreloadState::new(config.enable_push_preload);
    let contract_count_cache = moka::future::Cache::builder()
        .max_capacity(config.contract_count_cache_size)
        .time_to_live(std::time::Duration::from_secs(
            config.contract_count_cache_ttl_secs,
        ))
        .build();
    let stats_cache = moka::future::Cache::builder()
        .max_capacity(1)
        .time_to_live(std::time::Duration::from_secs(config.stats_cache_ttl_secs))
        .build();
    let super_admin_key_hash = std::env::var("SUPER_ADMIN_API_KEY")
        .ok()
        .filter(|s| !s.is_empty())
        .map(|k| crate::middleware::hash_api_key(&k));

    // Issue #607: build the ABI cache from config.
    let abi_cache = moka::future::Cache::builder()
        .max_capacity(config.abi_cache_max_entries)
        .time_to_live(std::time::Duration::from_secs(config.abi_cache_ttl_secs))
        .build();

    // Query result cache for materialized-view backed queries.
    let query_result_cache = crate::query_cache::build(
        config.query_cache_ttl_secs,
        config.query_cache_max_capacity,
    );

    // Issue #817: spawn pool monitor and adaptive tuner.
    let pool_stats = {
        let pool_cfg = crate::connection_pool::PoolMonitorConfig {
            max_connections: config.db_max_connections,
            min_connections: config.db_min_connections,
            exhaustion_threshold: 0.9,
            sample_interval: std::time::Duration::from_secs(15),
        };
        crate::connection_pool::spawn_pool_monitor(pool.clone(), pool_cfg)
    };
    let adaptive_pool = crate::adaptive_pool::spawn_adaptive_monitor(
        pool.clone(),
        crate::adaptive_pool::AdaptivePoolConfig::default(),
        config.db_max_connections,
        config.db_min_connections,
    );

    let circuit_breaker_manager = crate::webhook_circuit_breaker::CircuitBreakerManager::new(
        crate::webhook_circuit_breaker::CircuitBreakerConfig::default()
    );

    let bulk_export_manager = crate::bulk_export::BulkExportManager::new(
        std::path::PathBuf::from("/tmp/soroban-pulse-exports"),
        24, // 24 hour retention
    );

    let app_state = AppState {
        db: pool.clone(),
        pool,
        read_pool,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        sse_keepalive_interval_ms,
        sse_connections: Arc::new(AtomicUsize::new(0)),
        sse_max_connections,
        health_check_timeout_ms,
        encryption_key,
        encryption_key_old,
        contract_count_cache,
        config,
        schema_validator,
        tenant_map,
        stats_cache,
        shutdown_rx,
        sse_connections_per_ip: Arc::new(DashMap::new()),
        super_admin_key_hash,
        abi_cache,
        sse_ring_buffer,
        query_result_cache,
        anonymization_config: None,
        pool_stats,
        adaptive_pool,
        circuit_breaker_manager,
        bulk_export_manager,
        role: config.role.clone(),
    };

    // Spawn cache invalidation task: subscribe to the broadcast channel and
    // evict the contract_count_cache entry whenever a new event is indexed.
    {
        let mut rx = app_state.event_tx.subscribe();
        let cache = app_state.contract_count_cache.clone();
        tokio::spawn(async move {
            while let Ok(event) = rx.recv().await {
                cache.invalidate(&event.contract_id).await;
                crate::metrics::record_contract_count_cache_invalidation();
            }
        });
    }

    // Build governor config: burst = rate_limit_per_minute, replenish 1 token per (60/rate) seconds.
    // per_second(n) means n tokens replenished per second; we want rate_limit_per_minute / 60.
    // Use per_millisecond to avoid integer truncation: replenish 1 token every (60_000 / rate) ms.
    let replenish_ms = 60_000u64 / u64::from(rate_limit_per_minute.max(1));
    let burst = rate_limit_per_minute.max(1);

    // Admin routes (issue #409): every `/admin/*` route is registered once and
    // gated by a dedicated admin auth layer in addition to the global auth
    // layer. The admin layer requires an ADMIN_API_KEY even when no regular
    // API_KEY is configured.
    let admin_routes = admin::router()
        .merge(subscriptions::admin_router())
        .merge(webhooks::admin_router())
        .route_layer(axum::middleware::from_fn_with_state(
            Arc::clone(&admin_auth_state),
            middleware::admin_auth_middleware,
        ));

    // Versioned v1 routes
    let v1 = events::router()
        .merge(subscriptions::router())
        .merge(webhooks::router())
        .merge(admin_routes)
        // Append Link preload headers to responses for
        // /v1/events/contract/{contract_id} when the feature flag is on.
        .route_layer(axum::middleware::from_fn_with_state(
            push_preload_state,
            crate::push_preload::push_link_header_middleware,
        ));

    // Health endpoints — exempt from rate limiting.
    // The unsubscribe endpoint is public (reached from email links) and must
    // bypass both auth and rate limiting (Issue #483).
    let health_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/healthz/live", get(handlers::health_live))
        .route("/healthz/ready", get(handlers::health_ready))
        .route("/healthz/postgres", get(handlers::health_postgres))
        .route("/healthz/rpc", get(handlers::health_rpc))
        .route("/healthz/external/{service}", get(handlers::health_external))
        .route("/unsubscribe", get(handlers::unsubscribe))
        .route("/metrics", get(handlers::metrics));

    // All other routes — subject to rate limiting. The branches below differ
    // only in how the governor derives the rate-limit bucket key.
    let api_routes = Router::new()
        .route("/status", get(handlers::status))
        .route("/openapi.json", get(handlers::openapi_json))
        .route("/docs", get(handlers::swagger_ui))
        .merge(graphql_routes())
        .nest("/v1", v1)
        .merge(deprecated::router())
        .layer(axum::middleware::from_fn(middleware::rate_limit_reject_middleware));

    let rate_limited_routes = if config.multi_tenant {
        // In multi-tenant mode rate-limit per API key so each tenant has an
        // independent quota regardless of IP (tenants may share egress IPs).
        let effective_limit = config
            .tenant_rate_limit_per_minute
            .unwrap_or(rate_limit_per_minute);
        let replenish_ms_t = 60_000u64 / u64::from(effective_limit.max(1));
        let burst_t = effective_limit.max(1);
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(replenish_ms_t)
                .burst_size(burst_t)
                .key_extractor(ApiKeyExtractor)
                .use_headers()
                .finish()
                .expect("invalid governor config"),
        );
        api_routes.layer(GovernorLayer::new(governor_conf))
    } else if behind_proxy {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(replenish_ms)
                .burst_size(burst)
                .key_extractor(SmartIpKeyExtractor)
                .use_headers()
                .finish()
                .expect("invalid governor config"),
        );
        api_routes.layer(GovernorLayer::new(governor_conf))
    } else {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(replenish_ms)
                .burst_size(burst)
                .key_extractor(PeerIpKeyExtractor)
                .use_headers()
                .finish()
                .expect("invalid governor config"),
        );
        api_routes.layer(GovernorLayer::new(governor_conf))
    };

    // Issue #1112: static dashboard at /ui — outside auth-gated rate limiting.
    let dashboard_routes = crate::dashboard::router(&app_state.config);

    Router::new()
        .merge(health_routes)
        .merge(dashboard_routes)
        .merge(rate_limited_routes)
        .layer(axum::middleware::from_fn({
            let config = middleware::SecurityHeadersConfig::from_env();
            middleware::security_headers_with_config(config)
        }))
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            middleware::rate_limit_headers_middleware,
        ))
        // Issue #942: registered after (so it runs before, in tower's
        // outside-in layering) rate limiting — a blocked IP should never
        // spend a rate-limit quota check before being rejected.
        .layer(axum::middleware::from_fn_with_state(
            app_state.clone(),
            middleware::ip_access_control_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::head_middleware))
        .layer(axum::middleware::from_fn(middleware::request_id_middleware))
        .layer(axum::middleware::from_fn(
            middleware::tracing_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(middleware::slow_request_middleware(1000)))
        .layer(cors)
        .layer(
            TraceLayer::new_for_http().make_span_with(|request: &Request<Body>| {
                let request_id = request
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown")
                    .to_owned();
                tracing::info_span!(
                    "request",
                    method = %request.method(),
                    uri = %request.uri(),
                    request_id = %request_id,
                )
            }),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(crate::compression_config::CompressionSettings::from_env().layer())
        // Issue #961: registered *after* (so it wraps, and on the
        // response path runs *after*) CompressionLayer, so it observes the
        // final Content-Encoding header rather than the pre-compression
        // response.
        .layer(axum::middleware::from_fn(
            crate::compression_config::compression_metrics_middleware,
        ))
        .layer(SetRequestIdLayer::x_request_id(UuidMakeRequestId))
        .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1 MB default
        .with_state(app_state)
}

/// Issue #683: GraphQL API routes (requires `graphql` feature)
fn graphql_routes() -> Router<AppState> {
    Router::new()
}

fn build_cors(allowed_origins: &[String]) -> CorsLayer {
    let methods = [Method::GET, Method::POST];
    let allowed_headers = [
        axum::http::header::AUTHORIZATION,
        axum::http::header::CONTENT_TYPE,
        axum::http::header::HeaderName::from_static("x-api-key"),
        axum::http::header::HeaderName::from_static("x-request-id"),
        // Lets cross-origin SSE clients (e.g. the embeddable feed widget, #1111)
        // resume a stream after reconnecting.
        axum::http::header::HeaderName::from_static("last-event-id"),
    ];
    let exposed_headers = [axum::http::header::HeaderName::from_static("x-request-id")];
    let max_age = std::time::Duration::from_secs(86400);

    if allowed_origins.iter().any(|o| o == "*") {
        return CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(methods)
            .allow_headers(allowed_headers)
            .expose_headers(exposed_headers)
            .max_age(max_age);
    }

    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .allow_headers(allowed_headers)
        .expose_headers(exposed_headers)
        .max_age(max_age)
        .vary([axum::http::header::ORIGIN])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use tower::ServiceExt;
    use tracing_subscriber::layer::SubscriberExt;

    /// Build a minimal router that sleeps for `delay_ms` and runs the metrics
    /// middleware with the given `threshold_ms`.
    fn slow_request_test_app(delay_ms: u64, threshold_ms: u64) -> Router {
        Router::new()
            .route(
                "/slow",
                get(move || async move {
                    tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                    "ok"
                }),
            )
            .layer(axum::middleware::from_fn(
                move |req: axum::http::Request<Body>, next: axum::middleware::Next| async move {
                    let method = req.method().as_str().to_string();
                    let route = req
                        .extensions()
                        .get::<MatchedPath>()
                        .map(|p| p.as_str().to_string())
                        .unwrap_or_else(|| "<unknown>".to_string());
                    let request_id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("unknown")
                        .to_owned();
                    let start = std::time::Instant::now();
                    let response = next.run(req).await;
                    let duration = start.elapsed();
                    let status = response.status().as_u16().to_string();
                    if duration.as_millis() as u64 > threshold_ms {
                        tracing::warn!(
                            method = %method,
                            path = %route,
                            status = %status,
                            duration_ms = duration.as_millis(),
                            request_id = %request_id,
                            "slow request"
                        );
                    }
                    response
                },
            ))
    }

    #[tokio::test]
    async fn slow_request_warn_is_emitted() {
        // Capture warn-level events.
        let (writer, output) = tracing_subscriber::fmt::TestWriter::new();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let app = slow_request_test_app(20, 0); // threshold=0 → always warn
        app.oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let logs = output.into_string();
        assert!(
            logs.contains("slow request"),
            "expected 'slow request' warn, got: {logs}"
        );
    }

    #[tokio::test]
    async fn fast_request_no_warn() {
        let (writer, output) = tracing_subscriber::fmt::TestWriter::new();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::WARN)
            .with_writer(writer)
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let app = slow_request_test_app(0, 60_000); // threshold=60s → never warn
        app.oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();

        let logs = output.into_string();
        assert!(
            !logs.contains("slow request"),
            "unexpected 'slow request' warn: {logs}"
        );
    }

    #[tokio::test]
    async fn test_compression_header() {
        let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();

        let api = Router::new().route("/large", axum::routing::get(|| async { "A".repeat(2000) }));

        let app = Router::new()
            .merge(api)
            .layer(tower_http::compression::CompressionLayer::new())
            .with_state(pool);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/large")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get(header::CONTENT_ENCODING).unwrap(),
            "gzip"
        );

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/large")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
    }

    /// Issue #961: responses smaller than the configured floor must bypass
    /// compression even when the client advertises gzip support, while
    /// large responses are still compressed.
    #[tokio::test]
    async fn compression_settings_bypass_small_responses() {
        std::env::set_var("COMPRESSION_MIN_SIZE_BYTES", "512");

        let settings = crate::compression_config::CompressionSettings::from_env();
        let app = Router::new()
            .route("/small", axum::routing::get(|| async { "ok" }))
            .route("/large", axum::routing::get(|| async { "A".repeat(2000) }))
            .layer(settings.layer());

        let small = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/small")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            small.headers().get(header::CONTENT_ENCODING).is_none(),
            "small responses must bypass compression"
        );

        let large = app
            .oneshot(
                Request::builder()
                    .uri("/large")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            large.headers().get(header::CONTENT_ENCODING).unwrap(),
            "gzip"
        );

        std::env::remove_var("COMPRESSION_MIN_SIZE_BYTES");
    }

    /// Build a minimal router with GovernorLayer using SmartIpKeyExtractor so tests
    /// can inject a fake IP via X-Forwarded-For without a real TCP connection.
    fn rate_limited_test_app(burst: u32) -> Router {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(60_000u64 / u64::from(burst.max(1)))
                .burst_size(burst)
                .key_extractor(SmartIpKeyExtractor)
                .use_headers()
                .finish()
                .expect("invalid governor config"),
        );
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(GovernorLayer::new(governor_conf))
    }

    #[tokio::test]
    async fn rate_limit_returns_429_after_burst_exhausted() {
        let app = rate_limited_test_app(2);

        // First two requests (burst=2) should succeed.
        for _ in 0..2 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/test")
                        .header("X-Forwarded-For", "1.2.3.4")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        // Third request must be rate-limited.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Forwarded-For", "1.2.3.4")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(
            resp.headers().contains_key("retry-after"),
            "expected Retry-After header on 429"
        );
        assert!(
            resp.headers().contains_key("x-ratelimit-limit"),
            "expected X-RateLimit-Limit header on 429"
        );
    }

    #[tokio::test]
    async fn rate_limit_different_ips_are_independent() {
        let app = rate_limited_test_app(1);

        // Exhaust the quota for IP A.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Forwarded-For", "10.0.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // IP A is now rate-limited.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Forwarded-For", "10.0.0.1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

        // IP B still has quota.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Forwarded-For", "10.0.0.2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    fn cors_test_app(origins: &[&str]) -> Router {
        let origins: Vec<String> = origins.iter().map(|s| s.to_string()).collect();
        let cors = build_cors(&origins);
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(cors)
    }

    #[tokio::test]
    async fn preflight_includes_max_age() {
        let app = cors_test_app(&["http://example.com"]);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/test")
                    .header("Origin", "http://example.com")
                    .header("Access-Control-Request-Method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let max_age = resp
            .headers()
            .get("access-control-max-age")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(max_age, "86400");
    }

    #[tokio::test]
    async fn preflight_exposes_x_request_id() {
        let app = cors_test_app(&["http://example.com"]);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/test")
                    .header("Origin", "http://example.com")
                    .header("Access-Control-Request-Method", "GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let expose = resp
            .headers()
            .get("access-control-expose-headers")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert!(
            expose.to_lowercase().contains("x-request-id"),
            "expose headers: {expose}"
        );
    }

    // ── Issue #422: HEAD request support ─────────────────────────────────────

    #[tokio::test]
    async fn head_request_returns_200_no_body() {
        use crate::middleware::head_middleware;
        let app = Router::new()
            .route("/v1/events", get(|| async { "hello world" }))
            .layer(axum::middleware::from_fn(head_middleware));

        let resp = app
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        assert!(body_bytes.is_empty(), "HEAD response body must be empty");
    }

    #[tokio::test]
    async fn head_request_content_length_matches_get() {
        use crate::middleware::head_middleware;
        let app = Router::new()
            .route("/v1/events", get(|| async { "hello world" }))
            .layer(axum::middleware::from_fn(head_middleware));

        let head_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("HEAD")
                    .uri("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let get_resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let get_body = axum::body::to_bytes(get_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let expected_len = get_body.len().to_string();

        let content_length = head_resp
            .headers()
            .get(header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        assert_eq!(content_length, expected_len);
    }

    // ── Issue #424: machine-readable 429 JSON response ────────────────────────

    fn rate_limited_json_test_app(burst: u32) -> Router {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(60_000u64 / u64::from(burst.max(1)))
                .burst_size(burst)
                .key_extractor(SmartIpKeyExtractor)
                .use_headers()
                .finish()
                .expect("invalid governor config"),
        );
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(
                |req: Request<Body>, next: axum::middleware::Next| async move {
                    let resp = next.run(req).await;
                    if resp.status() == axum::http::StatusCode::TOO_MANY_REQUESTS {
                        return rate_limit_json_response(resp);
                    }
                    resp
                },
            ))
            .layer(GovernorLayer::new(governor_conf))
    }

    #[tokio::test]
    async fn rate_limit_429_returns_json_error_response() {
        let app = rate_limited_json_test_app(1);

        // Exhaust burst.
        let _ = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Forwarded-For", "5.6.7.8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // This request should be rate-limited.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("X-Forwarded-For", "5.6.7.8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(
            resp.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()),
            Some("application/json")
        );

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).expect("body must be valid JSON");
        assert_eq!(v["code"], "RATE_LIMIT_EXCEEDED");
        assert!(v["error"].as_str().is_some());
        assert!(v["correlation_id"].as_str().is_some());
    }
}

/// Build a minimal HTTP router for `ROLE=indexer` pods.
///
/// Exposes only health probes (`/healthz/*`, `/health`) and the Prometheus
/// `/metrics` endpoint.  No API routes, no rate limiting, no auth middleware.
/// Kubernetes can still probe liveness/readiness and Prometheus can still
/// scrape metrics from indexer pods.
pub fn create_minimal_router(
    pool: sqlx::PgPool,
    health_state: std::sync::Arc<crate::config::HealthState>,
    indexer_state: std::sync::Arc<crate::config::IndexerState>,
    prometheus_handle: metrics_exporter_prometheus::PrometheusHandle,
    config: crate::config::Config,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    sse_ring_buf: std::sync::Arc<crate::sse_ring_buffer::SseRingBuffer>,
) -> Router {
    use axum::routing::get;
    use tokio::sync::broadcast;

    // A dummy broadcast sender — no SSE on indexer pods, but AppState requires it.
    let (event_tx, _) = broadcast::channel::<crate::models::SorobanEvent>(1);

    let read_pool = pool.clone();

    let contract_count_cache = moka::future::Cache::builder()
        .max_capacity(config.contract_count_cache_size)
        .time_to_live(std::time::Duration::from_secs(config.contract_count_cache_ttl_secs))
        .build();

    let stats_cache = moka::future::Cache::builder()
        .max_capacity(128)
        .time_to_live(std::time::Duration::from_secs(config.stats_cache_ttl_secs))
        .build();

    let query_result_cache = std::sync::Arc::new(
        moka::future::Cache::builder()
            .max_capacity(config.query_cache_max_capacity)
            .time_to_live(std::time::Duration::from_secs(config.query_cache_ttl_secs))
            .build(),
    );

    let abi_cache = crate::abi::AbiCache::new(
        config.abi_cache_max_entries,
        std::time::Duration::from_secs(config.abi_cache_ttl_secs),
    );

    let sse_connections_per_ip = std::sync::Arc::new(dashmap::DashMap::new());

    let pool_stats = std::sync::Arc::new(crate::connection_pool::PoolStats::new(
        config.db_max_connections,
    ));
    let adaptive_pool = std::sync::Arc::new(crate::adaptive_pool::AdaptiveTunerState::new(
        crate::adaptive_pool::AdaptivePoolConfig::default(),
    ));

    let app_state = AppState {
        pool: pool.clone(),
        read_pool,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        sse_keepalive_interval_ms: config.sse_keepalive_interval_ms,
        sse_connections: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        sse_max_connections: config.sse_max_connections,
        health_check_timeout_ms: config.health_check_timeout_ms,
        encryption_key: config.event_data_encryption_key,
        encryption_key_old: config.event_data_encryption_key_old,
        contract_count_cache,
        config: config.clone(),
        schema_validator: None,
        tenant_map: std::sync::Arc::new(std::collections::HashMap::new()),
        stats_cache,
        shutdown_rx,
        sse_connections_per_ip,
        super_admin_key_hash: None,
        abi_cache,
        sse_ring_buffer: sse_ring_buf,
        query_result_cache,
        anonymization_config: None,
        pool_stats,
        adaptive_pool,
        db: pool,
        circuit_breaker_manager: crate::webhook_circuit_breaker::CircuitBreakerManager::new(),
        bulk_export_manager: crate::bulk_export::BulkExportManager::new(),
        role: config.role.clone(),
    };

    Router::new()
        .route("/health", get(handlers::health))
        .route("/healthz/live", get(handlers::health_live))
        .route("/healthz/ready", get(handlers::health_ready))
        .route("/healthz/postgres", get(handlers::health_postgres))
        .route("/metrics", get(handlers::metrics))
        .with_state(app_state)
}
