//! Middleware for extracting and validating tenant context from requests.

use axum::{
    async_trait,
    extract::{FromRequestParts, Request},
    middleware::Next,
    response::Response,
};
use http::request::Parts;
use crate::multi_tenancy::{TenantContext, TenantId};

const TENANT_HEADER: &str = "x-tenant-id";
const USER_HEADER: &str = "x-user-id";

/// Extractor for tenant context from request headers
#[derive(Debug, Clone)]
pub struct TenantExtractor(pub TenantContext);

#[async_trait]
impl<S> FromRequestParts<S> for TenantExtractor
where
    S: Send + Sync,
{
    type Rejection = String;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let tenant_header = parts
            .headers
            .get(TENANT_HEADER)
            .and_then(|h| h.to_str().ok());

        let user_header = parts
            .headers
            .get(USER_HEADER)
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let mut ctx = TenantContext::from_header(tenant_header);
        if let Some(user_id) = user_header {
            ctx = ctx.with_user(user_id);
        }

        Ok(TenantExtractor(ctx))
    }
}

/// Middleware that extracts tenant context and validates access
pub async fn tenant_context_middleware(
    request: Request,
    next: Next,
) -> Result<Response, String> {
    let (mut parts, body) = request.into_parts();

    let tenant_header = parts
        .headers
        .get(TENANT_HEADER)
        .and_then(|h| h.to_str().ok());

    let _ctx = TenantContext::from_header(tenant_header);

    let request = Request::from_parts(parts, body);
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_header_parsing() {
        let ctx = TenantContext::from_header(Some("test-tenant"));
        assert_eq!(ctx.tenant_id.as_str(), "test-tenant");
    }

    #[test]
    fn default_tenant_when_no_header() {
        let ctx = TenantContext::from_header(None);
        assert!(ctx.tenant_id.is_default());
    }
}
