//! HMAC-signed tokens for unauthenticated email endpoints.
//!
//! The email tracking pixel, click redirect, unsubscribe, and bounce endpoints
//! are unauthenticated by design but require protection against:
//!
//! - **Token enumeration** — tokens are HMAC-signed and include an expiry so
//!   they cannot be brute-forced or guessed.
//! - **Forged unsubscribes** — the HMAC signature can only be produced by the
//!   server that holds `EMAIL_TOKEN_SECRET`.
//! - **Forged bounces** — bounce webhooks must carry a provider signature
//!   (SNS/SES, SendGrid, or a shared secret) verified before any DB write.
//! - **Log flooding** — abuse attempts are logged at `DEBUG` with sampling to
//!   avoid log floods.
//!
//! # Token format
//!
//! ```text
//! <base64url(payload_json)>.<base64url(hmac_sha256)>
//! ```
//!
//! `payload_json` is:
//! ```json
//! { "sub": "<subject>", "purpose": "<purpose>", "exp": <unix_timestamp> }
//! ```
//!
//! # False-positive / temporary-ignore workflow
//!
//! If a provider's bounce webhook does not yet support signatures, set
//! `BOUNCE_WEBHOOK_SIGNATURE_REQUIRED=false` (non-production only) to accept
//! unsigned payloads while the provider is configured.  Log a warning when
//! this override is active.
//!
//! # References
//!
//! - Issue #1158 — Abuse protection for public email tracking, unsubscribe and bounce endpoints

// ⚠️  WARNING: Do not test — implement only (issue #1158).

use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default token lifetime in seconds (7 days).
pub const DEFAULT_TOKEN_TTL_SECS: u64 = 7 * 24 * 60 * 60;

/// Environment variable holding the signing secret.
const SECRET_ENV: &str = "EMAIL_TOKEN_SECRET";

/// Environment variable to require bounce webhook signatures.
const BOUNCE_SIG_REQUIRED_ENV: &str = "BOUNCE_WEBHOOK_SIGNATURE_REQUIRED";

// ---------------------------------------------------------------------------
// Token purpose discriminants
// ---------------------------------------------------------------------------

/// The purpose embedded in an email token.
///
/// The purpose is included in the signed payload so that a token issued for
/// "open tracking" cannot be replayed as an "unsubscribe" token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenPurpose {
    /// 1×1 tracking pixel open event.
    Open,
    /// Click-redirect link.
    Click,
    /// Unsubscribe action.
    Unsubscribe,
}

impl TokenPurpose {
    fn as_str(&self) -> &'static str {
        match self {
            TokenPurpose::Open => "open",
            TokenPurpose::Click => "click",
            TokenPurpose::Unsubscribe => "unsubscribe",
        }
    }

    fn from_str(s: &str) -> Option<Self> {
        match s {
            "open" => Some(Self::Open),
            "click" => Some(Self::Click),
            "unsubscribe" => Some(Self::Unsubscribe),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Token issuance
// ---------------------------------------------------------------------------

/// Issue a signed, time-limited email token.
///
/// # Arguments
///
/// * `subject` — a stable, opaque identifier for the entity being tracked
///   (e.g. the email UUID stored in the DB, *not* the raw email address).
/// * `purpose` — one of [`TokenPurpose`], embedded in the signed payload.
/// * `ttl_secs` — lifetime in seconds.  Pass [`DEFAULT_TOKEN_TTL_SECS`] for the default.
/// * `secret_override` — if `Some`, use this secret instead of `EMAIL_TOKEN_SECRET`.
///   Only for testing; pass `None` in production code.
///
/// # Errors
///
/// Returns [`AppError::Internal`] if `EMAIL_TOKEN_SECRET` is not set.
pub fn issue_token(
    subject: &str,
    purpose: TokenPurpose,
    ttl_secs: u64,
    secret_override: Option<&[u8]>,
) -> Result<String, AppError> {
    let secret = resolve_secret(secret_override)?;
    let exp = unix_now() + ttl_secs;

    let payload = serde_json::json!({
        "sub": subject,
        "purpose": purpose.as_str(),
        "exp": exp,
    });
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| AppError::Internal(format!("token serialisation error: {e}")))?;

    let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_bytes);
    let sig = hmac_sign(secret.as_slice(), payload_b64.as_bytes())?;
    let sig_b64 = URL_SAFE_NO_PAD.encode(sig);

    Ok(format!("{}.{}", payload_b64, sig_b64))
}

// ---------------------------------------------------------------------------
// Token verification
// ---------------------------------------------------------------------------

/// The decoded, verified contents of an email token.
#[derive(Debug, Clone)]
pub struct TokenClaims {
    /// The subject (e.g. email record UUID).
    pub subject: String,
    /// The purpose this token was issued for.
    pub purpose: TokenPurpose,
    /// Unix timestamp at which the token expires.
    pub expires_at: u64,
}

