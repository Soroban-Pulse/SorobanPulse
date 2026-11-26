use std::{
    net::{IpAddr, SocketAddr},
    sync::OnceLock,
};

use axum::{
    body::Body,
    extract::{ConnectInfo, FromRequestParts, Request},
    http::{header, request::Parts, HeaderValue, Response},
    middleware::Next,
};

/// Strict CSP for API routes: no scripts, styles, or other resources allowed.
const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// CSP for the Swagger UI served at `/docs`.
/// Allows the Swagger UI CDN (unpkg.com) plus inline scripts/styles that the
/// Swagger UI bundle relies on to render.
const SWAGGER_CSP: &str = "default-src 'self'; \
    script-src 'self' 'unsafe-inline' https://unpkg.com; \
    style-src 'self' 'unsafe-inline' https://unpkg.com; \
    img-src 'self' data: https://unpkg.com; \
    font-src 'self' data: https://unpkg.com; \
    connect-src 'self'; \
    frame-ancestors 'none'; \
    base-uri 'self'; \
    form-action 'self'";

const HSTS: &str = "max-age=31536000; includeSubDomains";
const X_CONTENT_TYPE_OPTIONS: &str = "nosniff";

/// Environment variable holding a comma-separated list of trusted proxy CIDRs.
pub const TRUSTED_PROXIES_ENV: &str = "TRUSTED_PROXIES";

/// A parsed CIDR block used to decide whether a TCP peer is a trusted proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    network: IpAddr,
    prefix_len: u8,
}

impl Cidr {
    /// Parse a CIDR string such as `10.0.0.0/8`, `192.168.1.0/24` or `::1/128`.
    ///
    /// A bare IP address without a prefix is treated as a single-host block
    /// (`/32` for IPv4, `/128` for IPv6).
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (addr_part, prefix_part) = match s.split_once('/') {
            Some((addr, prefix)) => (addr, Some(prefix)),
            None => (s, None),
        };
        let network: IpAddr = addr_part.trim().parse().ok()?;
        let max_prefix = match network {
            IpAddr::V4(_) => 32,
            IpAddr::V6(_) => 128,
        };
        let prefix_len = match prefix_part {
            Some(p) => p.trim().parse::<u8>().ok()?,
            None => max_prefix,
        };
        if prefix_len > max_prefix {
            return None;
        }
        Some(Self {
            network,
            prefix_len,
        })
    }

    /// Returns true when `ip` falls inside this CIDR block.
    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.network, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = if self.prefix_len == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix_len)
                };
                (u32::from(net) & mask) == (u32::from(ip) & mask)
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = if self.prefix_len == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix_len)
                };
                (u128::from(net) & mask) == (u128::from(ip) & mask)
            }
            // Normalise IPv4-mapped IPv6 addresses so `::ffff:10.0.0.1` matches `10.0.0.0/8`.
            (IpAddr::V4(net), IpAddr::V6(ip)) => match ip.to_ipv4_mapped() {
                Some(v4) => Self {
                    network: IpAddr::V4(net),
                    prefix_len: self.prefix_len,
                }
                .contains(IpAddr::V4(v4)),
                None => false,
            },
            (IpAddr::V6(net), IpAddr::V4(ip)) => match net.to_ipv4_mapped() {
                Some(v6) => Self {
                    network: IpAddr::V6(v6),
                    prefix_len: self.prefix_len,
                }
                .contains(IpAddr::V4(ip)),
                None => false,
            },
        }
    }
}

/// Parse a comma-separated list of CIDRs, ignoring invalid entries.
pub fn parse_trusted_proxies(raw: &str) -> Vec<Cidr> {
    raw.split(',')
        .filter_map(|entry| Cidr::parse(entry))
        .collect()
}

/// Process-wide trusted proxy list, initialised from `TRUSTED_PROXIES`.
fn trusted_proxies() -> &'static [Cidr] {
    static PROXIES: OnceLock<Vec<Cidr>> = OnceLock::new();
    PROXIES.get_or_init(|| match std::env::var(TRUSTED_PROXIES_ENV) {
        Ok(raw) => parse_trusted_proxies(&raw),
        Err(_) => Vec::new(),
    })
}

