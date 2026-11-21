//! `net` module — shared networking utilities.
//!
//! ## `outbound`
//!
//! Centralised SSRF protection for all user-supplied outbound URLs.
//! See [`outbound`] for details and the threat model at
//! `docs/security/threat-model.md`.
//!
//! ## Outbound HTTP policy
//!
//! **All** outbound HTTP requests to user-configurable endpoints MUST go
//! through [`OutboundClient`]. Direct construction of a `reqwest::Client`
//! outside this module is disallowed (see `clippy.toml`
//! `disallowed-methods`). The [`OutboundClient`] wrapper validates the URL
//! against the SSRF guard, pins the resolved IP to defend against DNS
//! rebinding, constrains redirects, and applies timeouts and response size
//! limits.

pub mod outbound;

pub use outbound::{
    validate_outbound_url, validate_outbound_url_static, OutboundClient, OutboundClientBuilder,
    OutboundError, OutboundPolicy,
};