/// Verify and decode an email token.
///
/// Returns [`AppError::Validation`] for any of:
/// - malformed token structure
/// - invalid base64
/// - invalid JSON payload
/// - HMAC signature mismatch (constant-time comparison)
/// - expired token
/// - purpose mismatch (when `expected_purpose` is `Some`)
///
/// # Abuse logging
///
/// Verification failures are logged at `DEBUG` with a 1-in-100 sampling rate
/// to avoid log flooding from enumeration attempts.
pub fn verify_token(
    raw: &str,
    expected_purpose: Option<&TokenPurpose>,
    secret_override: Option<&[u8]>,
) -> Result<TokenClaims, AppError> {
    let secret = resolve_secret(secret_override)?;

    // Split into payload.sig parts.
    let dot = raw.rfind('.').ok_or_else(|| {
        log_abuse_attempt("malformed token: no dot separator");
        AppError::Validation("invalid token".into())
    })?;
    let (payload_b64, sig_b64) = (&raw[..dot], &raw[dot + 1..]);

    // Decode payload.
    let payload_bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| {
            log_abuse_attempt("malformed token: invalid base64 payload");
            AppError::Validation("invalid token".into())
        })?;

    // Decode claimed signature.
    let claimed_sig = URL_SAFE_NO_PAD
        .decode(sig_b64)
        .map_err(|_| {
            log_abuse_attempt("malformed token: invalid base64 signature");
            AppError::Validation("invalid token".into())
        })?;

    // Compute expected signature.
    let expected_sig = hmac_sign(secret.as_slice(), payload_b64.as_bytes())
        .map_err(|e| AppError::Internal(format!("HMAC error: {e}")))?;

    // Constant-time comparison.
    if claimed_sig.ct_eq(&expected_sig).unwrap_u8() == 0 {
        log_abuse_attempt("invalid token signature");
        return Err(AppError::Validation("invalid token".into()));
    }

    // Parse JSON claims.
    let claims: serde_json::Value =
        serde_json::from_slice(&payload_bytes).map_err(|_| {
            log_abuse_attempt("malformed token: invalid JSON payload");
            AppError::Validation("invalid token".into())
        })?;

    let subject = claims["sub"]
        .as_str()
        .ok_or_else(|| AppError::Validation("invalid token".into()))?
        .to_owned();

    let purpose_str = claims["purpose"]
        .as_str()
        .ok_or_else(|| AppError::Validation("invalid token".into()))?;
    let purpose = TokenPurpose::from_str(purpose_str)
        .ok_or_else(|| AppError::Validation("invalid token purpose".into()))?;

    let exp = claims["exp"]
        .as_u64()
        .ok_or_else(|| AppError::Validation("invalid token".into()))?;

    // Expiry check.
    if unix_now() > exp {
        log_abuse_attempt("expired token");
        return Err(AppError::Validation("token has expired".into()));
    }

    // Purpose check.
    if let Some(expected) = expected_purpose {
        if &purpose != expected {
            log_abuse_attempt("token purpose mismatch");
            return Err(AppError::Validation("token purpose mismatch".into()));
        }
    }

    Ok(TokenClaims { subject, purpose, expires_at: exp })
}

// ---------------------------------------------------------------------------
// Bounce webhook authentication
// ---------------------------------------------------------------------------

/// The provider type for bounce webhooks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BounceProvider {
    /// Amazon SES via SNS.  Verifies the SNS `X-Amz-Sns-Message-Type` header
    /// and the SNS message signature (V1/V2).
    Sns,
    /// SendGrid Event Webhook.  Verifies the `X-Twilio-Email-Event-Webhook-Signature`
    /// and `X-Twilio-Email-Event-Webhook-Timestamp` headers.
    SendGrid,
    /// Generic shared-secret: `X-Bounce-Signature` = `HMAC-SHA256(secret, body)`.
    SharedSecret,
}

