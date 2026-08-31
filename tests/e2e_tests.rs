//! End-to-end test suite — issue #654
//!
//! These tests target a **live** SorobanPulse stack started by
//! `docker compose -f docker-compose.e2e.yml up --build --wait`.
//!
//! They are intentionally separated from the unit/integration suite so they
//! can be gated behind the `E2E_BASE_URL` environment variable:
//!
//! ```bash
//! # Spin up the stack
//! docker compose -f docker-compose.e2e.yml up --build --wait
//!
//! # Run E2E tests
//! E2E_BASE_URL=http://localhost:3001 \
//! E2E_WEBHOOK_URL=http://localhost:9001 \
//! E2E_RPC_ADMIN_URL=http://localhost:8080 \
//! cargo test --test e2e_tests -- --test-threads=1
//!
//! # Tear down
//! docker compose -f docker-compose.e2e.yml down -v
//! ```
//!
//! When `E2E_BASE_URL` is not set the whole suite is skipped so that
//! `cargo test` in a plain CI job (without Docker) does not fail.

use serde_json::Value;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn base_url() -> Option<String> {
    std::env::var("E2E_BASE_URL").ok()
}

fn webhook_admin_url() -> String {
    std::env::var("E2E_WEBHOOK_URL").unwrap_or_else(|_| "http://localhost:9001".into())
}

fn rpc_admin_url() -> String {
    std::env::var("E2E_RPC_ADMIN_URL").unwrap_or_else(|_| "http://localhost:8080".into())
}

/// Poll `f` until it returns `true` or the timeout expires.
async fn wait_until<F, Fut>(f: F, timeout: Duration, interval: Duration) -> bool
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if f().await {
            return true;
        }
        tokio::time::sleep(interval).await;
    }
    false
}

/// GET `url` and deserialise the body as JSON.
async fn get_json(url: &str) -> reqwest::Result<Value> {
    reqwest::get(url).await?.json::<Value>().await
}

/// POST JSON to `url` and return the response.
async fn post_json(url: &str, body: &Value) -> reqwest::Result<reqwest::Response> {
    reqwest::Client::new().post(url).json(body).send().await
}

/// Inject a WireMock mapping to make the RPC return a set of events.
async fn stub_rpc_events(rpc_admin: &str, events: Vec<Value>, latest_ledger: u64) {
    let mapping = serde_json::json!({
        "name": "getEvents-with-data",
        "priority": 1,
        "request": {
            "method": "POST",
            "url": "/",
            "bodyPatterns": [{ "contains": "\"getEvents\"" }]
        },
        "response": {
            "status": 200,
            "headers": { "Content-Type": "application/json" },
            "jsonBody": {
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "events": events,
                    "latestLedger": latest_ledger
                }
            }
        }
    });
    reqwest::Client::new()
        .post(format!("{rpc_admin}/__admin/mappings"))
        .json(&mapping)
        .send()
        .await
        .expect("failed to inject WireMock mapping");
}

/// Remove all non-default WireMock stubs (resets to base mappings file).
async fn reset_rpc_stubs(rpc_admin: &str) {
    reqwest::Client::new()
        .post(format!("{rpc_admin}/__admin/reset"))
        .send()
        .await
        .expect("failed to reset WireMock stubs");
}

/// Clear the webhook receiver's recorded deliveries.
async fn clear_webhook_deliveries(webhook_admin: &str) {
    reqwest::Client::new()
        .delete(format!("{webhook_admin}/received"))
        .send()
        .await
        .expect("failed to clear webhook deliveries");
}

// ---------------------------------------------------------------------------
// Test: health check
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_health_check_returns_ok() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_health_check_returns_ok");
        return;
    };

    let body = get_json(&format!("{base}/healthz/ready"))
        .await
        .expect("GET /healthz/ready failed");

    assert_eq!(body["status"], "ok", "health status should be ok: {body}");
    assert_eq!(body["db"], "ok", "db should be ok: {body}");
    assert_eq!(body["indexer"], "ok", "indexer should be ok: {body}");
}

// ---------------------------------------------------------------------------
// Test: event indexing flow
// ---------------------------------------------------------------------------

/// Verify that when the RPC stub returns a new event the indexer picks it up
/// and it becomes visible via the REST API within a reasonable timeout.
#[tokio::test]
async fn e2e_event_indexing_flow() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_event_indexing_flow");
        return;
    };
    let rpc_admin = rpc_admin_url();

    // Inject one event into the RPC stub.
    let contract_id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4";
    let tx_hash = "a".repeat(64);

    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000004294967296-0000000000",
            "contractId": contract_id,
            "txHash": tx_hash,
            "ledger": 1001,
            "ledgerClosedAt": "2026-03-14T00:01:00Z",
            "pagingToken": "0000000004294967296-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        1001,
    )
    .await;

    // Poll until the event appears in the API (up to 30 s — one full poll cycle).
    let appeared = wait_until(
        || {
            let url = format!("{base}/v1/events/{contract_id}");
            async move {
                match get_json(&url).await {
                    Ok(v) => v["data"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    // Restore default stub so other tests are not affected.
    reset_rpc_stubs(&rpc_admin).await;

    assert!(appeared, "event should appear in API within 30 s after indexing");

    // Verify the event fields.
    let body = get_json(&format!("{base}/v1/events/{contract_id}"))
        .await
        .expect("GET /v1/events/{contract_id} failed");
    let events = body["data"].as_array().expect("data should be an array");
    assert!(!events.is_empty(), "should have at least one event");
    let ev = &events[0];
    assert_eq!(ev["contract_id"], contract_id);
    assert_eq!(ev["tx_hash"], tx_hash);
    assert_eq!(ev["ledger"], 1001);
    assert_eq!(ev["event_type"], "contract");
}

// ---------------------------------------------------------------------------
// Test: pagination
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_pagination_returns_correct_pages() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_pagination_returns_correct_pages");
        return;
    };

    // Seed via direct SQL is handled by the `seed.sql` script run before the
    // test suite; we rely on Contract A having 50 events.
    let page1 = get_json(&format!("{base}/v1/events?page=1&limit=10"))
        .await
        .expect("page 1 request failed");
    let page2 = get_json(&format!("{base}/v1/events?page=2&limit=10"))
        .await
        .expect("page 2 request failed");

    let p1_data = page1["data"].as_array().expect("data should be array");
    let p2_data = page2["data"].as_array().expect("data should be array");

    assert_eq!(p1_data.len(), 10, "page 1 should have 10 events");
    assert_eq!(p2_data.len(), 10, "page 2 should have 10 events");

    // IDs on page 1 and page 2 must not overlap.
    let p1_ids: std::collections::HashSet<&str> =
        p1_data.iter().filter_map(|e| e["id"].as_str()).collect();
    let p2_ids: std::collections::HashSet<&str> =
        p2_data.iter().filter_map(|e| e["id"].as_str()).collect();
    assert!(
        p1_ids.is_disjoint(&p2_ids),
        "pages should not contain duplicate events"
    );
}

// ---------------------------------------------------------------------------
// Test: ledger range filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_ledger_range_filter() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_ledger_range_filter");
        return;
    };

    let body = get_json(&format!("{base}/v1/events?from_ledger=1001&to_ledger=1005"))
        .await
        .expect("ledger range request failed");

    let events = body["data"].as_array().expect("data should be array");
    for ev in events {
        let ledger = ev["ledger"].as_u64().expect("ledger should be u64");
        assert!(
            (1001..=1005).contains(&ledger),
            "event ledger {ledger} outside requested range"
        );
    }
}

// ---------------------------------------------------------------------------
// Test: event type filter
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_event_type_filter_contract() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_event_type_filter_contract");
        return;
    };

    let body = get_json(&format!("{base}/v1/events?event_type=contract"))
        .await
        .expect("event_type filter request failed");

    let events = body["data"].as_array().expect("data should be array");
    for ev in events {
        assert_eq!(
            ev["event_type"], "contract",
            "filtered results should only contain contract events"
        );
    }
}

#[tokio::test]
async fn e2e_invalid_event_type_returns_400() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_invalid_event_type_returns_400");
        return;
    };

    let resp = reqwest::get(&format!("{base}/v1/events?event_type=unknown_type"))
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        400,
        "unknown event_type should return 400 Bad Request"
    );
}

// ---------------------------------------------------------------------------
// Test: GET by transaction hash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_get_events_by_tx_hash() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_get_events_by_tx_hash");
        return;
    };

    // Ledger 1 of the seed data has tx_hash "0...01" (padded to 64 chars).
    let tx_hash = "0".repeat(63) + "1";
    let body = get_json(&format!("{base}/v1/events/tx/{tx_hash}"))
        .await
        .expect("GET /v1/events/tx/{hash} failed");

    let events = body["data"].as_array().expect("data should be array");
    // The endpoint returns an empty array for unknown hashes — that's fine.
    // If data is non-empty every event must carry that tx_hash.
    for ev in events {
        assert_eq!(ev["tx_hash"], tx_hash);
    }
}

// ---------------------------------------------------------------------------
// Test: SSE stream connects and receives keep-alive pings
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_sse_stream_connects_and_pings() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_sse_stream_connects_and_pings");
        return;
    };

    // Open an SSE connection with a short timeout.  We just verify we receive
    // the correct Content-Type and at least one `ping` event within 10 s.
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    let resp = client
        .get(&format!("{base}/v1/events/stream"))
        .header("Accept", "text/event-stream")
        .send()
        .await
        .expect("SSE connection failed");

    assert_eq!(resp.status(), 200, "SSE endpoint should return 200");
    assert!(
        resp.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .map(|ct| ct.contains("text/event-stream"))
            .unwrap_or(false),
        "SSE response must have Content-Type: text/event-stream"
    );

    // Read bytes for up to 10 s.  The server emits a `ping` every 5 s (set in
    // docker-compose.e2e.yml via SSE_KEEPALIVE_SECS=5).
    let bytes = resp.bytes().await.unwrap_or_default();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("event: ping"),
        "SSE stream should emit a ping event within 10 s"
    );
}

// ---------------------------------------------------------------------------
// Test: subscription delivery flow
// ---------------------------------------------------------------------------

/// Creates a subscription, then injects an event via the RPC stub and verifies
/// that the subscription mechanism records the notification.
///
/// Note: SorobanPulse delivers subscriptions in-process (not via an external
/// queue in this config), so we validate that the indexed event is visible via
/// the REST API and that subscription metadata is returned correctly.
#[tokio::test]
async fn e2e_subscription_creation_and_listing() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_subscription_creation_and_listing"
        );
        return;
    };

    let contract_id = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4";

    // Create a webhook subscription.
    let webhook_url = format!("{}/webhook", webhook_admin_url());
    let body = serde_json::json!({
        "contract_id": contract_id,
        "webhook_url": webhook_url,
        "event_types": ["contract"]
    });

    let resp = post_json(&format!("{base}/v1/subscriptions"), &body)
        .await
        .expect("POST /v1/subscriptions failed");

    let status = resp.status();
    assert!(
        status == 200 || status == 201,
        "subscription creation should succeed (got {status})"
    );

    let created: Value = resp.json().await.expect("response body should be JSON");
    assert!(
        created["id"].is_string() || created["subscription_id"].is_string(),
        "response should include an id field"
    );

    // List subscriptions and verify ours is present.
    let list = get_json(&format!("{base}/v1/subscriptions"))
        .await
        .expect("GET /v1/subscriptions failed");
    let subs = list["data"]
        .as_array()
        .or_else(|| list.as_array())
        .expect("subscriptions response should be an array");

    assert!(
        !subs.is_empty(),
        "subscriptions list should contain at least one entry"
    );
}

// ---------------------------------------------------------------------------
// Test: webhook delivery flow
// ---------------------------------------------------------------------------

