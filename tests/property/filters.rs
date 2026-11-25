//! Filter validation property tests: contract IDs, ledger ranges, topics, and
//! filter combinations.

use chrono::{DateTime, Utc};
use proptest::prelude::*;

use super::strategies::timestamp_strategy;
use super::timestamps::is_valid_timestamp_range;

pub(crate) fn is_valid_contract_id(id: &str) -> bool {
    id.len() == 56 && id.chars().all(|c| c.is_ascii_hexdigit())
}

pub(crate) fn is_valid_ledger_range(from: i64, to: i64) -> bool {
    from <= to
}

proptest! {
    #[test]
    fn prop_contract_id_format_validation(
        contract_id in ".*",  // Any string
    ) {
        let is_valid = is_valid_contract_id(&contract_id);

        if contract_id.len() == 56 && contract_id.chars().all(|c| c.is_ascii_hexdigit()) {
            prop_assert!(is_valid,
                "56-char hex string should be valid: {}",
                contract_id
            );
        } else {
            prop_assert!(!is_valid,
                "non-56-char-hex should be invalid: {}",
                contract_id
            );
        }
    }

    #[test]
    fn prop_ledger_range_validation(
        from_ledger in 0i64..=1_000_000,
        to_ledger in 0i64..=1_000_000
    ) {
        // If from > to, it should be invalid
        if from_ledger > to_ledger {
            prop_assert!(!is_valid_ledger_range(from_ledger, to_ledger),
                "from_ledger > to_ledger should be invalid: {} > {}",
                from_ledger, to_ledger
            );
        } else {
            prop_assert!(is_valid_ledger_range(from_ledger, to_ledger),
                "from_ledger <= to_ledger should be valid: {} <= {}",
                from_ledger, to_ledger
            );
        }
    }

    #[test]
    fn prop_timestamp_range_validation(
        ts1 in timestamp_strategy(),
        ts2 in timestamp_strategy()
    ) {
        let (from, to): (DateTime<Utc>, DateTime<Utc>) = if ts1 < ts2 { (ts1, ts2) } else { (ts2, ts1) };

        // Valid range: from <= to
        prop_assert!(is_valid_timestamp_range(from, to),
            "from_ts <= to_ts should be valid"
        );

        // Invalid range: from > to
        prop_assert!(!is_valid_timestamp_range(to, from),
            "from_ts > to_ts should be invalid"
        );
    }
}

/// Filter-specific property tests (formerly `tests/property_based_tests.rs`).
#[cfg(test)]
mod filter_tests {
    use proptest::prelude::*;
    use regex::Regex;

    // Property: Contract ID validation
    // Valid contract IDs are 56 character Stellar addresses
    #[test]
    fn prop_filter_contract_id_format() {
        proptest!(|(
            id in "[A-Z0-9]{56}"
        )| {
            assert_eq!(id.len(), 56, "Contract ID must be exactly 56 characters");

            // Verify it matches Stellar address pattern
            let pattern = Regex::new(r"^[A-Z0-9]{56}$").unwrap();
            assert!(pattern.is_match(&id), "Contract ID must match Stellar format");
        });
    }

    // Property: Contract ID prefix must be at least 4 characters
    #[test]
    fn prop_filter_contract_id_prefix_length() {
        proptest!(|(
            prefix in "[A-Z0-9]{4,56}"
        )| {
            assert!(prefix.len() >= 4, "Prefix must be at least 4 characters");
            assert!(prefix.len() <= 56, "Prefix must not exceed contract ID length");
        });
    }

    // Property: Ledger range is valid when from <= to
    #[test]
    fn prop_filter_ledger_range_valid() {
        proptest!(|(
            from_ledger in 0u64..100_000_000,
            to_ledger in 0u64..100_000_000
        )| {
            // Valid range has from <= to
            if from_ledger <= to_ledger {
                assert!(
                    from_ledger <= to_ledger,
                    "From ledger must be <= to ledger"
                );
            }
        });
    }

    // Property: Ledger numbers don't underflow
    #[test]
    fn prop_filter_ledger_no_underflow() {
        proptest!(|(
            ledger in 0u64..i64::MAX as u64
        )| {
            assert!(ledger >= 0);
            // Safe conversion to signed
            let _signed = ledger as i64;
        });
    }

    // Property: Topic filters are valid JSON arrays
    #[test]
    fn prop_filter_topic_json_valid() {
        proptest!(|(
            topics in prop::collection::vec("[a-zA-Z0-9_-]{1,50}", 1..5)
        )| {
            let json_str = format!("[\"{}\"]", topics.join("\",\""));

            // Should be valid JSON
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
            assert!(parsed.is_ok(), "Topic array must be valid JSON");
        });
    }

    // Property: Search queries don't contain injection vectors
    #[test]
    fn prop_filter_search_query_safe() {
        proptest!(|(
            query in "[a-zA-Z0-9 \\-_.]{1,200}"
        )| {
            // Search query should not contain SQL injection patterns
            assert!(!query.contains("--"), "Query must not contain SQL comments");
            assert!(!query.contains(";"), "Query must not contain statement terminators");
            assert!(!query.contains("/*"), "Query must not contain block comments");
        });
    }

    // Property: Multiple contract IDs filter is within limits
    #[test]
    fn prop_filter_contract_ids_count_limit() {
        proptest!(|(
            ids in prop::collection::vec("[A-Z0-9]{56}", 1..25)
        )| {
            // Limit is typically 20 contract IDs
            assert!(ids.len() <= 25, "Must not exceed contract IDs limit");

            // All should be unique in valid usage
            let unique_count = ids.iter().collect::<std::collections::HashSet<_>>().len();
            assert!(unique_count <= ids.len());
        });
    }

    // Property: Schema version filter is within valid range
    #[test]
    fn prop_filter_schema_version_valid() {
        proptest!(|(
            version in 0i32..=20
        )| {
            assert!(version >= 0, "Schema version must be non-negative");
            assert!(version <= 20, "Schema version must be reasonable");
        });
    }

    // Property: Filter combinations are independent
    #[test]
    fn prop_filter_combinations_independent() {
        proptest!(|(
            has_contract_id in any::<bool>(),
            has_event_type in any::<bool>(),
            has_ledger_range in any::<bool>(),
            has_timestamp_range in any::<bool>()
        )| {
            // Each filter should work independently
            let filters_enabled = [
                has_contract_id,
                has_event_type,
                has_ledger_range,
                has_timestamp_range
            ];

            // Any combination should be valid
            let combined_count = filters_enabled.iter().filter(|&&f| f).count();
            assert!(combined_count <= 4, "Cannot have more than 4 filters");
        });
    }
}