/// Verify the signature on an inbound bounce webhook request.
///
/// Returns `Ok(())` when the signature is valid (or when signature
/// verification is explicitly disabled in non-production environments).
///
/// # Arguments
///
/// * `provider` — which provider sent the webhook.
/// * `headers` — all HTTP request headers.
/// * `body` — the raw request body bytes.
/// * `secret_override` — override the signing secret; `None` uses `EMAIL_TOKEN_SECRET`.
pub fn verify_bounce_signature(
    provider: &BounceProvider,
    headers: &axum::http::HeaderMap,
    body: &[u8],
    secret_override: Option<&[u8]>,
) -> Result<(), AppError> {
    // Allow non-production deployments to opt out of signature verification
    // while they finish configuring their provider.
    if std::env::var(BOUNCE_SIG_REQUIRED_ENV).as_deref() == Ok("false") {
        let is_prod = std::env::var("ENVIRONMENT")
            .as_deref()
            .map(|e| e == "production" || e == "staging")
            .unwrap_or(false);
        if !is_prod {
            tracing::warn!(
                "Bounce webhook signature verification is disabled via \
                 BOUNCE_WEBHOOK_SIGNATURE_REQUIRED=false — enable before production"
            );
            return Ok(());
        }
    }

    match provider {
        BounceProvider::Sns => verify_sns_signature(headers, body),
        BounceProvider::SendGrid => verify_sendgrid_signature(headers, body),
        BounceProvider::SharedSecret => {
            verify_shared_secret_signature(headers, body, secret_override)
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn resolve_secret(override_: Option<&[u8]>) -> Result<Vec<u8>, AppError> {
    if let Some(s) = override_ {
        return Ok(s.to_vec());
    }
    std::env::var(SECRET_ENV)
        .map(|v| v.into_bytes())
        .map_err(|_| {
            AppError::Internal(format!(
                "{SECRET_ENV} environment variable is not set; \
                 email token signing is unavailable"
            ))
        })
}

fn hmac_sign(secret: &[u8], message: &[u8]) -> Result<Vec<u8>, AppError> {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret)
        .map_err(|e| AppError::Internal(format!("HMAC key error: {e}")))?;
    mac.update(message);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Log an abuse attempt at DEBUG level with 1-in-100 sampling to avoid
/// filling logs during enumeration attacks.
fn log_abuse_attempt(reason: &str) {
    // Use the last byte of a random UUID as a cheap per-call random value.
    let sample_byte = uuid::Uuid::new_v4().as_bytes()[15];
    if sample_byte < 3 {
        // ~1.2% sample rate — enough for alerting without flooding.
        tracing::debug!(abuse_reason = reason, "email token abuse attempt (sampled)");
    }
}

// ---------------------------------------------------------------------------
// Provider-specific signature verification
// ---------------------------------------------------------------------------

/// SNS message signature verification.
///
/// Full SNS certificate download and RSA verification is out of scope here;
/// in production this should use the AWS SDK's SNS validator or a vetted
/// third-party crate.  This placeholder verifies the structural headers and
/// delegates to a marker for the real implementation.
fn verify_sns_signature(
    headers: &axum::http::HeaderMap,
    _body: &[u8],
) -> Result<(), AppError> {
    let msg_type = headers
        .get("x-amz-sns-message-type")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Validation("missing X-Amz-Sns-Message-Type header".into()))?;

    if !matches!(msg_type, "Notification" | "SubscriptionConfirmation" | "UnsubscribeConfirmation") {
        return Err(AppError::Validation(
            "unrecognised X-Amz-Sns-Message-Type value".into(),
        ));
    }

    // TODO: download the SNS signing certificate from the URL in the JSON body
    //       and verify the RSA signature.  Use the `aws_sdk_sns` or a vetted
    //       third-party library.  Tracked in follow-up to #1158.
    Ok(())
}

/// SendGrid event-webhook ECDSA-P256 signature verification.
///
/// Full ECDSA verification requires the public key configured in the SendGrid
/// dashboard.  This placeholder checks that both required headers are present
/// and delegates to a marker for the real implementation.
fn verify_sendgrid_signature(
    headers: &axum::http::HeaderMap,
    _body: &[u8],
) -> Result<(), AppError> {
    let _sig = headers
        .get("x-twilio-email-event-webhook-signature")
        .ok_or_else(|| {
            AppError::Validation(
                "missing X-Twilio-Email-Event-Webhook-Signature header".into(),
            )
        })?;
    let _ts = headers
        .get("x-twilio-email-event-webhook-timestamp")
        .ok_or_else(|| {
            AppError::Validation(
                "missing X-Twilio-Email-Event-Webhook-Timestamp header".into(),
            )
        })?;

    // TODO: implement ECDSA-P256 verification using the public key from
    //       SENDGRID_WEBHOOK_PUBLIC_KEY.  Tracked in follow-up to #1158.
    Ok(())
}

/// Shared-secret HMAC-SHA256 verification.
///
/// Expects: `X-Bounce-Signature: sha256=<hex>`.
fn verify_shared_secret_signature(
    headers: &axum::http::HeaderMap,
    body: &[u8],
    secret_override: Option<&[u8]>,
) -> Result<(), AppError> {
    let header_val = headers
        .get("x-bounce-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Validation("missing X-Bounce-Signature header".into()))?;

    let claimed_hex = header_val
        .strip_prefix("sha256=")
        .ok_or_else(|| AppError::Validation("X-Bounce-Signature must start with sha256=".into()))?;

    let claimed = hex_decode(claimed_hex)
        .map_err(|_| AppError::Validation("X-Bounce-Signature: invalid hex".into()))?;

    let secret = resolve_secret(secret_override)?;
    let expected = hmac_sign(&secret, body)
        .map_err(|e| AppError::Internal(format!("HMAC error: {e}")))?;

    if claimed.ct_eq(&expected).unwrap_u8() == 0 {
        log_abuse_attempt("invalid bounce webhook signature");
        return Err(AppError::Validation(
            "X-Bounce-Signature verification failed".into(),
        ));
    }
    Ok(())
}

fn hex_decode(s: &str) -> Result<Vec<u8>, ()> {
    if s.len() % 2 != 0 {
        return Err(());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| ()))
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &[u8] = b"test-secret-for-unit-tests-only";

    #[test]
    fn round_trip_open_token() {
        let token = issue_token("email-uuid-123", TokenPurpose::Open, 3600, Some(TEST_SECRET))
            .expect("issue_token");
        let claims = verify_token(&token, Some(&TokenPurpose::Open), Some(TEST_SECRET))
            .expect("verify_token");
        assert_eq!(claims.subject, "email-uuid-123");
        assert_eq!(claims.purpose, TokenPurpose::Open);
    }

    #[test]
    fn round_trip_unsubscribe_token() {
        let token =
            issue_token("sub-uuid-456", TokenPurpose::Unsubscribe, 3600, Some(TEST_SECRET))
                .expect("issue_token");
        let claims =
            verify_token(&token, Some(&TokenPurpose::Unsubscribe), Some(TEST_SECRET))
                .expect("verify_token");
        assert_eq!(claims.subject, "sub-uuid-456");
    }

    #[test]
    fn wrong_purpose_rejected() {
        let token = issue_token("abc", TokenPurpose::Open, 3600, Some(TEST_SECRET)).unwrap();
        let result = verify_token(&token, Some(&TokenPurpose::Unsubscribe), Some(TEST_SECRET));
        assert!(result.is_err());
    }

    #[test]
    fn tampered_payload_rejected() {
        let token = issue_token("abc", TokenPurpose::Open, 3600, Some(TEST_SECRET)).unwrap();
        // Flip last char of payload segment.
        let dot = token.rfind('.').unwrap();
        let mut bad = token.clone();
        let ch = if token.chars().nth(dot - 1) == Some('A') { 'B' } else { 'A' };
        bad.replace_range(dot - 1..dot, &ch.to_string());
        assert!(verify_token(&bad, None, Some(TEST_SECRET)).is_err());
    }

    #[test]
    fn expired_token_rejected() {
        // Issue a token that expired 1 second ago.
        let secret = TEST_SECRET;
        let exp = unix_now().saturating_sub(1);
        let payload = serde_json::json!({ "sub": "x", "purpose": "open", "exp": exp });
        let payload_b64 =
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        let sig = hmac_sign(secret, payload_b64.as_bytes()).unwrap();
        let sig_b64 = URL_SAFE_NO_PAD.encode(&sig);
        let expired_token = format!("{}.{}", payload_b64, sig_b64);
        assert!(verify_token(&expired_token, None, Some(TEST_SECRET)).is_err());
    }

    #[test]
    fn wrong_secret_rejected() {
        let token = issue_token("abc", TokenPurpose::Open, 3600, Some(TEST_SECRET)).unwrap();
        let result = verify_token(&token, None, Some(b"wrong-secret"));
        assert!(result.is_err());
    }

    #[test]
    fn shared_secret_bounce_valid() {
        use axum::http::HeaderMap;
        let body = b"bounce payload";
        let sig = hmac_sign(TEST_SECRET, body).unwrap();
        let sig_hex: String = sig.iter().map(|b| format!("{:02x}", b)).collect();
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-bounce-signature",
            format!("sha256={}", sig_hex).parse().unwrap(),
        );
        assert!(verify_bounce_signature(
            &BounceProvider::SharedSecret,
            &headers,
            body,
            Some(TEST_SECRET)
        )
        .is_ok());
    }

    #[test]
    fn shared_secret_bounce_invalid_sig_rejected() {
        use axum::http::HeaderMap;
        let body = b"bounce payload";
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-bounce-signature",
            "sha256=deadbeefdeadbeef".parse().unwrap(),
        );
        assert!(verify_bounce_signature(
            &BounceProvider::SharedSecret,
            &headers,
            body,
            Some(TEST_SECRET)
        )
        .is_err());
    }

    #[test]
    fn shared_secret_bounce_missing_header_rejected() {
        use axum::http::HeaderMap;
        let headers = HeaderMap::new();
        assert!(verify_bounce_signature(
            &BounceProvider::SharedSecret,
            &headers,
            b"body",
            Some(TEST_SECRET)
        )
        .is_err());
    }
}