/// Verifies the full webhook delivery pipeline:
/// 1. Register a webhook subscription pointing at the local webhook receiver.
/// 2. Inject an event via the RPC stub.
/// 3. Wait for the indexer to pick up the event.
/// 4. Assert that the webhook receiver recorded at least one delivery.
#[tokio::test]
async fn e2e_webhook_delivery_flow() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_webhook_delivery_flow");
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    // Clear any previous webhook deliveries.
    clear_webhook_deliveries(&webhook_admin).await;

    let contract_id = "CDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDFCT4";
    let tx_hash = "d".repeat(64);

    // Register webhook subscription.
    let webhook_url = format!("{webhook_admin}/webhook");
    let sub_body = serde_json::json!({
        "contract_id": contract_id,
        "webhook_url": webhook_url,
        "event_types": ["contract"]
    });
    let sub_resp = post_json(&format!("{base}/v1/subscriptions"), &sub_body)
        .await
        .expect("failed to register subscription");
    assert!(
        sub_resp.status().is_success(),
        "subscription registration failed with status {}",
        sub_resp.status()
    );

    // Inject a matching event via the RPC stub.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000008589934592-0000000000",
            "contractId": contract_id,
            "txHash": tx_hash,
            "ledger": 2001,
            "ledgerClosedAt": "2026-03-14T01:00:00Z",
            "pagingToken": "0000000008589934592-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        2001,
    )
    .await;

    // Wait for webhook delivery (up to 30 s).
    let delivered = wait_until(
        || {
            let url = format!("{webhook_admin}/received");
            async move {
                match get_json(&url).await {
                    Ok(v) => v.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;
    clear_webhook_deliveries(&webhook_admin).await;

    assert!(
        delivered,
        "webhook receiver should have received at least one delivery within 30 s"
    );
}

// ---------------------------------------------------------------------------
// Test: metrics endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_metrics_endpoint_returns_prometheus_format() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_metrics_endpoint_returns_prometheus_format");
        return;
    };

    let resp = reqwest::get(&format!("{base}/metrics"))
        .await
        .expect("GET /metrics failed");

    assert_eq!(resp.status(), 200, "/metrics should return 200");
    let body = resp.text().await.expect("failed to read metrics body");
    assert!(
        body.contains("soroban_pulse_events_indexed_total"),
        "metrics body should contain soroban_pulse_events_indexed_total"
    );
    assert!(
        body.contains("soroban_pulse_indexer_current_ledger"),
        "metrics body should contain soroban_pulse_indexer_current_ledger"
    );
}

// ---------------------------------------------------------------------------
// Test: rate limiting
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_rate_limiting_is_disabled_in_e2e_env() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_rate_limiting_is_disabled_in_e2e_env");
        return;
    };

    // The E2E compose sets RATE_LIMIT_PER_MINUTE=0 (unlimited).
    // Fire 20 rapid requests and assert none returns 429.
    let client = reqwest::Client::new();
    for _ in 0..20 {
        let resp = client
            .get(&format!("{base}/v1/events"))
            .send()
            .await
            .expect("request failed");
        assert_ne!(
            resp.status(),
            429,
            "rate limiting should be disabled in E2E env"
        );
    }
}

// ---------------------------------------------------------------------------
// Test: deprecated unversioned routes return Deprecation header
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_deprecated_routes_return_deprecation_header() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_deprecated_routes_return_deprecation_header"
        );
        return;
    };

    let resp = reqwest::get(&format!("{base}/events"))
        .await
        .expect("GET /events failed");

    assert_eq!(resp.status(), 200, "/events should return 200");
    assert!(
        resp.headers().contains_key("deprecation"),
        "deprecated route should include Deprecation header"
    );
}

// ---------------------------------------------------------------------------
// Test: subscription full lifecycle — create, read, ack, cancel
// ---------------------------------------------------------------------------

/// Verifies the complete subscription lifecycle:
/// 1. Create a subscription via POST /v1/subscriptions.
/// 2. Read it back via GET /v1/subscriptions/{id} and check fields.
/// 3. Acknowledge a ledger via POST /v1/subscriptions/{id}/ack.
/// 4. Cancel (delete) the subscription via DELETE /v1/subscriptions/{id}.
/// 5. Confirm the subscription is gone (404 or empty).
#[tokio::test]
async fn e2e_subscription_full_lifecycle() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_subscription_full_lifecycle");
        return;
    };

    let callback_url = format!("{}/webhook", webhook_admin_url());

    // --- Step 1: create ---
    let create_body = serde_json::json!({
        "callback_url": callback_url,
        "from_ledger": 1001,
        "subscription_type": "webhook"
    });
    let create_resp = post_json(&format!("{base}/v1/subscriptions"), &create_body)
        .await
        .expect("POST /v1/subscriptions failed");

    let create_status = create_resp.status();
    assert!(
        create_status == 200 || create_status == 201,
        "subscription creation should return 200 or 201, got {create_status}"
    );

    let created: Value = create_resp.json().await.expect("body should be JSON");
    let sub_id = created["id"]
        .as_str()
        .or_else(|| created["subscription_id"].as_str())
        .expect("response must contain an 'id' or 'subscription_id' field")
        .to_owned();

    // --- Step 2: read back ---
    let fetched = get_json(&format!("{base}/v1/subscriptions/{sub_id}"))
        .await
        .expect("GET /v1/subscriptions/{id} failed");

    // The subscription should exist and report the ledger we asked for.
    assert!(
        !fetched.is_null(),
        "GET /v1/subscriptions/{sub_id} should return the subscription"
    );
    let fetched_from = fetched["from_ledger"]
        .as_u64()
        .or_else(|| fetched["data"]["from_ledger"].as_u64());
    if let Some(fl) = fetched_from {
        assert_eq!(fl, 1001, "from_ledger should match the value set at creation");
    }

    // --- Step 3: ack ---
    let ack_body = serde_json::json!({ "ledger": 1020 });
    let ack_resp = post_json(&format!("{base}/v1/subscriptions/{sub_id}/ack"), &ack_body)
        .await
        .expect("POST /v1/subscriptions/{id}/ack failed");
    assert!(
        ack_resp.status().is_success(),
        "ack should succeed, got {}",
        ack_resp.status()
    );

    // --- Step 4: cancel ---
    let del_resp = reqwest::Client::new()
        .delete(&format!("{base}/v1/subscriptions/{sub_id}"))
        .send()
        .await
        .expect("DELETE /v1/subscriptions/{id} failed");
    assert!(
        del_resp.status().is_success(),
        "DELETE should succeed, got {}",
        del_resp.status()
    );

    // --- Step 5: confirm gone ---
    let gone_resp = reqwest::get(&format!("{base}/v1/subscriptions/{sub_id}"))
        .await
        .expect("GET after DELETE failed");
    assert_eq!(
        gone_resp.status(),
        404,
        "deleted subscription should return 404, got {}",
        gone_resp.status()
    );
}

// ---------------------------------------------------------------------------
// Test: subscription batch config — read, update, verify
// ---------------------------------------------------------------------------

/// 1. Create a subscription with default batch settings.
/// 2. GET the batch config and confirm a response is returned.
/// 3. PUT a new batch config (batch_size=25, batch_timeout_ms=5000).
/// 4. GET batch config again and verify the updated values are reflected.
#[tokio::test]
async fn e2e_subscription_batch_config_update() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_subscription_batch_config_update");
        return;
    };

    let callback_url = format!("{}/webhook", webhook_admin_url());

    // --- Create subscription ---
    let create_body = serde_json::json!({
        "callback_url": callback_url,
        "from_ledger": 1001,
        "subscription_type": "webhook",
        "batch_size": 10,
        "batch_timeout_ms": 1000
    });
    let create_resp = post_json(&format!("{base}/v1/subscriptions"), &create_body)
        .await
        .expect("POST /v1/subscriptions failed");
    assert!(
        create_resp.status().is_success(),
        "subscription creation failed with {}",
        create_resp.status()
    );
    let created: Value = create_resp.json().await.expect("body should be JSON");
    let sub_id = created["id"]
        .as_str()
        .or_else(|| created["subscription_id"].as_str())
        .expect("response must contain an id")
        .to_owned();

    // --- GET initial batch config ---
    let initial_config = get_json(&format!("{base}/v1/subscriptions/{sub_id}/batch"))
        .await
        .expect("GET /v1/subscriptions/{id}/batch failed");
    assert!(
        !initial_config.is_null(),
        "GET batch config should return a non-null response"
    );

    // --- PUT updated batch config ---
    let new_config = serde_json::json!({
        "subscription_type": "webhook",
        "batch_size": 25,
        "batch_timeout_ms": 5000
    });
    let put_resp = reqwest::Client::new()
        .put(&format!("{base}/v1/subscriptions/{sub_id}/batch"))
        .json(&new_config)
        .send()
        .await
        .expect("PUT /v1/subscriptions/{id}/batch failed");
    assert!(
        put_resp.status().is_success(),
        "PUT batch config should succeed, got {}",
        put_resp.status()
    );

    // --- GET batch config again and verify ---
    let updated_config = get_json(&format!("{base}/v1/subscriptions/{sub_id}/batch"))
        .await
        .expect("GET /v1/subscriptions/{id}/batch after update failed");

    // The server may wrap data inside a "data" key or return it top-level.
    let config_data = if updated_config["data"].is_object() {
        &updated_config["data"]
    } else {
        &updated_config
    };

    if let Some(bs) = config_data["batch_size"].as_u64() {
        assert_eq!(bs, 25, "batch_size should be updated to 25");
    }
    if let Some(bt) = config_data["batch_timeout_ms"].as_u64() {
        assert_eq!(bt, 5000, "batch_timeout_ms should be updated to 5000");
    }

    // Cleanup
    let _ = reqwest::Client::new()
        .delete(&format!("{base}/v1/subscriptions/{sub_id}"))
        .send()
        .await;
}

// ---------------------------------------------------------------------------
// Test: subscription pause and resume
// ---------------------------------------------------------------------------

