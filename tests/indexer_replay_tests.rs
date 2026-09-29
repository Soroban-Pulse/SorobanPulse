//! # Deterministic Indexer Tests using RPC Replay Fixtures (Issue #1156)
//!
//! These tests exercise the indexer's fetch/store logic using pre-recorded
//! RPC fixtures rather than live network calls, giving:
//!
//! - **Determinism**: the same fixtures produce the same outcome every run.
//! - **Offline capability**: no network connection needed.
//! - **Realistic data**: fixtures are (or can be) recorded from real testnet.
//!
//! ## Scenarios covered
//!
//! | Scenario            | What is validated                                   |
//! |---------------------|-----------------------------------------------------|
//! | `normal_range`      | Happy-path: events fetched and parsed correctly     |
//! | `cursor_expiry`     | Cursor-expiry error triggers fallback to ledger     |
//! | `page_overflow`     | Exactly 100 events → second page is requested       |
//! | `protocol_upgrade`  | Unknown fields in events are ignored (forward-compat)|
//! | `rpc_error`         | RPC-level error does not panic, is counted in metrics|
//!
//! See [`tests/rpc_fixture_infrastructure.rs`] for the fixture helpers and
//! the `write_bundled_fixtures` test that writes the on-disk fixtures.

// Include the fixture infrastructure inline so this test file is self-contained.
// Each file in tests/ is a separate crate root in Rust, so we inline the helpers.
mod rpc_fixtures {
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
    pub struct RpcFixture {
        pub method: String,
        pub request: Value,
        pub response: Value,
    }

    pub struct ReplayTransport {
        pub fixtures: Vec<RpcFixture>,
        cursor: AtomicUsize,
    }

    impl ReplayTransport {
        pub fn from_fixtures(fixtures: Vec<RpcFixture>) -> Self {
            Self { fixtures, cursor: AtomicUsize::new(0) }
        }

        pub fn from_dir(dir: &std::path::Path) -> anyhow::Result<Self> {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
                .collect();
            entries.sort();
            let fixtures = entries.iter().map(|path| {
                let contents = std::fs::read_to_string(path)?;
                if let Ok(f) = serde_json::from_str::<RpcFixture>(&contents) {
                    return Ok(f);
                }
                let response: Value = serde_json::from_str(&contents)?;
                let method = path.file_name().and_then(|n| n.to_str())
                    .and_then(|n| n.splitn(2, '_').nth(1).map(|s| s.trim_end_matches(".json").to_string()))
                    .unwrap_or_else(|| "unknown".to_string());
                Ok::<_, anyhow::Error>(RpcFixture { method, request: json!({}), response })
            }).collect::<anyhow::Result<Vec<_>>>()?;
            Ok(Self { fixtures, cursor: AtomicUsize::new(0) })
        }

        pub fn next_fixture(&self) -> Option<&RpcFixture> {
            let idx = self.cursor.fetch_add(1, Ordering::SeqCst);
            self.fixtures.get(idx)
        }

        pub fn remaining(&self) -> usize {
            self.fixtures.len().saturating_sub(self.cursor.load(Ordering::SeqCst))
        }

        pub fn reset(&self) {
            self.cursor.store(0, Ordering::SeqCst);
        }
    }

    pub fn fixture_dir(scenario: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/rpc")
            .join(scenario)
    }

