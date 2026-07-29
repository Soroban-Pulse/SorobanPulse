//! Authentication middleware — Issue #663
//!
//! Provides:
//! - [`AuthState`] — shared state injected into the auth layer.
//! - [`auth_middleware`] — validates `Authorization: Bearer` / `X-Api-Key`
//!   headers and, in multi-tenant mode, resolves the tenant.
//! - [`AdminAuthState`] — shared state for the admin-only layer.
//! - [`admin_auth_middleware`] — gates `/v1/admin/*` endpoints.
//! - [`TenantId`] — request extension carrying the resolved tenant.
//! - [`hash_api_key`] — SHA-256 hex digest helper.

use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
    Json,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use subtle::ConstantTimeEq;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The resolved tenant for the current request.
///
/// Injected as a request extension by [`auth_middleware`] when multi-tenant
/// mode is enabled.  Handlers read it via
/// `req.extensions().get::<TenantId>()`.
#[derive(Clone, Debug)]
pub struct TenantId(pub String);

/// Shared state for the global authentication middleware layer.
#[derive(Clone)]
pub struct AuthState {
    /// Regular API keys.
    pub api_keys: Vec<String>,
    /// Admin API keys — also accepted at the global gate so admin requests
    /// pass through to the admin-only layer for privilege checking.
    pub admin_api_keys: Vec<String>,
    /// SHA-256(key) → tenant_id mapping; populated only in multi-tenant mode.
    pub tenant_map: Arc<std::collections::HashMap<String, String>>,
    /// Whether multi-tenant mode is active.
    pub multi_tenant: bool,
}

/// Shared state for the admin-only authentication layer.
#[derive(Clone)]
pub struct AdminAuthState {
    /// Keys that grant admin access.  When empty the layer is a no-op and
    /// admin routes fall back to whatever the regular auth layer enforced.
    pub admin_api_keys: Vec<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Constant-time check whether `key` equals any of `candidates`.
pub(crate) fn key_matches_any(key: &str, candidates: &[String]) -> bool {
    candidates.iter().any(|expected| {
        let m: bool = key.as_bytes().ct_eq(expected.as_bytes()).into();
        m
    })
}

/// Extract the API key from either the `Authorization: Bearer <key>` header
/// or the `X-Api-Key` header.
fn extract_api_key(req: &Request) -> Option<&str> {
    let bearer = req
        .headers()
        .get("Authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "));
    let x_api_key = req.headers().get("X-Api-Key").and_then(|h| h.to_str().ok());
    bearer.or(x_api_key)
}

/// SHA-256 hex digest of a raw API key — used as the lookup key in
/// `tenant_map` so plain-text keys are never stored in memory after hashing.
pub fn hash_api_key(key: &str) -> String {
    let mut h = Sha256::new();
    h.update(key.as_bytes());
    format!("{:x}", h.finalize())
}

// ---------------------------------------------------------------------------
// Middleware functions
// ---------------------------------------------------------------------------

/// Global authentication middleware.
///
/// - Skips `/health`, `/healthz/*`, and `/unsubscribe` (public paths).
/// - When `api_keys` is empty, auth is disabled and all requests pass.
/// - In multi-tenant mode, resolves the tenant and injects [`TenantId`].
pub async fn auth_middleware(
    State(state): State<Arc<AuthState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let path = req.uri().path();

    // Public paths — always bypass auth.
    if path == "/health" || path.starts_with("/healthz/") || path == "/unsubscribe" {
        return Ok(next.run(req).await);
    }

    if !state.api_keys.is_empty() {
        let provided_key = extract_api_key(&req);

        let is_admin = provided_key.map_or(false, |k| key_matches_any(k, &state.admin_api_keys));
        let is_valid =
            is_admin || provided_key.map_or(false, |k| key_matches_any(k, &state.api_keys));

        if !is_valid {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "unauthorized" })),
            ));
        }

        // Multi-tenant: resolve and inject tenant_id.  Admin keys are global
        // and skip tenant resolution.
        if state.multi_tenant && !is_admin {
            let key = provided_key.unwrap_or("");
            let key_hash = hash_api_key(key);
            match state.tenant_map.get(&key_hash) {
                Some(tid) => {
                    req.extensions_mut().insert(TenantId(tid.clone()));
                }
                None => {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(serde_json::json!({
                            "error": "api key is not associated with a tenant"
                        })),
                    ));
                }
            }
        }
    }

    Ok(next.run(req).await)
}

/// Admin-only authentication middleware.
///
/// Guards `/v1/admin/*` endpoints independently of the global API key:
/// - No key → **401 Unauthorized**.
/// - Non-admin key → **403 Forbidden**.
/// - Admin key → passes through.
/// - No admin keys configured → no-op (backward-compatible fallback).
pub async fn admin_auth_middleware(
    State(state): State<Arc<AdminAuthState>>,
    req: Request,
    next: Next,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    if state.admin_api_keys.is_empty() {
        return Ok(next.run(req).await);
    }

    match extract_api_key(&req) {
        None => Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "admin authentication required" })),
        )),
        Some(key) if key_matches_any(key, &state.admin_api_keys) => Ok(next.run(req).await),
        Some(_) => Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "admin privileges required" })),
        )),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, routing::get, Router};
    use tower::ServiceExt;

    fn make_auth_app(api_keys: Vec<String>) -> Router {
        let state = Arc::new(AuthState {
            api_keys,
            admin_api_keys: vec![],
            tenant_map: Arc::new(std::collections::HashMap::new()),
            multi_tenant: false,
        });
        Router::new()
            .route("/test", get(|| async { "OK" }))
            .route("/health", get(|| async { "OK" }))
            .route("/healthz/live", get(|| async { "OK" }))
            .route_layer(axum::middleware::from_fn_with_state(
                state,
                auth_middleware,
            ))
    }

    #[tokio::test]
    async fn no_keys_configured_passes_all() {
        let resp = make_auth_app(vec![])
            .oneshot(axum::http::Request::get("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bearer_token_accepted() {
        let resp = make_auth_app(vec!["secret".into()])
            .oneshot(
                axum::http::Request::get("/test")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn x_api_key_accepted() {
        let resp = make_auth_app(vec!["secret".into()])
            .oneshot(
                axum::http::Request::get("/test")
                    .header("X-Api-Key", "secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn wrong_key_returns_401() {
        let resp = make_auth_app(vec!["secret".into()])
            .oneshot(
                axum::http::Request::get("/test")
                    .header("X-Api-Key", "wrong")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn health_bypasses_auth() {
        let app = make_auth_app(vec!["secret".into()]);
        for path in ["/health", "/healthz/live"] {
            let resp = app
                .clone()
                .oneshot(axum::http::Request::get(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }

    #[test]
    fn hash_is_deterministic_and_hex() {
        let h = hash_api_key("key");
        assert_eq!(h, hash_api_key("key"));
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn admin_layer_requires_admin_key() {
        let state = Arc::new(AdminAuthState {
            admin_api_keys: vec!["admin".into()],
        });
        let app = Router::new()
            .route("/admin", get(|| async { "ok" }))
            .route_layer(axum::middleware::from_fn_with_state(
                state,
                admin_auth_middleware,
            ));

        // No key → 401
        let resp = app
            .clone()
            .oneshot(axum::http::Request::get("/admin").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        // Wrong key → 403
        let resp = app
            .clone()
            .oneshot(
                axum::http::Request::get("/admin")
                    .header("X-Api-Key", "regular")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);

        // Correct key → 200
        let resp = app
            .oneshot(
                axum::http::Request::get("/admin")
                    .header("X-Api-Key", "admin")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