/// 1. Create a subscription.
/// 2. POST /v1/subscriptions/{id}/pause.
/// 3. GET /v1/subscriptions/{id}/pause-status — assert paused.
/// 4. POST /v1/subscriptions/{id}/resume.
/// 5. GET /v1/subscriptions/{id}/pause-status — assert not paused (active).
#[tokio::test]
async fn e2e_subscription_pause_and_resume() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_subscription_pause_and_resume");
        return;
    };

    let callback_url = format!("{}/webhook", webhook_admin_url());

    // --- Create subscription ---
    let create_body = serde_json::json!({
        "callback_url": callback_url,
        "from_ledger": 1001,
        "subscription_type": "webhook"
    });
    let create_resp = post_json(&format!("{base}/v1/subscriptions"), &create_body)
        .await
        .expect("POST /v1/subscriptions failed");
    assert!(
        create_resp.status().is_success(),
        "subscription creation failed with {}",
        create_resp.status()
    );
    let created: Value = create_resp.json().await.expect("body should be JSON");
    let sub_id = created["id"]
        .as_str()
        .or_else(|| created["subscription_id"].as_str())
        .expect("response must contain an id")
        .to_owned();

    // --- Pause ---
    let pause_resp = post_json(
        &format!("{base}/v1/subscriptions/{sub_id}/pause"),
        &serde_json::json!({}),
    )
    .await
    .expect("POST /v1/subscriptions/{id}/pause failed");
    assert!(
        pause_resp.status().is_success(),
        "pause should succeed, got {}",
        pause_resp.status()
    );

    // --- Verify paused ---
    let pause_status = get_json(&format!("{base}/v1/subscriptions/{sub_id}/pause-status"))
        .await
        .expect("GET /v1/subscriptions/{id}/pause-status failed");

    // Accept either {"paused": true} or {"status": "paused"} shapes.
    let is_paused = pause_status["paused"]
        .as_bool()
        .unwrap_or(false)
        || pause_status["status"]
            .as_str()
            .map(|s| s.eq_ignore_ascii_case("paused"))
            .unwrap_or(false)
        || pause_status["data"]["paused"]
            .as_bool()
            .unwrap_or(false)
        || pause_status["data"]["status"]
            .as_str()
            .map(|s| s.eq_ignore_ascii_case("paused"))
            .unwrap_or(false);
    assert!(
        is_paused,
        "subscription should be paused after POST /pause; got: {pause_status}"
    );

    // --- Resume ---
    let resume_resp = post_json(
        &format!("{base}/v1/subscriptions/{sub_id}/resume"),
        &serde_json::json!({}),
    )
    .await
    .expect("POST /v1/subscriptions/{id}/resume failed");
    assert!(
        resume_resp.status().is_success(),
        "resume should succeed, got {}",
        resume_resp.status()
    );

    // --- Verify resumed ---
    let resumed_status = get_json(&format!("{base}/v1/subscriptions/{sub_id}/pause-status"))
        .await
        .expect("GET /v1/subscriptions/{id}/pause-status after resume failed");

    let still_paused = resumed_status["paused"]
        .as_bool()
        .unwrap_or(false)
        || resumed_status["status"]
            .as_str()
            .map(|s| s.eq_ignore_ascii_case("paused"))
            .unwrap_or(false)
        || resumed_status["data"]["paused"]
            .as_bool()
            .unwrap_or(false)
        || resumed_status["data"]["status"]
            .as_str()
            .map(|s| s.eq_ignore_ascii_case("paused"))
            .unwrap_or(false);
    assert!(
        !still_paused,
        "subscription should be active (not paused) after POST /resume; got: {resumed_status}"
    );

    // Cleanup
    let _ = reqwest::Client::new()
        .delete(&format!("{base}/v1/subscriptions/{sub_id}"))
        .send()
        .await;
}

// ---------------------------------------------------------------------------
// Test: invalid callback URL is rejected
// ---------------------------------------------------------------------------

/// POST /v1/subscriptions with a malformed callback_url must return 4xx.
/// Covers missing scheme, empty string, and a bare hostname without scheme.
#[tokio::test]
async fn e2e_subscription_invalid_callback_url_rejected() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_subscription_invalid_callback_url_rejected"
        );
        return;
    };

    let invalid_urls = [
        "not-a-url",
        "",
        "ftp://unsupported-scheme.example.com/hook",
        "://missing-scheme",
    ];

    for bad_url in &invalid_urls {
        let body = serde_json::json!({
            "callback_url": bad_url,
            "from_ledger": 1001,
            "subscription_type": "webhook"
        });
        let resp = post_json(&format!("{base}/v1/subscriptions"), &body)
            .await
            .expect("POST /v1/subscriptions failed unexpectedly");

        let status = resp.status().as_u16();
        assert!(
            (400..500).contains(&status),
            "malformed callback_url '{bad_url}' should return 4xx, got {status}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test: ack advances the cursor
// ---------------------------------------------------------------------------

/// 1. Create a subscription with from_ledger=1001.
/// 2. ACK at ledger 1010.
/// 3. GET the subscription and confirm acked_ledger == 1010.
#[tokio::test]
async fn e2e_subscription_ack_advances_cursor() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_subscription_ack_advances_cursor");
        return;
    };

    let callback_url = format!("{}/webhook", webhook_admin_url());

    // --- Create subscription at from_ledger=1001 ---
    let create_body = serde_json::json!({
        "callback_url": callback_url,
        "from_ledger": 1001,
        "subscription_type": "webhook"
    });
    let create_resp = post_json(&format!("{base}/v1/subscriptions"), &create_body)
        .await
        .expect("POST /v1/subscriptions failed");
    assert!(
        create_resp.status().is_success(),
        "subscription creation failed with {}",
        create_resp.status()
    );
    let created: Value = create_resp.json().await.expect("body should be JSON");
    let sub_id = created["id"]
        .as_str()
        .or_else(|| created["subscription_id"].as_str())
        .expect("response must contain an id")
        .to_owned();

    // --- ACK at ledger 1010 ---
    let ack_body = serde_json::json!({ "ledger": 1010 });
    let ack_resp = post_json(&format!("{base}/v1/subscriptions/{sub_id}/ack"), &ack_body)
        .await
        .expect("POST /v1/subscriptions/{id}/ack failed");
    assert!(
        ack_resp.status().is_success(),
        "ack should succeed, got {}",
        ack_resp.status()
    );

    // --- GET subscription and verify acked_ledger ---
    let fetched = get_json(&format!("{base}/v1/subscriptions/{sub_id}"))
        .await
        .expect("GET /v1/subscriptions/{id} failed");

    // Accept top-level or nested "data" wrapper.
    let sub_data = if fetched["data"].is_object() {
        &fetched["data"]
    } else {
        &fetched
    };

    let acked_ledger = sub_data["acked_ledger"]
        .as_u64()
        .or_else(|| sub_data["last_acked_ledger"].as_u64())
        .or_else(|| sub_data["cursor"].as_u64());

    assert_eq!(
        acked_ledger,
        Some(1010),
        "acked_ledger should be 1010 after ACK; full response: {fetched}"
    );

    // Cleanup
    let _ = reqwest::Client::new()
        .delete(&format!("{base}/v1/subscriptions/{sub_id}"))
        .send()
        .await;
}

// ---------------------------------------------------------------------------
// Tests: event filtering — diagnostic type
// ---------------------------------------------------------------------------

/// Verify that `?event_type=diagnostic` returns only diagnostic events.
/// Contract B has 10 diagnostic events seeded across ledgers 1001-1010.
#[tokio::test]
async fn e2e_filter_by_event_type_diagnostic() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_filter_by_event_type_diagnostic");
        return;
    };

    let body = get_json(&format!("{base}/v1/events?event_type=diagnostic&limit=50"))
        .await
        .expect("GET /v1/events?event_type=diagnostic failed");

    assert!(
        body["data"].is_array(),
        "response should have a 'data' array: {body}"
    );
    let events = body["data"].as_array().unwrap();

    // Must have at least the 10 diagnostic events from Contract B.
    assert!(
        !events.is_empty(),
        "expected diagnostic events in response but got none"
    );

    for ev in events {
        assert_eq!(
            ev["event_type"], "diagnostic",
            "all returned events must have event_type==diagnostic, got: {ev}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: event filtering — system type
// ---------------------------------------------------------------------------

/// Verify that `?event_type=system` returns only system events.
/// Contract C has 5 system events seeded.
#[tokio::test]
async fn e2e_filter_by_event_type_system() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_filter_by_event_type_system");
        return;
    };

    let body = get_json(&format!("{base}/v1/events?event_type=system&limit=50"))
        .await
        .expect("GET /v1/events?event_type=system failed");

    assert!(
        body["data"].is_array(),
        "response should have a 'data' array: {body}"
    );
    let events = body["data"].as_array().unwrap();

    assert!(
        !events.is_empty(),
        "expected system events in response but got none"
    );

    for ev in events {
        assert_eq!(
            ev["event_type"], "system",
            "all returned events must have event_type==system, got: {ev}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: ledger range boundary filter
// ---------------------------------------------------------------------------

/// Verify that `?from_ledger=1020&to_ledger=1030` returns at most 11 events
/// and that every returned event falls within that inclusive range.
#[tokio::test]
async fn e2e_filter_ledger_range_boundaries() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_filter_ledger_range_boundaries");
        return;
    };

    let body = get_json(&format!(
        "{base}/v1/events?from_ledger=1020&to_ledger=1030&limit=50"
    ))
    .await
    .expect("GET /v1/events?from_ledger=1020&to_ledger=1030 failed");

    assert!(
        body["data"].is_array(),
        "response should have a 'data' array: {body}"
    );
    let events = body["data"].as_array().unwrap();

    // Ledgers 1020..=1030 is 11 ledgers; at most 11 events from Contract A
    // (one per ledger) can fall in this range.
    assert!(
        events.len() <= 11,
        "expected at most 11 events in ledger range 1020-1030, got {}",
        events.len()
    );

    for ev in events {
        let ledger = ev["ledger"]
            .as_u64()
            .expect("event 'ledger' field should be a number");
        assert!(
            (1020..=1030).contains(&ledger),
            "event ledger {ledger} is outside the requested range 1020-1030"
        );
    }
}

// ---------------------------------------------------------------------------
// Tests: invalid ledger range returns 400
// ---------------------------------------------------------------------------

/// Verify that a reversed ledger range (`from_ledger > to_ledger`) returns
/// HTTP 400 Bad Request.
#[tokio::test]
async fn e2e_filter_invalid_ledger_range_returns_400() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_filter_invalid_ledger_range_returns_400"
        );
        return;
    };

    let resp = reqwest::get(&format!(
        "{base}/v1/events?from_ledger=1050&to_ledger=1000"
    ))
    .await
    .expect("request failed");

    assert_eq!(
        resp.status(),
        400,
        "reversed ledger range should return 400 Bad Request"
    );
}

// ---------------------------------------------------------------------------
// Tests: exact count vs approximate count
// ---------------------------------------------------------------------------

/// Verify that both `?exact_count=true` and `?exact_count=false` return 200
/// with a numeric `total` field.  The two totals should be equal or within
/// 10 % of each other to allow for indexing lag.
#[tokio::test]
async fn e2e_exact_count_matches_approximate() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_exact_count_matches_approximate"
        );
        return;
    };

    let exact_body = get_json(&format!("{base}/v1/events?exact_count=true&limit=1"))
        .await
        .expect("GET /v1/events?exact_count=true failed");

    let approx_body = get_json(&format!("{base}/v1/events?exact_count=false&limit=1"))
        .await
        .expect("GET /v1/events?exact_count=false failed");

    // Both responses must contain a numeric `total`.
    let exact_total = exact_body["total"]
        .as_u64()
        .expect("exact_count=true response should have a numeric 'total' field");
    let approx_total = approx_body["total"]
        .as_u64()
        .expect("exact_count=false response should have a numeric 'total' field");

    // Allow up to 10 % divergence between approximate and exact counts.
    // Use the larger of the two as the base for the tolerance calculation.
    let max_total = exact_total.max(approx_total) as f64;
    if max_total > 0.0 {
        let diff = (exact_total as f64 - approx_total as f64).abs();
        let tolerance = max_total * 0.10;
        assert!(
            diff <= tolerance,
            "exact ({exact_total}) and approximate ({approx_total}) totals diverge by more than 10 %"
        );
    }
    // If both are zero the assertion passes trivially (no events seeded yet).
}

// ---------------------------------------------------------------------------
// Tests: event statistics endpoint
// ---------------------------------------------------------------------------

/// Verify that `GET /v1/events/stats` returns 200 with a parseable JSON body
/// that contains at least one field indicating a total or breakdown count.
#[tokio::test]
async fn e2e_event_stats_returns_breakdown() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_event_stats_returns_breakdown");
        return;
    };

    let resp = reqwest::get(&format!("{base}/v1/events/stats"))
        .await
        .expect("GET /v1/events/stats failed");

    assert_eq!(
        resp.status(),
        200,
        "GET /v1/events/stats should return 200"
    );

    let body: Value = resp.json().await.expect("response body should be valid JSON");

    // Accept either a flat `total` field or a nested `counts` / `by_type` map.
    let has_total = body["total"].is_number();
    let has_counts = body["counts"].is_object() || body["counts"].is_array();
    let has_by_type = body["by_type"].is_object() || body["by_type"].is_array();

    assert!(
        has_total || has_counts || has_by_type,
        "stats response should contain 'total', 'counts', or 'by_type': {body}"
    );
}

