//! Tracing middleware — Issue #663
//!
//! Extracts distributed trace context from incoming request headers
//! (`traceparent`, `X-Trace-ID`) and records them as span attributes so that
//! every log line emitted during the request is correlated to the upstream
//! trace.

use axum::{extract::Request, middleware::Next, response::Response};

/// Extract and propagate distributed trace context (Issue #628).
///
/// Reads `traceparent` / `X-Trace-ID` headers and stores them in the current
/// tracing span so downstream log events and spans share the same trace.
pub async fn tracing_middleware(req: Request, next: Next) -> Response {
    let trace_context = crate::distributed_tracing::extract_trace_context(req.headers());

    if let Some(ctx) = trace_context {
        tracing::debug!(
            trace_id = %ctx.trace_id,
            parent_id = ?ctx.parent_id,
            "extracted trace context from headers"
        );

        crate::distributed_tracing::set_span_attribute("trace_id", &ctx.trace_id);
        if let Some(ref parent_id) = ctx.parent_id {
            crate::distributed_tracing::set_span_attribute("parent_id", parent_id);
        }
    }

    next.run(req).await
}

/// Middleware to track in-flight requests for graceful shutdown (Issue #633).
///
/// The actual increment/decrement is handled via `AppState`; this middleware
/// exists as an extension point for future per-request lifecycle hooks.
pub async fn request_tracking_middleware(
    req: Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    next.run(req).await
}
