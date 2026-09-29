#![no_main]
//! Fuzz target: contract spec / WASM custom section parsing (Issue #1154)
//!
//! Feed arbitrary WASM-like bytes through the contract spec extraction code:
//! - `custom_section(wasm, "contractspecv0")` — extract spec bytes
//! - `wasm_hash_from_instance(entry)` — parse a ContractData XDR entry
//! - `contract_hash(contract_id)` — decode a C-type Strkey
//!
//! The target must NOT panic on any input.
//!
//! ## Running
//!
//! ```bash
//! cargo +nightly fuzz run fuzz_contract_spec -- -max_total_time=300 -rss_limit_mb=512
//! ```
//!
//! ## Corpus
//!
//! Seeds are placed in `fuzz/corpus/fuzz_contract_spec/`.  Include real WASM
//! binaries from testnet contracts to seed realistic paths.

use libfuzzer_sys::fuzz_target;
use soroban_pulse::contract_specs::{contract_hash, custom_section, wasm_hash_from_instance};

fuzz_target!(|data: &[u8]| {
    // ── 1. custom_section — arbitrary WASM bytes ──────────────────────────
    // Must not panic regardless of the input.
    let _ = custom_section(data, "contractspecv0");
    let _ = custom_section(data, "contractenvmetav0");
    let _ = custom_section(data, ""); // empty section name

    // ── 2. wasm_hash_from_instance — arbitrary XDR-like bytes ────────────
    // Must not panic.
    let _ = wasm_hash_from_instance(data);

    // ── 3. contract_hash — arbitrary Strkey strings ───────────────────────
    // Attempt to interpret the data as a UTF-8 string.
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = contract_hash(s);
    }

    // ── 4. Determinism — same input must produce identical results ─────────
    let r1 = custom_section(data, "contractspecv0");
    let r2 = custom_section(data, "contractspecv0");
    assert_eq!(r1.is_some(), r2.is_some());

    // ── 5. Valid WASM magic guard ─────────────────────────────────────────
    // If the input starts with the WASM magic bytes, the parser must at
    // least not abort (it may return None for a malformed body).
    if data.starts_with(b"\0asm") {
        let result = custom_section(data, "contractspecv0");
        // Any outcome (Some / None) is valid; panic is not.
        let _ = result;
    }
});