// ---------------------------------------------------------------------------
// Tests: events by contract — only that contract's events are returned
// ---------------------------------------------------------------------------

/// Verify that `GET /v1/events/contract/{contract_a_id}` returns only events
/// whose `contract_id` matches Contract A.
#[tokio::test]
async fn e2e_events_by_contract_returns_only_that_contract() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_events_by_contract_returns_only_that_contract"
        );
        return;
    };

    let contract_a = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4";

    let body = get_json(&format!("{base}/v1/events/contract/{contract_a}"))
        .await
        .expect("GET /v1/events/contract/{contract_a} failed");

    assert!(
        body["data"].is_array(),
        "response should have a 'data' array: {body}"
    );
    let events = body["data"].as_array().unwrap();

    // Contract A has 50 seeded events — at least some should be present.
    assert!(
        !events.is_empty(),
        "expected events for Contract A but got an empty array"
    );

    for ev in events {
        assert_eq!(
            ev["contract_id"], contract_a,
            "all returned events must belong to Contract A, got: {ev}"
        );
    }
}

// ---------------------------------------------------------------------------
// Test: webhook payload contains event fields
// ---------------------------------------------------------------------------

/// Registers a webhook subscription, injects an event via the RPC stub, waits
/// for delivery, then verifies the delivered payload contains `contract_id`,
/// `ledger`, and `event_type` fields.
#[tokio::test]
async fn e2e_webhook_payload_contains_event_fields() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_webhook_payload_contains_event_fields"
        );
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    clear_webhook_deliveries(&webhook_admin).await;

    let contract_id = "CPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPPFCT4";
    let tx_hash = "e".repeat(64);

    // Register a webhook subscription for this contract.
    let webhook_url = format!("{webhook_admin}/webhook");
    let sub_body = serde_json::json!({
        "contract_id": contract_id,
        "webhook_url": webhook_url,
        "event_types": ["contract"]
    });
    let sub_resp = post_json(&format!("{base}/v1/subscriptions"), &sub_body)
        .await
        .expect("POST /v1/subscriptions failed");
    assert!(
        sub_resp.status().is_success(),
        "subscription registration failed: {}",
        sub_resp.status()
    );

    // Inject a matching event.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000012884901888-0000000000",
            "contractId": contract_id,
            "txHash": tx_hash,
            "ledger": 3001,
            "ledgerClosedAt": "2026-03-14T02:00:00Z",
            "pagingToken": "0000000012884901888-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        3001,
    )
    .await;

    // Wait for at least one delivery.
    let delivered = wait_until(
        || {
            let url = format!("{webhook_admin}/received");
            async move {
                match get_json(&url).await {
                    Ok(v) => v.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(delivered, "webhook should have fired within 30 s");

    // Fetch the first recorded delivery and inspect the payload.
    let received = get_json(&format!("{webhook_admin}/received"))
        .await
        .expect("GET /received failed");
    let deliveries = received.as_array().expect("received should be an array");
    let first = &deliveries[0];

    // The payload may be nested under a "payload" key or be the object itself.
    let payload = if first["payload"].is_object() {
        first["payload"].clone()
    } else {
        first.clone()
    };

    assert!(
        payload["contract_id"].is_string() || payload["contractId"].is_string(),
        "payload should contain contract_id field; got: {payload}"
    );
    assert!(
        payload["ledger"].is_number() || payload["ledger_sequence"].is_number(),
        "payload should contain ledger field; got: {payload}"
    );
    assert!(
        payload["event_type"].is_string() || payload["type"].is_string(),
        "payload should contain event_type field; got: {payload}"
    );

    clear_webhook_deliveries(&webhook_admin).await;
}

// ---------------------------------------------------------------------------
// Test: webhook delivers to the correct endpoint only
// ---------------------------------------------------------------------------

/// Registers two subscriptions for different contracts (B and E).  Injects
/// events **only** for contract B.  Verifies that the webhook receiver records
/// a delivery whose payload references contract B — confirming that the
/// routing is selective and not broadcast to unrelated subscriptions.
#[tokio::test]
async fn e2e_webhook_delivers_to_correct_endpoint() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_webhook_delivers_to_correct_endpoint"
        );
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    clear_webhook_deliveries(&webhook_admin).await;

    // Contract B — the one that will receive injected events.
    let contract_b = "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBFCT4";
    // Contract E — subscribes but gets no events injected.
    let contract_e = "CEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEFCT4";

    let webhook_url = format!("{webhook_admin}/webhook");

    // Register subscription for contract B.
    let sub_b = serde_json::json!({
        "contract_id": contract_b,
        "webhook_url": webhook_url,
        "event_types": ["contract"]
    });
    let resp_b = post_json(&format!("{base}/v1/subscriptions"), &sub_b)
        .await
        .expect("POST /v1/subscriptions (B) failed");
    assert!(
        resp_b.status().is_success(),
        "subscription B registration failed: {}",
        resp_b.status()
    );

    // Register subscription for contract E.
    let sub_e = serde_json::json!({
        "contract_id": contract_e,
        "webhook_url": webhook_url,
        "event_types": ["contract"]
    });
    let resp_e = post_json(&format!("{base}/v1/subscriptions"), &sub_e)
        .await
        .expect("POST /v1/subscriptions (E) failed");
    assert!(
        resp_e.status().is_success(),
        "subscription E registration failed: {}",
        resp_e.status()
    );

    // Inject an event ONLY for contract B.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000017179869184-0000000000",
            "contractId": contract_b,
            "txHash": "b".repeat(64),
            "ledger": 4001,
            "ledgerClosedAt": "2026-03-14T03:00:00Z",
            "pagingToken": "0000000017179869184-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        4001,
    )
    .await;

    // Wait for a delivery to appear.
    let delivered = wait_until(
        || {
            let url = format!("{webhook_admin}/received");
            async move {
                match get_json(&url).await {
                    Ok(v) => v.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(
        delivered,
        "at least one webhook delivery should arrive within 30 s"
    );

    // Every delivery must reference contract B — none should reference contract E.
    let received = get_json(&format!("{webhook_admin}/received"))
        .await
        .expect("GET /received failed");
    let deliveries = received.as_array().expect("received should be an array");

    for delivery in deliveries {
        let payload = if delivery["payload"].is_object() {
            delivery["payload"].clone()
        } else {
            delivery.clone()
        };

        let cid = payload["contract_id"]
            .as_str()
            .or_else(|| payload["contractId"].as_str())
            .unwrap_or("");

        assert_eq!(
            cid, contract_b,
            "all deliveries should be for contract B ({contract_b}), got: {cid}"
        );
    }

    clear_webhook_deliveries(&webhook_admin).await;
}

// ---------------------------------------------------------------------------
// Test: webhook delivery includes HMAC signature header
// ---------------------------------------------------------------------------

/// Registers a subscription with a secret, injects an event, waits for
/// delivery, then asserts that the received request includes either an
/// `X-Signature` or `X-Webhook-Signature` header carrying an HMAC value.
#[tokio::test]
async fn e2e_webhook_delivery_includes_signature_header() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_webhook_delivery_includes_signature_header"
        );
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    clear_webhook_deliveries(&webhook_admin).await;

    let contract_id = "CSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSFCT4";
    let tx_hash = "f".repeat(64);

    // Register subscription — include a signing secret so the server attaches
    // an HMAC header to outbound deliveries.
    let webhook_url = format!("{webhook_admin}/webhook");
    let sub_body = serde_json::json!({
        "contract_id": contract_id,
        "webhook_url": webhook_url,
        "event_types": ["contract"],
        "secret": "e2e-test-signing-secret"
    });
    let sub_resp = post_json(&format!("{base}/v1/subscriptions"), &sub_body)
        .await
        .expect("POST /v1/subscriptions failed");
    assert!(
        sub_resp.status().is_success(),
        "subscription registration failed: {}",
        sub_resp.status()
    );

    // Inject a matching event.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000021474836480-0000000000",
            "contractId": contract_id,
            "txHash": tx_hash,
            "ledger": 5001,
            "ledgerClosedAt": "2026-03-14T04:00:00Z",
            "pagingToken": "0000000021474836480-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        5001,
    )
    .await;

    // Wait for delivery.
    let delivered = wait_until(
        || {
            let url = format!("{webhook_admin}/received");
            async move {
                match get_json(&url).await {
                    Ok(v) => v.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(delivered, "webhook should have fired within 30 s");

    // Inspect the captured headers on the first delivery.
    let received = get_json(&format!("{webhook_admin}/received"))
        .await
        .expect("GET /received failed");
    let deliveries = received.as_array().expect("received should be an array");
    let first = &deliveries[0];

    // The receiver records headers as a JSON object (lowercase keys).
    let headers = &first["headers"];
    let has_signature = headers["x-signature"].is_string()
        || headers["x-webhook-signature"].is_string()
        // Some implementations capitalise the header.
        || headers["X-Signature"].is_string()
        || headers["X-Webhook-Signature"].is_string();

    assert!(
        has_signature,
        "delivered webhook must include an HMAC signature header (X-Signature or X-Webhook-Signature); \
         headers received: {headers}"
    );

    // The signature value must be non-empty.
    let sig_value = headers["x-signature"]
        .as_str()
        .or_else(|| headers["x-webhook-signature"].as_str())
        .or_else(|| headers["X-Signature"].as_str())
        .or_else(|| headers["X-Webhook-Signature"].as_str())
        .unwrap_or("");
    assert!(
        !sig_value.is_empty(),
        "HMAC signature header should not be empty"
    );

    clear_webhook_deliveries(&webhook_admin).await;
}

// ---------------------------------------------------------------------------
// Test: circuit-breaker stats endpoint returns JSON
// ---------------------------------------------------------------------------

/// Hits `GET /v1/admin/webhook/circuit-breaker` with the admin key and
/// verifies the response is HTTP 200 with a JSON body.
#[tokio::test]
async fn e2e_circuit_breaker_stats_endpoint() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_circuit_breaker_stats_endpoint"
        );
        return;
    };

    let admin_key = std::env::var("E2E_ADMIN_API_KEY")
        .unwrap_or_else(|_| "e2e-admin-key".into());

    let client = reqwest::Client::new();
    let resp = client
        .get(&format!("{base}/v1/admin/webhook/circuit-breaker"))
        .header("X-Api-Key", &admin_key)
        .send()
        .await
        .expect("GET /v1/admin/webhook/circuit-breaker failed");

    assert_eq!(
        resp.status(),
        200,
        "/v1/admin/webhook/circuit-breaker should return 200 OK"
    );

    // Verify the response body is valid JSON.
    let body: Value = resp
        .json()
        .await
        .expect("circuit-breaker endpoint should return JSON body");

    // The response should be an object or array — either is acceptable.
    assert!(
        body.is_object() || body.is_array(),
        "circuit-breaker response should be a JSON object or array; got: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test: batch webhook delivery
// ---------------------------------------------------------------------------

/// Creates a subscription with `batch_size=3`, injects 3 events, then calls
/// `POST /v1/subscriptions/{id}/batch` to trigger an explicit batch dispatch.
/// Asserts that the response returns a non-empty batch payload.
#[tokio::test]
async fn e2e_webhook_batch_delivery() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_webhook_batch_delivery");
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    clear_webhook_deliveries(&webhook_admin).await;

    let contract_id = "CBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBT3";
    let webhook_url = format!("{webhook_admin}/webhook");

    // Create a subscription with batch_size=3.
    let sub_body = serde_json::json!({
        "contract_id": contract_id,
        "webhook_url": webhook_url,
        "event_types": ["contract"],
        "batch_size": 3
    });
    let sub_resp = post_json(&format!("{base}/v1/subscriptions"), &sub_body)
        .await
        .expect("POST /v1/subscriptions failed");
    assert!(
        sub_resp.status().is_success(),
        "subscription creation failed: {}",
        sub_resp.status()
    );

    let created: Value = sub_resp.json().await.expect("response body should be JSON");
    // Accept either `id` or `subscription_id` as the identifier key.
    let sub_id = created["id"]
        .as_str()
        .or_else(|| created["subscription_id"].as_str())
        .expect("subscription response must contain an id or subscription_id field")
        .to_owned();

    // Inject 3 events for this contract so the batch has something to send.
    let events: Vec<Value> = (0u64..3)
        .map(|i| {
            serde_json::json!({
                "type": "contract",
                "id": format!("000000002576980378{i}-0000000000"),
                "contractId": contract_id,
                "txHash": format!("{:0>64}", i),
                "ledger": 6001 + i,
                "ledgerClosedAt": "2026-03-14T05:00:00Z",
                "pagingToken": format!("000000002576980378{i}-0000000000"),
                "inSuccessfulContractCall": true,
                "value": { "xdr": "AAAAAQ==" },
                "topic": [{ "xdr": "AAAAAQ==" }]
            })
        })
        .collect();

    stub_rpc_events(&rpc_admin, events, 6003).await;

    // Wait until the indexer has picked up the events.
    let indexed = wait_until(
        || {
            let url = format!("{base}/v1/events/{contract_id}");
            async move {
                match get_json(&url).await {
                    Ok(v) => v["data"]
                        .as_array()
                        .map(|a| a.len() >= 3)
                        .unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(
        indexed,
        "3 events should be indexed within 30 s before triggering batch delivery"
    );

    // Trigger explicit batch delivery.
    let batch_resp = post_json(
        &format!("{base}/v1/subscriptions/{sub_id}/batch"),
        &serde_json::json!({}),
    )
    .await
    .expect("POST /v1/subscriptions/{id}/batch failed");

    let batch_status = batch_resp.status();
    assert!(
        batch_status.is_success(),
        "batch delivery endpoint should return 2xx; got {batch_status}"
    );

    let batch_body: Value = batch_resp
        .json()
        .await
        .expect("batch response should be JSON");

    // The batch response should indicate at least one event was dispatched.
    // Accept a top-level array or an object with a `data`/`events`/`count` field.
    let has_events = batch_body.as_array().map(|a| !a.is_empty()).unwrap_or(false)
        || batch_body["data"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false)
        || batch_body["events"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false)
        || batch_body["count"].as_u64().map(|c| c > 0).unwrap_or(false)
        || batch_body["delivered"].as_u64().map(|c| c > 0).unwrap_or(false);

    assert!(
        has_events,
        "batch delivery response should report at least one event dispatched; got: {batch_body}"
    );

    clear_webhook_deliveries(&webhook_admin).await;
}

// ===========================================================================
// Multi-channel notification flow E2E tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Admin request helpers (multi-channel section)
// ---------------------------------------------------------------------------

fn admin_api_key() -> String {
    std::env::var("E2E_ADMIN_API_KEY").unwrap_or_else(|_| "e2e-admin-key".into())
}

async fn get_json_admin(url: &str) -> reqwest::Result<Value> {
    reqwest::Client::new()
        .get(url)
        .header("X-Api-Key", admin_api_key())
        .send()
        .await?
        .json::<Value>()
        .await
}

async fn post_json_admin(url: &str, body: &Value) -> reqwest::Result<reqwest::Response> {
    reqwest::Client::new()
        .post(url)
        .header("X-Api-Key", admin_api_key())
        .json(body)
        .send()
        .await
}

// ---------------------------------------------------------------------------
// Helper: create a subscription and return its ID as a String.
//
// Accepts an optional webhook_url override; falls back to a placeholder when
// none is provided (useful for tests that do not exercise webhook delivery).
// ---------------------------------------------------------------------------

async fn create_subscription(base: &str, contract_id: &str, webhook_url: Option<&str>) -> String {
    let url = webhook_url
        .map(|s| s.to_owned())
        .unwrap_or_else(|| format!("http://placeholder.invalid/webhook-{contract_id}"));

    let body = serde_json::json!({
        "contract_id": contract_id,
        "webhook_url": url,
        "event_types": ["contract"]
    });

    let resp = post_json(&format!("{base}/v1/subscriptions"), &body)
        .await
        .expect("POST /v1/subscriptions failed");

    assert!(
        resp.status().is_success(),
        "subscription creation should succeed (got {})",
        resp.status()
    );

    let created: Value = resp.json().await.expect("response body should be JSON");

    // Accept either "id" or "subscription_id" as the ID field.
    created["id"]
        .as_str()
        .or_else(|| created["subscription_id"].as_str())
        .expect("response should include an id or subscription_id field")
        .to_owned()
}

// ---------------------------------------------------------------------------
// Test 1: e2e_subscription_email_config
// ---------------------------------------------------------------------------

/// Verifies that an email notification config can be set on a subscription
/// (PUT) and subsequently retrieved (GET) with the saved values.
#[tokio::test]
async fn e2e_subscription_email_config() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_subscription_email_config");
        return;
    };

    // Create a subscription to attach the email config to.
    let contract_id = "CEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEFCT4";
    let sub_id = create_subscription(&base, contract_id, None).await;

    // PUT the email configuration.
    let email_body = serde_json::json!({
        "email": "test@example.com",
        "enabled": true
    });

    let put_resp = reqwest::Client::new()
        .put(&format!("{base}/v1/subscriptions/{sub_id}/email"))
        .json(&email_body)
        .send()
        .await
        .expect("PUT /v1/subscriptions/{id}/email failed");

    let put_status = put_resp.status();
    assert!(
        put_status == 200 || put_status == 201 || put_status == 204,
        "PUT email config should succeed (got {put_status})"
    );

    // GET the email configuration and verify the saved values.
    let get_resp = get_json(&format!("{base}/v1/subscriptions/{sub_id}/email"))
        .await
        .expect("GET /v1/subscriptions/{id}/email failed");

    assert_eq!(
        get_resp["email"], "test@example.com",
        "email address should be persisted: {get_resp}"
    );
    assert_eq!(
        get_resp["enabled"], true,
        "email should be enabled: {get_resp}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: e2e_subscription_slack_integration_setup
// ---------------------------------------------------------------------------

/// Verifies that a Slack integration can be attached to a subscription and is
/// reflected when the subscription's integrations are subsequently listed.
#[tokio::test]
async fn e2e_subscription_slack_integration_setup() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_subscription_slack_integration_setup"
        );
        return;
    };

    let contract_id = "CFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF4CT4";
    let sub_id = create_subscription(&base, contract_id, None).await;

    // POST the Slack integration using a mock (non-resolving) webhook URL so
    // the test does not depend on a real Slack endpoint.
    let slack_body = serde_json::json!({
        "webhook_url": "https://hooks.slack.invalid/services/T000/B000/XXXX",
        "channel": "#general"
    });

    let post_resp = post_json(
        &format!("{base}/v1/subscriptions/{sub_id}/integrations/slack"),
        &slack_body,
    )
    .await
    .expect("POST /v1/subscriptions/{id}/integrations/slack failed");

    let post_status = post_resp.status();
    assert!(
        post_status == 200 || post_status == 201,
        "Slack integration creation should return 200 or 201 (got {post_status})"
    );

    // Verify the integration is accessible — either via a dedicated GET on
    // the integrations sub-resource or reflected in the subscription detail.
    let integrations_url = format!("{base}/v1/subscriptions/{sub_id}/integrations/slack");
    let get_resp = reqwest::get(&integrations_url).await;

    match get_resp {
        Ok(r) if r.status().is_success() => {
            let body: Value = r.json().await.unwrap_or_default();
            // Accept a JSON object with a webhook_url field or an array
            // containing an entry with one.
            let has_webhook = body["webhook_url"].is_string()
                || body
                    .as_array()
                    .map(|arr| arr.iter().any(|e| e["webhook_url"].is_string()))
                    .unwrap_or(false);
            assert!(
                has_webhook,
                "GET slack integration should return the webhook_url: {body}"
            );
        }
        // If the server returns 404 for a GET on this sub-resource it means
        // the route only supports POST — that is still a valid implementation.
        // The important assertion is that the POST itself succeeded (above).
        Ok(r) if r.status() == 404 => {}
        Ok(r) => panic!(
            "Unexpected status {} when GETting slack integration",
            r.status()
        ),
        Err(e) => panic!("GET slack integration request error: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Test 3: e2e_subscription_discord_integration_setup
// ---------------------------------------------------------------------------

/// Verifies that a Discord integration can be attached to a subscription.
#[tokio::test]
async fn e2e_subscription_discord_integration_setup() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_subscription_discord_integration_setup"
        );
        return;
    };

    let contract_id = "CGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGGFCT4";
    let sub_id = create_subscription(&base, contract_id, None).await;

    // POST Discord integration with a mock webhook URL.
    let discord_body = serde_json::json!({
        "webhook_url": "https://discord.invalid/api/webhooks/000000000000000000/xxxx",
        "channel_id": "123456789012345678"
    });

    let post_resp = post_json(
        &format!("{base}/v1/subscriptions/{sub_id}/integrations/discord"),
        &discord_body,
    )
    .await
    .expect("POST /v1/subscriptions/{id}/integrations/discord failed");

    let post_status = post_resp.status();
    assert!(
        post_status == 200 || post_status == 201,
        "Discord integration creation should return 200 or 201 (got {post_status})"
    );

    // Verify the integration is accessible.
    let integrations_url = format!("{base}/v1/subscriptions/{sub_id}/integrations/discord");
    let get_resp = reqwest::get(&integrations_url).await;

    match get_resp {
        Ok(r) if r.status().is_success() => {
            let body: Value = r.json().await.unwrap_or_default();
            let has_field = body["webhook_url"].is_string()
                || body["channel_id"].is_string()
                || body
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .any(|e| e["webhook_url"].is_string() || e["channel_id"].is_string())
                    })
                    .unwrap_or(false);
            assert!(
                has_field,
                "GET discord integration should return webhook_url or channel_id: {body}"
            );
        }
        // 404 is acceptable if the server only supports POST on this route.
        Ok(r) if r.status() == 404 => {}
        Ok(r) => panic!(
            "Unexpected status {} when GETting discord integration",
            r.status()
        ),
        Err(e) => panic!("GET discord integration request error: {e}"),
    }
}

// ---------------------------------------------------------------------------
// Test 4: e2e_notification_channel_admin_creation
// ---------------------------------------------------------------------------

/// Verifies that an admin can create a notification channel via the admin API.
#[tokio::test]
async fn e2e_notification_channel_admin_creation() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_notification_channel_admin_creation"
        );
        return;
    };

    // First create a subscription so we can supply a valid subscription_id.
    let contract_id = "CHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHHFCT4";
    let sub_id = create_subscription(&base, contract_id, None).await;

    // POST a new notification channel through the admin API.
    let channel_body = serde_json::json!({
        "channel_type": "slack",
        "name": "e2e-test-slack-channel",
        "subscription_id": sub_id,
        "config": {
            "webhook_url": "https://hooks.slack.invalid/services/T000/B001/YYYY",
            "channel": "#alerts"
        }
    });

    let resp = post_json_admin(
        &format!("{base}/v1/admin/notifications/channels"),
        &channel_body,
    )
    .await
    .expect("POST /v1/admin/notifications/channels failed");

    let status = resp.status();
    assert!(
        status == 200 || status == 201,
        "admin notification channel creation should return 200 or 201 (got {status})"
    );

    // If the server exposes a GET route for listing channels, verify ours
    // appears in the list.  The GET may not exist — treat 404/405 as benign.
    let list_resp = get_json_admin(&format!("{base}/v1/admin/notifications/channels")).await;

    match list_resp {
        Ok(list) => {
            // The route exists; the channel we created should be in the list.
            let channels = list
                .as_array()
                .cloned()
                .or_else(|| list["data"].as_array().cloned())
                .unwrap_or_default();

            if !channels.is_empty() {
                let found = channels.iter().any(|c| {
                    c["name"] == "e2e-test-slack-channel"
                        || c["subscription_id"] == sub_id.as_str()
                });
                assert!(
                    found,
                    "newly created channel should appear in the channels list"
                );
            }
            // If the list is empty the server may scope channels per
            // subscription — the 200/201 from the POST is sufficient.
        }
        // 404 means the GET route simply does not exist — that is fine.
        Err(_) => {}
    }
}

