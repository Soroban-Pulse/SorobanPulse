use axum::{body::Body, routing::get, Router};
use axum::http::{HeaderValue, Method, Request};
use axum::extract::MatchedPath;
use sqlx::PgPool;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;
use tokio::sync::broadcast;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};
use tower_governor::{
    governor::GovernorConfigBuilder,
    key_extractor::{PeerIpKeyExtractor, SmartIpKeyExtractor},
    GovernorLayer,
};
use metrics_exporter_prometheus::PrometheusHandle;
use uuid::Uuid;
use utoipa::OpenApi;

use crate::{config::{HealthState, IndexerState}, handlers, middleware, metrics, models::SorobanEvent};

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
}

/// OpenAPI spec — all paths are documented via #[utoipa::path] on handlers.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Soroban Pulse API",
        version = "1.0.0",
        description = "Indexes Soroban smart contract events on the Stellar network."
    ),
    paths(
        handlers::health,
        handlers::status,
        handlers::get_events,
        handlers::export_events,
        handlers::get_events_by_contract,
        handlers::get_events_by_tx,
        handlers::stream_events,
        handlers::stream_events_by_contract,
        handlers::get_contracts,
        handlers::replay_events,
    ),
    components(schemas(
        crate::models::Event,
        crate::models::EventType,
        crate::models::PaginationParams,
        crate::models::ContractSummary,
        crate::models::ReplayRequest,
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
) -> Router {
    create_router_with_tx(pool, api_keys, allowed_origins, rate_limit_per_minute, false, health_state, indexer_state, prometheus_handle, broadcast::channel(256).0, 15000, 1000, health_check_timeout_ms, None, None, 1000, 30)
}

pub fn create_router_with_tx(
    pool: PgPool,
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
    contract_count_cache_size: u64,
    contract_count_cache_ttl_secs: u64,
) -> Router {
    let cors = build_cors(allowed_origins);
    let auth_state = Arc::new(middleware::AuthState { api_keys });
    let contract_count_cache = moka::future::Cache::builder()
        .max_capacity(contract_count_cache_size)
        .time_to_live(std::time::Duration::from_secs(contract_count_cache_ttl_secs))
        .build();
    let app_state = AppState {
        pool,
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
    };

    // Build governor config: burst = rate_limit_per_minute, replenish 1 token per (60/rate) seconds.
    // per_second(n) means n tokens replenished per second; we want rate_limit_per_minute / 60.
    // Use per_millisecond to avoid integer truncation: replenish 1 token every (60_000 / rate) ms.
    let replenish_ms = 60_000u64 / u64::from(rate_limit_per_minute.max(1));
    let burst = rate_limit_per_minute.max(1);

    // Versioned v1 routes
    let v1 = Router::new()
        .route("/events", get(handlers::get_events))
        .route("/events/export", get(handlers::export_events))
        .route("/events/stream", get(handlers::stream_events))
        .route("/events/contract/:contract_id", get(handlers::get_events_by_contract))
        .route("/events/contract/:contract_id/stream", get(handlers::stream_events_by_contract))
        .route("/events/tx/:tx_hash", get(handlers::get_events_by_tx))
        .route("/contracts", get(handlers::get_contracts))
        .route("/admin/replay", axum::routing::post(handlers::replay_events));

    // Unversioned deprecated aliases (same handlers, add Deprecation header via middleware)
    let deprecated = Router::new()
        .route("/events", get(handlers::get_events))
        .route("/events/stream", get(handlers::stream_events))
        .route("/events/contract/:contract_id", get(handlers::get_events_by_contract))
        .route("/events/contract/:contract_id/stream", get(handlers::stream_events_by_contract))
        .route("/events/tx/:tx_hash", get(handlers::get_events_by_tx))
        .route("/contracts", get(handlers::get_contracts))
        .layer(axum::middleware::from_fn(|req: Request<Body>, next: axum::middleware::Next| async move {
            let path = req.uri().path().to_string();
            let mut resp = next.run(req).await;
            resp.headers_mut().insert(
                "Deprecation",
                HeaderValue::from_static("true"),
            );
            resp.headers_mut().insert(
                "Sunset",
                HeaderValue::from_static("Sat, 24 Oct 2026 00:00:00 GMT"),
            );
            // Map the deprecated path to its versioned equivalent
            let versioned_path = format!("/v1{}", path);
            let link_value = format!("<{}>; rel=\"successor-version\"", versioned_path);
            resp.headers_mut().insert(
                "Link",
                HeaderValue::from_str(&link_value).unwrap_or_else(|_| HeaderValue::from_static("</v1/events>; rel=\"successor-version\"")),
            );
            resp
        }));

    // Health endpoints — exempt from rate limiting.
    let health_routes = Router::new()
        .route("/health", get(handlers::health))
        .route("/healthz/live", get(handlers::health_live))
        .route("/healthz/ready", get(handlers::health_ready));

    // All other routes — subject to rate limiting.
    let rate_limited_routes = if behind_proxy {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(replenish_ms)
                .burst_size(burst)
                .key_extractor(SmartIpKeyExtractor)
                .finish()
                .expect("invalid governor config"),
        );
        Router::new()
            .route("/status", get(handlers::status))
            .route("/metrics", get(handlers::metrics))
            .route("/openapi.json", get(handlers::openapi_json))
            .route("/docs", get(handlers::swagger_ui))
            .nest("/v1", v1)
            .merge(deprecated)
            .layer(GovernorLayer::new(governor_conf))
    } else {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(replenish_ms)
                .burst_size(burst)
                .key_extractor(PeerIpKeyExtractor)
                .finish()
                .expect("invalid governor config"),
        );
        Router::new()
            .route("/status", get(handlers::status))
            .route("/metrics", get(handlers::metrics))
            .route("/openapi.json", get(handlers::openapi_json))
            .route("/docs", get(handlers::swagger_ui))
            .nest("/v1", v1)
            .merge(deprecated)
            .layer(GovernorLayer::new(governor_conf))
    };

    Router::new()
        .merge(health_routes)
        .merge(rate_limited_routes)
        .layer(axum::middleware::from_fn(middleware::security_headers_middleware))
        .layer(axum::middleware::from_fn(middleware::request_id_middleware))
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(move |req: axum::http::Request<Body>, next: axum::middleware::Next| async move {
            let method = req.method().as_str().to_string();
            let route = req.extensions()
                .get::<MatchedPath>()
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| "<unknown>".to_string());
            let request_id = req.headers()
                .get("x-request-id")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("unknown")
                .to_owned();
            let start = Instant::now();
            let response = next.run(req).await;
            let duration = start.elapsed();
            let status = response.status().as_u16().to_string();
            metrics::record_http_request_duration(duration, &method, &route, &status);
            if duration.as_millis() as u64 > slow_request_threshold_ms {
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
        }))
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
        .layer(CompressionLayer::new())
        .layer(SetRequestIdLayer::x_request_id(UuidMakeRequestId))
        .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1 MB default
        .with_state(app_state)
}

