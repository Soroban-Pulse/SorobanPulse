//! # RPC Record/Replay Fixture Infrastructure (Issue #1156)
//!
//! This module provides a deterministic, offline-capable RPC transport for
//! indexer tests.  It has two modes:
//!
//! - **Replay** (default for tests): serves pre-recorded JSON fixtures from
//!   `tests/fixtures/rpc/<scenario>/` in the order they were recorded.
//! - **Record** (opt-in, cfg(test) only): forwards requests to a real RPC
//!   endpoint and persists the request/response pairs to disk.
//!
//! ## Directory Layout
//!
//! ```text
//! tests/fixtures/rpc/
//!   normal_range/
//!     00_getLatestLedger.json
//!     01_getEvents.json
//!     02_getEvents.json
//!   cursor_expiry/
//!     00_getLatestLedger.json
//!     01_getEvents.json          ← contains an RPC-level cursor-expiry error
//!     02_getEvents.json          ← recovery fetch from ledger start
//!   page_overflow/
//!     00_getLatestLedger.json
//!     01_getEvents.json          ← exactly 100 events (full page)
//!     02_getEvents.json          ← continuation page
//!   protocol_upgrade/
//!     00_getLatestLedger.json
//!     01_getEvents.json          ← events with new field added
//!   rpc_error/
//!     00_getLatestLedger.json
//!     01_getEvents_error.json    ← RPC-level error response
//! ```
//!
//! Each file is the raw JSON body of an RPC *response* (not the request).
//!
//! ## Enabling Recording Mode
//!
//! Set `RPC_RECORD=1` and `STELLAR_RPC_URL=<url>` before running tests:
//!
//! ```bash
//! RPC_RECORD=1 STELLAR_RPC_URL=https://soroban-testnet.stellar.org \
//!   cargo test --test indexer_replay_tests
//! ```
//!
//! Recorded fixtures are written to `tests/fixtures/rpc/<scenario>/`.

#![cfg(test)]

use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

// ---------------------------------------------------------------------------
// Core data structures
// ---------------------------------------------------------------------------

/// A single recorded RPC interaction: the JSON body sent and the JSON body
/// received.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RpcFixture {
    /// The JSON-RPC method name (`getLatestLedger`, `getEvents`, …).
    pub method: String,
    /// The raw JSON request body sent to the RPC endpoint.
    pub request: Value,
    /// The raw JSON response body received from the RPC endpoint.
    pub response: Value,
}

// ---------------------------------------------------------------------------
// Replay transport
// ---------------------------------------------------------------------------

/// An in-memory sequence of fixtures that are replayed in order.
///
/// Each call to `next_response()` advances the internal cursor by one.
/// When the sequence is exhausted, `next_response()` returns `None`.
pub struct ReplayTransport {
    fixtures: Vec<RpcFixture>,
    cursor: AtomicUsize,
}

impl ReplayTransport {
    /// Load fixtures from a directory.  Files are sorted lexicographically
    /// (the `00_`, `01_` prefix convention guarantees the correct order).
    pub fn from_dir(dir: &Path) -> anyhow::Result<Self> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
            .collect();
        entries.sort();