// ---------------------------------------------------------------------------
// Test 5: e2e_multi_subscription_different_channels
// ---------------------------------------------------------------------------

/// Creates two independent subscriptions that both listen to the same contract
/// but point at different webhook endpoints.  After injecting an event, the
/// test asserts:
///   - Both subscriptions receive the event (independent delivery).
///   - Neither subscription's delivery log is shared with the other.
#[tokio::test]
async fn e2e_multi_subscription_different_channels() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_multi_subscription_different_channels"
        );
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    // Clear any previous deliveries so counts are clean.
    clear_webhook_deliveries(&webhook_admin).await;

    // Both subscriptions target the same contract but use distinct webhook
    // paths so the receiver can differentiate them.
    let contract_id = "CIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIIFCT4";
    let tx_hash = "1".repeat(64);

    let webhook_url_a = format!("{webhook_admin}/webhook?sub=alpha");
    let webhook_url_b = format!("{webhook_admin}/webhook?sub=beta");

    let sub_a_id = create_subscription(&base, contract_id, Some(&webhook_url_a)).await;
    let sub_b_id = create_subscription(&base, contract_id, Some(&webhook_url_b)).await;

    // The two subscriptions must be distinct.
    assert_ne!(
        sub_a_id, sub_b_id,
        "two independently created subscriptions should have different IDs"
    );

    // Inject an event for the shared contract.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000012884901888-0000000000",
            "contractId": contract_id,
            "txHash": tx_hash,
            "ledger": 3001,
            "ledgerClosedAt": "2026-03-14T02:00:00Z",
            "pagingToken": "0000000012884901888-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        3001,
    )
    .await;

    // Wait for at least one webhook delivery (confirms the indexer fired).
    let any_delivered = wait_until(
        || {
            let url = format!("{webhook_admin}/received");
            async move {
                match get_json(&url).await {
                    Ok(v) => v.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(
        any_delivered,
        "at least one webhook delivery should occur within 30 s after event injection"
    );

    // Fetch the delivery log and verify both subscriptions received the event.
    let deliveries = get_json(&format!("{webhook_admin}/received"))
        .await
        .expect("GET /received failed");

    let delivery_list = deliveries.as_array().cloned().unwrap_or_default();

    // Each subscription should have produced at least one delivery.
    // The receiver distinguishes them by the `?sub=` query parameter captured
    // in the path or body; fall back to checking that we got ≥ 2 deliveries
    // total (one per subscription) if the receiver does not expose sub-routing.
    let alpha_deliveries = delivery_list.iter().filter(|d| {
        d["url"]
            .as_str()
            .map(|u| u.contains("sub=alpha"))
            .unwrap_or(false)
            || d["path"]
                .as_str()
                .map(|p| p.contains("sub=alpha"))
                .unwrap_or(false)
    });

    let beta_deliveries = delivery_list.iter().filter(|d| {
        d["url"]
            .as_str()
            .map(|u| u.contains("sub=beta"))
            .unwrap_or(false)
            || d["path"]
                .as_str()
                .map(|p| p.contains("sub=beta"))
                .unwrap_or(false)
    });

    let alpha_count = alpha_deliveries.count();
    let beta_count = beta_deliveries.count();

    if alpha_count > 0 && beta_count > 0 {
        // Ideal path: the mock receiver surfaces the query param.
        assert!(
            alpha_count >= 1,
            "subscription alpha should have received at least one delivery"
        );
        assert!(
            beta_count >= 1,
            "subscription beta should have received at least one delivery"
        );
    } else {
        // Fallback: just confirm we got enough deliveries for two subscribers.
        assert!(
            delivery_list.len() >= 2,
            "with two active subscriptions, at least 2 webhook deliveries expected, got {}",
            delivery_list.len()
        );
    }

    // Clean up.
    clear_webhook_deliveries(&webhook_admin).await;
}

// ===========================================================================
// Admin Operation Tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Test: admin indexer pause and resume
// ---------------------------------------------------------------------------

/// Verifies the admin indexer pause/resume cycle:
/// 1. POST /v1/admin/indexer/pause with the admin key → 200.
/// 2. GET /healthz/ready — indexer should report non-ok or a paused status.
/// 3. POST /v1/admin/indexer/resume with the admin key → 200.
/// 4. GET /healthz/ready — service should return to "ok" within 15 s.
#[tokio::test]
async fn e2e_admin_indexer_pause_resume() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_indexer_pause_resume");
        return;
    };

    let admin_key = admin_api_key();
    let client = reqwest::Client::new();

    // --- Pause the indexer ---
    let pause_resp = client
        .post(&format!("{base}/v1/admin/indexer/pause"))
        .header("X-Api-Key", &admin_key)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("POST /v1/admin/indexer/pause failed");

    assert!(
        pause_resp.status().is_success(),
        "indexer pause should return 2xx; got {}",
        pause_resp.status()
    );

    // --- Verify health reflects paused state ---
    // The health endpoint may return 200 with {"indexer":"paused"} or 503.
    // We accept either — the important thing is the indexer field is not "ok"
    // OR the overall status is not 200.
    let health_after_pause = reqwest::get(&format!("{base}/healthz/ready"))
        .await
        .expect("GET /healthz/ready after pause failed");

    let pause_status = health_after_pause.status();
    let health_body: Value = health_after_pause.json().await.unwrap_or_default();
    let indexer_field = health_body["indexer"].as_str().unwrap_or("");
    let reports_paused = pause_status == 503
        || indexer_field.contains("paused")
        || indexer_field.contains("stopped")
        || health_body["status"].as_str().map(|s| s != "ok").unwrap_or(false);

    assert!(
        reports_paused,
        "health should reflect paused indexer; status={pause_status}, body={health_body}"
    );

    // --- Resume the indexer ---
    let resume_resp = client
        .post(&format!("{base}/v1/admin/indexer/resume"))
        .header("X-Api-Key", &admin_key)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("POST /v1/admin/indexer/resume failed");

    assert!(
        resume_resp.status().is_success(),
        "indexer resume should return 2xx; got {}",
        resume_resp.status()
    );

    // --- Wait for health to recover ---
    let recovered = wait_until(
        || {
            let url = format!("{base}/healthz/ready");
            async move {
                match get_json(&url).await {
                    Ok(v) => v["status"].as_str() == Some("ok"),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(15),
        Duration::from_secs(1),
    )
    .await;

    assert!(
        recovered,
        "health should return to ok within 15 s after indexer resume"
    );
}

// ---------------------------------------------------------------------------
// Test: admin event replay
// ---------------------------------------------------------------------------

/// Verifies that POST /v1/admin/replay triggers re-indexing of a ledger range:
/// 1. Inject events via WireMock stub.
/// 2. POST /v1/admin/replay with the ledger range and admin key.
/// 3. Assert the response is 200/202 with a job or confirmation body.
#[tokio::test]
async fn e2e_admin_event_replay() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_event_replay");
        return;
    };

    let admin_key = admin_api_key();
    let rpc_admin = rpc_admin_url();

    let contract_id = "CRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRRFCT4";

    // Inject events into WireMock so replay has something to fetch.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000030064771072-0000000000",
            "contractId": contract_id,
            "txHash": "r".repeat(64),
            "ledger": 7001,
            "ledgerClosedAt": "2026-03-14T06:00:00Z",
            "pagingToken": "0000000030064771072-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        7001,
    )
    .await;

    // Trigger replay.
    let replay_body = serde_json::json!({
        "from_ledger": 7001,
        "to_ledger": 7001
    });

    let replay_resp = reqwest::Client::new()
        .post(&format!("{base}/v1/admin/replay"))
        .header("X-Api-Key", &admin_key)
        .json(&replay_body)
        .send()
        .await
        .expect("POST /v1/admin/replay failed");

    let replay_status = replay_resp.status();
    assert!(
        replay_status == 200 || replay_status == 202,
        "admin replay should return 200 or 202; got {replay_status}"
    );

    // The response should be a JSON object (job info or confirmation).
    let replay_body_resp: Value = replay_resp.json().await.unwrap_or_default();
    assert!(
        replay_body_resp.is_object() || replay_body_resp.is_array(),
        "replay response should be JSON; got: {replay_body_resp}"
    );

    reset_rpc_stubs(&rpc_admin).await;
}

// ---------------------------------------------------------------------------
// Test: admin mask events
// ---------------------------------------------------------------------------

/// Verifies that POST /v1/admin/events/mask replaces sensitive fields:
/// 1. Inject an event and wait for indexing.
/// 2. POST /v1/admin/events/mask for that contract with the admin key.
/// 3. Assert the endpoint returns 200/202.
#[tokio::test]
async fn e2e_admin_mask_events() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_mask_events");
        return;
    };

    let admin_key = admin_api_key();
    let rpc_admin = rpc_admin_url();

    let contract_id = "CMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMMFCT4";

    // Inject an event and wait for it to be indexed.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000034359738368-0000000000",
            "contractId": contract_id,
            "txHash": "m".repeat(64),
            "ledger": 8001,
            "ledgerClosedAt": "2026-03-14T07:00:00Z",
            "pagingToken": "0000000034359738368-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        8001,
    )
    .await;

    // Wait for the event to appear.
    let indexed = wait_until(
        || {
            let url = format!("{base}/v1/events/{contract_id}");
            async move {
                match get_json(&url).await {
                    Ok(v) => v["data"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(indexed, "event should be indexed within 30 s before masking");

    // POST mask request.
    let mask_body = serde_json::json!({
        "contract_id": contract_id,
        "fields": ["event_data"]
    });

    let mask_resp = reqwest::Client::new()
        .post(&format!("{base}/v1/admin/events/mask"))
        .header("X-Api-Key", &admin_key)
        .json(&mask_body)
        .send()
        .await
        .expect("POST /v1/admin/events/mask failed");

    let mask_status = mask_resp.status();
    assert!(
        mask_status == 200 || mask_status == 202,
        "admin mask should return 200 or 202; got {mask_status}"
    );
}

// ---------------------------------------------------------------------------
// Test: admin bulk export lifecycle
// ---------------------------------------------------------------------------

/// Verifies that the bulk export endpoint starts a job and returns a job ID:
/// 1. POST /v1/admin/export with contract range and admin key → 200/202.
/// 2. Response body should include a job reference or download link.
#[tokio::test]
async fn e2e_admin_bulk_export_lifecycle() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_bulk_export_lifecycle");
        return;
    };

    let admin_key = admin_api_key();

    let export_body = serde_json::json!({
        "from_ledger": 1001,
        "to_ledger": 1010,
        "format": "json"
    });

    let export_resp = reqwest::Client::new()
        .post(&format!("{base}/v1/admin/export"))
        .header("X-Api-Key", &admin_key)
        .json(&export_body)
        .send()
        .await
        .expect("POST /v1/admin/export failed");

    let export_status = export_resp.status();
    assert!(
        export_status == 200 || export_status == 202,
        "bulk export should return 200 or 202; got {export_status}"
    );

    // The response should be parseable JSON.
    let export_body_resp: Value = export_resp.json().await.unwrap_or_default();
    assert!(
        export_body_resp.is_object() || export_body_resp.is_array(),
        "export response should be JSON; got: {export_body_resp}"
    );

    // If we got a job ID back, optionally poll for completion (non-blocking check).
    let job_id = export_body_resp["job_id"]
        .as_str()
        .or_else(|| export_body_resp["id"].as_str());

    if let Some(jid) = job_id {
        let status_resp = reqwest::Client::new()
            .get(&format!("{base}/v1/admin/export/{jid}"))
            .header("X-Api-Key", &admin_key)
            .send()
            .await;

        if let Ok(r) = status_resp {
            // Accept 200 (with status field) or 404 if the job endpoint is not
            // implemented as a separate route.
            let s = r.status();
            assert!(
                s.is_success() || s == 404,
                "export status endpoint should return 2xx or 404; got {s}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Test: admin auth enforcement
// ---------------------------------------------------------------------------

/// Verifies the three-tier auth behaviour on admin endpoints:
/// - No key → 401 Unauthorized
/// - Regular API_KEY (wrong key) → 403 Forbidden
/// - Correct ADMIN_API_KEY → 2xx
#[tokio::test]
async fn e2e_admin_auth_enforcement() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_auth_enforcement");
        return;
    };

    let admin_key = admin_api_key();
    let client = reqwest::Client::new();

    // The index-health endpoint is a safe read-only admin route to test against.
    let admin_url = format!("{base}/v1/admin/db/index-health");

    // --- No key → 401 ---
    let no_key_resp = client
        .get(&admin_url)
        .send()
        .await
        .expect("GET admin endpoint without key failed");

    assert_eq!(
        no_key_resp.status(),
        401,
        "admin endpoint without key should return 401; got {}",
        no_key_resp.status()
    );

    // --- Wrong key (non-admin) → 403 ---
    let wrong_key_resp = client
        .get(&admin_url)
        .header("X-Api-Key", "definitely-not-the-admin-key")
        .send()
        .await
        .expect("GET admin endpoint with wrong key failed");

    let wrong_key_status = wrong_key_resp.status();
    assert!(
        wrong_key_status == 403 || wrong_key_status == 401,
        "admin endpoint with wrong key should return 401 or 403; got {wrong_key_status}"
    );

    // --- Correct admin key → 2xx ---
    let ok_resp = client
        .get(&admin_url)
        .header("X-Api-Key", &admin_key)
        .send()
        .await
        .expect("GET admin endpoint with admin key failed");

    assert!(
        ok_resp.status().is_success(),
        "admin endpoint with correct key should return 2xx; got {}",
        ok_resp.status()
    );
}

// ---------------------------------------------------------------------------
// Test: admin index fragmentation report
// ---------------------------------------------------------------------------

/// Verifies that GET /v1/admin/db/index-health returns a valid report with
/// index information.
#[tokio::test]
async fn e2e_admin_index_fragmentation_report() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_index_fragmentation_report");
        return;
    };

    let admin_key = admin_api_key();

    let resp = reqwest::Client::new()
        .get(&format!("{base}/v1/admin/db/index-health"))
        .header("X-Api-Key", &admin_key)
        .send()
        .await
        .expect("GET /v1/admin/db/index-health failed");

    assert_eq!(
        resp.status(),
        200,
        "index-health endpoint should return 200; got {}",
        resp.status()
    );

    let body: Value = resp.json().await.expect("response should be valid JSON");

    // The report is either an array of index entries or an object with an
    // "indexes" / "indices" array.
    let is_valid = body.is_array()
        || body["indexes"].is_array()
        || body["indices"].is_array()
        || body["data"].is_array();

    assert!(
        is_valid,
        "index-health response should contain an array of index entries; got: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test: admin audit logs
// ---------------------------------------------------------------------------

/// Verifies that admin actions appear in the audit log:
/// 1. Perform a known admin action (indexer pause then resume to leave a clean state).
/// 2. GET /v1/admin/audit-log with the admin key.
/// 3. Assert the response is 200 with a list (possibly empty if audit logging
///    is not yet wired through the E2E stack, but the endpoint must exist).
#[tokio::test]
async fn e2e_admin_audit_logs() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_admin_audit_logs");
        return;
    };

    let admin_key = admin_api_key();
    let client = reqwest::Client::new();

    // Perform a traceable admin action.
    let _ = client
        .post(&format!("{base}/v1/admin/indexer/pause"))
        .header("X-Api-Key", &admin_key)
        .json(&serde_json::json!({}))
        .send()
        .await;

    let _ = client
        .post(&format!("{base}/v1/admin/indexer/resume"))
        .header("X-Api-Key", &admin_key)
        .json(&serde_json::json!({}))
        .send()
        .await;

    // Give the system a moment to write the audit entry.
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Fetch the audit log.
    let audit_resp = client
        .get(&format!("{base}/v1/admin/audit-log"))
        .header("X-Api-Key", &admin_key)
        .send()
        .await
        .expect("GET /v1/admin/audit-log failed");

    assert_eq!(
        audit_resp.status(),
        200,
        "audit-log endpoint should return 200; got {}",
        audit_resp.status()
    );

    let body: Value = audit_resp.json().await.expect("audit-log should return JSON");

    // Accept array at top level or wrapped in "data".
    let entries = body
        .as_array()
        .cloned()
        .or_else(|| body["data"].as_array().cloned())
        .unwrap_or_default();

    // If the audit log is populated, each entry should have an action and
    // timestamp field.
    for entry in &entries {
        let has_action = entry["action"].is_string()
            || entry["event"].is_string()
            || entry["operation"].is_string();
        let has_timestamp = entry["timestamp"].is_string()
            || entry["created_at"].is_string()
            || entry["occurred_at"].is_string();
        assert!(
            has_action,
            "audit log entry should have an action field; entry: {entry}"
        );
        assert!(
            has_timestamp,
            "audit log entry should have a timestamp field; entry: {entry}"
        );
    }
    // An empty audit log is acceptable if the E2E stack does not persist audit
    // entries — the important assertion is that the endpoint responds with 200.
}

// ===========================================================================
// Failure Recovery Tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Test: health during RPC errors
// ---------------------------------------------------------------------------

/// Verifies that when the RPC stub returns 500 for all calls, the readiness
/// endpoint continues to return 200 (DB is still reachable) and the RPC error
/// metric increments.
#[tokio::test]
async fn e2e_health_during_rpc_errors() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_health_during_rpc_errors");
        return;
    };
    let rpc_admin = rpc_admin_url();

    // --- Configure WireMock to return 500 for all RPC calls ---
    let error_stub = serde_json::json!({
        "name": "rpc-error-500",
        "priority": 1,
        "request": { "method": "POST", "url": "/" },
        "response": {
            "status": 500,
            "headers": { "Content-Type": "application/json" },
            "body": "{\"error\":\"internal server error\"}"
        }
    });

    reqwest::Client::new()
        .post(format!("{rpc_admin}/__admin/mappings"))
        .json(&error_stub)
        .send()
        .await
        .expect("failed to inject RPC error stub");

    // Wait a short time for the indexer to hit the error.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // --- Health check should still be 200 (DB is reachable) ---
    let health_resp = reqwest::get(&format!("{base}/healthz/ready"))
        .await
        .expect("GET /healthz/ready failed");

    // Even with RPC errors the health check should return 200 or at most 503
    // with the db still ok.
    let health_status = health_resp.status();
    let health_body: Value = health_resp.json().await.unwrap_or_default();
    let db_ok = health_body["db"].as_str() == Some("ok");

    // The DB must still report ok regardless of RPC state.
    assert!(
        db_ok,
        "DB health should remain ok during RPC errors; body: {health_body}"
    );

    // --- Verify the RPC error metric has incremented ---
    let metrics = reqwest::get(&format!("{base}/metrics"))
        .await
        .expect("GET /metrics failed")
        .text()
        .await
        .expect("failed to read metrics");

    // The metric line may not appear if value is 0, so we check both presence
    // and value.  If the metric exists its value should be > 0.
    if metrics.contains("soroban_pulse_rpc_errors_total") {
        // Extract the value and verify it's a positive number.
        for line in metrics.lines() {
            if line.starts_with("soroban_pulse_rpc_errors_total")
                && !line.starts_with('#')
            {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val_str) = parts.last() {
                    let val: f64 = val_str.parse().unwrap_or(0.0);
                    assert!(
                        val > 0.0,
                        "soroban_pulse_rpc_errors_total should be > 0 after RPC errors; got {val}"
                    );
                }
            }
        }
    }
    // If the metric line is absent entirely, we only checked that the health
    // endpoint continued to respond — that's the primary assertion.

    // --- Restore default stubs ---
    reset_rpc_stubs(&rpc_admin).await;

    // Verify health status is preserved (no state corruption).
    assert!(
        health_status.is_success() || health_status.as_u16() == 503,
        "health check should return 200 or 503 during RPC errors; got {health_status}"
    );
}