fn build_cors(allowed_origins: &[String]) -> CorsLayer {
    let methods = [Method::GET, Method::POST];
    let headers = [axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION];

    if allowed_origins.iter().any(|o| o == "*") {
        return CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(methods)
            .allow_headers(headers);
    }

    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .allow_headers(headers)
        .vary([axum::http::header::ORIGIN])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, Request, StatusCode};
    use axum::body::Body;
    use tower::ServiceExt;
    use tracing_subscriber::layer::SubscriberExt;

    /// Build a minimal router that sleeps for `delay_ms` and runs the metrics
    /// middleware with the given `threshold_ms`.
    fn slow_request_test_app(delay_ms: u64, threshold_ms: u64) -> Router {
        Router::new()
            .route("/slow", get(move || async move {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                "ok"
            }))
            .layer(axum::middleware::from_fn(move |req: axum::http::Request<Body>, next: axum::middleware::Next| async move {
                let method = req.method().as_str().to_string();
                let route = req.extensions()
                    .get::<MatchedPath>()
                    .map(|p| p.as_str().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let request_id = req.headers()
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
            }))
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
        app.oneshot(
            Request::builder().uri("/slow").body(Body::empty()).unwrap()
        ).await.unwrap();

        let logs = output.into_string();
        assert!(logs.contains("slow request"), "expected 'slow request' warn, got: {logs}");
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
        app.oneshot(
            Request::builder().uri("/slow").body(Body::empty()).unwrap()
        ).await.unwrap();

        let logs = output.into_string();
        assert!(!logs.contains("slow request"), "unexpected 'slow request' warn: {logs}");
    }

    #[tokio::test]
    async fn test_compression_header() {
        let pool = PgPool::connect_lazy("postgres://localhost/unused").unwrap();

        let api = Router::new()
            .route("/large", axum::routing::get(|| async { "A".repeat(2000) }));

        let app = Router::new()
            .merge(api)
            .layer(tower_http::compression::CompressionLayer::new())
            .with_state(pool);

        let response = app.clone().oneshot(
            Request::builder()
                .uri("/large")
                .header(header::ACCEPT_ENCODING, "gzip")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();

        assert_eq!(response.headers().get(header::CONTENT_ENCODING).unwrap(), "gzip");

        let response = app.oneshot(
            Request::builder()
                .uri("/large")
                .body(Body::empty())
                .unwrap()
        ).await.unwrap();

        assert!(response.headers().get(header::CONTENT_ENCODING).is_none());
    }

    /// Build a minimal router with GovernorLayer using SmartIpKeyExtractor so tests
    /// can inject a fake IP via X-Forwarded-For without a real TCP connection.
    fn rate_limited_test_app(burst: u32) -> Router {
        let governor_conf = Arc::new(
            GovernorConfigBuilder::default()
                .per_millisecond(60_000u64 / u64::from(burst.max(1)))
                .burst_size(burst)
                .key_extractor(SmartIpKeyExtractor)
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
            let resp = app.clone()
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
        let resp = app.clone()
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
        assert!(resp.headers().contains_key("retry-after") || resp.headers().contains_key("x-ratelimit-after"));
    }

    #[tokio::test]
    async fn rate_limit_different_ips_are_independent() {
        let app = rate_limited_test_app(1);

        // Exhaust the quota for IP A.
        let resp = app.clone()
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
        let resp = app.clone()
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
        let resp = app.clone()
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
}
