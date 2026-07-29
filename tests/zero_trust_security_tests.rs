//! Integration tests for Zero-Trust Security Implementation (Issue #838).
//!
//! Tests cover:
//! - HMAC-SHA256 request signing and verification
//! - Timestamp freshness validation
//! - API key rotation with grace periods
//! - Access control policy evaluation
//! - Access logging and audit trail

use soroban_pulse::zero_trust::{
    AccessDecision, AccessLogger, ApiKeySet, PolicyEvaluator, RequestContext, RequestSignature,
};

// ---------------------------------------------------------------------------
// Request Signature
// ---------------------------------------------------------------------------

#[test]
fn signature_sign_and_verify_roundtrip() {
    let secret = "my-secret-key";
    let sig = RequestSignature::sign(secret, "POST", "/v1/events", 1_700_000_000, "{}");
    assert!(RequestSignature::verify(
        secret,
        "POST",
        "/v1/events",
        1_700_000_000,
        "{}",
        &sig
    ));
}

#[test]
fn signature_rejects_tampered_body() {
    let secret = "my-secret-key";
    let sig = RequestSignature::sign(secret, "POST", "/v1/events", 1_700_000_000, "{}");
    assert!(!RequestSignature::verify(
        secret,
        "POST",
        "/v1/events",
        1_700_000_000,
        "{\"tampered\": true}",
        &sig
    ));
}

#[test]
fn signature_rejects_tampered_path() {
    let secret = "my-secret-key";
    let sig = RequestSignature::sign(secret, "GET", "/v1/events", 1_700_000_000, "");
    assert!(!RequestSignature::verify(
        secret,
        "GET",
        "/v1/admin/secrets",
        1_700_000_000,
        "",
        &sig
    ));
}

#[test]
fn signature_rejects_wrong_secret() {
    let sig = RequestSignature::sign("secret-a", "GET", "/path", 1_700_000_000, "");
    assert!(!RequestSignature::verify(
        "secret-b",
        "GET",
        "/path",
        1_700_000_000,
        "",
        &sig
    ));
}

#[test]
fn signature_rejects_different_method() {
    let secret = "key";
    let sig = RequestSignature::sign(secret, "GET", "/path", 1_700_000_000, "");
    assert!(!RequestSignature::verify(
        secret,
        "POST",
        "/path",
        1_700_000_000,
        "",
        &sig
    ));
}

#[test]
fn signature_is_hex_string() {
    let sig = RequestSignature::sign("key", "GET", "/", 1_700_000_000, "");
    assert!(sig.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(sig.len(), 64); // SHA256 = 32 bytes = 64 hex chars
}

#[test]
fn signature_timestamp_freshness_accepts_recent() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(RequestSignature::is_timestamp_fresh(now, 300));
}

#[test]
fn signature_timestamp_freshness_rejects_old() {
    let old = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        - 600;
    assert!(!RequestSignature::is_timestamp_fresh(old, 300));
}

// ---------------------------------------------------------------------------
// API Key Rotation
// ---------------------------------------------------------------------------

#[test]
fn api_key_set_validates_primary() {
    let keys = ApiKeySet::new("primary-key");
    assert!(keys.is_valid("primary-key"));
    assert!(!keys.is_valid("wrong-key"));
}

#[test]
fn api_key_set_rotation_moves_primary_to_secondary() {
    let mut keys = ApiKeySet::new("original");
    keys.rotate("new-key");
    assert!(keys.is_valid("new-key"));
    assert!(keys.is_valid("original")); // still valid as secondary
    assert!(!keys.is_valid("random"));
}

#[test]
fn api_key_set_double_rotation_evicts_oldest() {
    let mut keys = ApiKeySet::new("key-1");
    keys.rotate("key-2");
    keys.rotate("key-3");
    assert!(keys.is_valid("key-3")); // current primary
    assert!(keys.is_valid("key-2")); // current secondary
    assert!(!keys.is_valid("key-1")); // evicted
}

