//! Unversioned aliases kept for backwards compatibility. They serve the same
//! handlers as their `/v1` counterparts and add a `Deprecation` header.

use axum::{routing::get, Router};

use super::AppState;
use crate::{handlers, middleware};

pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/events", get(handlers::get_events))
        .route("/events/stream", get(handlers::stream_events))
        .route(
            "/events/contract/{contract_id}",
            get(handlers::get_events_by_contract),
        )
        .route(
            "/events/contract/{contract_id}/stream",
            get(handlers::stream_events_by_contract),
        )
        .route("/events/tx/{tx_hash}", get(handlers::get_events_by_tx))
        .route("/contracts", get(handlers::get_contracts))
        .layer(axum::middleware::from_fn(
            middleware::deprecation_middleware,
        ))
}
