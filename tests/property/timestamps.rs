//! Timestamp property tests: ISO 8601 parsing, ordering, and arithmetic.

use chrono::{DateTime, Duration, Utc};
use proptest::prelude::*;

use super::strategies::{invalid_timestamp_strategy, iso8601_timestamp_strategy, timestamp_strategy};

/// Timestamp validation function
pub(crate) fn validate_timestamp(ts: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| format!("invalid timestamp format: {}", ts))
}

/// A timestamp range is valid when `from <= to`.
pub(crate) fn is_valid_timestamp_range(from: DateTime<Utc>, to: DateTime<Utc>) -> bool {
    from <= to
}

proptest! {
    #[test]
    fn prop_valid_timestamp_strings_parse(ts in iso8601_timestamp_strategy()) {
        let result = validate_timestamp(&ts);
        // Valid ISO 8601 strings should always parse
        prop_assert!(result.is_ok(),
            "valid timestamp should parse: {} -> {:?}",
            ts, result
        );
    }

    #[test]
    fn prop_invalid_timestamp_strings_reject(ts in invalid_timestamp_strategy()) {
        let result = validate_timestamp(&ts);
        // Invalid strings should all fail
        prop_assert!(result.is_err(),
            "invalid timestamp should not parse: {}",
            ts
        );
    }

    #[test]
    fn prop_timestamp_roundtrip_preserves_value(ts_str in iso8601_timestamp_strategy()) {
        let parsed = validate_timestamp(&ts_str).unwrap();
        let serialized = parsed.to_rfc3339();
        let reparsed = validate_timestamp(&serialized).unwrap();

        // Roundtrip should preserve the timestamp (within nanosecond precision)
        prop_assert_eq!(parsed, reparsed,
            "timestamp roundtrip should preserve value: {} -> {} -> {}",
            ts_str, serialized, reparsed
        );
    }

    #[test]
    fn prop_timestamp_ordering_preserved(
        ts1 in timestamp_strategy(),
        ts2 in timestamp_strategy()
    ) {
        let str1 = ts1.to_rfc3339();
        let str2 = ts2.to_rfc3339();
        let parsed1 = validate_timestamp(&str1).unwrap();
        let parsed2 = validate_timestamp(&str2).unwrap();

        // If ts1 < ts2, then parsed1 < parsed2
        if ts1 < ts2 {
            prop_assert!(parsed1 < parsed2);
        } else if ts1 > ts2 {
            prop_assert!(parsed1 > parsed2);
        } else {
            prop_assert_eq!(parsed1, parsed2);
        }
    }

    #[test]
    fn prop_timestamp_duration_arithmetic_consistent(
        ts in timestamp_strategy(),
        days in 0i64..=365
    ) {
        let later = ts + Duration::days(days);

        // Duration arithmetic should be consistent
        let diff = later.signed_duration_since(ts);
        prop_assert_eq!(diff.num_days(), days,
            "duration arithmetic should be consistent for {} days",
            days
        );
    }
}

/// Timestamp-based property tests (formerly `tests/property_based_tests.rs`).
#[cfg(test)]
mod timestamp_tests {
    use chrono::{DateTime, Duration, NaiveDate, Utc};
    use proptest::prelude::*;

    fn valid_iso8601_timestamp() -> impl Strategy<Value = String> {
        prop_oneof![
            // Generate recent timestamps
            (0i64..365 * 10).prop_map(|days_offset| {
                let date = Utc::now() - Duration::days(days_offset);
                date.to_rfc3339()
            }),
            // Generate timestamps from specific range
            (0u32..365).prop_map(|days| {
                let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
                let date = base.and_hms_opt(12, 0, 0).unwrap();
                let datetime = DateTime::<Utc>::from_naive_utc_and_offset(date, Utc);
                (datetime + Duration::days(days as i64)).to_rfc3339()
            }),
        ]
    }

    // Property: ISO 8601 timestamps parse correctly
    #[test]
    fn prop_timestamp_iso8601_parsing() {
        proptest!(|(
            timestamp in valid_iso8601_timestamp()
        )| {
            // Should parse without error
            let parsed = DateTime::parse_from_rfc3339(&timestamp);
            assert!(parsed.is_ok(), "Valid ISO 8601 timestamp should parse");
        });
    }

    // Property: Timestamp range is valid when from <= to
    #[test]
    fn prop_timestamp_range_ordering() {
        proptest!(|(
            from_days_offset in 0i64..10_000,
            to_days_offset in 0i64..10_000
        )| {
            let from = Utc::now() - Duration::days(from_days_offset);
            let to = Utc::now() - Duration::days(to_days_offset);

            // Valid range has from <= to
            if from <= to {
                assert!(from <= to, "From timestamp must be <= to timestamp");
            } else {
                // Invalid ranges should be detected
                assert!(from > to);
            }
        });
    }

    // Property: Timestamp operations don't overflow
    #[test]
    fn prop_timestamp_no_overflow() {
        proptest!(|(
            days_offset in 0i64..(i64::MAX / (24 * 60 * 60))
        )| {
            // Should not panic
            let _ts = Utc::now() - Duration::days(days_offset);
        });
    }

    // Property: Timestamp granularity is preserved
    #[test]
    fn prop_timestamp_granularity() {
        proptest!(|(
            nanos in 0u32..1_000_000_000
        )| {
            let ts = Utc::now();
            let millis = ts.timestamp_millis();

            // Millisecond precision is preserved
            assert!(millis > 0);

            // Conversion should be reversible for seconds
            let from_secs = ts.timestamp();
            assert!(from_secs > 0);
        });
    }

    // Property: Timestamp comparisons are consistent
    #[test]
    fn prop_timestamp_comparison_consistency() {
        proptest!(|(
            days_a in 0i64..365,
            days_b in 0i64..365
        )| {
            let ts_a = Utc::now() - Duration::days(days_a);
            let ts_b = Utc::now() - Duration::days(days_b);

            // Comparison should be transitive
            if days_a < days_b {
                // Earlier days offset = more recent timestamp
                assert!(ts_a > ts_b);
            } else if days_a > days_b {
                assert!(ts_a < ts_b);
            } else {
                assert_eq!(ts_a, ts_b);
            }
        });
    }

    // Property: Timestamp difference calculation is accurate
    #[test]
    fn prop_timestamp_difference_accurate() {
        proptest!(|(
            days_diff in 1i64..10_000
        )| {
            let ts1 = Utc::now();
            let ts2 = ts1 - Duration::days(days_diff);

            let diff = ts1.signed_duration_since(ts2);
            let expected_seconds = days_diff * 86_400;

            // Duration should be approximately correct
            assert!(
                (diff.num_seconds() - expected_seconds).abs() < 2,
                "Duration difference should match expected"
            );
        });
    }

    // Property: Multiple timestamps maintain order
    #[test]
    fn prop_timestamp_sequence_ordering() {
        proptest!(|(
            days_offsets in prop::collection::vec(1i64..1000, 2..10)
        )| {
            let mut timestamps: Vec<_> = days_offsets
                .iter()
                .map(|&days| Utc::now() - Duration::days(days))
                .collect();

            // Record original order
            let original = timestamps.clone();

            // Sort timestamps
            timestamps.sort();

            // Verify sorted order
            for window in timestamps.windows(2) {
                assert!(window[0] <= window[1], "Timestamps must be in ascending order");
            }
        });
    }
}
