# API Compliance Testing

This document describes SorobanPulse's automated API compliance testing
strategy: what is checked, why it matters, how to run the tests, and how to
extend them.

## Table of contents

- [Overview](#overview)
- [Test modules](#test-modules)
  - [OpenAPI compliance](#1-openapi-compliance)
  - [REST best practices](#2-rest-best-practices)
  - [Response format validation](#3-response-format-validation)
  - [Error response consistency](#4-error-response-consistency)
  - [HTTP status code verification](#5-http-status-code-verification)
  - [Header compliance](#6-header-compliance)
- [Running the tests](#running-the-tests)
- [Test infrastructure](#test-infrastructure)
- [Coverage summary](#coverage-summary)
- [Standards reference](#standards-reference)
- [Extending the suite](#extending-the-suite)
- [CI integration](#ci-integration)

---

## Overview

The compliance test suite (`tests/api_compliance.rs`) is a dedicated integration
test harness that verifies the SorobanPulse REST API conforms to:

1. Its own OpenAPI 3.0 specification (served at `/openapi.json`)
2. REST best-practice conventions (versioning, pagination, input validation)
3. The documented JSON envelope format for both success and error responses
4. Consistent error body shape across all failure scenarios
5. Correct HTTP status codes for every request/scenario combination
6. OWASP-recommended security response headers on all HTTP responses

Each area maps to a dedicated module inside the test file, making it easy to
run targeted checks during development.

---

## Test modules

### 1. OpenAPI compliance

**Module:** `openapi_compliance`

Verifies that the OpenAPI specification exposed by the live server is well-formed
and internally consistent.

| Test | What it checks |
|------|----------------|
| `openapi_endpoint_returns_200` | `/openapi.json` responds with 200 OK |
| `openapi_content_type_is_json` | Response carries `Content-Type: application/json` |
| `openapi_spec_has_required_top_level_fields` | `openapi`, `info`, `paths` fields are present |
| `openapi_version_is_3_dot_0` | Version string starts with `3.0` |
| `openapi_spec_title_is_soroban_pulse` | `info.title` is non-empty |
| `openapi_paths_include_v1_events` | `/v1/events` is documented in `paths` |
| `openapi_paths_include_health` | At least one health endpoint is documented |
| `openapi_event_schema_has_required_fields` | `Event` schema lists `id`, `contract_id`, `event_type`, `tx_hash`, `ledger`, `timestamp`, `event_data`, `created_at` as required |
| `openapi_event_type_schema_has_correct_enum_values` | `EventType` enum declares `contract`, `diagnostic`, `system` |
| `openapi_paths_include_contracts` | `/v1/contracts` is documented in `paths` |

**Why it matters:** Clients and SDK generators consume `/openapi.json` to build
their implementations. A malformed or incomplete spec causes client-side
breakage that is hard to diagnose.

---

### 2. REST best practices

**Module:** `rest_best_practices`

Verifies naming conventions, API versioning, input validation, and correct
resource semantics.

| Test | What it checks |
|------|----------------|
| `versioned_events_route_exists_at_v1` | `/v1/events` is reachable (not 404) |
| `versioned_contracts_route_exists_at_v1` | `/v1/contracts` is reachable |
| `events_endpoint_accepts_page_and_limit_params` | `?page=1&limit=10` returns 200 |
| `invalid_page_parameter_returns_400` | `?page=notanumber` returns 400 |
| `limit_over_100_returns_400` | `?limit=101` returns 400 |
| `inverted_ledger_range_returns_400` | `?from_ledger=100&to_ledger=50` returns 400 |
| `unknown_event_type_returns_400` | `?event_type=notavalidtype` returns 400 |
| `tx_hash_with_no_results_returns_200_with_empty_data` | Unindexed tx_hash returns 200 with `data: []`, not 404 |
| `valid_event_type_filter_returns_200` | `contract`, `diagnostic`, `system` all accepted |
| `sort_asc_parameter_returns_200` | `?sort=asc` accepted |
| `sort_desc_parameter_returns_200` | `?sort=desc` accepted |

**Key invariants enforced:**

- All production routes live under `/v1/`.
- Pagination is bounded: 1 ≤ `limit` ≤ 100.
- Ledger ranges must be non-inverted (`from_ledger` ≤ `to_ledger`).
- Unknown filter values are rejected eagerly; the API never silently ignores
  unknown parameters.
- An absent resource is not the same as a missing route — tx_hash queries
  return an empty list when nothing has been indexed, not 404.

---

### 3. Response format validation

**Module:** `response_format`

Verifies that success responses use the documented JSON envelope shape.

#### Pagination envelope (`/v1/events`, `/v1/contracts`)

All list endpoints return:

```json
{
  "data":        [...],
  "total":       100,
  "page":        1,
  "limit":       20,
  "approximate": true
}
```

| Test | What it checks |
|------|----------------|
| `events_response_has_pagination_envelope` | `data`, `total`, `page`, `limit`, `approximate` all present |
| `events_data_field_is_array` | `data` is a JSON array |
| `events_total_is_non_negative_integer` | `total` ≥ 0 |
| `events_page_is_positive_integer` | `page` ≥ 1 |
| `events_limit_matches_requested_limit` | `limit` in response equals requested value |
| `events_approximate_is_boolean` | `approximate` is `true` or `false` |
| `contracts_response_has_pagination_envelope` | `/v1/contracts` also uses the envelope |

#### Health endpoints

| Endpoint | Expected shape |
|----------|---------------|
| `/healthz/ready` | `{ "status": "ok", "db": "ok", "indexer": "ok" }` |
| `/healthz/live` | `{ "status": "alive" }` |

| Test | What it checks |
|------|----------------|
| `health_ready_response_has_required_fields` | `status`, `db`, `indexer` all present |
| `health_live_response_has_status_field` | `status` is present |

#### Content-Type

| Test | What it checks |
|------|----------------|
| `json_responses_have_correct_content_type` | All JSON endpoints return `Content-Type: application/json` |
| `health_response_content_type_is_json` | Health endpoints are also JSON |

---

### 4. Error response consistency

**Module:** `error_consistency`

Verifies that every error response uses the same machine-readable envelope.

#### Error envelope shape

```json
{
  "error":          "human-readable description",
  "code":           "MACHINE_READABLE_CODE",
  "correlation_id": "uuid-or-request-id",
  "operation":      "optional_operation_name",
  "entity":         "optional_entity_reference",
  "validation_errors": [
    {
      "instance_path": "/limit",
      "schema_path":   "#/maximum",
      "message":       "value 101 exceeds maximum of 100"
    }
  ]
}
```

The `operation`, `entity`, and `validation_errors` fields are optional and
omitted when not applicable.

#### Machine-readable error codes

| Scenario | `code` |
|----------|--------|
| Invalid query parameters | `VALIDATION_ERROR` |
| Missing / wrong API key | `UNAUTHORIZED` |
| Insufficient privileges | `FORBIDDEN` |
| Resource not found | `NOT_FOUND` |
| DB timeout | `DATABASE_TIMEOUT` |
| DB pool exhausted | `DATABASE_POOL_EXHAUSTED` |
| Rate limited | `SUBSCRIPTION_LIMIT_EXCEEDED` or `UPSTREAM_RATE_LIMITED` |

| Test | What it checks |
|------|----------------|
| `bad_request_400_has_standard_error_envelope` | `error`, `code`, `correlation_id` present on 400 |
| `validation_error_code_is_validation_error` | 400 errors use `VALIDATION_ERROR` code |
| `unauthorized_401_has_standard_error_envelope` | 401 uses the envelope |
| `unauthorized_code_is_unauthorized` | `code` is `UNAUTHORIZED` on 401 |
| `invalid_contract_id_400_has_error_envelope` | Malformed contract ID errors use the envelope |
| `error_responses_do_not_expose_stack_traces` | No `panicked at`, `.rs:`, or backtrace text leaks |
| `correlation_id_is_uuid_format` | `correlation_id` is non-empty and opaque |
| `invalid_tx_hash_returns_error_envelope` | Short tx_hash → 400 with envelope |
| `invalid_event_type_returns_error_envelope` | Unknown event_type → 400 with envelope |

**Security note:** The `error_responses_do_not_expose_stack_traces` test
protects against information leakage. Internal Rust source paths and panic
messages must never reach API consumers.

---

### 5. HTTP status code verification

**Module:** `status_codes`

Verifies that every route + scenario maps to the correct HTTP status code.

| Scenario | Expected status |
|----------|-----------------|
| `GET /healthz/ready` (DB up) | 200 OK |
| `GET /healthz/live` | 200 OK |
| `GET /health` | 200 OK |
| `GET /v1/events` | 200 OK |
| `GET /v1/contracts` | 200 OK |
| `GET /docs` | 200 OK |
| `GET /v1/events?page=abc` | 400 Bad Request |
| `GET /v1/events?from_ledger=100&to_ledger=50` | 400 Bad Request |
| `GET /v1/events?event_type=unknown` | 400 Bad Request |
| `GET /v1/events?limit=0` | 400 Bad Request |
| `GET /v1/events?limit=101` | 400 Bad Request |
| `GET /v1/events?limit=100` | 200 OK (boundary) |
| `GET /v1/events/tx/notahash` | 400 Bad Request |
| `GET /v1/events` (no key, key configured) | 401 Unauthorized |
| `GET /v1/events` (wrong key) | 401 Unauthorized |
| `GET /v1/events` (correct Bearer key) | 200 OK |
| `GET /v1/events` (correct X-Api-Key) | 200 OK |
| `GET /health` (no key, key configured) | 200 OK (exempt) |
| `GET /healthz/live` (no key, key configured) | 200 OK (exempt) |
| `GET /healthz/ready` (no key, key configured) | 200 OK (exempt) |

**Boundary conditions tested:**
- `limit=100` is the maximum allowed value and must return 200.
- `limit=101` exceeds the cap and must return 400.
- `limit=0` is not a valid page size and must return 400.

---

### 6. Header compliance

**Module:** `header_compliance`

Verifies that all HTTP responses carry the full set of OWASP-recommended
security headers and correct protocol headers.

#### Security headers (OWASP)

| Header | Expected value |
|--------|---------------|
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Strict-Transport-Security` | Contains `max-age=` |
| `Referrer-Policy` | `no-referrer` |
| `X-XSS-Protection` | Present |
| `Permissions-Policy` | Present |
| `Content-Security-Policy` | Present; strict `default-src 'none'` on API routes |

The `/docs` route uses a relaxed CSP to allow Swagger UI assets from `unpkg.com`.
All other routes use `default-src 'none'; frame-ancestors 'none';`.

#### Deprecation headers

Unversioned routes (e.g. `/events`, `/events/{contract_id}`) return:

```
Deprecation: true
Link: </v1/events>; rel="successor-version"
```

Versioned routes (`/v1/events`) must **not** carry a `Deprecation` header.

#### Content-Type

All JSON endpoints must return `Content-Type: application/json` (possibly with
a charset suffix).

#### Request ID propagation

When a request includes an `X-Request-ID` header, the server echoes it back
unchanged. This enables end-to-end tracing from client logs to server logs
without requiring distributed trace infrastructure.

| Test | What it checks |
|------|----------------|
| `x_content_type_options_nosniff_on_all_responses` | `nosniff` on health and API routes |
| `x_frame_options_deny_on_all_responses` | `DENY` on multiple routes |
| `strict_transport_security_present_on_responses` | HSTS with `max-age` |
| `referrer_policy_no_referrer_on_responses` | `no-referrer` value |
| `content_security_policy_present_on_api_responses` | CSP present on `/v1/*` |
| `api_csp_is_strict_no_scripts` | API routes don't use the relaxed `/docs` CSP |
| `docs_csp_allows_swagger_assets` | `/docs` CSP allows `unpkg.com` |
| `json_api_responses_have_application_json_content_type` | `application/json` across all JSON endpoints |
| `unversioned_events_route_returns_deprecation_header` | `Deprecation: true` on `/events` |
| `versioned_events_route_has_no_deprecation_header` | No deprecation on `/v1/events` |
| `x_request_id_header_is_propagated_in_response` | Request ID echo |
| `permissions_policy_header_present` | Permissions-Policy is set |
| `x_xss_protection_header_present` | X-XSS-Protection is set |
| `security_headers_present_on_error_responses` | Security headers survive 4xx responses |

---

## Running the tests

### Prerequisites

- A running PostgreSQL instance reachable via `DATABASE_URL`.
- The `sqlx` CLI is **not** required — `sqlx::test` handles schema migration
  automatically using a fresh temporary database for each test.

### Commands

```bash
# Run the full compliance suite
cargo test --test api_compliance

# Run a specific module
cargo test --test api_compliance openapi_compliance
cargo test --test api_compliance rest_best_practices
cargo test --test api_compliance response_format
cargo test --test api_compliance error_consistency
cargo test --test api_compliance status_codes
cargo test --test api_compliance header_compliance

# Run with live output (useful for diagnosing failures)
cargo test --test api_compliance -- --nocapture

# Run via Make
make compliance-tests
```

### Environment variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `RUST_LOG` | No | Set to `debug` for verbose handler output |

---

## Test infrastructure

### Router factory

All tests share two factory functions defined at the top of the test file:

```rust
/// Router without authentication (most tests).
fn make_router(pool: PgPool) -> axum::Router { ... }

/// Router with a single API key configured (auth tests).
fn make_router_with_key(pool: PgPool, api_key: &str) -> axum::Router { ... }
```

Both functions mirror the setup used in `tests/integration_tests.rs`, ensuring
that compliance tests exercise the same middleware stack as production.

### Body helper

```rust
/// Deserialise the response body as JSON.
async fn body_json(resp: axum::response::Response) -> Value { ... }
```

### Database isolation

Each `#[sqlx::test(migrations = "./migrations")]` test function receives a
freshly created, migrated database that is dropped after the test completes.
Tests are fully isolated from one another.

---

## Coverage summary

| Area | Tests | Routes covered |
|------|-------|----------------|
| OpenAPI spec | 10 | `/openapi.json` |
| REST practices | 11 | `/v1/events`, `/v1/contracts`, `/v1/events/tx/{hash}` |
| Response format | 11 | `/v1/events`, `/v1/contracts`, `/healthz/*` |
| Error consistency | 9 | `/v1/events` (400/401 scenarios), `/v1/events/tx/{hash}` |
| Status codes | 16 | All primary routes, all auth scenarios |
| Header compliance | 15 | All routes, error responses, deprecated routes |
| **Total** | **72** | |

---

## Standards reference

| Standard | Description |
|----------|-------------|
| [OpenAPI 3.0](https://spec.openapis.org/oas/v3.0.3) | API specification format |
| [RFC 9110](https://www.rfc-editor.org/rfc/rfc9110) | HTTP Semantics |
| [RFC 8594](https://www.rfc-editor.org/rfc/rfc8594) | `Sunset` and `Deprecation` headers |
| [OWASP Secure Headers](https://owasp.org/www-project-secure-headers/) | Security header requirements |
| [RFC 4122](https://www.rfc-editor.org/rfc/rfc4122) | UUID format for correlation IDs |

---

## Extending the suite

### Adding a new route check

1. Identify the test module that best fits the assertion (e.g. `status_codes`
   for a new route, `response_format` for a new field).
2. Add a `#[sqlx::test(migrations = "./migrations")]` test function.
3. Use `make_router(pool)` to build the router and `tower::ServiceExt::oneshot`
   to send a request.
4. Assert on status, headers, or body as needed.

### Adding a new error code

When a new `AppError` variant is added in `src/error.rs`:

1. Add its `(StatusCode, &'static str)` mapping to `AppError::status_and_code`.
2. Add a test in `error_consistency` that triggers that error and asserts the
   correct `code` value in the response body.
3. Add a test in `status_codes` that asserts the correct HTTP status.

### Adding a new response field

When a new field is added to a response type:

1. Add a test in `response_format` that asserts the new field is present and
   has the correct type.
2. If the field is required by the spec, assert it is in the OpenAPI schema's
   `required` array via an `openapi_compliance` test.

### Checking a new security header

When a new security header is added to the middleware stack
(`src/middleware/security_headers.rs`):

1. Add a `#[sqlx::test]` to `header_compliance` that checks the header value
   on at least two routes (an API route and a health route).

---

## CI integration

The compliance tests run automatically on every pull request as part of the
main CI pipeline (`.github/workflows/ci.yml`). They are grouped under the
`compliance` label so that failures are easy to spot.

To run only compliance tests in CI:

```yaml
- name: API compliance tests
  run: cargo test --test api_compliance
  env:
    DATABASE_URL: postgres://postgres:postgres@localhost/soroban_pulse_test
```

The suite is intentionally separate from the security test suite
(`cargo test --test security`) and the contract test suite
(`cargo test --test contract_tests`) so that failures are scoped and
actionable.
