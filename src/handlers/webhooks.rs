//! Inbound webhook receivers and outbound webhook delivery controls
//! (email bounce webhook, webhook circuit-breaker administration).

use super::*;

// ── Issue #879: Webhook Circuit Breaker Admin Endpoints ─────────────────────

/// GET /v1/admin/webhook/circuit-breaker
/// Get circuit breaker stats for all webhook endpoints
pub async fn get_circuit_breaker_stats(
    State(state): State<AppState>,
) -> Json<Vec<crate::webhook_circuit_breaker::EndpointCircuitBreakerStats>> {
    let stats = state.circuit_breaker_manager.get_all_stats().await;
    Json(stats)
}

/// GET /v1/admin/webhook/circuit-breaker/{endpoint}
/// Get circuit breaker stats for a specific endpoint
pub async fn get_endpoint_circuit_breaker_stats(
    State(state): State<AppState>,
    Path(endpoint): Path<String>,
) -> Result<Json<crate::webhook_circuit_breaker::EndpointCircuitBreakerStats>, AppError> {
    let stats = state.circuit_breaker_manager.get_stats(&endpoint).await
        .ok_or_else(|| AppError::NotFound("Endpoint not found".to_string()))?;
    Ok(Json(stats))
}

/// POST /v1/admin/webhook/circuit-breaker/{endpoint}/reset
/// Manually reset a circuit breaker for a specific endpoint
pub async fn reset_circuit_breaker(
    State(state): State<AppState>,
    Path(endpoint): Path<String>,
) -> Json<serde_json::Value> {
    state.circuit_breaker_manager.reset(&endpoint).await;
    Json(json!({
        "status": "reset",
        "endpoint": endpoint,
    }))
}
/// Webhook that receives email bounce notifications from SendGrid, AWS SES
/// (including SNS-wrapped notifications) and Mailgun (Issue #484). Bounced
/// addresses are persisted so future notifications skip them.
///
/// Bounce webhook authentication (issue #1158): the provider signature is
/// verified before any DB write.  The provider is determined from the
/// `X-Bounce-Provider` header:
///   - `sns` — Amazon SNS (SES)
///   - `sendgrid` — SendGrid Event Webhook
///   - `shared_secret` (default) — generic HMAC shared secret
#[utoipa::path(
    post,
    path = "/v1/notifications/email/bounce",
    tag = "system",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Bounce payload processed", body = serde_json::Value),
        (status = 401, description = "Missing or invalid provider signature"),
    )
)]
pub async fn email_bounce_webhook(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> impl IntoResponse {
    use axum::body::to_bytes;
    use crate::email_token::{verify_bounce_signature, BounceProvider};

    // Read the raw body so we can verify the signature over it.
    let headers = request.headers().clone();
    let provider_header = headers
        .get("x-bounce-provider")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("shared_secret");
    let provider = match provider_header {
        "sns" => BounceProvider::Sns,
        "sendgrid" => BounceProvider::SendGrid,
        _ => BounceProvider::SharedSecret,
    };

    let body_bytes = match to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "Failed to read bounce webhook body");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "failed to read request body" })),
            );
        }
    };

    // Verify signature before touching the DB.
    if let Err(e) = verify_bounce_signature(&provider, &headers, &body_bytes, None) {
        tracing::warn!(error = %e, "Bounce webhook signature verification failed");
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "invalid or missing provider signature" })),
        );
    }

    let payload: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid JSON: {e}") })),
            );
        }
    };

    let recipients = crate::email::extract_bounced_recipients(&payload);
    let mut recorded = 0usize;
    for recipient in &recipients {
        match crate::email::record_bounce(&state.pool, recipient).await {
            Ok(()) => {
                crate::metrics::record_email_bounce();
                recorded += 1;
            }
            Err(e) => {
                tracing::error!(error = %e, email = %recipient.email, "Failed to record email bounce");
            }
        }
    }
    (
        StatusCode::OK,
        Json(json!({ "received": recipients.len(), "recorded": recorded })),
    )
}
