//! `net` module — shared networking utilities.
//!
//! ## `outbound`
//!
//! Centralised SSRF protection for all user-supplied outbound URLs.
//! See [`outbound`] for details and the threat model at
//! `docs/security/threat-model.md`.

pub mod outbound;
