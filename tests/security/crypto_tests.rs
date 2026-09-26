//! Cryptographic Strength Verification Tests
//!
//! Verifies the security properties of every cryptographic primitive used by
//! SorobanPulse:
//!
//! - HMAC-SHA256 request signing (zero-trust layer)
//! - API key hashing (SHA-256)
//! - Webhook HMAC-SHA256 signature scheme
//! - API key rotation (double-rotation eviction)
//! - Constant-time comparison properties
//!
//! All tests are self-contained — no database required.

use chrono::{Duration, Utc};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use soroban_pulse::middleware::auth::{hash_api_key, key_matches_any};
use soroban_pulse::zero_trust::{ApiKeySet, RequestSignature};

type HmacSha256 = Hmac<Sha256>;

// ---------------------------------------------------------------------------
// Helper: fresh RFC3339 timestamp for tests that need a valid timestamp
// ---------------------------------------------------------------------------

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

fn old_rfc3339(secs: i64) -> String {
    (Utc::now() - Duration::seconds(secs)).to_rfc3339()
}

fn future_rfc3339(secs: i64) -> String {
    (Utc::now() + Duration::seconds(secs)).to_rfc3339()
}

// ---------------------------------------------------------------------------
// Standalone webhook HMAC verification helper
// ---------------------------------------------------------------------------

/// Verifies a `X-Signature-256: sha256=<hex>` header value against the body.
fn verify_webhook_hmac(header_value: &str, secret: &str, body: &[u8]) -> bool {
    let Some(sig_hex) = header_value.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(body);
    let computed = hex::encode(mac.finalize().into_bytes());
    sig_hex == computed
}

/// Produce a correctly-formatted webhook signature header value.
fn sign_webhook(secret: &str, body: &[u8]) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

// ===========================================================================
// HMAC-SHA256 Request Signatures (zero-trust layer)
// ===========================================================================

#[test]
fn hmac_signature_is_64_hex_chars() {
    let ts = now_rfc3339();
    let sig = RequestSignature::sign("secret", "GET", "/v1/events", &ts, "");
    assert_eq!(sig.len(), 64, "SHA-256 output must be 64 hex chars");
    assert!(
        sig.chars().all(|c| c.is_ascii_hexdigit()),
        "Output must be lowercase hex"
    );
}

#[test]
fn hmac_signature_is_deterministic() {
    let ts = now_rfc3339();
    let sig1 = RequestSignature::sign("secret", "GET", "/v1/events", &ts, "body");
    let sig2 = RequestSignature::sign("secret", "GET", "/v1/events", &ts, "body");
    assert_eq!(sig1, sig2, "Same inputs must always produce the same signature");
}

#[test]
fn hmac_different_secrets_produce_different_signatures() {
    let ts = now_rfc3339();
    let sig_a = RequestSignature::sign("secret-a", "GET", "/", &ts, "");
    let sig_b = RequestSignature::sign("secret-b", "GET", "/", &ts, "");
    assert_ne!(sig_a, sig_b);
}

