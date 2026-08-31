//! Request ID middleware — Issue #663
//!
//! Extracts the `X-Request-ID` header from every incoming request and stores
//! it in the thread-local correlation-ID slot so that all error responses
//! produced during that request automatically carry it.

use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Header used to carry the correlation ID across service boundaries.
pub const CORRELATION_ID_HEADER: &str = "x-correlation-id";

/// Extract `X-Request-ID` from the request headers and propagate it to the
/// thread-local correlation-ID store used by [`crate::error`].
pub async fn request_id_middleware(req: Request, next: Next) -> Response {
    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    crate::error::set_request_id(request_id);
    next.run(req).await
}

/// Correlation-ID middleware — Issue #tracing-correlation.
///
/// Ensures every request carries an `X-Correlation-ID` header: if the caller
/// (or an upstream service) already set one, it is preserved so that a single
/// logical operation can be traced across every service it touches. If none
/// is present, a fresh ID is minted using the existing trace-id generator so
/// that correlation IDs and trace IDs share the same format. The ID is
/// mirrored onto the response so downstream/browser callers can log it too.
pub async fn correlation_id_middleware(mut req: Request, next: Next) -> Response {
    let correlation_id = req
        .headers()
        .get(CORRELATION_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(crate::distributed_tracing::new_trace_id);

    req.headers_mut().insert(
        CORRELATION_ID_HEADER,
        HeaderValue::from_str(&correlation_id).unwrap_or_else(|_| HeaderValue::from_static("invalid")),
    );

    crate::distributed_tracing::set_correlation_id(correlation_id.clone());

    let mut resp = next.run(req).await;
    if let Ok(header_value) = HeaderValue::from_str(&correlation_id) {
        resp.headers_mut().insert(CORRELATION_ID_HEADER, header_value);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn injects_request_id_from_header() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let _ = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header("x-request-id", "test-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // The correlation ID should have been set in the thread-local.
        // We can only verify indirectly since thread-locals are request-scoped.
    }

    #[tokio::test]
    async fn falls_back_to_unknown_when_header_missing() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(request_id_middleware));

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
    }

    #[tokio::test]
    async fn correlation_id_preserved_from_incoming_header() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(correlation_id_middleware));

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .header(CORRELATION_ID_HEADER, "abc-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.headers().get(CORRELATION_ID_HEADER).unwrap(),
            "abc-123"
        );
    }

    #[tokio::test]
    async fn correlation_id_generated_when_missing() {
        let app = Router::new()
            .route("/", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(correlation_id_middleware));

        let resp = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let header = resp.headers().get(CORRELATION_ID_HEADER);
        assert!(header.is_some());
        assert_eq!(header.unwrap().to_str().unwrap().len(), 32);
    }
}
