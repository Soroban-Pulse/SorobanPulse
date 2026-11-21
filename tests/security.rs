//! Security test suite entry point.
//!
//! Run all security tests:
//!   cargo test --test security
//!
//! Run a specific sub-module:
//!   cargo test --test security owasp_tests
//!   cargo test --test security auth_bypass_tests
//!   cargo test --test security crypto_tests
//!   cargo test --test security regression_tests
//!   cargo test --test security graphql_limit_tests
//!   cargo test --test security problem_detail_tests
//!   cargo test --test security statistics_injection_tests
//!   cargo test --test security webhook_ssrf_tests
//!   cargo test --test security outbound_ssrf_tests
//!   cargo test --test security cors_tests

mod security;