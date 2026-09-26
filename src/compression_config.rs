//! HTTP response compression configuration — Issue #961
//!
//! [`routes::build_router`](crate::routes) wires a bare
//! `tower_http::compression::CompressionLayer` into the middleware stack.
//! That gives every response gzip/br/deflate negotiation "for free", but it
//! ships with three defaults that are wrong for a high-throughput event API:
//!
//! 1. The compression *level* is fixed at whatever `CompressionLayer::new()`
//!    picks (fast, low ratio) with no way to trade CPU for bandwidth.
//! 2. Every response gets run through the compressor, including tiny ones
//!    (a `{"status":"ok"}` health check) where the gzip header/footer
//!    overhead outweighs any savings and the syscall cost isn't worth it.
//! 3. There is no visibility into how often compression actually fires.
//!
//! This module centralizes those knobs so they can be tuned per-deployment
//! via environment variables without touching `routes.rs`, and exposes a
//! small middleware that records compression metrics.
//!
//! See `docs/compression-optimization.md` for operator-facing docs.

use axum::{body::Body, http::Request};
use tower_http::compression::{
    predicate::{DefaultPredicate, Predicate, SizeAbove},
    CompressionLayer, CompressionLevel,
};

/// Responses smaller than this are never compressed (issue #961: "bypass
/// for small responses"). 256 bytes comfortably covers gzip's own overhead
/// (~20 bytes) plus enough margin that we don't burn CPU compressing
/// near-empty JSON bodies like `{"status":"ok"}`.
pub const DEFAULT_MIN_COMPRESS_BYTES: u16 = 256;

/// Default gzip/br/deflate quality level (1 = fastest/lowest ratio,
/// 9 = slowest/highest ratio). 6 is zlib's own default and a reasonable
/// balance for JSON event payloads.
pub const DEFAULT_COMPRESSION_LEVEL: u32 = 6;

/// Resolved compression settings for the process.
#[derive(Debug, Clone, Copy)]
pub struct CompressionSettings {
    pub level: u32,
    pub min_size_bytes: u16,
}

impl Default for CompressionSettings {
    fn default() -> Self {
        Self {
            level: DEFAULT_COMPRESSION_LEVEL,
            min_size_bytes: DEFAULT_MIN_COMPRESS_BYTES,
        }
    }
}

impl CompressionSettings {
    /// Read settings from the environment, falling back to defaults for any
    /// variable that is unset or fails to parse.
    ///
    /// - `COMPRESSION_LEVEL` — integer 1..=9, clamped.
    /// - `COMPRESSION_MIN_SIZE_BYTES` — integer, responses smaller than this
    ///   (in bytes, based on `Content-Length`) skip compression entirely.
    pub fn from_env() -> Self {
        let level = std::env::var("COMPRESSION_LEVEL")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .map(|v| v.clamp(1, 9))
            .unwrap_or(DEFAULT_COMPRESSION_LEVEL);

        let min_size_bytes = std::env::var("COMPRESSION_MIN_SIZE_BYTES")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(DEFAULT_MIN_COMPRESS_BYTES);

        Self { level, min_size_bytes }
    }

    fn quality(self) -> CompressionLevel {
        CompressionLevel::Precise(self.level as i32)
    }

    /// Predicate combining the configured size floor with tower-http's
    /// [`DefaultPredicate`] (which already skips content types that are
    /// pointless or actively harmful to compress: SSE streams, gRPC,
    /// already-compressed media). Issue #961: "configure compression for
    /// all response types" — we want the *content-type* exclusions kept
    /// (compressing `text/event-stream` would break streaming semantics),
    /// while making the *size* floor configurable.
    fn predicate(self) -> impl Predicate {
        SizeAbove::new(self.min_size_bytes).and(DefaultPredicate::new())
    }

    /// Build a [`CompressionLayer`] configured with this process's level and
    /// bypass threshold.
    pub fn layer(self) -> CompressionLayer<impl Predicate + Clone> {
        CompressionLayer::new()
            .quality(self.quality())
            .compress_when(self.predicate())
    }
}

// ---------------------------------------------------------------------------
// Metrics middleware
// ---------------------------------------------------------------------------

/// Records whether a response left the compression layer compressed or was
/// bypassed. Must be registered *after* (i.e. wrapping) the
/// [`CompressionLayer`] so it observes the final `Content-Encoding` header.
pub async fn compression_metrics_middleware(
    req: Request<Body>,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let response = next.run(req).await;
    if response.headers().contains_key(axum::http::header::CONTENT_ENCODING) {
        crate::metrics::record_http_compression_outcome(true);
    } else {
        crate::metrics::record_http_compression_outcome(false);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_documented_constants() {
        let s = CompressionSettings::default();
        assert_eq!(s.level, DEFAULT_COMPRESSION_LEVEL);
        assert_eq!(s.min_size_bytes, DEFAULT_MIN_COMPRESS_BYTES);
    }

    #[test]
    fn from_env_clamps_out_of_range_level() {
        std::env::set_var("COMPRESSION_LEVEL", "42");
        std::env::set_var("COMPRESSION_MIN_SIZE_BYTES", "128");
        let s = CompressionSettings::from_env();
        assert_eq!(s.level, 9, "level must clamp to the max valid quality");
        assert_eq!(s.min_size_bytes, 128);
        std::env::remove_var("COMPRESSION_LEVEL");
        std::env::remove_var("COMPRESSION_MIN_SIZE_BYTES");
    }

    #[test]
    fn from_env_falls_back_on_garbage_input() {
        std::env::set_var("COMPRESSION_LEVEL", "not-a-number");
        let s = CompressionSettings::from_env();
        assert_eq!(s.level, DEFAULT_COMPRESSION_LEVEL);
        std::env::remove_var("COMPRESSION_LEVEL");
    }
}
