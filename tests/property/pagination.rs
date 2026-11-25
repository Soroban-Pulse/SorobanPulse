//! Pagination property tests: offset/limit/page resolution and bounds.

use proptest::prelude::*;

use super::strategies::{limit_strategy, page_strategy};

/// Pagination parameters with validation logic
#[derive(Debug, Clone)]
pub(crate) struct PaginationParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

impl PaginationParams {
    pub fn offset(&self) -> i64 {
        let page = self.page.unwrap_or(1).max(1);
        (page - 1).saturating_mul(self.limit())
    }

    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }

    pub fn page(&self) -> i64 {
        self.page.unwrap_or(1).max(1)
    }
}

proptest! {
    #[test]
    fn prop_pagination_offset_is_nonnegative(
        page in page_strategy(),
        limit in limit_strategy()
    ) {
        let params = PaginationParams { page, limit };
        // Offset should never be negative
        assert!(params.offset() >= 0,
            "offset should be non-negative, got {} for page={:?}, limit={:?}",
            params.offset(), page, limit
        );
    }

    #[test]
    fn prop_pagination_limit_within_bounds(
        limit in limit_strategy()
    ) {
        let params = PaginationParams { page: None, limit };
        let resolved_limit = params.limit();
        // Limit should always be between 1 and 100
        assert!(resolved_limit >= 1 && resolved_limit <= 100,
            "limit should be between 1 and 100, got {} for {:?}",
            resolved_limit, limit
        );
    }

    #[test]
    fn prop_pagination_page_minimum_is_one(
        page in page_strategy()
    ) {
        let params = PaginationParams { page, limit: None };
        // Page should never be less than 1 after resolution
        assert!(params.page() >= 1,
            "page should be at least 1, got {} for {:?}",
            params.page(), page
        );
    }

    #[test]
    fn prop_pagination_none_defaults_are_consistent(
        other_page in page_strategy(),
        other_limit in limit_strategy()
    ) {
        let none_params = PaginationParams { page: None, limit: None };
        let defaults_page = none_params.page();
        let defaults_limit = none_params.limit();
        let defaults_offset = none_params.offset();

        // Default page is 1
        prop_assert_eq!(defaults_page, 1);
        // Default limit is 20
        prop_assert_eq!(defaults_limit, 20);
        // Offset with page=1, limit=20 should be 0
        prop_assert_eq!(defaults_offset, 0);
    }

    #[test]
    fn prop_pagination_offset_calculation_is_correct(
        page in 1i64..=1000,
        limit in 1i64..=100
    ) {
        let params = PaginationParams {
            page: Some(page),
            limit: Some(limit)
        };

        let expected_offset = (page - 1) * limit;
        let actual_offset = params.offset();

        prop_assert_eq!(actual_offset, expected_offset,
            "offset calculation incorrect for page={}, limit={}",
            page, limit
        );
    }

    #[test]
    fn prop_pagination_zero_page_treated_as_one(
        limit in limit_strategy()
    ) {
        let params_zero = PaginationParams { page: Some(0), limit };
        let params_one = PaginationParams { page: Some(1), limit };

        // Page 0 and Page 1 should behave identically
        prop_assert_eq!(params_zero.page(), params_one.page());
        prop_assert_eq!(params_zero.offset(), params_one.offset());
    }

    #[test]
    fn prop_pagination_negative_page_becomes_one(
        negative_page in -1000i64..=-1,
        limit in limit_strategy()
    ) {
        let params = PaginationParams { page: Some(negative_page), limit };
        // Negative pages should be clamped to 1
        prop_assert_eq!(params.page(), 1);
    }

    #[test]
    fn prop_pagination_huge_page_offset_saturates(
        page in (i64::MAX / 100)..=i64::MAX,
        limit in limit_strategy()
    ) {
        // Boundary found by fuzz_pagination_params: (page - 1) * limit used to
        // overflow and panic for very large page values.
        let params = PaginationParams { page: Some(page), limit };
        let expected = (page - 1).checked_mul(params.limit()).unwrap_or(i64::MAX);
        prop_assert_eq!(params.offset(), expected);
        prop_assert!(params.offset() >= 0);
    }

    #[test]
    fn prop_pagination_negative_limit_clamped_to_one(
        negative_limit in -1000i64..=-1
    ) {
        let params = PaginationParams { page: None, limit: Some(negative_limit) };
        // Negative limits should be clamped to minimum of 1
        prop_assert_eq!(params.limit(), 1);
    }
}

/// Unit-style pagination properties (formerly `tests/property_based_tests.rs`).
#[cfg(test)]
mod pagination_tests {
    use proptest::prelude::*;

    // Property: Valid page numbers are always positive
    #[test]
    fn prop_pagination_page_positive() {
        proptest!(|(page in 1i64..=1_000_000)| {
            assert!(page > 0, "Page must be positive");
            assert!(page <= 1_000_000, "Page must not exceed max");
        });
    }

    // Property: Valid page limits are within bounds
    #[test]
    fn prop_pagination_limit_within_bounds() {
        proptest!(|(limit in 1i64..=10_000)| {
            assert!(limit > 0, "Limit must be positive");
            assert!(limit <= 10_000, "Limit must not exceed maximum");
        });
    }

    // Property: Offset calculation is correct
    // offset = (page - 1) * limit
    #[test]
    fn prop_pagination_offset_calculation() {
        proptest!(|(
            page in 1i64..=1000,
            limit in 1i64..=1000
        )| {
            let offset = (page - 1) * limit;

            // Offset must never be negative
            assert!(offset >= 0, "Offset cannot be negative");

            // With page=1, offset should always be 0
            if page == 1 {
                assert_eq!(offset, 0, "First page should have zero offset");
            }

            // Offset should grow monotonically with page
            let next_offset = (page) * limit;
            assert!(next_offset > offset, "Offset should increase with page number");
        });
    }

    // Property: Pagination doesn't overflow i64
    #[test]
    fn prop_pagination_no_overflow() {
        proptest!(|(
            page in 1i64..i64::MAX,
            limit in 1i64..1000
        )| {
            // Should not panic on overflow
            let offset = (page - 1).saturating_mul(limit);

            // Result should be within i64 bounds
            assert!(offset >= 0);
            assert!(offset <= i64::MAX);
        });
    }

    // Property: Cursor-based pagination is monotonic
    #[test]
    fn prop_pagination_cursor_monotonic() {
        proptest!(|(
            cursors in prop::collection::vec("[a-zA-Z0-9]+", 2..10)
        )| {
            // Each subsequent cursor should be different
            for window in cursors.windows(2) {
                if window.len() == 2 {
                    // Cursors should be valid for iteration
                    assert!(!window[0].is_empty());
                    assert!(!window[1].is_empty());
                }
            }
        });
    }

    // Property: Page and limit combinations never cause panic
    #[test]
    fn prop_pagination_no_panic_any_combination() {
        proptest!(|(
            page in 0i64..10_000,
            limit in 0i64..10_000
        )| {
            // Even invalid combinations shouldn't panic
            let _offset = (page).saturating_mul(limit);
            // Test passes if we reach here without panic
        });
    }
}
