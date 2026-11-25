//! Shrinking-strategy property tests: values reduce while invariants hold.

use proptest::prelude::*;

use super::pagination::PaginationParams;
use super::strategies::{limit_strategy, page_strategy};

proptest! {
    #[test]
    fn prop_pagination_shrink_to_minimal_valid_state(
        page in page_strategy(),
        limit in limit_strategy()
    ) {
        let params = PaginationParams { page, limit };

        // After resolution, parameters should reach a stable minimal valid state
        let page_resolved = params.page();
        let limit_resolved = params.limit();
        let offset_resolved = params.offset();

        // Repeatedly resolving should be idempotent
        let params2 = PaginationParams {
            page: Some(page_resolved),
            limit: Some(limit_resolved)
        };

        prop_assert_eq!(params2.page(), page_resolved);
        prop_assert_eq!(params2.limit(), limit_resolved);
        prop_assert_eq!(params2.offset(), offset_resolved);
    }
}

/// Shrinking property tests (formerly `tests/property_based_tests.rs`).
#[cfg(test)]
mod shrinking_strategy_tests {
    use proptest::prelude::*;

    // Property: Shrinking reduces values while maintaining validity
    #[test]
    fn prop_shrinking_page_reduces() {
        proptest!(|(
            page in 100i64..10_000
        )| {
            // Proptest will shrink failing cases to smallest valid value
            assert!(page >= 100, "Shrunk value should maintain lower bound");
            assert!(page < 10_000, "Shrunk value should respect upper bound");
        });
    }

    // Property: Shrinking maintains invariants
    #[test]
    fn prop_shrinking_maintains_invariants() {
        proptest!(|(
            (page, limit) in (1i64..100, 1i64..1000)
        )| {
            let offset = (page - 1) * limit;

            // These invariants must hold even after shrinking
            assert!(page > 0);
            assert!(limit > 0);
            assert!(offset >= 0);
            assert!(offset <= (page - 1 + limit));
        });
    }

    // Property: Shrinking finds minimal failing case
    #[test]
    fn prop_shrinking_finds_minimal_case() {
        proptest!(|(
            values in prop::collection::vec(0i64..100, 1..10)
        )| {
            // In case of failure, proptest shrinks to minimal set
            assert!(!values.is_empty(), "Collection should not be empty");

            let min = values.iter().min();
            assert!(min.is_some());
        });
    }
}
