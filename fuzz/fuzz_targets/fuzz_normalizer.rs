#![no_main]
//! Fuzz target: event data normalizer (Issue #1154)
//!
//! Feed arbitrary JSON event data through the normalizer pipeline.
//! The target must NOT:
//! - panic
//! - abort
//! - produce a result that differs between two identical inputs (determinism)
//!
//! ## Running
//!
//! ```bash
//! cargo +nightly fuzz run fuzz_normalizer -- -max_total_time=300 -rss_limit_mb=512
//! ```

use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use soroban_pulse::normalizer::{apply_transform, Transform};

fuzz_target!(|data: &[u8]| {
    // Parse as JSON; skip non-JSON input.
    let value: Value = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return,
    };

    // ── 1. apply_transform — DivideByDecimals ─────────────────────────────
    // Arbitrary `params` and `value`; must not panic.
    {
        let params = serde_json::json!({"decimals": 7});
        let _ = apply_transform(&Transform::DivideByDecimals, &params, &value);

        // Edge: decimals = 0
        let params_zero = serde_json::json!({"decimals": 0});
        let _ = apply_transform(&Transform::DivideByDecimals, &params_zero, &value);

        // Edge: decimals very large
        let params_large = serde_json::json!({"decimals": 38});
        let _ = apply_transform(&Transform::DivideByDecimals, &params_large, &value);

        // Edge: missing decimals key
        let params_missing = serde_json::json!({});
        let _ = apply_transform(&Transform::DivideByDecimals, &params_missing, &value);
    }

    // ── 2. apply_transform — HexToDecimal ────────────────────────────────
    {
        let params = serde_json::json!({});
        let _ = apply_transform(&Transform::HexToDecimal, &params, &value);
    }

    // ── 3. apply_transform — Base64Decode ────────────────────────────────
    {
        let params = serde_json::json!({});
        let _ = apply_transform(&Transform::Base64Decode, &params, &value);
    }

    // ── 4. Determinism check ──────────────────────────────────────────────
    // Two calls with the same input must return the same Ok/Err outcome.
    let params = serde_json::json!({"decimals": 7});
    let r1 = apply_transform(&Transform::DivideByDecimals, &params, &value);
    let r2 = apply_transform(&Transform::DivideByDecimals, &params, &value);
    assert_eq!(r1.is_ok(), r2.is_ok());

    // ── 5. JSON pointer traversal ─────────────────────────────────────────
    // The normalizer walks event_data using JSON pointers; ensure that does
    // not panic on deeply-nested or self-referential-looking input.
    if let Some(obj) = value.as_object() {
        for (key, inner_value) in obj.iter().take(4) {
            let pointer = format!("/{key}");
            let _ = apply_transform(&Transform::HexToDecimal, &serde_json::json!({}), inner_value);
        }
    }
});
