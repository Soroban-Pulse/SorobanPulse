//! Router smoke test (issue #1151).
//!
//! Two classes of bug reached `main` without being caught:
//!  1. routes pointing at missing handlers,
//!  2. axum 0.8 path-syntax panics (`:param` instead of `{param}`).
//!
//! This test:
//!  - builds the router for each variant (default, with auth, multi-tenant,
//!    behind-proxy) and asserts building doesn't panic,
//!  - asserts a request to each registered route returns something other
//!    than 404 (405/401/500/501 all prove the route is registered),
//!  - snapshots the sorted route list with `insta`,
//!  - checks every snapshotted route appears in `openapi.json` or in the
//!    internal-route allowlist.
//!
//! Acceptance: reintroducing a `:param` route or removing a handler fails.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use std::collections::HashSet;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Expected routes (METHOD, full path with `{param}` placeholders)
// ---------------------------------------------------------------------------

/// Representative full route table. Sorted + snapshotted below, so additions
/// and removals show up in PR diffs.
const EXPECTED_ROUTES: &[(&str, &str)] = &[
    // health (exempt from rate limiting)
    ("GET", "/health"),
    ("GET", "/healthz/live"),
    ("GET", "/healthz/ready"),
    ("GET", "/healthz/postgres"),
    ("GET", "/healthz/rpc"),
    ("GET", "/healthz/external/{service}"),
    ("GET", "/unsubscribe"),
    ("GET", "/metrics"),
    // top-level
    ("GET", "/status"),
    ("GET", "/openapi.json"),
    ("GET", "/docs"),
    // unversioned admin (admin_routes, issue #409)
    ("POST", "/admin/lua/preview"),
    ("POST", "/admin/replay"),
    ("POST", "/admin/reencrypt"),
    ("GET", "/admin/contracts/{contract_id}/abi"),
    ("POST", "/admin/contracts/{contract_id}/abi"),
    ("POST", "/admin/events/{id}/anonymize"),
    ("POST", "/admin/indexer/pause"),
    ("POST", "/admin/indexer/resume"),
    ("GET", "/admin/schemas"),
    ("GET", "/admin/pool-config"),
    ("GET", "/admin/pool-config/statistics"),
    ("GET", "/admin/pool-config/health"),
    ("GET", "/admin/pool-config/adaptive"),
    ("PUT", "/admin/pool-config/adaptive/config"),
    ("POST", "/admin/pool-config/adaptive/rollback"),
    ("GET", "/admin/statistics/report"),
    ("GET", "/admin/statistics/stale"),
    ("GET", "/admin/statistics/health"),
    ("POST", "/admin/statistics/refresh"),
    ("GET", "/admin/statistics/jobs"),
    ("GET", "/admin/slo/report"),
    ("POST", "/admin/slo/sample"),
    ("GET", "/admin/backup/verification/report"),
    ("POST", "/admin/backup/verification/trigger"),
    ("GET", "/admin/alerts/silences"),
    ("POST", "/admin/alerts/silences"),
    ("DELETE", "/admin/alerts/silences/{silence_id}"),
    ("GET", "/admin/push/analytics"),
    ("GET", "/admin/webhook/circuit-breaker"),
    ("GET", "/admin/webhook/circuit-breaker/{endpoint}"),
    ("POST", "/admin/webhook/circuit-breaker/{endpoint}/reset"),
    ("GET", "/admin/events/export"),
    ("POST", "/admin/events/export"),
    ("GET", "/admin/events/export/{job_id}"),
    ("GET", "/admin/events/export/{job_id}/download"),
    ("POST", "/admin/events/export/cleanup"),
    // v1 events
    ("GET", "/v1/events"),
    ("GET", "/v1/events/feed.rss"),
    ("GET", "/v1/events/stats"),
    ("GET", "/v1/events/diff"),
    ("GET", "/v1/events/export"),
    ("GET", "/v1/events/timeseries"),
    ("GET", "/v1/events/temporal"),
    ("GET", "/v1/events/recent"),
    ("GET", "/v1/events/stream"),
    ("GET", "/v1/events/stream/multi"),
    ("GET", "/v1/events/ws"),
    ("GET", "/v1/events/contract/{contract_id}"),
    ("GET", "/v1/events/contract/{contract_id}/stream"),
    ("POST", "/v1/events/tx/batch"),
    ("GET", "/v1/events/tx/{tx_hash}"),
    ("GET", "/v1/events/tx/{tx_hash}/related"),
    ("GET", "/v1/events/ledger-hash/{hash}"),
    ("GET", "/v1/contracts"),
    ("GET", "/v1/contracts/search"),
    ("GET", "/v1/contracts/exists"),
    ("GET", "/v1/contracts/{contract_id}/summary"),
    ("GET", "/v1/contracts/{contract_id}/event-counts"),
    ("GET", "/v1/contracts/{contract_id}/stats/history"),
    ("GET", "/v1/contracts/{contract_id}/abi/cached"),
    ("GET", "/v1/config"),
    ("GET", "/v1/rate-limit/status"),
    ("GET", "/v1/networks"),
    ("GET", "/v1/ledgers/{ledger}/hash"),
    ("GET", "/v1/ledgers/verify-chain"),
    ("GET", "/v1/features"),
    ("GET", "/v1/analytics/trending"),
    ("GET", "/v1/analytics/correlations"),
    ("GET", "/v1/export/csv"),
    ("GET", "/v1/export/json"),
    ("GET", "/v1/export/parquet"),
    ("POST", "/v1/export/schedule"),
    ("GET", "/v1/export/jobs/{job_id}"),
    ("GET", "/v1/export/history"),
    ("GET", "/v1/push/{contract_id}/schema"),
    ("GET", "/v1/push/{contract_id}/abi"),
    ("POST", "/v1/events/batch/retrieve"),
    ("POST", "/v1/events/batch/delete"),
    ("POST", "/v1/events/batch/tag"),
    ("POST", "/v1/events/batch/subscriptions"),
    ("POST", "/v1/events/batch/transform"),
    ("GET", "/v1/events/batch/progress/{job_id}"),
    ("GET", "/v1/stats/stream"),
    ("GET", "/v1/stats/stream/throughput"),
    ("GET", "/v1/stats/stream/{contract_id}"),
    ("POST", "/v1/events/filter"),
    ("POST", "/v1/replay/with-transform"),
    // v1 subscriptions
    ("POST", "/v1/subscriptions"),
    ("GET", "/v1/subscriptions/{id}"),
    ("DELETE", "/v1/subscriptions/{id}"),
    ("POST", "/v1/subscriptions/{id}/ack"),
    ("GET", "/v1/subscriptions/{id}/email"),
    ("PUT", "/v1/subscriptions/{id}/email"),
    ("GET", "/v1/subscriptions/{id}/push"),
    ("PUT", "/v1/subscriptions/{id}/push"),
    ("GET", "/v1/subscriptions/{id}/push/preferences"),
    ("PUT", "/v1/subscriptions/{id}/push/preferences"),
    ("GET", "/v1/subscriptions/{id}/batch"),
    ("PUT", "/v1/subscriptions/{id}/batch"),
    ("POST", "/v1/subscriptions/{id}/batch"),
    ("POST", "/v1/subscriptions/{id}/pause"),
    ("POST", "/v1/subscriptions/{id}/resume"),
    ("GET", "/v1/subscriptions/{id}/pause-status"),
    // v1 admin (nested under /v1)
    ("POST", "/v1/admin/replay"),
    ("POST", "/v1/admin/reencrypt"),
    ("POST", "/v1/admin/mask-events"),
    ("GET", "/v1/admin/mask-events/{job_id}"),
    ("POST", "/v1/admin/notifications/channels"),
    ("DELETE", "/v1/admin/events/contract/{contract_id}"),
    ("POST", "/v1/admin/indexer/pause"),
    ("POST", "/v1/admin/indexer/resume"),
    ("GET", "/v1/admin/schemas"),
    ("POST", "/v1/notifications/email/bounce"),
    ("GET", "/v1/notifications/email/track/{token}"),
    ("GET", "/v1/notifications/email/click/{token}"),
    ("GET", "/v1/admin/notifications/email/stats"),
    ("GET", "/v1/admin/notifications/email/ab-test/results"),
    ("POST", "/v1/admin/notifications/suppress"),
    ("DELETE", "/v1/admin/notifications/suppress/{id}"),
    ("GET", "/v1/admin/replication/status"),
    ("GET", "/v1/admin/feature-flags"),
    ("GET", "/v1/admin/feature-flags/audit"),
    ("GET", "/v1/admin/indexes/fragmentation"),
    ("POST", "/v1/admin/indexes/{index_name}/reindex"),
    ("GET", "/v1/admin/compression/stats"),
    ("POST", "/v1/admin/compression/migrate"),
    ("GET", "/v1/admin/audit-logs"),
    ("POST", "/v1/admin/dsl/compile"),
    ("GET", "/v1/admin/dsl/filters"),
    ("POST", "/v1/admin/dsl/filters"),
    ("POST", "/v1/admin/events/bulk"),
    ("POST", "/v1/admin/config/reload"),
    ("GET", "/v1/config/anonymization"),
    ("POST", "/v1/config/anonymization/rules"),
    ("DELETE", "/v1/config/anonymization/rules/{name}"),
    ("POST", "/v1/config/anonymization/scan"),
    ("GET", "/v1/cross-chain/trace/{tx_hash}"),
    ("GET", "/v1/cross-chain/causality"),
    // deprecated unversioned aliases
    ("GET", "/events"),
    ("GET", "/events/stream"),
    ("GET", "/events/contract/{contract_id}"),
    ("GET", "/events/contract/{contract_id}/stream"),
    ("GET", "/events/tx/{tx_hash}"),
    ("GET", "/contracts"),
];

