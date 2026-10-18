//! Webhook routes: inbound provider webhooks and outbound webhook delivery
//! controls.

use axum::Router;

use super::AppState;
use crate::handlers;

/// Public `/v1` webhook receivers.
pub(super) fn router() -> Router<AppState> {
    Router::new()
        .route("/notifications/email/bounce", axum::routing::post(handlers::email_bounce_webhook))
}

/// Admin-only webhook routes; `super` applies the admin auth layer.
pub(super) fn admin_router() -> Router<AppState> {
    Router::new()
        // #879: Webhook circuit breaker admin endpoints
        .route("/admin/webhook/circuit-breaker", axum::routing::get(handlers::get_circuit_breaker_stats))
        .route("/admin/webhook/circuit-breaker/{endpoint}", axum::routing::get(handlers::get_endpoint_circuit_breaker_stats))
        .route("/admin/webhook/circuit-breaker/{endpoint}/reset", axum::routing::post(handlers::reset_circuit_breaker))
}
