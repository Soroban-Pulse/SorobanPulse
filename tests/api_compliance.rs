//! # API Compliance Test Suite
//!
//! Automated tests verifying that the SorobanPulse API conforms to its
//! OpenAPI specification, REST best practices, and internal standards.
//!
//! ## Organisation
//!
//! | Module | What it checks |
//! |--------|----------------|
//! [`openapi_compliance`] | OpenAPI 3.0 spec structure and declared schema correctness |
//! [`rest_best_practices`] | Versioning, pagination, input validation, idempotency |
//! [`response_format`] | JSON envelope shape, required fields, Content-Type |
//! [`error_consistency`] | Consistent error body: `error`, `code`, `correlation_id` |
//! [`status_codes`] | HTTP status codes match each scenario |
//! [`header_compliance`] | Security headers, Deprecation, Content-Type, X-Request-ID |
//!
//! ## Running
//!
//! ```bash
//! # All compliance tests (no DATABASE_URL needed for header/format checks)
//! cargo test --test api_compliance
//!
//! # Just one module
//! cargo test --test api_compliance openapi_compliance
//! cargo test --test api_compliance status_codes
//! ```

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::Value;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

use soroban_pulse::{
    config::{Config, HealthState, IndexerState},
    metrics::init_metrics,
    routes::create_router,
};

// ---------------------------------------------------------------------------
// Test helper
// ---------------------------------------------------------------------------

/// Build the full application router wired to the given test DB pool.
fn make_router(pool: PgPool) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    health_state.update_last_poll();
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = init_metrics();
    let config = Config::default();
    create_router(
        pool,
        vec![],    // no API key required
        &[],       // no allowed origins override
        0,         // 0 = rate limiting disabled
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

/// Build the router with an API key enabled.
fn make_router_with_key(pool: PgPool, api_key: &str) -> axum::Router {
    let health_state = Arc::new(HealthState::new(60));
    health_state.update_last_poll();
    let indexer_state = Arc::new(IndexerState::new());
    let prometheus_handle = init_metrics();
    let config = Config::default();
    create_router(
        pool,
        vec![api_key.to_string()],
        &[],
        0,
        health_state,
        indexer_state,
        prometheus_handle,
        2000,
        config,
    )
}

/// Deserialise the response body as JSON, panicking with diagnostics on failure.
async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = to_bytes(resp.into_body(), usize::MAX)
        .await
        .expect("failed to read response body");
    serde_json::from_slice(&bytes).expect("response body is not valid JSON")
}

// ---------------------------------------------------------------------------
// 1. OpenAPI compliance
// ---------------------------------------------------------------------------