#[test]
fn hmac_different_bodies_produce_different_signatures() {
    let ts = now_rfc3339();
    let sig1 = RequestSignature::sign("key", "POST", "/", &ts, r#"{"a":1}"#);
    let sig2 = RequestSignature::sign("key", "POST", "/", &ts, r#"{"a":2}"#);
    assert_ne!(sig1, sig2);
}

#[test]
fn hmac_different_paths_produce_different_signatures() {
    let ts = now_rfc3339();
    let sig1 = RequestSignature::sign("key", "GET", "/v1/events", &ts, "");
    let sig2 = RequestSignature::sign("key", "GET", "/v1/admin", &ts, "");
    assert_ne!(sig1, sig2, "Path must be included in the HMAC input");
}

#[test]
fn hmac_different_methods_produce_different_signatures() {
    let ts = now_rfc3339();
    let sig_get = RequestSignature::sign("key", "GET", "/v1/events", &ts, "");
    let sig_post = RequestSignature::sign("key", "POST", "/v1/events", &ts, "");
    assert_ne!(sig_get, sig_post, "HTTP method must be included in the HMAC input");
}

#[test]
fn hmac_verify_roundtrip_passes() {
    let ts = now_rfc3339();
    let sig = RequestSignature::sign("my-secret", "PUT", "/v1/events/123", &ts, "data");
    assert!(RequestSignature::verify(
        "my-secret",
        "PUT",
        "/v1/events/123",
        &ts,
        "data",
        &sig
    ));
}

#[test]
fn hmac_verify_rejects_wrong_secret() {
    let ts = now_rfc3339();
    let sig = RequestSignature::sign("secret-a", "GET", "/path", &ts, "");
    assert!(!RequestSignature::verify(
        "secret-b",
        "GET",
        "/path",
        &ts,
        "",
        &sig
    ));
}

#[test]
fn hmac_verify_rejects_tampered_path() {
    let ts = now_rfc3339();
    let sig = RequestSignature::sign("key", "GET", "/v1/events", &ts, "");
    assert!(!RequestSignature::verify(
        "key",
        "GET",
        "/v1/admin/secrets",
        &ts,
        "",
        &sig
    ));
}

#[test]
fn hmac_verify_rejects_tampered_body() {
    let ts = now_rfc3339();
    let sig = RequestSignature::sign("key", "POST", "/", &ts, r#"{"ok":true}"#);
    assert!(!RequestSignature::verify(
        "key",
        "POST",
        "/",
        &ts,
        r#"{"ok":false}"#,
        &sig
    ));
}

#[test]
fn hmac_verify_rejects_tampered_method() {
    let ts = now_rfc3339();
    let sig = RequestSignature::sign("key", "GET", "/path", &ts, "");
    assert!(!RequestSignature::verify(
        "key",
        "POST",
        "/path",
        &ts,
        "",
        &sig
    ));
}

#[test]
fn hmac_verify_rejects_all_zeros_signature() {
    let ts = now_rfc3339();
    let zeroes = "0".repeat(64);
    assert!(!RequestSignature::verify(
        "key",
        "GET",
        "/path",
        &ts,
        "",
        &zeroes
    ));
}

#[test]
fn hmac_verify_rejects_all_f_signature() {
    let ts = now_rfc3339();
    let ones = "f".repeat(64);
    assert!(!RequestSignature::verify(
        "key",
        "GET",
        "/path",
        &ts,
        "",
        &ones
    ));
}

// ===========================================================================
// Timestamp Freshness (tested through verify() since is_timestamp_fresh is private)
// ===========================================================================

#[test]
fn timestamp_10_minutes_old_fails_verify() {
    // RequestSignature::verify() internally checks timestamp freshness.
    // A signature with a 10-minute-old timestamp must fail even if the HMAC is valid.
    let old_ts = old_rfc3339(600);
    let sig = RequestSignature::sign("key", "GET", "/path", &old_ts, "");
    assert!(
        !RequestSignature::verify("key", "GET", "/path", &old_ts, "", &sig),
        "Stale timestamp must fail verification"
    );
}

#[test]
fn timestamp_just_within_window_passes_verify() {
    // 4 minutes old, within the 5-minute window
    let recent_ts = old_rfc3339(240);
    let sig = RequestSignature::sign("key", "GET", "/path", &recent_ts, "");
    assert!(
        RequestSignature::verify("key", "GET", "/path", &recent_ts, "", &sig),
        "Recent timestamp should pass"
    );
}

#[test]
fn unparseable_timestamp_fails_verify() {
    let sig = RequestSignature::sign("key", "GET", "/path", "not-a-date", "");
    assert!(
        !RequestSignature::verify("key", "GET", "/path", "not-a-date", "", &sig),
        "Invalid timestamp format must fail"
    );
}

#[test]
fn far_future_timestamp_fails_verify() {
    // 1 hour in the future — should be outside the freshness window
    let future_ts = future_rfc3339(3600);
    let sig = RequestSignature::sign("key", "GET", "/path", &future_ts, "");
    assert!(
        !RequestSignature::verify("key", "GET", "/path", &future_ts, "", &sig),
        "Far-future timestamp must fail"
    );
}

// ===========================================================================
// Webhook HMAC-SHA256 Signature
// ===========================================================================

#[test]
fn webhook_signature_roundtrip() {
    let body = b"event data payload";
    let header = sign_webhook("webhook-secret", body);
    assert!(verify_webhook_hmac(&header, "webhook-secret", body));
}

#[test]
fn webhook_signature_header_has_sha256_prefix() {
    let header = sign_webhook("secret", b"payload");
    assert!(
        header.starts_with("sha256="),
        "Webhook signature must start with 'sha256='"
    );
}

#[test]
fn webhook_signature_hex_part_is_64_chars() {
    let header = sign_webhook("secret", b"payload");
    let hex_part = header.strip_prefix("sha256=").unwrap();
    assert_eq!(hex_part.len(), 64);
    assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn webhook_wrong_secret_fails_verification() {
    let body = b"payload";
    let header = sign_webhook("correct-secret", body);
    assert!(!verify_webhook_hmac(&header, "wrong-secret", body));
}

#[test]
fn webhook_tampered_body_fails_verification() {
    let body = b"original body";
    let header = sign_webhook("secret", body);
    assert!(!verify_webhook_hmac(&header, "secret", b"tampered body"));
}

#[test]
fn webhook_missing_prefix_fails_verification() {
    let body = b"body";
    let mut mac = HmacSha256::new_from_slice(b"secret").unwrap();
    mac.update(body);
    let raw_hex = hex::encode(mac.finalize().into_bytes());
    assert!(!verify_webhook_hmac(&raw_hex, "secret", body));
}

#[test]
fn webhook_wrong_algorithm_prefix_fails_verification() {
    assert!(!verify_webhook_hmac("md5=abcdef1234", "secret", b"body"));
    assert!(!verify_webhook_hmac("sha1=abcdef1234", "secret", b"body"));
    assert!(!verify_webhook_hmac("sha512=abcdef", "secret", b"body"));
}

#[test]
fn webhook_empty_signature_fails_verification() {
    assert!(!verify_webhook_hmac("", "secret", b"body"));
}

#[test]
fn webhook_short_secret_handled_without_panic() {
    let header = sign_webhook("s", b"body");
    assert!(verify_webhook_hmac(&header, "s", b"body"));
}

#[test]
fn webhook_empty_body_has_valid_signature() {
    let header = sign_webhook("secret", b"");
    assert!(verify_webhook_hmac(&header, "secret", b""));
}

#[test]
fn webhook_large_body_handled_without_panic() {
    let large_body = vec![0xABu8; 100_000];
    let header = sign_webhook("secret", &large_body);
    assert!(verify_webhook_hmac(&header, "secret", &large_body));
}

// ===========================================================================
// API Key Hashing (SHA-256)
// ===========================================================================

#[test]
fn api_key_hash_is_deterministic() {
    assert_eq!(hash_api_key("test-key"), hash_api_key("test-key"));
}

#[test]
fn api_key_hash_is_64_hex_chars() {
    let hash = hash_api_key("test-key");
    assert_eq!(hash.len(), 64);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn api_key_hash_different_keys_produce_different_hashes() {
    assert_ne!(hash_api_key("key-a"), hash_api_key("key-b"));
}

#[test]
fn api_key_hash_of_empty_string_is_valid() {
    // SHA-256("") = e3b0c44298fc1c149afbf4c8996fb924...
    let hash = hash_api_key("");
    assert_eq!(hash.len(), 64);
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn api_key_hash_handles_unicode_input() {
    let hash = hash_api_key("caf\u{00e9}"); // "café"
    assert_eq!(hash.len(), 64);
    assert_ne!(hash, hash_api_key("cafe")); // different from ASCII "cafe"
}

#[test]
fn api_key_hash_handles_very_long_key() {
    let long = "a".repeat(100_000);
    let hash = hash_api_key(&long);
    assert_eq!(hash.len(), 64);
}

#[test]
fn api_key_hash_does_not_contain_plaintext_key() {
    let key = "my-super-secret-api-key";
    let hash = hash_api_key(key);
    assert!(!hash.contains(key));
    assert!(!hash.contains("secret"));
}

// ===========================================================================
// API Key Rotation (ApiKeySet)
// ===========================================================================

#[test]
fn key_set_new_key_is_valid() {
    let keys = ApiKeySet::new("initial");
    assert!(keys.is_valid("initial"));
}

#[test]
fn key_set_wrong_key_is_invalid() {
    let keys = ApiKeySet::new("correct");
    assert!(!keys.is_valid("wrong"));
}

#[test]
fn key_set_rotation_makes_new_key_valid() {
    let mut keys = ApiKeySet::new("original");
    keys.rotate("new-key");
    assert!(keys.is_valid("new-key"));
}

#[test]
fn key_set_rotation_keeps_old_key_valid_during_grace() {
    let mut keys = ApiKeySet::new("original");
    keys.rotate("new-key");
    assert!(
        keys.is_valid("original"),
        "Old key must remain valid during grace period"
    );
}

#[test]
fn key_set_double_rotation_evicts_oldest() {
    let mut keys = ApiKeySet::new("key-1");
    keys.rotate("key-2");
    keys.rotate("key-3");
    assert!(!keys.is_valid("key-1"), "key-1 must be evicted");
    assert!(keys.is_valid("key-2"), "key-2 still valid");
    assert!(keys.is_valid("key-3"), "key-3 is current");
}

#[test]
fn key_set_grace_period_active_right_after_rotation() {
    let mut keys = ApiKeySet::new("old");
    keys.rotate("new");
    assert!(
        keys.is_in_grace_period(3600),
        "Grace period must be active immediately after rotation"
    );
}

#[test]
fn key_set_no_grace_period_without_rotation() {
    let keys = ApiKeySet::new("only");
    assert!(
        !keys.is_in_grace_period(3600),
        "No grace period without rotation"
    );
}

// ===========================================================================
// Constant-Time Comparison
// ===========================================================================

#[test]
fn key_matches_any_exact_match_returns_true() {
    let candidates = vec!["secret-key".to_string()];
    assert!(key_matches_any("secret-key", &candidates));
}

#[test]
fn key_matches_any_no_match_returns_false() {
    let candidates = vec!["correct".to_string()];
    assert!(!key_matches_any("wrong", &candidates));
}

#[test]
fn key_matches_any_empty_list_returns_false() {
    assert!(!key_matches_any("anything", &[]));
}

#[test]
fn key_matches_any_partial_match_returns_false() {
    let candidates = vec!["secret".to_string()];
    assert!(!key_matches_any("sec", &candidates));
}

#[test]
fn key_matches_any_superset_does_not_match() {
    let candidates = vec!["secret".to_string()];
    assert!(!key_matches_any("secret-extra", &candidates));
}

#[test]
fn key_matches_any_different_length_does_not_panic() {
    let candidates = vec!["short".to_string()];
    let result = key_matches_any("this-is-a-very-long-key", &candidates);
    assert!(!result);
}

#[test]
fn key_matches_any_unicode_not_confused_with_ascii() {
    let candidates = vec!["caf\u{00e9}".to_string()]; // café (precomposed)
    assert!(!key_matches_any("cafe\u{0301}", &candidates)); // café (decomposed)
}
