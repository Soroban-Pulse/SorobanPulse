use axum::{routing::get, Router};
use http::{HeaderValue, Method};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::{handlers, middleware};

pub fn create_router(pool: PgPool, api_key: Option<String>, allowed_origins: &[String]) -> Router {
    let cors = build_cors(allowed_origins);

    let auth_state = Arc::new(middleware::AuthState { api_key });

    Router::new()
        .route("/health", get(handlers::health))
        .route("/events", get(handlers::get_events))
        .route("/events/:contract_id", get(handlers::get_events_by_contract))
        .route("/events/tx/:tx_hash", get(handlers::get_events_by_tx))
        .layer(axum::middleware::from_fn_with_state(auth_state, middleware::auth_middleware))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(pool)
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
        .vary([http::header::ORIGIN])
}
