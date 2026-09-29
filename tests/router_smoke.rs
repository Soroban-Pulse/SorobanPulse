//! Router smoke test (issue #1151).
//!
//! Builds the router for every configuration variant and checks that:
//! - building does not panic (catches axum 0.8 `:param` path syntax),
//! - every registered route is reachable (no router-level 404),
//! - the sorted route list matches the insta snapshot,
//! - every route is documented in openapi.json or allowlisted.
//!
//! Cargo features do not change the route table (routes.rs has no
//! `cfg(feature)` gates), so variants cover runtime configuration only.

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use metrics_exporter_prometheus::PrometheusHandle;
use regex::Regex;
use secrecy::SecretString;
use sqlx::PgPool;
use std::collections::{BTreeSet, HashMap};
use std::net::SocketAddr;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::broadcast;
use tower::ServiceExt;

use soroban_pulse::config::{Config, HealthState, IndexerState};
use soroban_pulse::metrics::init_metrics;
use soroban_pulse::middleware::hash_api_key;
use soroban_pulse::routes::create_router_with_tx_and_tenant_map;
use soroban_pulse::sse_ring_buffer::SseRingBuffer;

const API_KEY: &str = "smoke-api-key";
const ADMIN_KEY: &str = "smoke-admin-key";
const FALLBACK_MARKER: &str = "x-router-smoke-fallback";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

// === Variants

struct Variant {
    name: &'static str,
    api_keys: Vec<String>,
    admin_keys: Vec<String>,
    behind_proxy: bool,
    multi_tenant: bool,
}

fn variants() -> Vec<Variant> {
    vec![
        Variant {
            name: "default",
            api_keys: vec![],
            admin_keys: vec![],
            behind_proxy: false,
            multi_tenant: false,
        },
        Variant {
            name: "auth",
            api_keys: vec![API_KEY.to_string()],
            admin_keys: vec![ADMIN_KEY.to_string()],
            behind_proxy: false,
            multi_tenant: false,
        },
        Variant {
            name: "behind_proxy",
            api_keys: vec![],
            admin_keys: vec![],
            behind_proxy: true,
            multi_tenant: false,
        },
        Variant {
            name: "multi_tenant",
            api_keys: vec![API_KEY.to_string()],
            admin_keys: vec![ADMIN_KEY.to_string()],
            behind_proxy: false,
            multi_tenant: true,
        },
    ]
}

// init_metrics installs a global recorder and panics if called twice.
fn prometheus_handle() -> PrometheusHandle {
    static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();
    HANDLE.get_or_init(init_metrics).clone()
}

fn build_router(pool: PgPool, variant: &Variant) -> Router {
    let mut config = Config::default();
    config.multi_tenant = variant.multi_tenant;
    config.admin_api_keys = variant
        .admin_keys
        .iter()
        .map(|k| SecretString::new(k.clone()))
        .collect();

    let tenant_map: HashMap<String, String> = variant
        .api_keys
        .iter()
        .map(|k| (hash_api_key(k), "smoke-tenant".to_string()))
        .collect();

    let health_state = Arc::new(HealthState::new(60));
    health_state.update_last_poll();
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let ring_buffer = SseRingBuffer::new(config.sse_ring_buffer_capacity);

    create_router_with_tx_and_tenant_map(
        pool.clone(),
        pool,
        variant.api_keys.clone(),
        &[],
        // High enough that probing every route never trips the limiter.
        100_000,
        variant.behind_proxy,
        health_state,
        Arc::new(IndexerState::new()),
        prometheus_handle(),
        broadcast::channel(256).0,
        15000,
        1000,
        15000,
        None,
        None,
        config,
        None,
        Arc::new(tenant_map),
        shutdown_rx,
        ring_buffer,
    )
}

fn build_router_or_fail(pool: PgPool, variant: &Variant) -> Router {
    catch_unwind(AssertUnwindSafe(|| build_router(pool, variant))).unwrap_or_else(|panic| {
        let msg = panic
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| panic.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic>");
        panic!("building the `{}` router panicked: {msg}", variant.name)
    })
}

// === Route listing

