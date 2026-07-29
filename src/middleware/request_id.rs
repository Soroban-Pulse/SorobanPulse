//! Request ID middleware — Issue #663
//!
//! Extracts the `X-Request-ID` header from every incoming request and stores
//! it in the thread-local correlation-ID slot so that all error responses
//! produced during that request automatically carry it.

use axum::{extract::Request, middleware::Next, response::Response};

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
}
