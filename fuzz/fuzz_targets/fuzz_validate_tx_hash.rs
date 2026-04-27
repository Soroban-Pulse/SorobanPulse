#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };

    // Must not panic.
    let result1 = soroban_pulse::handlers::validate_tx_hash(s);
    // Must be deterministic.
    let result2 = soroban_pulse::handlers::validate_tx_hash(s);
    assert_eq!(result1.is_ok(), result2.is_ok());

    // A valid tx_hash must be accepted.
    // 64-char hex string is always valid.
    if s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()) {
        assert!(result1.is_ok(), "valid tx_hash was rejected: {s}");
    }
});
