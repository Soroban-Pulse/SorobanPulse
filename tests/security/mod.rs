//! Security test suite for SorobanPulse.
//!
//! This module organises all security tests into focused sub-modules:
//! - [`owasp_tests`]      — OWASP Top 10 coverage
//! - [`auth_bypass_tests`] — authentication/authorisation bypass scenarios
//! - [`crypto_tests`]     — cryptographic strength verification
//! - [`regression_tests`] — locked-in security invariants preventing regressions

pub mod auth_bypass_tests;
pub mod crypto_tests;
pub mod owasp_tests;
pub mod regression_tests;
