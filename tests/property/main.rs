//! Consolidated property-based test binary.
//!
//! Merges the former `tests/property_tests.rs` and `tests/property_based_tests.rs`
//! into one test target (Issue #1503). Each domain lives in its own submodule,
//! and shared proptest strategies live in `strategies`.
//!
//! Run with: `cargo test --test property`

mod edge_cases;
mod filters;
mod pagination;
mod shrinking;
mod strategies;
mod timestamps;