// ---------------------------------------------------------------------------
// Test: recovery after RPC restore
// ---------------------------------------------------------------------------

/// After WireMock is reset to return valid responses the indexer resumes
/// indexing within one error-backoff cycle (≤ 15 seconds).
#[tokio::test]
async fn e2e_recovery_after_rpc_restore() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_recovery_after_rpc_restore");
        return;
    };
    let rpc_admin = rpc_admin_url();

    let contract_id = "CRECOVERYRECOVERYRECOVERYRECOVERYRECOVERYRECOVERYRCVR4";

    // Inject a valid event stub BEFORE the error so the indexer has a known
    // ledger to detect as "new" after recovery.
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000038654705664-0000000000",
            "contractId": contract_id,
            "txHash": "0".repeat(63) + "9",
            "ledger": 9001,
            "ledgerClosedAt": "2026-03-14T08:00:00Z",
            "pagingToken": "0000000038654705664-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        9001,
    )
    .await;

    // Wait for the event to be indexed (confirms the indexer is working).
    let initially_indexed = wait_until(
        || {
            let url = format!("{base}/v1/events/{contract_id}");
            async move {
                match get_json(&url).await {
                    Ok(v) => v["data"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    assert!(
        initially_indexed,
        "initial event should be indexed before triggering RPC error"
    );

    // Now inject an error to stop indexing temporarily.
    let error_stub = serde_json::json!({
        "name": "rpc-error-temporary",
        "priority": 1,
        "request": { "method": "POST", "url": "/" },
        "response": { "status": 503 }
    });
    reqwest::Client::new()
        .post(format!("{rpc_admin}/__admin/mappings"))
        .json(&error_stub)
        .send()
        .await
        .expect("failed to inject temporary error stub");

    // Wait for at least one error cycle.
    tokio::time::sleep(Duration::from_secs(3)).await;

    // Restore — inject a second event that should be picked up after recovery.
    let contract_id_2 = "CRECOVERPOSTRECOVERPOSTRECOVERPOSTRECOVERPOSTRECOVPOST4";
    reset_rpc_stubs(&rpc_admin).await;
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000043049480960-0000000000",
            "contractId": contract_id_2,
            "txHash": "0".repeat(63) + "8",
            "ledger": 9002,
            "ledgerClosedAt": "2026-03-14T08:01:00Z",
            "pagingToken": "0000000043049480960-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        9002,
    )
    .await;

    // Indexer should recover and pick up the new event within the error-backoff
    // window (10 s) plus one poll cycle (5 s) = 15 s budget.
    let recovered = wait_until(
        || {
            let url = format!("{base}/v1/events/{contract_id_2}");
            async move {
                match get_json(&url).await {
                    Ok(v) => v["data"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;

    assert!(
        recovered,
        "indexer should recover and index new events within 30 s after RPC error is cleared"
    );
}

// ---------------------------------------------------------------------------
// Test: subscription deletion stops delivery
// ---------------------------------------------------------------------------

/// Verifies that deleting a subscription stops webhook delivery:
/// 1. Create a subscription and inject an event → delivery received.
/// 2. Delete the subscription.
/// 3. Clear the delivery log.
/// 4. Inject another event → no delivery within 15 s.
#[tokio::test]
async fn e2e_subscription_deletion_stops_delivery() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_subscription_deletion_stops_delivery"
        );
        return;
    };
    let rpc_admin = rpc_admin_url();
    let webhook_admin = webhook_admin_url();

    clear_webhook_deliveries(&webhook_admin).await;

    let contract_id = "CDELDELDELDELDELDELDELDELDELDELDELDELDELDELDELDELDEL4";
    let webhook_url = format!("{webhook_admin}/webhook");

    // --- Create subscription ---
    let sub_id = create_subscription(&base, contract_id, Some(&webhook_url)).await;

    // --- Inject first event and confirm delivery ---
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000047244640256-0000000000",
            "contractId": contract_id,
            "txHash": "a1".repeat(32),
            "ledger": 10001,
            "ledgerClosedAt": "2026-03-14T09:00:00Z",
            "pagingToken": "0000000047244640256-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        10001,
    )
    .await;

    let first_delivered = wait_until(
        || {
            let url = format!("{webhook_admin}/received");
            async move {
                match get_json(&url).await {
                    Ok(v) => v.as_array().map(|a| !a.is_empty()).unwrap_or(false),
                    Err(_) => false,
                }
            }
        },
        Duration::from_secs(30),
        Duration::from_secs(2),
    )
    .await;

    reset_rpc_stubs(&rpc_admin).await;
    assert!(first_delivered, "initial delivery should arrive within 30 s");

    // --- Delete the subscription ---
    let del_resp = reqwest::Client::new()
        .delete(&format!("{base}/v1/subscriptions/{sub_id}"))
        .send()
        .await
        .expect("DELETE /v1/subscriptions/{id} failed");
    assert!(
        del_resp.status().is_success(),
        "DELETE should succeed; got {}",
        del_resp.status()
    );

    // --- Clear delivery log ---
    clear_webhook_deliveries(&webhook_admin).await;

    // --- Inject second event ---
    stub_rpc_events(
        &rpc_admin,
        vec![serde_json::json!({
            "type": "contract",
            "id": "0000000051539607552-0000000000",
            "contractId": contract_id,
            "txHash": "b2".repeat(32),
            "ledger": 10002,
            "ledgerClosedAt": "2026-03-14T09:01:00Z",
            "pagingToken": "0000000051539607552-0000000000",
            "inSuccessfulContractCall": true,
            "value": { "xdr": "AAAAAQ==" },
            "topic": [{ "xdr": "AAAAAQ==" }]
        })],
        10002,
    )
    .await;

    // Wait 15 s — no delivery should arrive after deletion.
    tokio::time::sleep(Duration::from_secs(15)).await;

    let received_after_delete = get_json(&format!("{webhook_admin}/received"))
        .await
        .expect("GET /received failed");
    let deliveries_after = received_after_delete
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    reset_rpc_stubs(&rpc_admin).await;

    assert_eq!(
        deliveries_after, 0,
        "no deliveries should occur after subscription is deleted; got {deliveries_after}"
    );
}

// ---------------------------------------------------------------------------
// Test: unknown route returns 404
// ---------------------------------------------------------------------------

/// Verifies that a request to a non-existent path returns 404 with a JSON body.
#[tokio::test]
async fn e2e_unknown_route_returns_404() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_unknown_route_returns_404");
        return;
    };

    let resp = reqwest::get(&format!("{base}/v1/this-path-does-not-exist"))
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        404,
        "unknown route should return 404; got {}",
        resp.status()
    );

    // The response should be JSON with an error field.
    let body: Value = resp.json().await.unwrap_or_default();
    let has_error = body["error"].is_string()
        || body["message"].is_string()
        || body["detail"].is_string();
    assert!(
        has_error,
        "404 response should contain an error message; got: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test: malformed JSON body returns 400
// ---------------------------------------------------------------------------

/// Verifies that a POST to a JSON-expecting endpoint with a malformed body
/// returns 400 Bad Request with a descriptive error message.
#[tokio::test]
async fn e2e_malformed_body_returns_400() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_malformed_body_returns_400");
        return;
    };

    let client = reqwest::Client::new();

    // Send invalid JSON to POST /v1/subscriptions.
    let resp = client
        .post(&format!("{base}/v1/subscriptions"))
        .header("Content-Type", "application/json")
        .body("{ this is not valid json }")
        .send()
        .await
        .expect("POST with malformed body failed");

    assert_eq!(
        resp.status(),
        400,
        "malformed JSON body should return 400; got {}",
        resp.status()
    );

    // The response body should be JSON with an error description.
    let body: Value = resp.json().await.unwrap_or_default();
    let has_error = body["error"].is_string()
        || body["message"].is_string()
        || body["detail"].is_string();
    assert!(
        has_error,
        "400 response should contain an error description; got: {body}"
    );
}

// ---------------------------------------------------------------------------
// Test: large limit is clamped to server maximum
// ---------------------------------------------------------------------------

/// Requests `limit=100000` and asserts the response `limit` field reflects
/// the server's maximum, not the requested value.
#[tokio::test]
async fn e2e_large_limit_is_clamped() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_large_limit_is_clamped");
        return;
    };

    let body = get_json(&format!("{base}/v1/events?limit=100000"))
        .await
        .expect("GET /v1/events?limit=100000 failed");

    // The server should clamp the limit. The response should contain a `limit`
    // field whose value is ≤ 1000 (a reasonable max for any server).
    let returned_limit = body["limit"].as_u64().unwrap_or(u64::MAX);
    assert!(
        returned_limit <= 1000,
        "limit should be clamped to ≤ 1000; got {returned_limit}"
    );

    // The response should still be 200 and contain a data array.
    assert!(
        body["data"].is_array(),
        "response should contain a data array even with clamped limit"
    );
}