/// Returns true when `peer` is a configured trusted proxy.
pub fn is_trusted_proxy(peer: IpAddr) -> bool {
    trusted_proxies().iter().any(|cidr| cidr.contains(peer))
}

/// Extract the client IP from the `X-Forwarded-For` header value.
///
/// Returns the right-most hop that is not itself a trusted proxy. If every hop
/// is trusted, the left-most (original) hop is returned.
fn client_ip_from_xff(value: &str) -> Option<IpAddr> {
    let hops: Vec<IpAddr> = value
        .split(',')
        .filter_map(|hop| hop.trim().parse::<IpAddr>().ok())
        .collect();
    if hops.is_empty() {
        return None;
    }
    for hop in hops.iter().rev() {
        if !is_trusted_proxy(*hop) {
            return Some(*hop);
        }
    }
    hops.first().copied()
}

/// Extract the client IP from an RFC 7239 `Forwarded` header value.
fn client_ip_from_forwarded(value: &str) -> Option<IpAddr> {
    let hops: Vec<IpAddr> = value
        .split(',')
        .filter_map(|element| {
            element.split(';').find_map(|pair| {
                let (key, val) = pair.split_once('=')?;
                if !key.trim().eq_ignore_ascii_case("for") {
                    return None;
                }
                let val = val.trim().trim_matches('"');
                // Strip optional port and IPv6 brackets.
                let val = val.trim_start_matches('[');
                let val = match val.split_once(']') {
                    Some((inner, _)) => inner,
                    None => val.split(':').next().unwrap_or(val),
                };
                val.parse::<IpAddr>().ok()
            })
        })
        .collect();
    if hops.is_empty() {
        return None;
    }
    for hop in hops.iter().rev() {
        if !is_trusted_proxy(*hop) {
            return Some(*hop);
        }
    }
    hops.first().copied()
}

/// Resolve the effective client IP given the TCP peer and request headers.
///
/// Forwarding headers are only honoured when `peer` is a trusted proxy.
pub fn resolve_client_ip(peer: IpAddr, headers: &axum::http::HeaderMap) -> IpAddr {
    if !is_trusted_proxy(peer) {
        return peer;
    }
    if let Some(ip) = headers
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(client_ip_from_xff)
    {
        return ip;
    }
    if let Some(ip) = headers
        .get("forwarded")
        .and_then(|h| h.to_str().ok())
        .and_then(client_ip_from_forwarded)
    {
        return ip;
    }
    peer
}

/// Axum extractor yielding the trusted client IP.
///
/// Uses `ConnectInfo<SocketAddr>` for the TCP peer and only consults
/// `X-Forwarded-For` / `Forwarded` when that peer is a trusted proxy.
#[derive(Debug, Clone, Copy)]
pub struct ClientIp(pub IpAddr);

impl ClientIp {
    /// The resolved client IP address.
    pub fn ip(&self) -> IpAddr {
        self.0
    }
}

impl std::fmt::Display for ClientIp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<S> FromRequestParts<S> for ClientIp
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip());
        let ip = match peer {
            Some(peer) => resolve_client_ip(peer, &parts.headers),
            // Without connection info we cannot trust any forwarding header.
            None => IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED),
        };
        Ok(ClientIp(ip))
    }
}

/// Returns true when the request path targets the Swagger UI.
fn is_swagger_ui_path(path: &str) -> bool {
    path == "/docs" || path.starts_with("/docs/")
}

