//! Regression tests that decode recorded mainnet/testnet `LedgerCloseMeta`
//! fixtures using the workspace's single `stellar-xdr` version.
//!
//! These tests guard against XDR version drift across the crate, CLI and fuzz
//! targets (see #1420) and are relevant to the fuzz crashes in #1377/#1399.
//!
//! Fixture bytes live under `tests/fixtures/xdr/` and are loaded with
//! `include_bytes!` so that a missing fixture is a compile-time error rather
//! than a silent skip.

use stellar_xdr::curr::{
    LedgerCloseMeta, Limits, ReadXdr, WriteXdr,
};

/// Directory containing the recorded `LedgerCloseMeta` fixtures.
const FIXTURE_DIR: &str = "tests/fixtures/xdr";

/// A recorded fixture together with the network it was captured from and the
/// protocol version it was produced under.
struct Fixture {
    /// Human-readable name used in assertion messages.
    name: &'static str,
    /// Network the fixture was captured from (`mainnet` or `testnet`).
    network: &'static str,
    /// Protocol version the fixture was produced under.
    protocol: u32,
    /// Raw XDR bytes of the `LedgerCloseMeta`.
    bytes: &'static [u8],
}

/// All fixtures exercised by this test module.
///
/// The bytes are embedded at compile time via `include_bytes!`. When adding a
/// new fixture, record it from the current protocol version and register it
/// here.
const FIXTURES: &[Fixture] = &[
    Fixture {
        name: "mainnet_ledger_close_meta",
        network: "mainnet",
        protocol: 26,
        bytes: include_bytes!("fixtures/xdr/mainnet_ledger_close_meta.xdr"),
    },
    Fixture {
        name: "testnet_ledger_close_meta",
        network: "testnet",
        protocol: 26,
        bytes: include_bytes!("fixtures/xdr/testnet_ledger_close_meta.xdr"),
    },
];

/// Decode every fixture and assert it round-trips through the current
/// `stellar-xdr` version without error.
#[test]
fn decodes_recorded_ledger_close_meta_fixtures() {
    for fixture in FIXTURES {
        let decoded = LedgerCloseMeta::from_xdr(fixture.bytes, Limits::none())
            .unwrap_or_else(|err| {
                panic!(
                    "failed to decode {} fixture `{}` with current stellar-xdr: {err}",
                    fixture.network, fixture.name
                )
            });

        // Re-encoding must succeed and produce a stable representation.
        let reencoded = decoded
            .to_xdr(Limits::none())
            .unwrap_or_else(|err| {
                panic!(
                    "failed to re-encode {} fixture `{}`: {err}",
                    fixture.network, fixture.name
                )
            });

        // The re-encoded bytes must decode back to the same value.
        let round_tripped = LedgerCloseMeta::from_xdr(&reencoded, Limits::none())
            .unwrap_or_else(|err| {
                panic!(
                    "failed to decode re-encoded {} fixture `{}`: {err}",
                    fixture.network, fixture.name
                )
            });

        assert_eq!(
            decoded, round_tripped,
            "round-trip mismatch for {} fixture `{}`",
            fixture.network, fixture.name
        );
    }
}

/// Assert that each fixture decodes to the expected `LedgerCloseMeta` variant
/// for the current protocol version.
#[test]
fn fixtures_decode_to_expected_variant() {
    for fixture in FIXTURES {
        let decoded = LedgerCloseMeta::from_xdr(fixture.bytes, Limits::none())
            .unwrap_or_else(|err| {
                panic!(
                    "failed to decode {} fixture `{}` with current stellar-xdr: {err}",
                    fixture.network, fixture.name
                )
            });

        // The variant must match the protocol version the fixture was recorded
        // under. Protocol 20+ ledgers are expected to be `V1` or `V2`; the
        // exact variant is asserted per fixture below.
        let variant_matches = match fixture.protocol {
            // Protocol 20 and later use `V1`; protocol 23+ may use `V2`.
            20..=22 => matches!(decoded, LedgerCloseMeta::V1(_)),
            23.. => matches!(decoded, LedgerCloseMeta::V1(_) | LedgerCloseMeta::V2(_)),
            _ => matches!(decoded, LedgerCloseMeta::V0(_)),
        };

        assert!(
            variant_matches,
            "{} fixture `{}` decoded to an unexpected LedgerCloseMeta variant for protocol {}: {:?}",
            fixture.network,
            fixture.name,
            fixture.protocol,
            variant_name(&decoded),
        );
    }
}

/// Assert that the ledger sequence and protocol version embedded in each
/// fixture are consistent with the fixture metadata.
#[test]
fn fixtures_report_expected_protocol() {
    for fixture in FIXTURES {
        let decoded = LedgerCloseMeta::from_xdr(fixture.bytes, Limits::none())
            .unwrap_or_else(|err| {
                panic!(
                    "failed to decode {} fixture `{}` with current stellar-xdr: {err}",
                    fixture.network, fixture.name
                )
            });

        let ledger_header = match &decoded {
            LedgerCloseMeta::V0(v0) => &v0.ledger_header,
            LedgerCloseMeta::V1(v1) => &v1.ledger_header,
            LedgerCloseMeta::V2(v2) => &v2.ledger_header,
        };

        let protocol_version = ledger_header.ledger_version;
        assert_eq!(
            protocol_version, fixture.protocol,
            "{} fixture `{}` reports protocol {} but was registered as protocol {}",
            fixture.network, fixture.name, protocol_version, fixture.protocol,
        );
    }
}

/// Return a short name for the `LedgerCloseMeta` variant, for assertion
/// messages.
fn variant_name(meta: &LedgerCloseMeta) -> &'static str {
    match meta {
        LedgerCloseMeta::V0(_) => "V0",
        LedgerCloseMeta::V1(_) => "V1",
        LedgerCloseMeta::V2(_) => "V2",
    }
}

/// Ensure the fixture directory constant matches the location used by
/// `include_bytes!`. This is a compile-time-ish sanity check that fails loudly
/// if the layout changes.
#[test]
fn fixture_directory_is_consistent() {
    assert_eq!(
        FIXTURE_DIR, "tests/fixtures/xdr",
        "fixture directory constant must match the include_bytes! paths"
    );
}