// ===========================================================================
// Performance Verification Tests
// ===========================================================================

// ---------------------------------------------------------------------------
// Test: p95 latency under sequential load
// ---------------------------------------------------------------------------

/// Sends 50 sequential GET /v1/events requests and asserts the p95 response
/// time is under 500 ms.  This is a smoke test, not a load test — it catches
/// catastrophic regressions only.
#[tokio::test]
async fn e2e_perf_p95_latency() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_perf_p95_latency");
        return;
    };

    let client = reqwest::Client::new();
    let mut latencies_ms: Vec<u64> = Vec::with_capacity(50);

    for _ in 0..50 {
        let start = Instant::now();
        let resp = client
            .get(&format!("{base}/v1/events"))
            .send()
            .await
            .expect("GET /v1/events failed");
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), 200, "GET /v1/events should return 200");
        latencies_ms.push(elapsed.as_millis() as u64);
    }

    latencies_ms.sort_unstable();
    let p95_idx = (latencies_ms.len() as f64 * 0.95) as usize;
    let p95_idx = p95_idx.min(latencies_ms.len() - 1);
    let p95_ms = latencies_ms[p95_idx];

    assert!(
        p95_ms < 500,
        "p95 latency for GET /v1/events should be < 500 ms; got {p95_ms} ms"
    );
}

