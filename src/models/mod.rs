//! Models module — Issue #990.
//!
//! Split from the monolithic `models.rs` into focused sub-modules.
//!
//! | Sub-module       | Contents                                      |
//! |------------------|-----------------------------------------------|
//! | `event`         | Event types, pagination, query params, SorobanEvent |
//! | `notification`  | Notification channels, webhooks, system webhooks |
//! | `integration`   | Pool config, statistics, health checks        |
//!
//! All types are re-exported at the module root for backward
//! compatibility with existing import paths (`crate::models::Event`, etc.).

pub mod event;
pub mod integration;
pub mod notification;

// ---------------------------------------------------------------------------
// Re-exports for backward compatibility
// ---------------------------------------------------------------------------

pub use event::*;
pub use integration::*;
pub use notification::*;