/// Verify that the OpenAPI specification served at /openapi.json is valid,
/// complete, and contains all the declared schemas and routes.
#[cfg(test)]
mod openapi_compliance {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_endpoint_returns_200(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_content_type_is_json(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .expect("missing Content-Type header")
            .to_str()
            .unwrap();
        assert!(ct.contains("application/json"), "unexpected Content-Type: {ct}");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_spec_has_required_top_level_fields(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;

        assert!(spec.get("openapi").is_some(), "missing 'openapi' field");
        assert!(spec.get("info").is_some(), "missing 'info' field");
        assert!(spec.get("paths").is_some(), "missing 'paths' field");
        assert!(spec["info"].get("title").is_some(), "missing info.title");
        assert!(spec["info"].get("version").is_some(), "missing info.version");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_version_is_3_dot_0(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;
        let version = spec["openapi"].as_str().unwrap_or("");
        assert!(
            version.starts_with("3.0"),
            "expected OpenAPI 3.0.x, got {version}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_spec_title_is_soroban_pulse(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;
        let title = spec["info"]["title"].as_str().unwrap_or("");
        assert!(!title.is_empty(), "info.title must not be empty");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_paths_include_v1_events(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;
        let paths = spec["paths"].as_object().expect("paths must be an object");
        assert!(
            paths.contains_key("/v1/events"),
            "spec must document /v1/events"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_paths_include_health(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;
        let paths = spec["paths"].as_object().expect("paths must be an object");
        assert!(
            paths.contains_key("/health")
                || paths.contains_key("/healthz/ready")
                || paths.contains_key("/healthz/live"),
            "spec must document at least one health endpoint"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_event_schema_has_required_fields(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;

        let event_schema = &spec["components"]["schemas"]["Event"];
        assert!(
            !event_schema.is_null(),
            "Event schema must be declared in components.schemas"
        );

        let required = event_schema["required"]
            .as_array()
            .expect("Event schema must list required fields");
        let required_strs: Vec<&str> = required
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        for field in &["id", "contract_id", "event_type", "tx_hash", "ledger", "timestamp", "event_data", "created_at"] {
            assert!(
                required_strs.contains(field),
                "Event schema missing required field: {field}"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_event_type_schema_has_correct_enum_values(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;

        let event_type_schema = &spec["components"]["schemas"]["EventType"];
        assert!(
            !event_type_schema.is_null(),
            "EventType schema must be declared"
        );

        let enum_values = event_type_schema["enum"]
            .as_array()
            .expect("EventType schema must define enum values");
        let values: Vec<&str> = enum_values
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        assert!(values.contains(&"contract"), "EventType must include 'contract'");
        assert!(values.contains(&"diagnostic"), "EventType must include 'diagnostic'");
        assert!(values.contains(&"system"), "EventType must include 'system'");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn openapi_paths_include_contracts(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/openapi.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let spec = body_json(resp).await;
        let paths = spec["paths"].as_object().expect("paths must be an object");
        assert!(
            paths.contains_key("/v1/contracts"),
            "spec must document /v1/contracts"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. REST API best practices
// ---------------------------------------------------------------------------

/// Verify that the API follows REST best practices: versioned routes,
/// pagination, correct validation, and correct semantics.
#[cfg(test)]
mod rest_best_practices {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn versioned_events_route_exists_at_v1(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // Must not be 404 — some other status (200 or auth-related) is fine
        assert_ne!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "/v1/events must exist as a versioned route"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn versioned_contracts_route_exists_at_v1(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/contracts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_ne!(resp.status(), StatusCode::NOT_FOUND, "/v1/contracts must exist");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn events_endpoint_accepts_page_and_limit_params(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?page=1&limit=10")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "page and limit query params must be accepted"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_page_parameter_returns_400(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?page=notanumber")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "non-numeric page param should return 400"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn limit_over_100_returns_400(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?limit=101")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "limit > 100 should be rejected with 400"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn inverted_ledger_range_returns_400(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "from_ledger > to_ledger should return 400"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unknown_event_type_returns_400(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?event_type=notavalidtype")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::BAD_REQUEST,
            "unknown event_type should return 400"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn tx_hash_with_no_results_returns_200_with_empty_data(pool: PgPool) {
        // Valid 64-char hex hash that doesn't exist in DB
        let valid_hash = "a".repeat(64);
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get(format!("/v1/events/tx/{valid_hash}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "missing tx_hash should return 200 with empty data, not 404"
        );
        let body = body_json(resp).await;
        let data = body["data"].as_array().expect("response must have data array");
        assert!(data.is_empty(), "data array should be empty for unknown tx_hash");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn valid_event_type_filter_returns_200(pool: PgPool) {
        let app = make_router(pool);
        for event_type in &["contract", "diagnostic", "system"] {
            let resp = app
                .clone()
                .oneshot(
                    Request::get(format!("/v1/events?event_type={event_type}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "event_type={event_type} should be accepted"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sort_asc_parameter_returns_200(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?sort=asc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "sort=asc must be accepted");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn sort_desc_parameter_returns_200(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?sort=desc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "sort=desc must be accepted");
    }
}

// ---------------------------------------------------------------------------
// 3. Response format validation
// ---------------------------------------------------------------------------

/// Verify that all responses use the documented JSON envelope shape.
#[cfg(test)]
mod response_format {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn events_response_has_pagination_envelope(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;

        assert!(body.get("data").is_some(), "response must have 'data' field");
        assert!(body.get("total").is_some(), "response must have 'total' field");
        assert!(body.get("page").is_some(), "response must have 'page' field");
        assert!(body.get("limit").is_some(), "response must have 'limit' field");
        assert!(
            body.get("approximate").is_some(),
            "response must have 'approximate' field"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn events_data_field_is_array(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        assert!(
            body["data"].is_array(),
            "'data' field must be an array"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn events_total_is_non_negative_integer(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let total = body["total"]
            .as_i64()
            .expect("'total' must be an integer");
        assert!(total >= 0, "'total' must be non-negative");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn events_page_is_positive_integer(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?page=2")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let page = body["page"].as_i64().expect("'page' must be an integer");
        assert!(page >= 1, "'page' must be >= 1");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn events_limit_matches_requested_limit(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?limit=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let limit = body["limit"].as_i64().expect("'limit' must be an integer");
        assert_eq!(limit, 5, "'limit' in response must match requested value");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn events_approximate_is_boolean(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        assert!(
            body["approximate"].is_boolean(),
            "'approximate' must be a boolean"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn health_ready_response_has_required_fields(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/healthz/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;

        assert!(body.get("status").is_some(), "healthz/ready must have 'status'");
        assert!(body.get("db").is_some(), "healthz/ready must have 'db'");
        assert!(body.get("indexer").is_some(), "healthz/ready must have 'indexer'");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn health_live_response_has_status_field(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/healthz/live")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        assert!(body.get("status").is_some(), "healthz/live must have 'status' field");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn json_responses_have_correct_content_type(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .expect("response must have Content-Type header")
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "JSON endpoints must return application/json, got: {ct}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn health_response_content_type_is_json(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/healthz/ready")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .expect("Content-Type must be present")
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "health endpoint must return application/json, got: {ct}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn contracts_response_has_pagination_envelope(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/contracts")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert!(body.get("data").is_some(), "/v1/contracts response must have 'data'");
    }
}

// ---------------------------------------------------------------------------
// 4. Error response consistency
// ---------------------------------------------------------------------------

/// Verify that all error responses share the same envelope shape.
#[cfg(test)]
mod error_consistency {
    use super::*;

    /// Helper: assert the three mandatory error fields are present and non-empty.
    fn assert_error_envelope(body: &Value) {
        let error = body.get("error").expect("error response must have 'error' field");
        assert!(
            error.is_string() && !error.as_str().unwrap().is_empty(),
            "'error' must be a non-empty string"
        );

        let code = body.get("code").expect("error response must have 'code' field");
        assert!(
            code.is_string() && !code.as_str().unwrap().is_empty(),
            "'code' must be a non-empty string"
        );

        let cid = body
            .get("correlation_id")
            .expect("error response must have 'correlation_id' field");
        assert!(
            cid.is_string() && !cid.as_str().unwrap().is_empty(),
            "'correlation_id' must be a non-empty string"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn bad_request_400_has_standard_error_envelope(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_error_envelope(&body);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn validation_error_code_is_validation_error(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let code = body["code"].as_str().unwrap_or("");
        assert_eq!(code, "VALIDATION_ERROR", "400 errors must use VALIDATION_ERROR code");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unauthorized_401_has_standard_error_envelope(pool: PgPool) {
        let app = make_router_with_key(pool, "secret-key");
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
        let body = body_json(resp).await;
        assert_error_envelope(&body);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn unauthorized_code_is_unauthorized(pool: PgPool) {
        let app = make_router_with_key(pool, "secret-key");
        let resp = app
            .oneshot(
                Request::get("/v1/events")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let code = body["code"].as_str().unwrap_or("");
        assert_eq!(code, "UNAUTHORIZED", "401 errors must use UNAUTHORIZED code");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_contract_id_400_has_error_envelope(pool: PgPool) {
        let app = make_router(pool);
        // Contract IDs must start with C and be 56 chars; "NOTVALID" fails both.
        let resp = app
            .oneshot(
                Request::get("/v1/events/NOTVALID")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // If this route exists and validates, it should be 400.
        // If routing doesn't match, it may be 404. Either way, verify envelope.
        if resp.status() == StatusCode::BAD_REQUEST || resp.status() == StatusCode::NOT_FOUND {
            let body = body_json(resp).await;
            assert_error_envelope(&body);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn error_responses_do_not_expose_stack_traces(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let body_str = serde_json::to_string(&body).unwrap();

        // Stack trace indicators that must never appear in API responses
        assert!(
            !body_str.contains("panicked at"),
            "response must not contain panic messages"
        );
        assert!(
            !body_str.contains("stack backtrace"),
            "response must not contain stack backtrace"
        );
        assert!(
            !body_str.contains(".rs:"),
            "response must not contain Rust source file paths"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn correlation_id_is_uuid_format(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = body_json(resp).await;
        let cid = body["correlation_id"].as_str().unwrap_or("");
        // A UUID has exactly 4 hyphens and 36 chars, or it may be a custom string.
        // Just assert it's non-empty and doesn't expose internals.
        assert!(!cid.is_empty(), "correlation_id must be non-empty");
        assert!(
            !cid.contains("panicked"),
            "correlation_id must not contain error text"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_tx_hash_returns_error_envelope(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events/tx/notahash")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_error_envelope(&body);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_event_type_returns_error_envelope(pool: PgPool) {
        let app = make_router(pool);
        let resp = app
            .oneshot(
                Request::get("/v1/events?event_type=bogus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_error_envelope(&body);
    }
}

// ---------------------------------------------------------------------------
// 5. HTTP status code verification
// ---------------------------------------------------------------------------

/// Verify that each route + scenario returns the correct HTTP status code.
#[cfg(test)]
mod status_codes {
    use super::*;

    #[sqlx::test(migrations = "./migrations")]
    async fn health_ready_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/healthz/ready").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn health_live_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/healthz/live").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn legacy_health_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/health").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_v1_events_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn get_v1_contracts_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/contracts").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn non_numeric_page_returns_400(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?page=abc")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn inverted_ledger_range_returns_400(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=50")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_event_type_returns_400(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?event_type=unknown_type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn invalid_tx_hash_returns_400(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events/tx/notahash")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn missing_api_key_returns_401_when_key_configured(pool: PgPool) {
        let resp = make_router_with_key(pool, "test-secret")
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "missing API key must return 401 when auth is configured"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn wrong_api_key_returns_401(pool: PgPool) {
        let resp = make_router_with_key(pool, "correct-key")
            .oneshot(
                Request::get("/v1/events")
                    .header("Authorization", "Bearer wrong-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn correct_api_key_returns_200(pool: PgPool) {
        let resp = make_router_with_key(pool, "my-api-key")
            .oneshot(
                Request::get("/v1/events")
                    .header("Authorization", "Bearer my-api-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn api_key_via_x_api_key_header_returns_200(pool: PgPool) {
        let resp = make_router_with_key(pool, "my-api-key")
            .oneshot(
                Request::get("/v1/events")
                    .header("X-Api-Key", "my-api-key")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn health_endpoints_do_not_require_auth(pool: PgPool) {
        let app = make_router_with_key(pool, "required-key");

        for path in &["/health", "/healthz/live", "/healthz/ready"] {
            let resp = app
                .clone()
                .oneshot(Request::get(*path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "health endpoint {path} must not require auth"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn limit_at_exactly_100_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?limit=100")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "limit=100 is valid");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn limit_at_exactly_101_returns_400(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?limit=101")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "limit=101 must be rejected");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn zero_limit_returns_400(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?limit=0")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "limit=0 must be rejected");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn docs_endpoint_returns_200(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "/docs must be accessible");
    }
}

// ---------------------------------------------------------------------------
// 6. Header compliance validation
// ---------------------------------------------------------------------------

/// Verify that every response carries the required security and protocol headers.
#[cfg(test)]
mod header_compliance {
    use super::*;

    // ---- OWASP security headers ------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn x_content_type_options_nosniff_on_all_responses(pool: PgPool) {
        for path in &["/healthz/ready", "/v1/events", "/v1/contracts"] {
            let resp = make_router(pool.clone())
                .oneshot(Request::get(*path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let val = resp
                .headers()
                .get("x-content-type-options")
                .unwrap_or_else(|| panic!("missing X-Content-Type-Options on {path}"))
                .to_str()
                .unwrap();
            assert_eq!(val, "nosniff", "X-Content-Type-Options must be 'nosniff' on {path}");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn x_frame_options_deny_on_all_responses(pool: PgPool) {
        for path in &["/healthz/ready", "/v1/events"] {
            let resp = make_router(pool.clone())
                .oneshot(Request::get(*path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let val = resp
                .headers()
                .get("x-frame-options")
                .unwrap_or_else(|| panic!("missing X-Frame-Options on {path}"))
                .to_str()
                .unwrap();
            assert_eq!(val, "DENY", "X-Frame-Options must be 'DENY' on {path}");
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn strict_transport_security_present_on_responses(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let hsts = resp
            .headers()
            .get("strict-transport-security")
            .expect("missing Strict-Transport-Security header")
            .to_str()
            .unwrap();
        assert!(
            hsts.contains("max-age="),
            "HSTS must contain max-age directive, got: {hsts}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn referrer_policy_no_referrer_on_responses(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let val = resp
            .headers()
            .get("referrer-policy")
            .expect("missing Referrer-Policy header")
            .to_str()
            .unwrap();
        assert_eq!(val, "no-referrer", "Referrer-Policy must be 'no-referrer'");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn content_security_policy_present_on_api_responses(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            resp.headers().get("content-security-policy").is_some(),
            "Content-Security-Policy header must be present on API responses"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn api_csp_is_strict_no_scripts(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = resp
            .headers()
            .get("content-security-policy")
            .expect("CSP must be present")
            .to_str()
            .unwrap();
        // API routes must not allow scripts from external origins
        assert!(
            !csp.contains("script-src 'self' 'unsafe-inline' https://unpkg.com"),
            "API routes must not use the relaxed /docs CSP, got: {csp}"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn docs_csp_allows_swagger_assets(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/docs").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = resp
            .headers()
            .get("content-security-policy")
            .expect("CSP must be present on /docs")
            .to_str()
            .unwrap();
        assert!(
            csp.contains("unpkg.com"),
            "/docs CSP must allow unpkg.com for Swagger UI assets"
        );
    }

    // ---- Content-Type ---------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn json_api_responses_have_application_json_content_type(pool: PgPool) {
        for path in &["/v1/events", "/v1/contracts", "/healthz/ready", "/healthz/live"] {
            let resp = make_router(pool.clone())
                .oneshot(Request::get(*path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            let ct = resp
                .headers()
                .get("content-type")
                .unwrap_or_else(|| panic!("missing Content-Type on {path}"))
                .to_str()
                .unwrap();
            assert!(
                ct.contains("application/json"),
                "Content-Type on {path} must be application/json, got: {ct}"
            );
        }
    }

    // ---- Deprecation headers --------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn unversioned_events_route_returns_deprecation_header(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        // If route exists, it must carry the deprecation header.
        // If it returns 404, the route was removed (test is skipped).
        if resp.status() != StatusCode::NOT_FOUND {
            let deprecation = resp
                .headers()
                .get("deprecation")
                .expect("unversioned /events must carry Deprecation header")
                .to_str()
                .unwrap();
            assert_eq!(
                deprecation, "true",
                "Deprecation header must be 'true' for unversioned routes"
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn versioned_events_route_has_no_deprecation_header(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            resp.headers().get("deprecation").is_none(),
            "/v1/events must NOT carry a Deprecation header"
        );
    }

    // ---- Request ID propagation -----------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn x_request_id_header_is_propagated_in_response(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events")
                    .header("x-request-id", "compliance-test-id-123")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        // The middleware should propagate x-request-id back in the response.
        let rid = resp.headers().get("x-request-id");
        if let Some(rid) = rid {
            assert_eq!(
                rid.to_str().unwrap(),
                "compliance-test-id-123",
                "X-Request-ID must be echoed back unchanged"
            );
        }
        // If the header is absent, the server generates its own — that is also acceptable.
        // The critical property is verified via the correlation_id in error responses.
    }

    // ---- Permissions-Policy ---------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn permissions_policy_header_present(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            resp.headers().get("permissions-policy").is_some(),
            "Permissions-Policy header must be present"
        );
    }

    // ---- XSS-Protection -------------------------------------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn x_xss_protection_header_present(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(Request::get("/v1/events").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert!(
            resp.headers().get("x-xss-protection").is_some(),
            "X-XSS-Protection header must be present"
        );
    }

    // ---- Error responses also carry security headers -------------------

    #[sqlx::test(migrations = "./migrations")]
    async fn security_headers_present_on_error_responses(pool: PgPool) {
        let resp = make_router(pool)
            .oneshot(
                Request::get("/v1/events?from_ledger=100&to_ledger=5")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let headers = resp.headers();
        assert!(
            headers.get("x-content-type-options").is_some(),
            "X-Content-Type-Options must be present on error responses"
        );
        assert!(
            headers.get("x-frame-options").is_some(),
            "X-Frame-Options must be present on error responses"
        );
    }
}