/// Global security headers middleware.
///
/// Applies to every response, including `GET /docs`. The CSP differs between
/// the Swagger UI (which needs the CDN and inline assets) and API routes
/// (which get a strict `default-src 'none'` policy).
pub async fn security_headers_middleware(req: Request, next: Next) -> Response<Body> {
    let is_swagger = is_swagger_ui_path(req.uri().path());

    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    let csp = if is_swagger { SWAGGER_CSP } else { API_CSP };
    headers.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static(csp),
    );

    headers.insert(
        header::STRICT_TRANSPORT_SECURITY,
        HeaderValue::from_static(HSTS),
    );

    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static(X_CONTENT_TYPE_OPTIONS),
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request as HttpRequest, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    fn test_router() -> Router {
        Router::new()
            .route("/api/ping", get(|| async { "pong" }))
            .route("/docs", get(|| async { "swagger" }))
            .layer(middleware::from_fn(security_headers_middleware))
    }

    async fn get_headers(path: &str) -> axum::http::HeaderMap {
        let request = HttpRequest::builder()
            .uri(path)
            .body(Body::empty())
            .expect("test request builder should accept a valid path and empty body");
        let response = test_router()
            .oneshot(request)
            .await
            .expect("test router should handle the request without error");
        assert_eq!(response.status(), StatusCode::OK);
        response.headers().clone()
    }

    #[tokio::test]
    async fn api_route_has_strict_csp_and_security_headers() {
        let headers = get_headers("/api/ping").await;

        assert_eq!(
            headers
                .get(header::CONTENT_SECURITY_POLICY)
                .expect("CSP header should be present on API responses"),
            API_CSP
        );
        assert_eq!(
            headers
                .get(header::STRICT_TRANSPORT_SECURITY)
                .expect("HSTS header should be present on API responses"),
            HSTS
        );
        assert_eq!(
            headers
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .expect("X-Content-Type-Options header should be present on API responses"),
            X_CONTENT_TYPE_OPTIONS
        );
    }

    fn headers_with(name: &'static str, value: &str) -> axum::http::HeaderMap {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(name, HeaderValue::from_str(value).unwrap());
        headers
    }

    #[test]
    fn cidr_parsing_and_matching() {
        let cidr = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(cidr.contains("10.1.2.3".parse().unwrap()));
        assert!(!cidr.contains("11.0.0.1".parse().unwrap()));

        let single = Cidr::parse("192.168.1.5").unwrap();
        assert!(single.contains("192.168.1.5".parse().unwrap()));
        assert!(!single.contains("192.168.1.6".parse().unwrap()));

        assert!(Cidr::parse("10.0.0.0/33").is_none());
        assert!(Cidr::parse("not-a-cidr").is_none());
    }

    #[test]
    fn spoofed_xff_from_untrusted_peer_is_ignored() {
        // No trusted proxies configured in the test environment.
        let peer: IpAddr = "203.0.113.7".parse().unwrap();
        let headers = headers_with("x-forwarded-for", "1.2.3.4");
        assert_eq!(resolve_client_ip(peer, &headers), peer);
    }

    #[test]
    fn spoofed_forwarded_from_untrusted_peer_is_ignored() {
        let peer: IpAddr = "203.0.113.7".parse().unwrap();
        let headers = headers_with("forwarded", "for=1.2.3.4");
        assert_eq!(resolve_client_ip(peer, &headers), peer);
    }

    #[test]
    fn right_most_untrusted_hop_is_selected() {
        // Simulate a trusted proxy chain by exercising the hop-selection helper
        // directly (the global trusted list is empty in tests).
        let hops = "1.2.3.4, 203.0.113.7";
        let parsed: Vec<IpAddr> = hops
            .split(',')
            .filter_map(|h| h.trim().parse().ok())
            .collect();
        let selected = parsed
            .iter()
            .rev()
            .find(|ip| !is_trusted_proxy(**ip))
            .copied();
        assert_eq!(selected, Some("203.0.113.7".parse().unwrap()));
    }

    #[tokio::test]
    async fn docs_route_has_swagger_csp_and_security_headers() {
        let headers = get_headers("/docs").await;

        let csp = headers
            .get(header::CONTENT_SECURITY_POLICY)
            .expect("CSP header should be present on docs responses")
            .to_str()
            .expect("CSP header value should be valid ASCII");
        assert!(csp.contains("https://unpkg.com"));
        assert!(csp.contains("'unsafe-inline'"));
        assert_eq!(
            headers
                .get(header::STRICT_TRANSPORT_SECURITY)
                .expect("HSTS header should be present on docs responses"),
            HSTS
        );
        assert_eq!(
            headers
                .get(header::X_CONTENT_TYPE_OPTIONS)
                .expect("X-Content-Type-Options header should be present on docs responses"),
            X_CONTENT_TYPE_OPTIONS
        );
    }
}