        let fixtures = entries
            .iter()
            .map(|path| {
                let contents = std::fs::read_to_string(path)
                    .map_err(|e| anyhow::anyhow!("reading {}: {}", path.display(), e))?;
                // The fixture file may be either:
                //   (a) a plain RPC response object, or
                //   (b) a full RpcFixture with `method`, `request`, `response`.
                // Try (b) first; fall back to (a).
                if let Ok(fixture) = serde_json::from_str::<RpcFixture>(&contents) {
                    Ok(fixture)
                } else {
                    let response: Value = serde_json::from_str(&contents)
                        .map_err(|e| anyhow::anyhow!("parsing {}: {}", path.display(), e))?;
                    let method = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .and_then(|n| {
                            // Extract method from filename e.g. "01_getEvents.json"
                            n.splitn(2, '_').nth(1).map(|s| s.trim_end_matches(".json").to_string())
                        })
                        .unwrap_or_else(|| "unknown".to_string());
                    Ok(RpcFixture {
                        method,
                        request: json!({}),
                        response,
                    })
                }
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        Ok(Self {
            fixtures,
            cursor: AtomicUsize::new(0),
        })
    }

    /// Create a transport from a slice of fixtures (for in-memory tests).
    pub fn from_fixtures(fixtures: Vec<RpcFixture>) -> Self {
        Self {
            fixtures,
            cursor: AtomicUsize::new(0),
        }
    }

    /// Advance the cursor and return the next fixture, or `None` if exhausted.
    pub fn next_fixture(&self) -> Option<&RpcFixture> {
        let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
        self.fixtures.get(idx)
    }

    /// Return how many fixtures remain to be replayed.
    pub fn remaining(&self) -> usize {
        let used = self.cursor.load(Ordering::SeqCst);
        self.fixtures.len().saturating_sub(used)
    }

    /// Reset the cursor to the beginning (allows replaying the same scenario
    /// multiple times within a single test).
    pub fn reset(&self) {
        self.cursor.store(0, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------
// Recording transport
// ---------------------------------------------------------------------------

/// When `RPC_RECORD=1` is set, wrap a real HTTP client and save every
/// request/response pair to disk.
///
/// Only compiled under `cfg(test)` to prevent accidental use in production.
#[cfg(test)]
pub struct RecordingTransport {
    client: reqwest::Client,
    recorded: Arc<Mutex<Vec<RpcFixture>>>,
    counter: AtomicUsize,
    output_dir: PathBuf,
}

#[cfg(test)]
impl RecordingTransport {
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            client: reqwest::Client::new(),
            recorded: Arc::new(Mutex::new(Vec::new())),
            counter: AtomicUsize::new(0),
            output_dir: output_dir.into(),
        }
    }

    /// Send a JSON-RPC request to `url` and save the pair to the output dir.
    pub async fn call(&self, url: &str, body: Value) -> anyhow::Result<Value> {
        let method = body
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let response: Value = self
            .client
            .post(url)
            .json(&body)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP error: {e}"))?
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("JSON parse error: {e}"))?;

        let fixture = RpcFixture {
            method: method.clone(),
            request: body,
            response: response.clone(),
        };

        let idx = self.counter.fetch_add(1, Ordering::SeqCst);
        std::fs::create_dir_all(&self.output_dir)?;
        let filename = self
            .output_dir
            .join(format!("{:02}_{method}.json", idx));
        std::fs::write(
            &filename,
            serde_json::to_string_pretty(&fixture)?,
        )?;

        self.recorded.lock().unwrap().push(fixture);
        Ok(response)
    }

    /// Return all recorded fixtures (useful for assertions in record mode).
    pub fn recorded_fixtures(&self) -> Vec<RpcFixture> {
        self.recorded.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// Fixture builders — inline fixtures for unit tests that don't touch disk
// ---------------------------------------------------------------------------

/// Build the fixture directory path for a named scenario.
pub fn fixture_dir(scenario: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/rpc")
        .join(scenario)
}

/// Build a minimal `getLatestLedger` response fixture.
pub fn latest_ledger_fixture(sequence: u64) -> RpcFixture {
    RpcFixture {
        method: "getLatestLedger".to_string(),
        request: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger"
        }),
        response: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "id": format!("{sequence:0>64}"),
                "protocolVersion": 21,
                "sequence": sequence
            }
        }),
    }
}

/// Build a `getEvents` response fixture with `n` synthetic events starting at
/// `ledger`.
pub fn get_events_fixture(
    ledger: u64,
    n: usize,
    cursor: Option<&str>,
    latest_ledger: u64,
) -> RpcFixture {
    let events: Vec<Value> = (0..n)
        .map(|i| {
            json!({
                "contractId": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
                "type": "contract",
                "txHash": format!("{:0<64}", format!("{}{}", ledger, i)),
                "ledger": ledger,
                "ledgerClosedAt": "2026-03-14T00:00:00Z",
                "value": {"xdr": "AAAABQAAAAk="},
                "topic": [{"xdr": "AAAADwAAAAh0cmFuc2Zlcg=="}],
                "id": format!("{}-{}", ledger, i)
            })
        })
        .collect();

    let mut result = json!({
        "events": events,
        "latestLedger": latest_ledger,
        "cursor": format!("{}-{}", ledger, n)
    });

    if let Some(c) = cursor {
        result["cursor"] = json!(c);
    }

    RpcFixture {
        method: "getEvents".to_string(),
        request: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": {
                "startLedger": ledger,
                "filters": [],
                "pagination": {"limit": 100}
            }
        }),
        response: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": result
        }),
    }
}