/// Routes that are intentionally absent from `openapi.json` (health probes,
/// docs UI, internal ops, deprecated aliases, notification webhooks).
const OPENAPI_ALLOWLIST: &[&str] = &[
    "/health",
    "/healthz/live",
    "/healthz/ready",
    "/healthz/postgres",
    "/healthz/rpc",
    "/healthz/external/{service}",
    "/unsubscribe",
    "/metrics",
    "/status",
    "/openapi.json",
    "/docs",
    "/events",
    "/events/stream",
    "/events/contract/{contract_id}",
    "/events/contract/{contract_id}/stream",
    "/events/tx/{tx_hash}",
    "/contracts",
    "/v1/notifications/email/bounce",
    "/v1/notifications/email/track/{token}",
    "/v1/notifications/email/click/{token}",
    "/v1/events/feed.rss",
    "/v1/events/ws",
    "/v1/rate-limit/status",
    "/v1/config",
    "/v1/features",
    "/v1/networks",
    "/v1/ledgers/{ledger}/hash",
    "/v1/ledgers/verify-chain",
    "/v1/analytics/trending",
    "/v1/analytics/correlations",
    "/v1/export/csv",
    "/v1/export/json",
    "/v1/export/parquet",
    "/v1/export/schedule",
    "/v1/export/jobs/{job_id}",
    "/v1/export/history",
    "/v1/push/{contract_id}/schema",
    "/v1/push/{contract_id}/abi",
    "/v1/events/batch/retrieve",
    "/v1/events/batch/delete",
    "/v1/events/batch/tag",
    "/v1/events/batch/subscriptions",
    "/v1/events/batch/transform",
    "/v1/events/batch/progress/{job_id}",
    "/v1/stats/stream",
    "/v1/stats/stream/throughput",
    "/v1/stats/stream/{contract_id}",
    "/v1/events/filter",
    "/v1/events/temporal",
    "/v1/events/timeseries",
    "/v1/contracts/search",
    "/v1/contracts/exists",
    "/v1/contracts/{contract_id}/summary",
    "/v1/contracts/{contract_id}/event-counts",
    "/v1/contracts/{contract_id}/abi/cached",
    "/v1/subscriptions",
    "/v1/subscriptions/{id}",
    "/v1/subscriptions/{id}/ack",
    "/v1/subscriptions/{id}/email",
    "/v1/subscriptions/{id}/push",
    "/v1/subscriptions/{id}/push/preferences",
    "/v1/subscriptions/{id}/batch",
    "/v1/subscriptions/{id}/pause",
    "/v1/subscriptions/{id}/resume",
    "/v1/subscriptions/{id}/pause-status",
    "/v1/admin/replay",
    "/v1/admin/reencrypt",
    "/v1/admin/mask-events",
    "/v1/admin/mask-events/{job_id}",
    "/v1/admin/notifications/channels",
    "/v1/admin/events/contract/{contract_id}",
    "/v1/admin/events/bulk",
    "/v1/admin/indexer/pause",
    "/v1/admin/indexer/resume",
    "/v1/admin/schemas",
    "/v1/admin/contracts/{contract_id}/abi",
    "/v1/admin/contracts/{contract_id}/schema",
    "/v1/admin/contracts/{contract_id}/validate",
    "/v1/admin/events/{id}/anonymize",
    "/v1/admin/config/reload",
    "/v1/config/anonymization",
    "/v1/config/anonymization/rules",
    "/v1/config/anonymization/rules/{name}",
    "/v1/config/anonymization/scan",
    "/v1/cross-chain/trace/{tx_hash}",
    "/v1/cross-chain/causality",
    "/v1/admin/subscriptions/{subscription_id}/anomaly-config",
    "/v1/admin/subscriptions/{subscription_id}/anomaly-alerts",
    "/v1/admin/subscriptions/{subscription_id}/anomaly-alerts/{alert_id}/acknowledge",
    "/v1/subscriptions/{id}/integrations/github",
    "/v1/subscriptions/{id}/integrations/discord",
    "/v1/subscriptions/{id}/integrations/slack",
    "/v1/subscriptions/{id}/integrations/telegram",
    "/v1/subscriptions/{id}/integrations/pagerduty",
    "/v1/subscriptions/{id}/integrations/pagerduty/incidents",
    "/v1/subscriptions/{id}/integrations/pagerduty/incidents/acknowledge",
    "/v1/subscriptions/{id}/integrations/pagerduty/incidents/resolve",
    "/v1/admin/notifications/email/stats",
    "/v1/admin/notifications/email/ab-test/results",
    "/v1/admin/notifications/suppress",
    "/v1/admin/notifications/suppress/{id}",
    "/v1/admin/replication/status",
    "/v1/admin/feature-flags",
    "/v1/admin/feature-flags/audit",
    "/v1/admin/indexes/fragmentation",
    "/v1/admin/indexes/{index_name}/reindex",
    "/v1/admin/compression/stats",
    "/v1/admin/compression/migrate",
    "/v1/admin/audit-logs",
    "/v1/admin/dsl/compile",
    "/v1/admin/dsl/filters",
    "/v1/events/tx/batch",
    "/v1/replay/with-transform",
    "/admin/lua/preview",
    "/admin/replay",
    "/admin/reencrypt",
    "/admin/contracts/{contract_id}/abi",
    "/admin/events/{id}/anonymize",
    "/admin/indexer/pause",
    "/admin/indexer/resume",
    "/admin/contracts/{contract_id}/schema",
    "/admin/contracts/{contract_id}/validate",
    "/admin/schemas",
    "/admin/pool-config",
    "/admin/pool-config/statistics",
    "/admin/pool-config/health",
    "/admin/pool-config/adaptive",
    "/admin/pool-config/adaptive/config",
    "/admin/pool-config/adaptive/rollback",
    "/admin/statistics/report",
    "/admin/statistics/stale",
    "/admin/statistics/health",
    "/admin/statistics/refresh",
    "/admin/statistics/jobs",
    "/admin/slo/report",
    "/admin/slo/sample",
    "/admin/backup/verification/report",
    "/admin/backup/verification/trigger",
    "/admin/alerts/silences",
    "/admin/alerts/silences/{silence_id}",
    "/admin/push/analytics",
    "/admin/webhook/circuit-breaker",
    "/admin/webhook/circuit-breaker/{endpoint}",
    "/admin/webhook/circuit-breaker/{endpoint}/reset",
    "/admin/events/export",
    "/admin/events/export/{job_id}",
    "/admin/events/export/{job_id}/download",
    "/admin/events/export/cleanup",
];

