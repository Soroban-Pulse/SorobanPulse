//! IP allow/deny list enforcement middleware (Issue #942).
//!
//! Wraps [`crate::zero_trust::IpDenyListRule`] / [`crate::zero_trust::IpAllowListRule`]
//! (CIDR-aware; see zero_trust.rs) as an Axum middleware layer. Both rules
//! already existed but had zero callers anywhere in the codebase before
//! this — `IpDenyListRule` in particular only ever supported exact-string
//! IP matches, with no way to block or allow a whole CIDR block, which
//! this also fixes (see zero_trust.rs's `IpFilterEntry`).
//!
//! Configure `IP_DENYLIST` and/or `IP_ALLOWLIST` (comma-separated IPs
//! and/or CIDR blocks, IPv4 or IPv6). When neither is set this middleware
//! is a no-op. If both are set, the deny-list takes precedence (evaluated
//! first) — an explicit block should never be silently overridden by an
//! allow-list.

extern crate metrics as m;

use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::net::SocketAddr;

use crate::client_ip::ClientIp;
use crate::zero_trust::{AccessDecision, IpAllowListRule, IpDenyListRule, PolicyRule, RequestContext};

pub async fn ip_access_control_middleware(
    State(state): State<crate::routes::AppState>,
    req: Request,
    next: Next,
) -> Response {
    let cfg = &state.config;

    let rule: Option<Box<dyn PolicyRule>> = if !cfg.ip_denylist.is_empty() {
        Some(Box::new(IpDenyListRule::new(cfg.ip_denylist.clone())))
    } else if !cfg.ip_allowlist.is_empty() {
        Some(Box::new(IpAllowListRule::new(cfg.ip_allowlist.clone())))
    } else {
        None
    };

    let Some(rule) = rule else {
        return next.run(req).await;
    };

    // Resolve the client IP through the shared `ClientIp` extractor so that
    // `X-Forwarded-For` / `Forwarded` headers are only honoured when the TCP
    // peer is a configured trusted proxy (Issue #1432). Untrusted peers cannot
    // spoof their way past the allow/deny lists by rotating header values.
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0);
    let ip = ClientIp::resolve(peer, req.headers(), &cfg.trusted_proxies).to_string();
    let ctx = RequestContext::new(ip, String::new(), req.uri().path(), req.method().as_str());

    if let Some(AccessDecision::Deny(reason)) = rule.evaluate(&ctx) {
        m::counter!("soroban_pulse_ip_access_blocked_total").increment(1);
        let mut response = StatusCode::FORBIDDEN.into_response();
        let _ = response
            .headers_mut()
            .insert("X-Access-Denied-Reason", axum::http::HeaderValue::from_static("ip-policy"));
        tracing::warn!(reason = %reason, "Request blocked by IP access control policy");
        return response;
    }

    next.run(req).await
}