/// Build a cursor-expiry error fixture.
pub fn cursor_expiry_fixture(ledger: u64) -> RpcFixture {
    RpcFixture {
        method: "getEvents".to_string(),
        request: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": {
                "pagination": {"cursor": "expired-cursor-value", "limit": 100},
                "filters": []
            }
        }),
        response: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": -32600,
                "message": "start ledger must be between the oldest ledger and the latest ledger",
                "data": {"ledger": ledger}
            }
        }),
    }
}

/// Build an RPC error fixture (generic server-side error).
pub fn rpc_error_fixture(code: i64, message: &str) -> RpcFixture {
    RpcFixture {
        method: "getEvents".to_string(),
        request: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": {}
        }),
        response: json!({
            "jsonrpc": "2.0",
            "id": 1,
            "error": {
                "code": code,
                "message": message
            }
        }),
    }
}

/// Build a protocol-upgrade fixture where a new field appears.
pub fn protocol_upgrade_fixture(ledger: u64) -> RpcFixture {
    let mut base = get_events_fixture(ledger, 2, None, ledger + 100);
    // Add a hypothetical new field to events.
    if let Some(events) = base
        .response
        .get_mut("result")
        .and_then(|r| r.get_mut("events"))
        .and_then(|e| e.as_array_mut())
    {
        for event in events.iter_mut() {
            event["newField"] = json!("protocol-21-data");
        }
    }
    base.method = "getEvents_protocol_upgrade".to_string();
    base
}

// ---------------------------------------------------------------------------
// On-disk fixture scenarios (written once, shipped in the repo)
// ---------------------------------------------------------------------------

