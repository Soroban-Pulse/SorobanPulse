use axum::{body::Body, routing::get, Router};
use axum::http::{HeaderValue, Method, Request};
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::broadcast;
use tower_http::{
    compression::CompressionLayer,
    cors::CorsLayer,
    limit::RequestBodyLimitLayer,
    request_id::{MakeRequestId, PropagateRequestIdLayer, RequestId, SetRequestIdLayer},
    trace::TraceLayer,
};
use metrics_exporter_prometheus::PrometheusHandle;
use uuid::Uuid;
use utoipa::OpenApi;

use crate::{config::{HealthState, IndexerState}, handlers, middleware, metrics, models::SorobanEvent};

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
        handlers::get_events_by_contract,
        handlers::get_events_by_tx,
        handlers::stream_events,
    ),
    components(schemas(
        crate::models::Event,
        crate::models::PaginationParams,
        crate::error::ErrorResponse,
    )),
    tags(
        (name = "events", description = "Event indexing endpoints"),
        (name = "system", description = "Health and observability endpoints"),
    )
)]
pub struct ApiDoc;

pub fn create_router(
    pool: PgPool,
    api_key: Option<String>,
    allowed_origins: &[String],
    rate_limit_per_minute: u32,
    health_state: Arc<HealthState>,
    indexer_state: Arc<IndexerState>,
    prometheus_handle: PrometheusHandle,
    max_body_size_bytes: usize,
) -> Router {
    create_router_with_tx(pool, api_key, allowed_origins, rate_limit_per_minute, health_state, indexer_state, prometheus_handle, broadcast::channel(256).0, max_body_size_bytes)
}

pub fn create_router_with_tx(
    pool: PgPool,
    api_key: Option<String>,
    allowed_origins: &[String],
    rate_limit_per_minute: u32,
    health_state: Arc<HealthState>,
    indexer_state: Arc<IndexerState>,
    prometheus_handle: PrometheusHandle,
    event_tx: broadcast::Sender<SorobanEvent>,
    max_body_size_bytes: usize,
) -> Router {
    let cors = build_cors(allowed_origins);
    let auth_state = Arc::new(middleware::AuthState { api_key });
    let app_state = AppState { pool, health_state, indexer_state, prometheus_handle, event_tx };

    let _period_secs = 60u64.div_ceil(u64::from(rate_limit_per_minute));

    // Versioned v1 routes
    let v1 = Router::new()
        .route("/events", get(handlers::get_events))
        .route("/events/stream", get(handlers::stream_events))
        .route("/events/contract/:contract_id", get(handlers::get_events_by_contract))
        .route("/events/tx/:tx_hash", get(handlers::get_events_by_tx));

    // Unversioned deprecated aliases (same handlers, add Deprecation header via middleware)
    let deprecated = Router::new()
        .route("/events", get(handlers::get_events))
        .route("/events/stream", get(handlers::stream_events))
        .route("/events/contract/:contract_id", get(handlers::get_events_by_contract))
        .route("/events/tx/:tx_hash", get(handlers::get_events_by_tx))
        .layer(axum::middleware::from_fn(|req: Request<Body>, next: axum::middleware::Next| async move {
            let mut resp = next.run(req).await;
            resp.headers_mut().insert(
                "Deprecation",
                HeaderValue::from_static("true"),
            );
            resp.headers_mut().insert(
                "Link",
                HeaderValue::from_static("</v1/events>; rel=\"successor-version\""),
            );
            resp
        }));

    Router::new()
        .route("/health", get(handlers::health))
        .route("/healthz/live", get(handlers::health_live))
        .route("/healthz/ready", get(handlers::health_ready))
        .route("/status", get(handlers::status))
        .route("/metrics", get(handlers::metrics))
        .route("/openapi.json", get(handlers::openapi_json))
        .route("/docs", get(handlers::swagger_ui))
        .nest("/v1", v1)
        .merge(deprecated)
        .layer(axum::middleware::from_fn_with_state(
            auth_state,
            middleware::auth_middleware,
        ))
        .layer(axum::middleware::from_fn(|req: axum::http::Request<Body>, next: axum::middleware::Next| async move {
            let method = req.method().as_str().to_string();
            let route = req.uri().path().to_string();
            let start = Instant::now();
            let response = next.run(req).await;
            let duration = start.elapsed();
            let status = response.status().as_u16().to_string();
            metrics::record_http_request_duration(duration, &method, &route, &status);
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
        .layer(RequestBodyLimitLayer::new(max_body_size_bytes))
        .with_state(app_state)
}

fn build_cors(allowed_origins: &[String]) -> CorsLayer {
    let methods = [Method::GET];

    if allowed_origins.iter().any(|o| o == "*") {
        return CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(methods);
    }

    let origins: Vec<HeaderValue> = allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods(methods)
        .vary([axum::http::header::ORIGIN])
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{header, Request};
    use axum::body::Body;
    use tower::ServiceExt;

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
}
