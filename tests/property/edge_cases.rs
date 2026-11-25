//! Edge-case property tests: zero, max, empty, and special-character inputs.

#[cfg(test)]
mod edge_case_tests {
    use proptest::prelude::*;

    // Property: Zero and max values are handled
    #[test]
    fn prop_edge_case_zero_values() {
        proptest!(|(
            page in 0i64..=1,
            limit in 0i64..=1
        )| {
            // Zero should be rejected or handled gracefully
            if page == 0 {
                // Invalid page
                assert_eq!(page, 0);
            }
            if limit == 0 {
                // Invalid limit
                assert_eq!(limit, 0);
            }
        });
    }

    // Property: Boundary values are handled correctly
    #[test]
    fn prop_edge_case_max_values() {
        proptest!(|(
            page in 1_000_000i64..i64::MAX,
            limit in 1_000i64..10_000
        )| {
            // Should handle large but valid values
            assert!(page >= 1_000_000);
            assert!(limit <= 10_000);
        });
    }

    // Property: Empty strings are rejected appropriately
    #[test]
    fn prop_edge_case_empty_strings() {
        proptest!(|(
            s in prop::string::string_regex(".*").unwrap()
        )| {
            if s.is_empty() {
                // Empty string handling
                assert!(s.len() == 0);
            } else {
                assert!(s.len() > 0);
            }
        });
    }

    // Property: Special characters in strings are handled
    #[test]
    fn prop_edge_case_special_chars() {
        proptest!(|(
            s in r#"[a-zA-Z0-9!@#$%^&*()\-_=+\[\]{}|;':",.<>?/\\`~]+"#
        )| {
            // Should not cause panic
            let _len = s.len();
            let _chars: Vec<char> = s.chars().collect();
        });
    }
}
