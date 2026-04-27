#![no_main]
use libfuzzer_sys::fuzz_target;
use soroban_pulse::models::PaginationParams;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else { return };

    // Deserialize from a query-string-like JSON object. Must not panic.
    let result: Result<PaginationParams, _> = serde_json::from_str(s);

    if let Ok(params) = result {
        // offset() and limit() must not panic and must be in valid ranges.
        let limit = params.limit();
        let offset = params.offset();
        assert!((1..=100).contains(&limit));
        assert!(offset >= 0);

        // columns() must not panic.
        let _ = params.columns();

        // Determinism: same input → same output.
        let params2: PaginationParams = serde_json::from_str(s).unwrap();
        assert_eq!(params.limit(), params2.limit());
        assert_eq!(params.offset(), params2.offset());
    }
});