/// Write all bundled scenarios to `tests/fixtures/rpc/` so the repo
/// contains deterministic fixtures from day 1.
///
/// Call this once with `cargo test -- write_bundled_fixtures --ignored` or
/// in a CI step after recording from testnet.
#[test]
#[ignore]
fn write_bundled_fixtures() {
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rpc");

    let scenarios: &[(&str, Vec<RpcFixture>)] = &[
        (
            "normal_range",
            vec![
                latest_ledger_fixture(1_000_100),
                get_events_fixture(1_000_000, 5, None, 1_000_100),
                get_events_fixture(1_000_005, 3, None, 1_000_100),
            ],
        ),
        (
            "cursor_expiry",
            vec![
                latest_ledger_fixture(2_000_100),
                cursor_expiry_fixture(2_000_000),
                // Recovery: restart from ledger after detecting cursor expiry.
                get_events_fixture(2_000_000, 2, None, 2_000_100),
            ],
        ),
        (
            "page_overflow",
            vec![
                latest_ledger_fixture(3_000_200),
                // First page: exactly 100 events (signals more may exist).
                get_events_fixture(3_000_000, 100, Some("3000000-100"), 3_000_200),
                // Continuation page via cursor.
                get_events_fixture(3_000_100, 50, Some("3000100-50"), 3_000_200),
            ],
        ),
        (
            "protocol_upgrade",
            vec![
                latest_ledger_fixture(4_000_100),
                protocol_upgrade_fixture(4_000_000),
            ],
        ),
        (
            "rpc_error",
            vec![
                latest_ledger_fixture(5_000_100),
                rpc_error_fixture(-32603, "Internal RPC error"),
            ],
        ),
    ];

    for (name, fixtures) in scenarios {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, fixture) in fixtures.iter().enumerate() {
            let path = dir.join(format!("{:02}_{}.json", i, fixture.method));
            std::fs::write(
                &path,
                serde_json::to_string_pretty(fixture).unwrap(),
            )
            .unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// Replay tests
// ---------------------------------------------------------------------------

/// Verify the replay transport advances the cursor correctly.
#[test]
fn replay_transport_advances_cursor() {
    let fixtures = vec![
        latest_ledger_fixture(1000),
        get_events_fixture(900, 5, None, 1000),
    ];
    let transport = ReplayTransport::from_fixtures(fixtures);

    let f0 = transport.next_fixture().expect("first fixture");
    assert_eq!(f0.method, "getLatestLedger");

    let f1 = transport.next_fixture().expect("second fixture");
    assert_eq!(f1.method, "getEvents");

    assert!(transport.next_fixture().is_none(), "should be exhausted");
}

/// Reset brings the cursor back to zero.
#[test]
fn replay_transport_reset_restarts_sequence() {
    let fixtures = vec![latest_ledger_fixture(1000)];
    let transport = ReplayTransport::from_fixtures(fixtures);

    let _f = transport.next_fixture().unwrap();
    assert!(transport.next_fixture().is_none());

    transport.reset();
    let f = transport.next_fixture().expect("should replay from start");
    assert_eq!(f.method, "getLatestLedger");
}

/// `remaining()` decrements as fixtures are consumed.
#[test]
fn replay_transport_remaining_count() {
    let fixtures = vec![
        latest_ledger_fixture(1000),
        get_events_fixture(900, 2, None, 1000),
    ];
    let transport = ReplayTransport::from_fixtures(fixtures);
    assert_eq!(transport.remaining(), 2);
    transport.next_fixture();
    assert_eq!(transport.remaining(), 1);
    transport.next_fixture();
    assert_eq!(transport.remaining(), 0);
}

/// Fixture loaded from disk must round-trip through JSON.
#[test]
fn fixture_round_trips_json() {
    let original = latest_ledger_fixture(42_000);
    let serialized = serde_json::to_string(&original).unwrap();
    let deserialized: RpcFixture = serde_json::from_str(&serialized).unwrap();
    assert_eq!(original.method, deserialized.method);
    assert_eq!(original.response, deserialized.response);
}

/// The `getEvents` fixture with `n` events has the correct event count.
#[test]
fn get_events_fixture_event_count() {
    let fixture = get_events_fixture(1000, 17, None, 2000);
    let events = fixture
        .response
        .get("result")
        .and_then(|r| r.get("events"))
        .and_then(|e| e.as_array())
        .expect("events array");
    assert_eq!(events.len(), 17);
}

/// The cursor-expiry fixture has an RPC-level `error` field, not a `result`.
#[test]
fn cursor_expiry_fixture_is_an_error_response() {
    let fixture = cursor_expiry_fixture(1000);
    assert!(
        fixture.response.get("error").is_some(),
        "cursor expiry response must have an 'error' key"
    );
    assert!(
        fixture.response.get("result").is_none(),
        "cursor expiry response must not have a 'result' key"
    );
}

/// The page-overflow fixture with exactly 100 events signals a full page.
#[test]
fn page_overflow_fixture_has_full_page() {
    let fixture = get_events_fixture(3_000_000, 100, Some("3000000-100"), 3_000_200);
    let count = fixture
        .response
        .get("result")
        .and_then(|r| r.get("events"))
        .and_then(|e| e.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(count, 100, "page overflow scenario must have exactly 100 events");
}

/// The protocol-upgrade fixture contains the new field in each event.
#[test]
fn protocol_upgrade_fixture_has_new_field() {
    let fixture = protocol_upgrade_fixture(4_000_000);
    let events = fixture
        .response
        .get("result")
        .and_then(|r| r.get("events"))
        .and_then(|e| e.as_array())
        .expect("events");
    for event in events {
        assert!(
            event.get("newField").is_some(),
            "each event must contain the new protocol field"
        );
    }
}

/// Load fixtures from the on-disk directory if it exists.
/// This test is a smoke test: it just verifies the directory loads without
/// error.  It is skipped when the fixture directory has not been written yet.
#[test]
fn load_normal_range_fixtures_from_disk_if_present() {
    let dir = fixture_dir("normal_range");
    if !dir.exists() {
        return; // fixtures not written yet — run write_bundled_fixtures first
    }
    let transport = ReplayTransport::from_dir(&dir).expect("load fixtures from disk");
    assert!(
        transport.remaining() >= 2,
        "normal_range scenario must have at least 2 fixtures"
    );
}
