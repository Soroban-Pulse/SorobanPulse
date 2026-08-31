# E2E Testing

End-to-end (E2E) tests for SorobanPulse verify the fully integrated system — the running application, a real PostgreSQL database, a mocked Soroban RPC, and a live webhook receiver — behave correctly together. Where unit and integration tests validate individual components in isolation, E2E tests exercise the same paths a real client would take: HTTP requests in, observable side effects out.

## Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Test Categories](#test-categories)
- [Running E2E Tests](#running-e2e-tests)
- [Environment Variables](#environment-variables)
- [CI Integration](#ci-integration)
- [Adding New Tests](#adding-new-tests)
- [Seed Data Reference](#seed-data-reference)
- [Full Test Index](#full-test-index)

---

## Overview

E2E tests exist to catch problems that unit tests cannot: misconfigured middleware, incorrect SQL migrations, broken serialization between layers, and cross-cutting concerns like authentication, rate limiting, and observability. They also serve as executable documentation — each test name describes a concrete user-visible behaviour.

What E2E tests cover:

- The full HTTP request/response lifecycle through Axum handlers
- Database reads and writes via real SQL queries against PostgreSQL
- Event indexing triggered by mocked Soroban RPC responses (via WireMock)
- Webhook delivery to a real HTTP receiver, including signature verification
- SSE stream connectivity and event push
- Admin operations and their auth enforcement
- Failure and recovery scenarios
- Baseline performance assertions

What E2E tests do **not** cover:

- Internal function correctness (handled by unit tests)
- Property edge cases (handled by `proptest`)
- API contract stability (handled by contract tests)

---

## Architecture

The E2E stack is defined in `docker-compose.e2e.yml` and consists of four services:

```
┌─────────────────────────────────────────────────────────────┐
│                    docker-compose.e2e.yml                   │
│                                                             │
│  ┌──────────┐    ┌──────────────┐    ┌───────────────────┐  │
│  │ PostgreSQL│    │   WireMock   │    │  webhook-receiver │  │
│  │  :5433   │    │   :8080      │    │     :9001         │  │
│  └────┬─────┘    └──────┬───────┘    └─────────┬─────────┘  │
│       │                 │                      │             │
│       └────────┬────────┘                      │             │
│                │                               │             │
│        ┌───────┴──────────────────────────┐    │             │
│        │        SorobanPulse app          │────┘             │
│        │           :3001                  │                  │
│        └──────────────────────────────────┘                  │
└─────────────────────────────────────────────────────────────┘
```

| Service | Role |
|---|---|
| **PostgreSQL** | Real database instance seeded with `tests/e2e/seed.sql` before tests run |
| **WireMock** | Stubs the Soroban RPC `getEvents` and `getLatestLedger` endpoints; can be reset between tests via its admin API |
| **SorobanPulse app** | The application under test, built from the local source and configured to point at the other services |
| **webhook-receiver** | A minimal HTTP server that accepts POST requests, records delivered payloads, and exposes them for test assertions |

The app is configured at startup with environment variables that wire it to the E2E services (e.g., `STELLAR_RPC_URL=http://wiremock:8080`, `DATABASE_URL=postgres://e2e:e2e@db/soroban_pulse_e2e`).

Tests run outside the compose network using the ports mapped to `localhost`. WireMock stubs are configured and inspected via its REST admin API at `E2E_RPC_ADMIN_URL`. Webhook payloads are inspected via the receiver's own API at `E2E_WEBHOOK_URL`.

---

## Test Categories

All tests live in `tests/e2e_tests.rs`. Each test function is prefixed `e2e_` and is skipped automatically when `E2E_BASE_URL` is unset, so the suite is safe to include in `cargo test` without the stack running.

### Health Checks

Verify that the liveness and readiness endpoints respond correctly.

- `e2e_health_check_returns_ok` — `GET /healthz/ready` returns `200` with `{"status":"ok","db":"ok","indexer":"ok"}` when the stack is healthy

### Event Indexing Flow

Verify that the indexer polls WireMock, persists events, and makes them queryable.

- `e2e_event_indexing_flow` — Configures WireMock to return a synthetic event, waits up to 30 s for the indexer to poll, then asserts `GET /v1/events/{contract_id}` returns the record with correct fields (`contract_id`, `tx_hash`, `ledger`, `event_type`)

### Pagination

Verify that `page` and `limit` query parameters work correctly.

- `e2e_pagination_returns_correct_pages` — Requests page 1 and page 2 (`limit=10`) and asserts each page has 10 items with no overlapping IDs

### Ledger Range and Event Type Filtering

Verify query filters produce correct subsets.

- `e2e_ledger_range_filter` — `?from_ledger=1001&to_ledger=1005` returns only events within that range
- `e2e_event_type_filter_contract` — `?event_type=contract` returns only contract-type events
- `e2e_invalid_event_type_returns_400` — `?event_type=unknown_type` returns `400 Bad Request`
- `e2e_get_events_by_tx_hash` — `GET /v1/events/tx/{tx_hash}` returns events for a known hash (or an empty array for an unknown hash) with no errors
- `e2e_filter_by_event_type_diagnostic` — `?event_type=diagnostic&limit=50` returns only diagnostic events; Contract B's 10 diagnostic events are present
- `e2e_filter_by_event_type_system` — `?event_type=system&limit=50` returns only system events; Contract C's 5 events are present
- `e2e_filter_ledger_range_boundaries` — `?from_ledger=1020&to_ledger=1030` returns at most 11 events, all within the inclusive range
- `e2e_filter_invalid_ledger_range_returns_400` — `?from_ledger=1050&to_ledger=1000` (reversed) returns `400 Bad Request`
- `e2e_exact_count_matches_approximate` — Both `?exact_count=true` and `?exact_count=false` return a numeric `total`; the two values agree within 10 %
- `e2e_event_stats_returns_breakdown` — `GET /v1/events/stats` returns `200` with a body containing at least one of `"total"`, `"counts"`, or `"by_type"`
- `e2e_events_by_contract_returns_only_that_contract` — `GET /v1/events/contract/{contract_a}` returns only events whose `contract_id` matches Contract A

### SSE Stream

Verify the Server-Sent Events endpoint connects and delivers events.

- `e2e_sse_stream_connects_and_pings` — `GET /v1/events/stream` returns `Content-Type: text/event-stream`; an `event: ping` line is received within 10 s (the E2E stack sets `SSE_KEEPALIVE_SECS=5`)

### Subscription Creation and Configuration

Verify the subscription lifecycle from creation through cancellation.

- `e2e_subscription_creation_and_listing` — Creates a webhook subscription, reads back the list, and asserts the new subscription appears
- `e2e_subscription_full_lifecycle` — Creates a subscription, reads it back, sends an ACK at ledger 1020, then deletes it; asserts a `404` is returned for the deleted ID
- `e2e_subscription_batch_config_update` — Creates a subscription with `batch_size=10`; PUTs a new config (`batch_size=25`, `batch_timeout_ms=5000`); GETs the config and asserts the updated values are reflected
- `e2e_subscription_pause_and_resume` — POSTs to `/pause`, GETs `/pause-status` (expects `"paused": true`), POSTs to `/resume`, GETs `/pause-status` again (expects active/not-paused)
- `e2e_subscription_invalid_callback_url_rejected` — Creating a subscription with malformed callback URLs (`"not-a-url"`, empty string, FTP, missing scheme) returns `4xx`
- `e2e_subscription_ack_advances_cursor` — ACKing at ledger 1010 and then GETting the subscription reflects `acked_ledger` (or equivalent) as `1010`

### Webhook Delivery

Verify that the webhook dispatcher delivers events to the registered callback URL.

- `e2e_webhook_delivery_flow` — Registers a subscription, injects an event via WireMock, waits up to 30 s for the webhook receiver to record at least one delivery
- `e2e_webhook_payload_contains_event_fields` — Inspects the first received delivery; asserts the payload contains `contract_id` / `contractId`, `ledger` / `ledger_sequence`, and `event_type` / `type` fields
- `e2e_webhook_delivers_to_correct_endpoint` — Two subscriptions for different contracts share the same receiver URL; only contract-B events are injected; all recorded deliveries reference contract B (no cross-routing to contract E)
- `e2e_webhook_delivery_includes_signature_header` — Subscription created with a signing secret; received delivery headers include a non-empty `X-Signature` or `X-Webhook-Signature` HMAC value
- `e2e_circuit_breaker_stats_endpoint` — `GET /v1/admin/webhook/circuit-breaker` with the admin key returns `200` with a JSON object or array
- `e2e_webhook_batch_delivery` — Subscription created with `batch_size=3`; three events are indexed; `POST /v1/subscriptions/{id}/batch` returns `2xx`

### Event Filtering and Transformation

The filtering tests are listed under [Ledger Range and Event Type Filtering](#ledger-range-and-event-type-filtering) above. Additional coverage:

- `e2e_exact_count_matches_approximate` — Verifies approximate and exact totals are within 10 % of each other
- `e2e_event_stats_returns_breakdown` — Stats endpoint returns a parseable breakdown
- `e2e_events_by_contract_returns_only_that_contract` — Contract-scoped query returns only matching events

### Miscellaneous

Additional tests that cover observability, backwards compatibility, and rate limiting.

- `e2e_metrics_endpoint_returns_prometheus_format` — `GET /metrics` returns `200` and the body contains `soroban_pulse_events_indexed_total` and `soroban_pulse_indexer_current_ledger`
- `e2e_rate_limiting_is_disabled_in_e2e_env` — The E2E stack sets `RATE_LIMIT_PER_MINUTE=0`; 20 rapid requests each return a non-`429` status
- `e2e_deprecated_routes_return_deprecation_header` — `GET /events` returns `200` with a `Deprecation: true` header

### Multi-Channel Notifications

Verify that notification channels beyond webhooks can be configured and operate independently.

- `e2e_subscription_email_config` — `PUT /v1/subscriptions/{id}/email` persists the email address and enabled flag; `GET /v1/subscriptions/{id}/email` returns the saved values
- `e2e_subscription_slack_integration_setup` — `POST /v1/subscriptions/{id}/integrations/slack` with a mock webhook URL returns `200` or `201`; if the route supports `GET`, the response includes the registered `webhook_url`
- `e2e_subscription_discord_integration_setup` — `POST /v1/subscriptions/{id}/integrations/discord` returns `200` or `201`; if GET is supported, the response includes `webhook_url` or `channel_id`
- `e2e_notification_channel_admin_creation` — `POST /v1/admin/notifications/channels` with `channel_type`, `name`, `subscription_id`, and `config` returns `200` or `201`; if the listing route exists, the new channel appears in the results
- `e2e_multi_subscription_different_channels` — Two independent subscriptions for the same contract but different webhook URLs each receive a delivery when an event is indexed; the delivery log shows `≥ 2` entries

### Admin Operations

Verify admin-only endpoints work correctly and are properly protected.

- `e2e_admin_indexer_pause_resume` — `POST /v1/admin/indexer/pause` stops event ingestion; `POST /v1/admin/indexer/resume` restarts it; `/healthz/ready` reflects the paused state; the indexer recovers to "ok" within 15 s after resume
- `e2e_admin_event_replay` — `POST /v1/admin/replay` with a ledger range triggers re-indexing of WireMock-stubbed events and returns `200` or `202` with a job/confirmation body
- `e2e_admin_mask_events` — `POST /v1/admin/events/mask` with `contract_id` and a list of fields to redact returns `200` or `202`; the endpoint accepts a JSON body specifying which fields to replace
- `e2e_admin_bulk_export_lifecycle` — `POST /v1/admin/export` with a ledger range and format starts an async export job and returns `200` or `202` with a parseable JSON body; if a `job_id` is returned, the status endpoint at `/v1/admin/export/{id}` is also probed
- `e2e_admin_auth_enforcement` — Requests to `/v1/admin/db/index-health` with no key return `401`; requests with an incorrect key return `401` or `403`; requests with `ADMIN_API_KEY` return `2xx`
- `e2e_admin_index_fragmentation_report` — `GET /v1/admin/db/index-health` returns `200` with a JSON body containing an array of index entries (top-level or under `"indexes"` / `"indices"` / `"data"`)
- `e2e_admin_audit_logs` — After performing a pause/resume cycle, `GET /v1/admin/audit-log` returns `200` with a JSON body; non-empty entries each contain an action field and a timestamp field

### Failure Recovery

Verify the system degrades gracefully and recovers correctly from errors.

- `e2e_health_during_rpc_errors` — When WireMock is configured to return `500` for all RPC calls, the DB health field in `/healthz/ready` remains `"ok"` (the process is still alive and the database is reachable); `soroban_pulse_rpc_errors_total` in `/metrics` is `> 0` once errors accumulate
- `e2e_recovery_after_rpc_restore` — A temporary RPC error stub is injected then removed; a new event stubbed after the restore is indexed within 30 s, confirming the indexer's error-backoff loop recovered automatically
- `e2e_subscription_deletion_stops_delivery` — After a subscription is deleted via `DELETE /v1/subscriptions/{id}`, a subsequently injected event produces zero webhook deliveries within a 15 s observation window
- `e2e_unknown_route_returns_404` — A `GET` to `/v1/this-path-does-not-exist` returns `404` with a JSON body containing an `"error"` or `"message"` field
- `e2e_malformed_body_returns_400` — A `POST` to `/v1/subscriptions` with the body `{ this is not valid json }` and `Content-Type: application/json` returns `400` with a JSON error body
- `e2e_large_limit_is_clamped` — `GET /v1/events?limit=100000` returns `200`; the `"limit"` field in the response is `≤ 1000` (clamped to the server maximum)

### Performance Verification

Verify baseline performance characteristics under the E2E stack. These tests are not load tests (see `tests/load/` for k6 scripts) but act as a smoke check for obvious regressions.

- `e2e_perf_p95_latency` — Sends 50 sequential `GET /v1/events` requests; records each response time; asserts the p95 value is `< 500 ms`
- `e2e_perf_concurrent_no_errors` — Issues 20 concurrent `GET /v1/events` requests via `tokio::spawn`; asserts all return `200` with zero failures
- `e2e_perf_pagination_consistency_under_load` — Fetches pages 1–5 (`limit=10`) concurrently; collects all returned event IDs; asserts no duplicates across pages (set size equals total count)
- `e2e_perf_metrics_endpoint_speed` — Times a single `GET /metrics` request; asserts the response arrives in `< 200 ms`
- `e2e_perf_health_check_speed` — Times a single `GET /healthz/live` request (no external I/O); asserts the response arrives in `< 50 ms`

---

## Running E2E Tests

### Prerequisites

- Docker and Docker Compose v2
- Rust stable toolchain
- Ports `3001`, `5433`, `8080`, and `9001` available on the host

### Start the stack

```bash
docker compose -f docker-compose.e2e.yml up --build --wait
```

The `--wait` flag blocks until all service health checks pass. The app service health check hits `/healthz/ready`.

### Seed the database

```bash
docker compose -f docker-compose.e2e.yml exec -T db \
  psql -U e2e -d soroban_pulse_e2e \
  -f /dev/stdin < tests/e2e/seed.sql
```

This populates the three seed contracts (A, B, C) and their events. See [Seed Data Reference](#seed-data-reference) for details.

### Run all E2E tests

```bash
E2E_BASE_URL=http://localhost:3001 \
E2E_WEBHOOK_URL=http://localhost:9001 \
E2E_RPC_ADMIN_URL=http://localhost:8080 \
E2E_ADMIN_API_KEY=e2e-admin-key \
cargo test --test e2e_tests -- --test-threads=1 --nocapture
```

`--test-threads=1` is required. The tests share the running application and database, so parallel execution causes race conditions in WireMock stubs, webhook delivery counts, and indexer state.

`--nocapture` prints log output from each test to stdout, which is useful for debugging failures.

### Run a specific test

```bash
E2E_BASE_URL=http://localhost:3001 \
E2E_WEBHOOK_URL=http://localhost:9001 \
E2E_RPC_ADMIN_URL=http://localhost:8080 \
E2E_ADMIN_API_KEY=e2e-admin-key \
cargo test --test e2e_tests e2e_subscription_full_lifecycle -- --test-threads=1
```

Replace `e2e_subscription_full_lifecycle` with the name of any test function.

### Tear down

```bash
docker compose -f docker-compose.e2e.yml down -v
```

The `-v` flag removes the named volumes (including the PostgreSQL data volume) so each run starts from a clean state.

---

## Environment Variables

| Variable | Description | Default |
|---|---|---|
| `E2E_BASE_URL` | Base URL of the SorobanPulse app under test | required — tests are skipped if unset |
| `E2E_WEBHOOK_URL` | Base URL of the webhook receiver service | `http://localhost:9001` |
| `E2E_RPC_ADMIN_URL` | WireMock admin API base URL | `http://localhost:8080` |
| `E2E_ADMIN_API_KEY` | Admin API key sent in `X-Api-Key` for admin endpoint tests | `e2e-admin-key` |

`E2E_BASE_URL` is the only required variable. When it is unset, every E2E test function returns immediately (skips), so `cargo test` continues to work in environments without the stack running.

---

## CI Integration

E2E tests run in `.github/workflows/e2e.yml`. The workflow:

1. Starts the compose stack with `docker compose -f docker-compose.e2e.yml up --build --wait`
2. Seeds the database from `tests/e2e/seed.sql`
3. Runs the full E2E suite with all four environment variables set
4. Tears down the stack and volumes

The E2E workflow is a separate job from the standard `ci.yml` test job. It runs on every pull request that touches `src/`, `tests/e2e/`, `docker-compose.e2e.yml`, or `migrations/`, and on every push to `main`.

E2E failures block merge. If the stack fails to start (health check timeout), the workflow fails at the `up --wait` step with the compose logs attached as a job artifact.

---

## Adding New Tests

Follow these conventions to keep the suite reliable.

### Guard against missing environment

Every test must start with this guard:

```rust
let Some(base) = base_url() else { return; };
```

`base_url()` reads `E2E_BASE_URL` and returns `None` when unset. Without this guard, a test would panic or fail with a connection error in environments where the stack is not running.

### Reset shared state in tests that inject events

Tests that configure WireMock stubs or verify webhook deliveries must reset state at the start of the test, not just at the end. This ensures a clean slate even if a previous test failed mid-way.

```rust
// Reset WireMock stubs to defaults
reset_wiremock_stubs(&rpc_admin_url).await;

// Clear recorded webhook deliveries
clear_webhook_deliveries(&webhook_url).await;
```

### Use `wait_until()` for async assertions

The indexer polls on its own schedule. Never `sleep` for a fixed duration — use the `wait_until()` helper, which retries the assertion at short intervals up to a configurable timeout:

```rust
wait_until(Duration::from_secs(15), || async {
    let resp = client.get(&format!("{base}/v1/events")).send().await?;
    let body: EventsResponse = resp.json().await?;
    Ok(body.data.len() >= expected_count)
})
.await
.expect("events did not appear within timeout");
```

### Keep tests independent

Each test should create its own data rather than relying on the order of execution or state left by another test. For event injection tests, generate a unique contract ID per test (e.g., using a UUID or a test-name-derived string) so WireMock stubs and database rows do not collide.

### Serialise execution

Always pass `--test-threads=1` when running E2E tests. Because all tests share the same running application, database, and WireMock instance, concurrent tests will interfere with each other's WireMock stubs, webhook counts, and indexer pause/resume state.

---

## Seed Data Reference

`tests/e2e/seed.sql` inserts a fixed dataset used by filtering, pagination, and stats tests. Do not modify the seed data without updating the tests that depend on exact counts.

| Contract | ID | Events | Ledgers | Event Type |
|---|---|---|---|---|
| A | `CAAAA...FCT4` | 50 | 1001–1050 | `contract` |
| B | `CBBBB...FCT4` | 10 | 1001–1010 | `diagnostic` |
| C | `CCCCC...FCT4` | 5 | 1001–1005 | `system` |

Total seeded events: **65** across 3 contracts.

Key properties to be aware of:

- All events fall within ledger range **1001–1050**.
- Contract A is the only source of `contract`-type events in the seed.
- Contracts B and C each cover a sub-range of A's ledger span, so ledger range queries that span 1006–1050 will return A events only.
- The seed assigns deterministic `tx_hash` values, making `GET /v1/events/tx/{tx_hash}` tests reproducible.

---

## Full Test Index

All 42 test functions in `tests/e2e_tests.rs`, grouped by category.

### Health & Observability

| Function | What it verifies |
|---|---|
| `e2e_health_check_returns_ok` | `/healthz/ready` returns `{"status":"ok","db":"ok","indexer":"ok"}` |
| `e2e_metrics_endpoint_returns_prometheus_format` | `/metrics` contains expected Prometheus metric names |
| `e2e_rate_limiting_is_disabled_in_e2e_env` | 20 rapid requests none return 429 (E2E sets unlimited rate) |
| `e2e_deprecated_routes_return_deprecation_header` | `/events` (unversioned) includes `Deprecation: true` header |

### Event Indexing & Querying

| Function | What it verifies |
|---|---|
| `e2e_event_indexing_flow` | WireMock event is indexed and visible in API within 30 s |
| `e2e_pagination_returns_correct_pages` | Pages 1 and 2 have 10 events each with no overlapping IDs |
| `e2e_ledger_range_filter` | `from_ledger`/`to_ledger` filter returns only matching events |
| `e2e_event_type_filter_contract` | `event_type=contract` returns only contract events |
| `e2e_invalid_event_type_returns_400` | Unknown `event_type` value returns `400` |
| `e2e_get_events_by_tx_hash` | `GET /v1/events/tx/{hash}` returns matching or empty result |
| `e2e_filter_by_event_type_diagnostic` | `event_type=diagnostic` returns only diagnostic events |
| `e2e_filter_by_event_type_system` | `event_type=system` returns only system events |
| `e2e_filter_ledger_range_boundaries` | Inclusive ledger range boundary enforcement |
| `e2e_filter_invalid_ledger_range_returns_400` | Reversed range returns `400` |
| `e2e_exact_count_matches_approximate` | Exact and approximate totals agree within 10 % |
| `e2e_event_stats_returns_breakdown` | `/v1/events/stats` returns parseable count breakdown |
| `e2e_events_by_contract_returns_only_that_contract` | Contract-scoped query filters correctly |

### SSE Stream

| Function | What it verifies |
|---|---|
| `e2e_sse_stream_connects_and_pings` | Stream returns `text/event-stream` and emits keep-alive ping |

### Subscriptions

| Function | What it verifies |
|---|---|
| `e2e_subscription_creation_and_listing` | Created subscription appears in the list |
| `e2e_subscription_full_lifecycle` | Create → read → ACK → delete → 404 |
| `e2e_subscription_batch_config_update` | PUT batch config; GET reflects updated values |
| `e2e_subscription_pause_and_resume` | Pause sets paused state; resume clears it |
| `e2e_subscription_invalid_callback_url_rejected` | Malformed callback URLs return `4xx` |
| `e2e_subscription_ack_advances_cursor` | ACK at ledger 1010 updates `acked_ledger` |

### Webhook Delivery

| Function | What it verifies |
|---|---|
| `e2e_webhook_delivery_flow` | End-to-end delivery from indexing to receiver |
| `e2e_webhook_payload_contains_event_fields` | Payload includes `contract_id`, `ledger`, `event_type` |
| `e2e_webhook_delivers_to_correct_endpoint` | Events route only to matching subscription |
| `e2e_webhook_delivery_includes_signature_header` | HMAC signature header present and non-empty |
| `e2e_circuit_breaker_stats_endpoint` | Circuit-breaker admin endpoint returns JSON |
| `e2e_webhook_batch_delivery` | Batch trigger endpoint returns `2xx` |

### Multi-Channel Notifications

| Function | What it verifies |
|---|---|
| `e2e_subscription_email_config` | PUT/GET email config persists and retrieves values |
| `e2e_subscription_slack_integration_setup` | Slack integration POST returns `200`/`201` |
| `e2e_subscription_discord_integration_setup` | Discord integration POST returns `200`/`201` |
| `e2e_notification_channel_admin_creation` | Admin can create a notification channel |
| `e2e_multi_subscription_different_channels` | Two subs on same contract each receive delivery |

### Admin Operations

| Function | What it verifies |
|---|---|
| `e2e_admin_indexer_pause_resume` | Pause stops indexer; resume restores health within 15 s |
| `e2e_admin_event_replay` | Replay endpoint returns `200`/`202` with JSON body |
| `e2e_admin_mask_events` | Mask endpoint returns `200`/`202` |
| `e2e_admin_bulk_export_lifecycle` | Export job starts and returns parseable response |
| `e2e_admin_auth_enforcement` | No key → 401; wrong key → 401/403; admin key → 2xx |
| `e2e_admin_index_fragmentation_report` | Index-health returns array of index entries |
| `e2e_admin_audit_logs` | Audit-log endpoint returns `200` with JSON body |

### Failure Recovery

| Function | What it verifies |
|---|---|
| `e2e_health_during_rpc_errors` | DB health remains ok when RPC returns 500 |
| `e2e_recovery_after_rpc_restore` | Indexer resumes after RPC error is cleared |
| `e2e_subscription_deletion_stops_delivery` | No delivery occurs after subscription is deleted |
| `e2e_unknown_route_returns_404` | Unknown path returns `404` with JSON error |
| `e2e_malformed_body_returns_400` | Invalid JSON body returns `400` with JSON error |
| `e2e_large_limit_is_clamped` | `limit=100000` is clamped to server maximum (≤ 1000) |

### Performance Verification

| Function | What it verifies |
|---|---|
| `e2e_perf_p95_latency` | p95 of 50 sequential `/v1/events` requests < 500 ms |
| `e2e_perf_concurrent_no_errors` | 20 concurrent requests all return `200` |
| `e2e_perf_pagination_consistency_under_load` | 5 concurrent pages produce no duplicate IDs |
| `e2e_perf_metrics_endpoint_speed` | `/metrics` responds in < 200 ms |
| `e2e_perf_health_check_speed` | `/healthz/live` responds in < 50 ms |