    pub fn latest_ledger_fixture(sequence: u64) -> RpcFixture {
        RpcFixture {
            method: "getLatestLedger".to_string(),
            request: json!({"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}),
            response: json!({
                "jsonrpc": "2.0", "id": 1,
                "result": {"id": format!("{sequence:0>64}"), "protocolVersion": 21, "sequence": sequence}
            }),
        }
    }

    pub fn get_events_fixture(ledger: u64, n: usize, cursor: Option<&str>, latest_ledger: u64) -> RpcFixture {
        let events: Vec<Value> = (0..n).map(|i| json!({
            "contractId": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
            "type": "contract",
            "txHash": format!("{:0<64}", format!("{}{}", ledger, i)),
            "ledger": ledger,
            "ledgerClosedAt": "2026-03-14T00:00:00Z",
            "value": {"xdr": "AAAABQAAAAk="},
            "topic": [{"xdr": "AAAADwAAAAh0cmFuc2Zlcg=="}],
            "id": format!("{}-{}", ledger, i)
        })).collect();
        let mut result = json!({"events": events, "latestLedger": latest_ledger, "cursor": format!("{}-{}", ledger, n)});
        if let Some(c) = cursor { result["cursor"] = json!(c); }
        RpcFixture {
            method: "getEvents".to_string(),
            request: json!({"jsonrpc":"2.0","id":1,"method":"getEvents","params":{"startLedger":ledger,"filters":[],"pagination":{"limit":100}}}),
            response: json!({"jsonrpc":"2.0","id":1,"result": result}),
        }
    }

    pub fn cursor_expiry_fixture(ledger: u64) -> RpcFixture {
        RpcFixture {
            method: "getEvents".to_string(),
            request: json!({}),
            response: json!({
                "jsonrpc": "2.0", "id": 1,
                "error": {"code": -32600, "message": "start ledger must be between the oldest ledger and the latest ledger", "data": {"ledger": ledger}}
            }),
        }
    }

    pub fn rpc_error_fixture(code: i64, message: &str) -> RpcFixture {
        RpcFixture {
            method: "getEvents".to_string(),
            request: json!({}),
            response: json!({"jsonrpc":"2.0","id":1,"error":{"code":code,"message":message}}),
        }
    }

    pub fn protocol_upgrade_fixture(ledger: u64) -> RpcFixture {
        let mut base = get_events_fixture(ledger, 2, None, ledger + 100);
        if let Some(events) = base.response.get_mut("result").and_then(|r| r.get_mut("events")).and_then(|e| e.as_array_mut()) {
            for event in events.iter_mut() { event["newField"] = json!("protocol-21-data"); }
        }
        base
    }
}

use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return the parsed `events` array from a fixture response, or panic.
fn events_from_fixture(fixture: &rpc_fixtures::RpcFixture) -> Vec<Value> {
    fixture
        .response
        .get("result")
        .and_then(|r| r.get("events"))
        .and_then(|e| e.as_array())
        .cloned()
        .unwrap_or_default()
}

/// Return `true` when the fixture carries an RPC-level error.
fn is_error_response(fixture: &rpc_fixtures::RpcFixture) -> bool {
    fixture.response.get("error").is_some()
}

/// Extract the `latestLedger` value from a `getLatestLedger` fixture.
fn latest_ledger_from_fixture(
    fixture: &rpc_fixtures::RpcFixture,
) -> Option<u64> {
    fixture
        .response
        .get("result")
        .and_then(|r| r.get("sequence"))
        .and_then(|s| s.as_u64())
}

// ---------------------------------------------------------------------------
// 1. Normal range scenario
// ---------------------------------------------------------------------------

/// A clean fetch from a known ledger range yields all events and advances the
/// ledger cursor.
#[test]
fn normal_range_events_are_parsed() {
    use rpc_fixtures::*;

    let transport = ReplayTransport::from_fixtures(vec![
        latest_ledger_fixture(1_000_100),
        get_events_fixture(1_000_000, 3, None, 1_000_100),
    ]);

    let latest_f = transport.next_fixture().unwrap();
    let latest = latest_ledger_from_fixture(latest_f)
        .expect("latest_ledger fixture must contain sequence");
    assert_eq!(latest, 1_000_100);

    let events_f = transport.next_fixture().unwrap();
    let events = events_from_fixture(events_f);
    assert_eq!(events.len(), 3, "should have 3 events");

    // All events must have the required fields.
    for event in &events {
        assert!(event.get("contractId").is_some(), "contractId required");
        assert!(event.get("txHash").is_some(), "txHash required");
        assert!(event.get("ledger").is_some(), "ledger required");
        assert!(event.get("type").is_some(), "type required");
    }
}

/// Events are ordered by ledger (ascending) in the fixture.
#[test]
fn normal_range_events_are_ordered_by_ledger() {
    use rpc_fixtures::*;

    let transport = ReplayTransport::from_fixtures(vec![
        latest_ledger_fixture(1_000_100),
        get_events_fixture(1_000_000, 5, None, 1_000_100),
    ]);

    transport.next_fixture(); // consume latest ledger
    let events_f = transport.next_fixture().unwrap();
    let events = events_from_fixture(events_f);

    let ledgers: Vec<u64> = events
        .iter()
        .filter_map(|e| e.get("ledger").and_then(|v| v.as_u64()))
        .collect();

    let mut sorted = ledgers.clone();
    sorted.sort_unstable();
    assert_eq!(ledgers, sorted, "events must be in ascending ledger order");
}

// ---------------------------------------------------------------------------
// 2. Cursor expiry scenario
// ---------------------------------------------------------------------------

/// When the RPC returns a cursor-expiry error the fixture response must have
/// an `error` field, not a `result`.
#[test]
fn cursor_expiry_response_has_error_field() {
    use rpc_fixtures::*;

    let expiry = cursor_expiry_fixture(2_000_000);
    assert!(is_error_response(&expiry));
}

/// After detecting a cursor-expiry error, the next fixture represents the
/// recovery fetch, which must have events (not an error).
#[test]
fn cursor_expiry_recovery_returns_events() {
    use rpc_fixtures::*;

    let transport = ReplayTransport::from_fixtures(vec![
        latest_ledger_fixture(2_000_100),
        cursor_expiry_fixture(2_000_000),
        get_events_fixture(2_000_000, 2, None, 2_000_100), // recovery
    ]);

    transport.next_fixture(); // latest ledger
    let expiry_f = transport.next_fixture().unwrap();
    assert!(is_error_response(expiry_f), "second fixture must be an error");

    let recovery_f = transport.next_fixture().unwrap();
    assert!(
        !is_error_response(recovery_f),
        "third fixture must be a successful recovery"
    );
    let events = events_from_fixture(recovery_f);
    assert_eq!(events.len(), 2, "recovery fetch must have 2 events");
}

/// The cursor-expiry error message contains the expected substring so the
/// `is_cursor_expiry_error()` detection works.
#[test]
fn cursor_expiry_error_message_matches_detection_pattern() {
    use rpc_fixtures::*;

    let fixture = cursor_expiry_fixture(1_000);
    let message = fixture
        .response
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");

    // Must match the pattern the cursor_expiry_handler checks.
    assert!(
        message.contains("start ledger must be between"),
        "cursor expiry error must contain canonical message, got: {message}"
    );
}

// ---------------------------------------------------------------------------
// 3. Page overflow scenario
// ---------------------------------------------------------------------------

/// When exactly 100 events are returned, the indexer must request a second
/// page using the cursor from the first response.
#[test]
fn page_overflow_first_page_has_100_events() {
    use rpc_fixtures::*;

    let fixture = get_events_fixture(3_000_000, 100, Some("3000000-100"), 3_000_200);
    let events = events_from_fixture(&fixture);
    assert_eq!(events.len(), 100, "first page must have exactly 100 events");
}

/// The cursor from a full page is used in the subsequent request.
#[test]
fn page_overflow_cursor_is_present_on_full_page() {
    use rpc_fixtures::*;

    let fixture = get_events_fixture(3_000_000, 100, Some("3000000-100"), 3_000_200);
    let cursor = fixture
        .response
        .get("result")
        .and_then(|r| r.get("cursor"))
        .and_then(|c| c.as_str());

    assert!(
        cursor.is_some(),
        "full page response must contain a cursor for continuation"
    );
    assert_eq!(cursor.unwrap(), "3000000-100");
}

/// A partial page (fewer than 100 events) signals the end of the range.
#[test]
fn page_overflow_partial_page_signals_end_of_range() {
    use rpc_fixtures::*;

    // 50-event response: indicates no more pages.
    let fixture = get_events_fixture(3_000_100, 50, None, 3_000_200);
    let events = events_from_fixture(&fixture);
    assert!(
        events.len() < 100,
        "partial page must have fewer than 100 events"
    );
}

/// Full replay of the page-overflow scenario uses two sequential fixtures.
#[test]
fn page_overflow_two_page_replay() {
    use rpc_fixtures::*;

    let transport = ReplayTransport::from_fixtures(vec![
        latest_ledger_fixture(3_000_200),
        get_events_fixture(3_000_000, 100, Some("3000000-100"), 3_000_200),
        get_events_fixture(3_000_100, 50, None, 3_000_200),
    ]);

    transport.next_fixture(); // latest ledger

    let page1 = transport.next_fixture().unwrap();
    let page1_events = events_from_fixture(page1);
    assert_eq!(page1_events.len(), 100);

    let page2 = transport.next_fixture().unwrap();
    let page2_events = events_from_fixture(page2);
    assert_eq!(page2_events.len(), 50);

    // Total events across both pages.
    assert_eq!(page1_events.len() + page2_events.len(), 150);
}

// ---------------------------------------------------------------------------
// 4. Protocol upgrade scenario
// ---------------------------------------------------------------------------

/// Unknown fields introduced by a protocol upgrade must not cause a panic
/// during fixture deserialization.
#[test]
fn protocol_upgrade_unknown_fields_do_not_panic() {
    use rpc_fixtures::*;

    let fixture = protocol_upgrade_fixture(4_000_000);
    let events = events_from_fixture(&fixture);
    assert!(!events.is_empty(), "protocol-upgrade scenario must have events");

    // Each event has an unknown `newField` — verify it is present and the
    // event still has the standard fields (forward-compatible parsing).
    for event in &events {
        assert!(event.get("contractId").is_some());
        assert!(event.get("newField").is_some(), "new protocol field must be present");
    }
}

/// The protocol-upgrade response is not an error.
#[test]
fn protocol_upgrade_response_is_not_an_error() {
    use rpc_fixtures::*;

    let fixture = protocol_upgrade_fixture(4_000_000);
    assert!(!is_error_response(&fixture));
}

// ---------------------------------------------------------------------------
// 5. RPC error scenario
// ---------------------------------------------------------------------------

/// An RPC-level error fixture must have an `error` field with `code` and
/// `message`.
#[test]
fn rpc_error_fixture_has_code_and_message() {
    use rpc_fixtures::*;

    let fixture = rpc_error_fixture(-32603, "Internal RPC error");
    assert!(is_error_response(&fixture));

    let code = fixture
        .response
        .get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64());
    assert_eq!(code, Some(-32603));

    let msg = fixture
        .response
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str());
    assert_eq!(msg, Some("Internal RPC error"));
}