#[test]
fn api_key_set_grace_period_after_rotation() {
    let mut keys = ApiKeySet::new("old");
    keys.rotate("new");
    // Just rotated, should be in grace period
    assert!(keys.is_in_grace_period(3600));
}

#[test]
fn api_key_set_no_grace_period_without_rotation() {
    let keys = ApiKeySet::new("only");
    assert!(!keys.is_in_grace_period(3600));
}

// ---------------------------------------------------------------------------
// Access Decisions & Policy
// ---------------------------------------------------------------------------

#[test]
fn policy_allows_health_endpoint() {
    let policy = PolicyEvaluator::default();
    let ctx = RequestContext {
        ip_address: "127.0.0.1".to_string(),
        api_key_hash: None,
        path: "/health".to_string(),
        method: "GET".to_string(),
        timestamp: current_timestamp(),
        user_agent: "test".to_string(),
    };
    assert!(matches!(policy.evaluate(&ctx), AccessDecision::Allow));
}

#[test]
fn policy_denies_admin_without_key() {
    let policy = PolicyEvaluator::default();
    let ctx = RequestContext {
        ip_address: "1.2.3.4".to_string(),
        api_key_hash: None,
        path: "/v1/admin/secrets".to_string(),
        method: "GET".to_string(),
        timestamp: current_timestamp(),
        user_agent: "test".to_string(),
    };
    assert!(matches!(policy.evaluate(&ctx), AccessDecision::Deny(_)));
}

// ---------------------------------------------------------------------------
// Access Logger
// ---------------------------------------------------------------------------

#[test]
fn access_logger_records_and_retrieves() {
    let logger = AccessLogger::new(100);
    let ctx = RequestContext {
        ip_address: "10.0.0.1".to_string(),
        api_key_hash: Some("abc123".to_string()),
        path: "/v1/events".to_string(),
        method: "GET".to_string(),
        timestamp: current_timestamp(),
        user_agent: "curl/7.0".to_string(),
    };
    logger.record(&ctx, &AccessDecision::Allow);
    logger.record(&ctx, &AccessDecision::Deny("test".to_string()));

    let recent = logger.recent_entries(10);
    assert_eq!(recent.len(), 2);
}

#[test]
fn access_logger_filters_by_key() {
    let logger = AccessLogger::new(100);
    let ctx1 = RequestContext {
        ip_address: "10.0.0.1".to_string(),
        api_key_hash: Some("key-a".to_string()),
        path: "/v1/events".to_string(),
        method: "GET".to_string(),
        timestamp: current_timestamp(),
        user_agent: "test".to_string(),
    };
    let ctx2 = RequestContext {
        ip_address: "10.0.0.2".to_string(),
        api_key_hash: Some("key-b".to_string()),
        path: "/v1/events".to_string(),
        method: "GET".to_string(),
        timestamp: current_timestamp(),
        user_agent: "test".to_string(),
    };
    logger.record(&ctx1, &AccessDecision::Allow);
    logger.record(&ctx2, &AccessDecision::Allow);
    logger.record(&ctx1, &AccessDecision::Allow);

    let key_a = logger.entries_for_key("key-a", 10);
    assert_eq!(key_a.len(), 2);

    let key_b = logger.entries_for_key("key-b", 10);
    assert_eq!(key_b.len(), 1);
}

#[test]
fn access_logger_respects_limit() {
    let logger = AccessLogger::new(100);
    for i in 0..20 {
        let ctx = RequestContext {
            ip_address: format!("10.0.0.{i}"),
            api_key_hash: None,
            path: "/v1/events".to_string(),
            method: "GET".to_string(),
            timestamp: current_timestamp(),
            user_agent: "test".to_string(),
        };
        logger.record(&ctx, &AccessDecision::Allow);
    }
    let recent = logger.recent_entries(5);
    assert_eq!(recent.len(), 5);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn current_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
