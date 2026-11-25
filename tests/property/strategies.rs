//! Proptest strategies shared across the property-test domains.

#![allow(dead_code)]

use chrono::{DateTime, Utc};
use proptest::prelude::*;

/// Strategy for generating valid page numbers
pub(crate) fn page_strategy() -> impl Strategy<Value = Option<i64>> {
    prop_oneof![
        Just(None),
        Just(Some(0)),  // Edge case: 0
        Just(Some(1)),  // Boundary: 1
        (1i64..=1000).prop_map(Some),
        (1001i64..=i64::MAX).prop_map(Some),
    ]
}

/// Strategy for generating valid limit numbers
pub(crate) fn limit_strategy() -> impl Strategy<Value = Option<i64>> {
    prop_oneof![
        Just(None),
        Just(Some(-1)),  // Negative edge case
        Just(Some(0)),   // Zero edge case
        Just(Some(1)),   // Boundary: min valid
        Just(Some(100)), // Boundary: max valid
        Just(Some(101)), // Edge case: exceeds max
        (1i64..=100).prop_map(Some),
        (101i64..=i64::MAX).prop_map(Some),
    ]
}

/// Strategy for generating valid RFC3339 timestamps
pub(crate) fn timestamp_strategy() -> impl Strategy<Value = DateTime<Utc>> {
    // Generate timestamps between 2020 and 2030
    (1577836800i64..=1893456000i64)
        .prop_map(|secs| DateTime::<Utc>::from_timestamp(secs, 0).unwrap())
}

/// Strategy for ISO 8601 timestamp strings
pub(crate) fn iso8601_timestamp_strategy() -> impl Strategy<Value = String> {
    timestamp_strategy().prop_map(|dt| dt.to_rfc3339())
}

/// Strategy for invalid timestamp strings
pub(crate) fn invalid_timestamp_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("2024-13-45T25:61:61Z".to_string()),  // Invalid month/day/hour
        Just("not-a-timestamp".to_string()),
        Just("2024/01/01 12:00:00".to_string()),   // Wrong format
        Just("2024-01-01".to_string()),             // Missing time
        (0u32..1000).prop_map(|n| format!("garbage_{}", n)),
    ]
}

/// Strategy for contract IDs (hex strings)
pub(crate) fn contract_id_strategy() -> impl Strategy<Value = String> {
    "[0-9a-f]{56}".prop_map(|s| s)
}
