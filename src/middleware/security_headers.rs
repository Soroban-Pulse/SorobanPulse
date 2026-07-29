//! Security headers middleware — Issue #663
//!
//! Adds the full OWASP-recommended set of HTTP security headers to every
//! response (Issue #566).  The `/docs` path receives a relaxed
//! Content-Security-Policy to allow the Swagger UI assets from `unpkg.com`.

use axum::{extract::Request, middleware::Next, response::Response};

/// Applies OWASP security headers to every response.
///
/// Headers set:
/// - `X-Content-Type-Options: nosniff`
/// - `X-Frame-Options: DENY`
/// - `Referrer-Policy: no-referrer`
/// - `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload`
/// - `X-XSS-Protection: 1; mode=block`
/// - `Permissions-Policy: …` (all powerful features disabled)
/// - `Content-Security-Policy` (strict for API routes; relaxed for `/docs`)
pub async fn security_headers_middleware(req: Request, next: Next) -> Response {
    let path = req.uri().path().to_owned();
    let mut response = next.run(req).await;

    let h = response.headers_mut();

    h.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    h.insert("X-Frame-Options", "DENY".parse().unwrap());
    h.insert("Referrer-Policy", "no-referrer".parse().unwrap());
    h.insert(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains; preload"
            .parse()
            .unwrap(),
    );
    h.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    h.insert(
        "Permissions-Policy",
        "accelerometer=(), ambient-light-sensor=(), autoplay=(), camera=(), \
         encrypted-media=(), fullscreen=(), geolocation=(), gyroscope=(), \
         magnetometer=(), microphone=(), midi=(), payment=(), usb=()"
            .parse()
            .unwrap(),
    );

    let csp = if path == "/docs" {
        "default-src 'self'; script-src 'self' 'unsafe-inline' https://unpkg.com; \
         style-src 'self' 'unsafe-inline' https://unpkg.com; img-src 'self' data:; \
         connect-src 'self'; frame-ancestors 'none';"
    } else {
        "default-src 'none'; frame-ancestors 'none';"
    };
    h.insert("Content-Security-Policy", csp.parse().unwrap());

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, routing::get, Router};
    use tower::ServiceExt;

    async fn app() -> Router {
        Router::new()
            .route("/api", get(|| async { "ok" }))
            .route("/docs", get(|| async { "swagger" }))
            .layer(axum::middleware::from_fn(security_headers_middleware))
    }

    #[tokio::test]
    async fn all_owasp_headers_present_on_api_route() {
        let resp = app()
            .await
            .oneshot(axum::http::Request::get("/api").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let h = resp.headers();
        assert_eq!(h.get("X-Content-Type-Options").unwrap(), "nosniff");
        assert_eq!(h.get("X-Frame-Options").unwrap(), "DENY");
        assert_eq!(h.get("Referrer-Policy").unwrap(), "no-referrer");
        assert_eq!(h.get("X-XSS-Protection").unwrap(), "1; mode=block");
        assert!(h.get("Content-Security-Policy").is_some());
        assert!(h.get("Strict-Transport-Security").is_some());
        assert!(h.get("Permissions-Policy").is_some());
    }

    #[tokio::test]
    async fn docs_route_gets_relaxed_csp() {
        let resp = app()
            .await
            .oneshot(axum::http::Request::get("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = resp
            .headers()
            .get("Content-Security-Policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(csp.contains("unpkg.com"));
    }

    #[tokio::test]
    async fn api_route_gets_strict_csp() {
        let resp = app()
            .await
            .oneshot(axum::http::Request::get("/api").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = resp
            .headers()
            .get("Content-Security-Policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(csp, "default-src 'none'; frame-ancestors 'none';");
    }
}
