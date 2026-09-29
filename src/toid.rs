//! Deterministic intra-ledger ordering (#1065).
//!
//! Stellar RPC event ids are TOIDs: a 64-bit integer encoding
//! `(ledger_sequence, transaction_order, operation_index)` packed as
//! `ledger << 32 | tx_order << 20 | op_index << 12`, with an event index
//! appended after a `-` (e.g. `"0000123456-0000000042"`). Parsing these
//! gives a fully deterministic ordering within a ledger that is stable
//! across re-indexing and independent of insertion order.

/// The (ledger, tx_index, op_index, event_index) tuple decoded from an RPC
/// event id / paging token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct EventOrdinal {
    pub ledger: u32,
    pub tx_index: i32,
    pub op_index: i32,
    pub event_index: i32,
}

/// Parse an RPC event id of the form `"<toid>-<event_index>"` where `<toid>`
/// is the standard Stellar TOID (transaction order id) and `<event_index>`
/// is the zero-based index of the event within that operation.
///
/// Returns `None` if the id does not match the expected shape; callers
/// should fall back to `NULL` ordering columns in that case rather than
/// failing the whole ingest.
pub fn parse_event_id(id: &str) -> Option<EventOrdinal> {
    let (toid_str, event_index_str) = id.split_once('-')?;
    let toid: u64 = toid_str.parse().ok()?;
    let event_index: i32 = event_index_str.parse().ok()?;

    // TOID layout (see stellar/go/toid): bits 63-32 ledger sequence,
    // bits 31-12 transaction order within the ledger, bits 11-0 operation
    // index within the transaction.
    let ledger = (toid >> 32) as u32;
    let tx_index = ((toid >> 12) & 0xF_FFFF) as i32;
    let op_index = (toid & 0xFFF) as i32;

    Some(EventOrdinal {
        ledger,
        tx_index,
        op_index,
        event_index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_well_formed_id() {
        let toid: u64 = (123_456u64 << 32) | (42u64 << 12) | 3u64;
        let id = format!("{toid}-7");
        let parsed = parse_event_id(&id).unwrap();
        assert_eq!(parsed.ledger, 123_456);
        assert_eq!(parsed.tx_index, 42);
        assert_eq!(parsed.op_index, 3);
        assert_eq!(parsed.event_index, 7);
    }

    #[test]
    fn rejects_malformed_id() {
        assert!(parse_event_id("not-a-toid").is_none());
        assert!(parse_event_id("12345").is_none());
    }

    #[test]
    fn ordinals_order_deterministically_within_ledger() {
        let a = EventOrdinal { ledger: 1, tx_index: 0, op_index: 0, event_index: 0 };
        let b = EventOrdinal { ledger: 1, tx_index: 0, op_index: 0, event_index: 1 };
        let c = EventOrdinal { ledger: 1, tx_index: 1, op_index: 0, event_index: 0 };
        assert!(a < b);
        assert!(b < c);
    }
}
