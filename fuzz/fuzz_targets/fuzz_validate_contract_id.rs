#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };

    // Must not panic.
    let result1 = soroban_pulse::handlers::validate_contract_id(s);
    // Must be deterministic.
    let result2 = soroban_pulse::handlers::validate_contract_id(s);
    assert_eq!(result1.is_ok(), result2.is_ok());

    // A valid contract_id must be accepted.
    // 56-char alphanumeric string starting with 'C' is always valid.
    if s.len() == 56 && s.starts_with('C') && s.chars().all(|c| c.is_ascii_alphanumeric()) {
        assert!(result1.is_ok(), "valid contract_id was rejected: {s}");
    }
});