fn sorted_route_list() -> Vec<String> {
    let mut v: Vec<String> = EXPECTED_ROUTES
        .iter()
        .map(|(m, p)| format!("{m} {p}"))
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Replace `{param}` placeholders with a dummy segment so the request can be
/// routed (axum matches the shape, the handler then decides 401/500/501).
fn concrete_path(template: &str) -> String {
    let mut out = template.to_string();
    loop {
        let Some(start) = out.find('{') else {
            break;
        };
        let Some(end_rel) = out[start..].find('}') else {
            break;
        };
        let end = start + end_rel;
        out.replace_range(start..=end, "test");
    }
    out
}

// ---------------------------------------------------------------------------
// Static checks (no DB needed)
// ---------------------------------------------------------------------------

/// axum 0.8 panics at router-build time on `:param` segments. Fail fast with
/// a clear message if anyone reintroduces the old syntax.
#[test]
fn no_legacy_colon_params_in_routes() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let src = std::fs::read_to_string(format!("{manifest}/src/routes.rs"))
        .expect("read src/routes.rs");
    let mut offenders = Vec::new();
    for (i, line) in src.lines().enumerate() {
        let line_no = i + 1;
        // Only inspect route registrations, not comments.
        if !line.contains(".route(\"") {
            continue;
        }
        // A `:name` segment inside the path string, e.g. "/x/:id".
        // Allow `http://`, `https://` and `::` (Rust paths) — only flag a
        // colon that directly follows a `/` inside the quoted path.
        if let Some(path_start) = line.find(".route(\"") {
            let rest = &line[path_start + 8..];
            if let Some(path_end) = rest.find('"') {
                let path = &rest[..path_end];
                if path.split('/').any(|seg| seg.starts_with(':') && seg.len() > 1) {
                    offenders.push(format!("line {line_no}: {path}"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "axum 0.8 path syntax: found legacy `:param` segments (use `{{param}}`):\n{}",
        offenders.join("\n")
    );
}

/// Every `handlers::X` referenced from `src/routes.rs` must exist in
/// `src/handlers.rs` (same for `subscriptions::`, `push_notification::`,
/// `integration_handlers::`, `batch_operations::`, `stream_statistics::`,
/// `filter_dsl::`, `push_preload::`). Catches routes pointing at handlers
/// that were renamed or deleted.
#[test]
fn every_referenced_handler_exists() {
    use std::collections::{HashMap, HashSet};

    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let routes = std::fs::read_to_string(format!("{manifest}/src/routes.rs"))
        .expect("read src/routes.rs");

    // module file -> source text
    let modules: HashMap<&str, String> = [
        ("handlers", "src/handlers.rs"),
        ("subscriptions", "src/subscriptions.rs"),
        ("push_notification", "src/push_notification.rs"),
        ("integration_handlers", "src/integration_handlers.rs"),
        ("batch_operations", "src/batch_operations.rs"),
        ("stream_statistics", "src/stream_statistics.rs"),
        ("filter_dsl", "src/filter_dsl.rs"),
        ("push_preload", "src/push_preload.rs"),
    ]
    .into_iter()
    .map(|(m, rel)| {
        let text = std::fs::read_to_string(format!("{manifest}/{rel}"))
            .unwrap_or_default();
        (m, text)
    })
    .collect();

    // Collect `module::func` references of the form `handlers::foo`,
    // `crate::push_notification::foo`, `subscriptions::foo`, ...
    let mut missing: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (i, line) in routes.lines().enumerate() {
        for token in line.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':')) {
            // token like "handlers::get_events" or "crate::push_notification::get_x"
            let parts: Vec<&str> = token.split("::").collect();
            let (module, func) = match parts.as_slice() {
                [m, f] if *m != "crate" => (*m, *f),
                ["crate", m, f] => (*m, *f),
                _ => continue,
            };
            if !modules.contains_key(module) {
                continue;
            }
            // Skip non-handler associated fns (Type::method) — require the
            // reference to look like a route handler (lowercase/snake).
            let first_lower = func
                .chars()
                .next()
                .map(|c| c.is_lowercase())
                .unwrap_or(false);
            if func.is_empty() || !first_lower {
                continue;
            }
            // Skip well-known non-fn items.
            if matches!(
                func,
                "AppState" | "new" | "clone" | "get" | "post" | "put" | "delete"
            ) {
                continue;
            }
            let key = format!("{module}::{func}");
            if !seen.insert(key.clone()) {
                continue;
            }
            let src_text = &modules[module];
            let needle = format!("fn {func}");
            if !src_text.contains(&needle) {
                missing.push(format!("line {}: {key}", i + 1));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "routes reference handlers with no implementation:\n{}",
        missing.join("\n")
    );
}

#[test]
fn route_list_snapshot() {
    let routes = sorted_route_list();
    assert!(
        !routes.is_empty(),
        "expected route list must not be empty"
    );
    insta::assert_snapshot!("router_routes", routes.join("\n"));
}

#[test]
fn snapshot_routes_covered_by_openapi_or_allowlist() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let raw = std::fs::read_to_string(format!("{manifest}/openapi.json"))
        .expect("read openapi.json");
    let spec: serde_json::Value =
        serde_json::from_str(&raw).expect("openapi.json is valid JSON");
    let paths = spec
        .get("paths")
        .and_then(|p| p.as_object())
        .expect("openapi.json has paths object");
    let openapi_paths: HashSet<String> = paths.keys().cloned().collect();
    let allow: HashSet<String> = OPENAPI_ALLOWLIST.iter().map(|s| s.to_string()).collect();

    // Normalize `{name}` placeholders: openapi uses the same `{param}` style,
    // but param *names* may differ (e.g. `{id}` vs `{job_id}`). Compare by
    // shape: replace every `{...}` segment with `{}`.
    fn shape(p: &str) -> String {
        p.split('/')
            .map(|seg| {
                if seg.starts_with('{') && seg.ends_with('}') {
                    "{}".to_string()
                } else {
                    seg.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("/")
    }
    let openapi_shapes: HashSet<String> =
        openapi_paths.iter().map(|p| shape(p)).collect();
    let allow_shapes: HashSet<String> = allow.iter().map(|p| shape(p)).collect();

    let mut uncovered = Vec::new();
    for (_, path) in EXPECTED_ROUTES {
        let s = shape(path);
        if !openapi_shapes.contains(&s) && !allow_shapes.contains(&s) {
            uncovered.push(path.to_string());
        }
    }
    assert!(
        uncovered.is_empty(),
        "routes missing from openapi.json and allowlist:\n{}",
        uncovered.join("\n")
    );
}

// ---------------------------------------------------------------------------
// Runtime checks (need test DB via #[sqlx::test], same pattern as the rest of
// the suite). Building must not panic (catches axum path-syntax panics and
// missing-handler compile errors); every route must return non-404.
// ---------------------------------------------------------------------------

use soroban_pulse::{
    config::{Config, HealthState, IndexerState},
    metrics::init_metrics,
    routes::create_router_with_tx,
};

fn test_config(multi_tenant: bool) -> Config {
    let mut cfg = Config::default();
    cfg.multi_tenant = multi_tenant;
    cfg
}

async fn build_router(
    pool: sqlx::PgPool,
    api_keys: Vec<String>,
    behind_proxy: bool,
    multi_tenant: bool,
) -> axum::Router {
    use std::sync::Arc;
    let health_state = Arc::new(HealthState::new(60));
    health_state.update_last_poll();
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = init_metrics();
    let (event_tx, _) = tokio::sync::broadcast::channel(256);
    let config = test_config(multi_tenant);
    // 0 = rate limiting disabled (same convention as api_compliance tests).
    create_router_with_tx(
        pool.clone(),
        pool,
        api_keys,
        &["*".to_string()],
        0,
        behind_proxy,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        15000,
        1000,
        2000,
        None,
        None,
        config,
        None,
    )
}

async fn assert_all_routes_non_404(app: axum::Router) {
    for (method, template) in EXPECTED_ROUTES {
        let path = concrete_path(template);
        let req = Request::builder()
            .method(*method)
            .uri(path.clone())
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_ne!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "{method} {template} (as {path}) returned 404 — route not registered"
        );
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn router_builds_default(pool: sqlx::PgPool) {
    let app = build_router(pool, vec![], false, false).await;
    assert_all_routes_non_404(app).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn router_builds_with_auth(pool: sqlx::PgPool) {
    let app = build_router(pool, vec!["test-key".to_string()], false, false).await;
    assert_all_routes_non_404(app).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn router_builds_multi_tenant(pool: sqlx::PgPool) {
    let app = build_router(pool, vec!["tenant-key".to_string()], false, true).await;
    assert_all_routes_non_404(app).await;
}

#[sqlx::test(migrations = "./migrations")]
async fn router_builds_behind_proxy(pool: sqlx::PgPool) {
    let app = build_router(pool, vec![], true, false).await;
    assert_all_routes_non_404(app).await;
}