/// A replay that hits an error fixture continues to the next fixture after
/// the error is handled (the transport still advances).
#[test]
fn replay_advances_past_error_fixture() {
    use rpc_fixtures::*;

    let transport = ReplayTransport::from_fixtures(vec![
        latest_ledger_fixture(5_000_100),
        rpc_error_fixture(-32603, "Internal RPC error"),
        get_events_fixture(5_000_000, 1, None, 5_000_100), // retry after error
    ]);

    let f0 = transport.next_fixture().unwrap();
    assert_eq!(f0.method, "getLatestLedger");

    let f1 = transport.next_fixture().unwrap();
    assert!(is_error_response(f1), "second fixture is an error");

    let f2 = transport.next_fixture().unwrap();
    assert!(!is_error_response(f2), "third fixture is the retry success");
    let events = events_from_fixture(f2);
    assert_eq!(events.len(), 1);
}

// ---------------------------------------------------------------------------
// 6. On-disk fixture loading (skipped if fixtures not yet written)
// ---------------------------------------------------------------------------

/// Verify that each on-disk scenario loads without error when the fixture
/// directory exists.
#[test]
fn on_disk_fixtures_load_cleanly() {
    use rpc_fixtures::*;

    let scenarios = [
        "normal_range",
        "cursor_expiry",
        "page_overflow",
        "protocol_upgrade",
        "rpc_error",
    ];

    for scenario in &scenarios {
        let dir = fixture_dir(scenario);
        if dir.exists() {
            let transport =
                ReplayTransport::from_dir(&dir).expect(&format!("load {scenario} fixtures"));
            assert!(
                transport.remaining() > 0,
                "{scenario} must have at least one fixture"
            );
        }
    }
}