// axum has no public route listing API, so read the paths from the router's
// Debug output. Only the `path_router` section is used, so the internal
// fallback routes are excluded.
fn registered_routes(router: &Router) -> BTreeSet<String> {
    let debug = format!("{router:?}");
    let start = debug.find("path_router").expect(
        "axum Router Debug output no longer contains `path_router`; update registered_routes",
    );
    let section = &debug[start..];
    let end = ["fallback_router", "default_fallback", "catch_all_fallback"]
        .iter()
        .filter_map(|marker| section.find(marker))
        .min()
        .unwrap_or(section.len());

    let path_re = Regex::new(r#"RouteId\(\d+\): "([^"]*)""#).unwrap();
    let routes: BTreeSet<String> = path_re
        .captures_iter(&section[..end])
        .map(|c| c[1].to_string())
        .filter(|p| !p.contains("__private__axum"))
        .collect();

    assert!(
        !routes.is_empty(),
        "no routes parsed from the axum Router Debug output; update registered_routes"
    );
    routes
}

fn concrete_path(route: &str) -> String {
    let param_re = Regex::new(r"\{\*?[^}]+\}").unwrap();
    param_re.replace_all(route, "smoke").into_owned()
}

fn normalize_params(route: &str) -> String {
    let param_re = Regex::new(r"\{[^}]*\}").unwrap();
    param_re.replace_all(route, "{}").into_owned()
}

// === Reachability

// Marks router-level 404s so they can be told apart from handlers that
// legitimately return 404 for an unknown resource.
fn with_marked_fallback(router: Router) -> Router {
    router.fallback(|| async { (StatusCode::NOT_FOUND, [(FALLBACK_MARKER, "1")]) })
}

async fn assert_routes_reachable(router: Router, routes: &BTreeSet<String>, variant: &Variant) {
    let router = with_marked_fallback(router);
    let key = if variant.admin_keys.is_empty() {
        None
    } else {
        Some(ADMIN_KEY)
    };

    let mut unreachable: Vec<String> = Vec::new();
    for route in routes {
        let path = concrete_path(route);
        let mut builder = Request::builder().uri(&path);
        if let Some(key) = key {
            builder = builder.header("Authorization", format!("Bearer {key}"));
        }
        let mut req = builder.body(Body::empty()).unwrap();
        req.extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 40000))));

        // A timeout means a handler is running (e.g. a stream), which is fine:
        // the fallback always answers immediately.
        let Ok(resp) = tokio::time::timeout(REQUEST_TIMEOUT, router.clone().oneshot(req)).await
        else {
            continue;
        };
        let resp = resp.expect("router service is infallible");
        if resp.headers().contains_key(FALLBACK_MARKER) {
            unreachable.push(format!("{route} (requested {path})"));
        }
    }

    assert!(
        unreachable.is_empty(),
        "`{}` router returned a router-level 404 for registered routes:\n{}",
        variant.name,
        unreachable.join("\n")
    );
}

// === OpenAPI coverage

fn openapi_paths() -> BTreeSet<String> {
    let raw = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/openapi.json"))
        .expect("read openapi.json");
    let spec: serde_json::Value = serde_json::from_str(&raw).expect("parse openapi.json");
    spec["paths"]
        .as_object()
        .expect("openapi.json has a `paths` object")
        .keys()
        .map(|p| normalize_params(p))
        .collect()
}

fn openapi_allowlist() -> BTreeSet<String> {
    include_str!("router_smoke_openapi_allowlist.txt")
        .lines()
        .map(|l| l.split('#').next().unwrap_or("").trim())
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn assert_routes_documented(routes: &BTreeSet<String>) {
    let documented = openapi_paths();
    let allowlist = openapi_allowlist();

    let undocumented: Vec<&String> = routes
        .iter()
        .filter(|r| !documented.contains(&normalize_params(r)) && !allowlist.contains(*r))
        .collect();
    let stale: Vec<&String> = allowlist.iter().filter(|r| !routes.contains(*r)).collect();

    assert!(
        undocumented.is_empty(),
        "routes missing from openapi.json; document them or add them to \
         tests/router_smoke_openapi_allowlist.txt:\n{}",
        undocumented.iter().map(|r| r.as_str()).collect::<Vec<_>>().join("\n")
    );
    assert!(
        stale.is_empty(),
        "tests/router_smoke_openapi_allowlist.txt lists routes that are no longer registered:\n{}",
        stale.iter().map(|r| r.as_str()).collect::<Vec<_>>().join("\n")
    );
}

// === Tests

#[sqlx::test(migrations = "./migrations")]
async fn every_router_variant_builds_and_serves_its_routes(pool: PgPool) {
    let mut baseline: Option<BTreeSet<String>> = None;

    for variant in variants() {
        let router = build_router_or_fail(pool.clone(), &variant);
        let routes = registered_routes(&router);

        match &baseline {
            None => {
                insta::assert_snapshot!(
                    "routes",
                    routes.iter().cloned().collect::<Vec<_>>().join("\n")
                );
                assert_routes_documented(&routes);
                baseline = Some(routes.clone());
            }
            Some(expected) => assert_eq!(
                &routes, expected,
                "`{}` router registers a different route set than `default`",
                variant.name
            ),
        }

        assert_routes_reachable(router, &routes, &variant).await;
    }
}