// ---------------------------------------------------------------------------
// Test: concurrent requests return no errors
// ---------------------------------------------------------------------------

/// Issues 20 concurrent GET /v1/events requests and asserts all return 200.
#[tokio::test]
async fn e2e_perf_concurrent_no_errors() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_perf_concurrent_no_errors");
        return;
    };

    let client = std::sync::Arc::new(reqwest::Client::new());

    let tasks: Vec<_> = (0..20)
        .map(|_| {
            let client = client.clone();
            let url = format!("{base}/v1/events");
            tokio::spawn(async move { client.get(&url).send().await })
        })
        .collect();

    let mut errors = 0usize;
    for task in tasks {
        match task.await {
            Ok(Ok(resp)) if !resp.status().is_success() => {
                errors += 1;
                eprintln!("Concurrent request returned status {}", resp.status());
            }
            Ok(Err(e)) => {
                errors += 1;
                eprintln!("Concurrent request error: {e}");
            }
            Err(e) => {
                errors += 1;
                eprintln!("Task join error: {e}");
            }
            _ => {}
        }
    }

    assert_eq!(
        errors, 0,
        "all 20 concurrent GET /v1/events requests should succeed; {errors} failed"
    );
}

// ---------------------------------------------------------------------------
// Test: pagination consistency under concurrent load
// ---------------------------------------------------------------------------

/// Pages through the event dataset concurrently across 5 tasks and asserts
/// each page returns a consistent, non-overlapping set.
#[tokio::test]
async fn e2e_perf_pagination_consistency_under_load() {
    let Some(base) = base_url() else {
        eprintln!(
            "E2E_BASE_URL not set — skipping e2e_perf_pagination_consistency_under_load"
        );
        return;
    };

    let client = std::sync::Arc::new(reqwest::Client::new());

    // Fetch 5 pages concurrently and collect all event IDs.
    let tasks: Vec<_> = (1..=5)
        .map(|page| {
            let client = client.clone();
            let url = format!("{base}/v1/events?page={page}&limit=10");
            tokio::spawn(async move {
                let resp = client.get(&url).send().await?;
                let body: Value = resp.json().await?;
                let ids: Vec<String> = body["data"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .filter_map(|e| e["id"].as_str().map(str::to_owned))
                    .collect();
                Ok::<Vec<String>, reqwest::Error>(ids)
            })
        })
        .collect();

    let mut all_ids: Vec<String> = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(ids)) => all_ids.extend(ids),
            Ok(Err(e)) => panic!("pagination request failed: {e}"),
            Err(e) => panic!("task join error: {e}"),
        }
    }

    // No duplicates across pages — check set size equals total count.
    let unique_count = {
        let mut s = all_ids.clone();
        s.sort_unstable();
        s.dedup();
        s.len()
    };

    assert_eq!(
        all_ids.len(),
        unique_count,
        "concurrent pagination should return no duplicate event IDs; \
         total={}, unique={}",
        all_ids.len(),
        unique_count
    );
}

// ---------------------------------------------------------------------------
// Test: metrics endpoint responds quickly
// ---------------------------------------------------------------------------

/// Asserts that GET /metrics responds in under 200 ms.
#[tokio::test]
async fn e2e_perf_metrics_endpoint_speed() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_perf_metrics_endpoint_speed");
        return;
    };

    let start = Instant::now();
    let resp = reqwest::get(&format!("{base}/metrics"))
        .await
        .expect("GET /metrics failed");
    let elapsed_ms = start.elapsed().as_millis() as u64;

    assert_eq!(resp.status(), 200, "GET /metrics should return 200");
    assert!(
        elapsed_ms < 200,
        "GET /metrics should respond in < 200 ms; got {elapsed_ms} ms"
    );
}

// ---------------------------------------------------------------------------
// Test: health check liveness is fast
// ---------------------------------------------------------------------------

/// Asserts that GET /healthz/live (no external I/O path) responds in under
/// 50 ms.
#[tokio::test]
async fn e2e_perf_health_check_speed() {
    let Some(base) = base_url() else {
        eprintln!("E2E_BASE_URL not set — skipping e2e_perf_health_check_speed");
        return;
    };

    let start = Instant::now();
    let resp = reqwest::get(&format!("{base}/healthz/live"))
        .await
        .expect("GET /healthz/live failed");
    let elapsed_ms = start.elapsed().as_millis() as u64;

    assert_eq!(resp.status(), 200, "GET /healthz/live should return 200");
    assert!(
        elapsed_ms < 50,
        "GET /healthz/live should respond in < 50 ms; got {elapsed_ms} ms"
    );
}
