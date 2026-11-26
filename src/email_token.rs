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
//! - Issue #1421 — Real SNS/SES bounce webhook signature verification
//! - Issue #1422 — Real SendGrid event webhook ECDSA signature verification

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha1::Sha1;
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

/// Environment variable holding the SendGrid webhook public key (base64 DER).
const SENDGRID_PUBLIC_KEY_ENV: &str = "SENDGRID_WEBHOOK_PUBLIC_KEY";

/// Environment variable overriding the SNS timestamp tolerance window (seconds).
const SNS_TIMESTAMP_WINDOW_ENV: &str = "SNS_TIMESTAMP_WINDOW_SECS";

/// Environment variable overriding the SendGrid timestamp tolerance window (seconds).
const SENDGRID_TIMESTAMP_WINDOW_ENV: &str = "SENDGRID_TIMESTAMP_WINDOW_SECS";

/// Default tolerance window for SNS message timestamps (1 hour).
pub const DEFAULT_SNS_TIMESTAMP_WINDOW_SECS: u64 = 60 * 60;

/// Default tolerance window for SendGrid event timestamps (5 minutes).
pub const DEFAULT_SENDGRID_TIMESTAMP_WINDOW_SECS: u64 = 5 * 60;

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

/// Validate the webhook configuration at startup.
///
/// Returns an error when a provider is enabled but the material required to
/// verify its signatures is missing.  Callers should refuse to start (or
/// disable the affected endpoint) when this returns `Err`.
///
/// # Arguments
///
/// * `sns_enabled` — whether the SNS/SES bounce endpoint is enabled.
/// * `sendgrid_enabled` — whether the SendGrid event endpoint is enabled.
pub fn validate_webhook_config(
    sns_enabled: bool,
    sendgrid_enabled: bool,
) -> Result<(), AppError> {
    if sendgrid_enabled && sendgrid_public_key().is_none() {
        let msg = format!(
            "SendGrid event webhook is enabled but {SENDGRID_PUBLIC_KEY_ENV} is not set; \
             refusing to accept unsigned events"
        );
        tracing::error!("{msg}");
        return Err(AppError::Internal(msg));
    }
    if sns_enabled {
        // SNS verification material (the signing certificate) is fetched per
        // message; the only startup requirement is that the outbound client is
        // available.  Nothing to check here beyond logging.
        tracing::debug!("SNS bounce webhook signature verification enabled");
    }
    Ok(())
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
/// Implements the AWS SNS message-authentication scheme (issue #1421):
///
/// 1. Parse the SNS JSON envelope and validate `SigningCertURL` — it must be
///    HTTPS, its host must match `^sns\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$`,
///    and its path must end in `.pem`.
/// 2. Fetch the signing certificate through the SSRF-safe outbound client and
///    cache it by URL.
/// 3. Rebuild the canonical string-to-sign (which differs for `Notification`
///    vs `SubscriptionConfirmation`/`UnsubscribeConfirmation`) and verify the
///    RSA PKCS#1 v1.5 signature for `SignatureVersion` 1 (SHA1) and 2 (SHA256).
/// 4. Reject messages whose `Timestamp` is outside the configured window.
///
/// Any failure returns [`AppError::Validation`], which the HTTP layer maps to
/// `401 Unauthorized` before any suppression state is touched.
fn verify_sns_signature(
    headers: &axum::http::HeaderMap,
    body: &[u8],
) -> Result<(), AppError> {
    let msg_type = headers
        .get("x-amz-sns-message-type")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Validation("missing X-Amz-Sns-Message-Type header".into()))?;

    if !matches!(
        msg_type,
        "Notification" | "SubscriptionConfirmation" | "UnsubscribeConfirmation"
    ) {
        return Err(AppError::Validation(
            "unrecognised X-Amz-Sns-Message-Type value".into(),
        ));
    }

    let envelope: SnsEnvelope = serde_json::from_slice(body)
        .map_err(|_| AppError::Validation("SNS body is not a valid JSON envelope".into()))?;

    // The header and the body must agree on the message type.
    if envelope.message_type() != msg_type {
        return Err(AppError::Validation(
            "X-Amz-Sns-Message-Type header does not match body Type".into(),
        ));
    }

    // 1. Validate the certificate URL before any network access.
    validate_sns_cert_url(&envelope.signing_cert_url)?;

    // 4. Replay window.
    check_sns_timestamp(&envelope.timestamp)?;

    // 3. Rebuild the canonical string-to-sign.
    let string_to_sign = envelope.string_to_sign()?;

    // 2. Fetch (or reuse cached) certificate, then verify the RSA signature.
    let cert_pem = sns_cert_cache()
        .get_or_fetch(&envelope.signing_cert_url)
        .map_err(|e| {
            log_abuse_attempt("SNS signing certificate unavailable");
            e
        })?;

    verify_sns_rsa_signature(
        &cert_pem,
        &string_to_sign,
        &envelope.signature_version,
        &envelope.signature,
    )
    .map_err(|e| {
        log_abuse_attempt("invalid SNS message signature");
        e
    })
}

// ---------------------------------------------------------------------------
// SNS envelope parsing & canonicalisation
// ---------------------------------------------------------------------------

/// The subset of the SNS JSON envelope needed for signature verification.
#[derive(Debug, Clone, serde::Deserialize)]
struct SnsEnvelope {
    #[serde(rename = "Type")]
    type_: String,
    #[serde(rename = "MessageId")]
    message_id: String,
    #[serde(rename = "TopicArn")]
    topic_arn: String,
    #[serde(rename = "Timestamp")]
    timestamp: String,
    #[serde(rename = "SignatureVersion")]
    signature_version: String,
    #[serde(rename = "Signature")]
    signature: String,
    #[serde(rename = "SigningCertURL")]
    signing_cert_url: String,
    /// Present on `Notification` messages.
    #[serde(default)]
    subject: Option<String>,
    /// Present on `Notification` messages.
    #[serde(default)]
    message: Option<String>,
    /// Present on `SubscriptionConfirmation` / `UnsubscribeConfirmation`.
    #[serde(default)]
    token: Option<String>,
    /// Present on `SubscriptionConfirmation` / `UnsubscribeConfirmation`.
    #[serde(default)]
    subscribe_url: Option<String>,
}

impl SnsEnvelope {
    fn message_type(&self) -> &str {
        &self.type_
    }

    /// Build the canonical string-to-sign exactly as AWS documents it.
    ///
    /// The field order and the set of fields differ between message types:
    /// `Notification` includes `Subject`/`Message`, while the confirmation
    /// types include `Token`/`SubscribeURL` instead.
    fn string_to_sign(&self) -> Result<String, AppError> {
        let mut s = String::new();
        let mut push = |key: &str, value: &str| {
            s.push_str(key);
            s.push('\n');
            s.push_str(value);
            s.push('\n');
        };

        push("Message", self.message.as_deref().unwrap_or(""));
        push("MessageId", &self.message_id);

        match self.type_.as_str() {
            "Notification" => {
                if let Some(subject) = &self.subject {
                    push("Subject", subject);
                }
            }
            "SubscriptionConfirmation" | "UnsubscribeConfirmation" => {
                push("SubscribeURL", self.subscribe_url.as_deref().unwrap_or(""));
                push("Timestamp", &self.timestamp);
                push("Token", self.token.as_deref().unwrap_or(""));
                // `Timestamp` already emitted above for confirmation types.
                push("TopicArn", &self.topic_arn);
                push("Type", &self.type_);
                return Ok(s);
            }
            other => {
                return Err(AppError::Validation(format!(
                    "unsupported SNS message type '{other}'"
                )));
            }
        }

        push("Timestamp", &self.timestamp);
        push("TopicArn", &self.topic_arn);
        push("Type", &self.type_);

        Ok(s)
    }
}

/// Validate the `SigningCertURL` against the AWS SNS host allowlist.
///
/// Rejects anything that is not HTTPS, whose host does not match
/// `^sns\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$`, or whose path does not end in
/// `.pem`.  This is the primary defence against an attacker pointing the
/// verifier at an arbitrary (or internal) URL.
fn validate_sns_cert_url(raw: &str) -> Result<(), AppError> {
    let url = url::Url::parse(raw)
        .map_err(|_| AppError::Validation("SNS SigningCertURL is not a valid URL".into()))?;

    if url.scheme() != "https" {
        return Err(AppError::Validation(
            "SNS SigningCertURL must use HTTPS".into(),
        ));
    }

    let host = url.host_str().unwrap_or("");
    if !is_sns_cert_host(host) {
        return Err(AppError::Validation(format!(
            "SNS SigningCertURL host '{host}' is not an AWS SNS host"
        )));
    }

    if !url.path().ends_with(".pem") {
        return Err(AppError::Validation(
            "SNS SigningCertURL path must end in .pem".into(),
        ));
    }

    Ok(())
}

/// True if `host` matches `^sns\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$`.
fn is_sns_cert_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let rest = match host.strip_prefix("sns.") {
        Some(r) => r,
        None => return false,
    };
    let rest = match rest
        .strip_suffix(".amazonaws.com.cn")
        .or_else(|| rest.strip_suffix(".amazonaws.com"))
    {
        Some(r) => r,
        None => return false,
    };
    // The region label must be non-empty and contain only [a-z0-9-].
    !rest.is_empty()
        && rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Reject messages whose `Timestamp` is older (or further in the future) than
/// the configured replay window.
fn check_sns_timestamp(timestamp: &str) -> Result<(), AppError> {
    let ts = chrono::DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| AppError::Validation("SNS Timestamp is not valid RFC 3339".into()))?;
    let ts = ts.with_timezone(&chrono::Utc);

    let window = sns_timestamp_window();
    let now = chrono::Utc::now();
    let age = now.signed_duration_since(ts);

    if age > window {
        return Err(AppError::Validation(
            "SNS message Timestamp is outside the accepted replay window".into(),
        ));
    }
    // Tolerate a small amount of clock skew into the future.
    if age < -window {
        return Err(AppError::Validation(
            "SNS message Timestamp is too far in the future".into(),
        ));
    }
    Ok(())
}

/// The accepted replay window, configurable via `SNS_TIMESTAMP_WINDOW_SECS`
/// (default: 1 hour).
fn sns_timestamp_window() -> chrono::Duration {
    let secs = std::env::var("SNS_TIMESTAMP_WINDOW_SECS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(3600);
    chrono::Duration::seconds(secs)
}

// ---------------------------------------------------------------------------
// RSA signature verification
// ---------------------------------------------------------------------------

/// Verify an SNS RSA PKCS#1 v1.5 signature over `string_to_sign`.
///
/// Supports `SignatureVersion` 1 (SHA1) and 2 (SHA256).
fn verify_sns_rsa_signature(
    cert_pem: &str,
    string_to_sign: &str,
    signature_version: &str,
    signature_b64: &str,
) -> Result<(), AppError> {
    use base64::engine::general_purpose::STANDARD;
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::pkcs8::DecodePublicKey;
    use rsa::signature::Verifier;
    use rsa::RsaPublicKey;

    let der = pem_to_der(cert_pem)
        .ok_or_else(|| AppError::Validation("SNS signing certificate is not valid PEM".into()))?;

    let public_key = RsaPublicKey::from_public_key_der(&der)
        .map_err(|_| AppError::Validation("SNS signing certificate has no RSA public key".into()))?;

    let sig_bytes = STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| AppError::Validation("SNS Signature is not valid base64".into()))?;
    let signature = Signature::try_from(sig_bytes.as_slice())
        .map_err(|_| AppError::Validation("SNS Signature has an invalid length".into()))?;

    let message = string_to_sign.as_bytes();

    let verified = match signature_version {
        "1" => {
            let key = VerifyingKey::<sha1::Sha1>::new(public_key);
            key.verify(message, &signature).is_ok()
        }
        "2" => {
            let key = VerifyingKey::<sha2::Sha256>::new(public_key);
            key.verify(message, &signature).is_ok()
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported SNS SignatureVersion '{other}'"
            )));
        }
    };

    if !verified {
        return Err(AppError::Validation("SNS message signature verification failed".into()));
    }
    Ok(())
}

/// Extract the DER bytes from a PEM document (first `CERTIFICATE` block).
fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    use base64::engine::general_purpose::STANDARD;
    let begin = "-----BEGIN CERTIFICATE-----";
    let end = "-----END CERTIFICATE-----";
    let start = pem.find(begin)? + begin.len();
    let stop = pem[start..].find(end)? + start;
    let b64: String = pem[start..stop]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    STANDARD.decode(b64).ok()
}

// ---------------------------------------------------------------------------
// Signing-certificate cache
// ---------------------------------------------------------------------------

/// Process-wide cache of SNS signing certificates, keyed by URL.
fn sns_cert_cache() -> &'static SnsCertCache {
    static CACHE: std::sync::OnceLock<SnsCertCache> = std::sync::OnceLock::new();
    CACHE.get_or_init(SnsCertCache::new)
}

/// A small TTL cache of fetched SNS signing certificates.
struct SnsCertCache {
    entries: dashmap::DashMap<String, (String, std::time::Instant)>,
}

impl SnsCertCache {
    fn new() -> Self {
        Self { entries: dashmap::DashMap::new() }
    }

    /// Return the cached certificate for `url`, fetching it if absent or stale.
    fn get_or_fetch(&self, url: &str) -> Result<String, AppError> {
        const TTL: std::time::Duration = std::time::Duration::from_secs(6 * 60 * 60);

        if let Some(entry) = self.entries.get(url) {
            if entry.1.elapsed() < TTL {
                return Ok(entry.0.clone());
            }
        }

        let pem = fetch_sns_cert(url)?;
        self.entries
            .insert(url.to_string(), (pem.clone(), std::time::Instant::now()));
        Ok(pem)
    }
}

/// Fetch an SNS signing certificate through the SSRF-safe outbound client.
///
/// The URL has already been validated by [`validate_sns_cert_url`]; the
/// outbound client additionally disables redirects and enforces a timeout.
fn fetch_sns_cert(url: &str) -> Result<String, AppError> {
    let client = crate::net::outbound::build_outbound_client(&["amazonaws.com"])
        .map_err(|e| AppError::Internal(format!("failed to build outbound client: {e}")))?;

    let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
        AppError::Internal("SNS certificate fetch requires a Tokio runtime".into())
    })?;

    let url_owned = url.to_string();
    let pem = runtime.block_on(async move {
        let resp = client.get(&url_owned).send().await.map_err(|e| {
            AppError::Validation(format!("failed to fetch SNS signing certificate: {e}"))
        })?;
        if !resp.status().is_success() {
            return Err(AppError::Validation(format!(
                "SNS signing certificate fetch returned HTTP {}",
                resp.status()
            )));
        }
        resp.text().await.map_err(|e| {
            AppError::Validation(format!("failed to read SNS signing certificate: {e}"))
        })
    })?;

    Ok(pem)
}

/// SendGrid event-webhook ECDSA-P256 signature verification.
///
/// Verifies `ECDSA-P256-SHA256(timestamp || raw_body)` using the public key
/// from `SENDGRID_WEBHOOK_PUBLIC_KEY` (base64 DER).  The signature header is
/// base64 DER.  A timestamp tolerance window prevents replay.
fn verify_sendgrid_signature(
    headers: &axum::http::HeaderMap,
    body: &[u8],
) -> Result<(), AppError> {
    let sig_b64 = headers
        .get("x-twilio-email-event-webhook-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            AppError::Validation(
                "missing X-Twilio-Email-Event-Webhook-Signature header".into(),
            )
        })?;
    let ts_str = headers
        .get("x-twilio-email-event-webhook-timestamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            AppError::Validation(
                "missing X-Twilio-Email-Event-Webhook-Timestamp header".into(),
            )
        })?;

    // Fail closed when no public key is configured.
    let public_key_der = sendgrid_public_key().ok_or_else(|| {
        tracing::error!(
            "{SENDGRID_PUBLIC_KEY_ENV} is not set; rejecting SendGrid event webhook"
        );
        AppError::Internal("SendGrid webhook public key is not configured".into())
    })?;

    // Replay protection.
    let ts_secs: u64 = ts_str.parse().map_err(|_| {
        AppError::Validation("invalid SendGrid webhook timestamp".into())
    })?;
    let window = sendgrid_timestamp_window_secs();
    if unix_now().abs_diff(ts_secs) > window {
        log_abuse_attempt("sendgrid: timestamp outside tolerance window");
        return Err(AppError::Validation(
            "SendGrid webhook timestamp outside tolerance window".into(),
        ));
    }

    // Decode the signature (base64 DER).
    let signature_der = STANDARD.decode(sig_b64).map_err(|_| {
        log_abuse_attempt("sendgrid: invalid base64 signature");
        AppError::Validation("SendGrid signature is not valid base64".into())
    })?;

    // Build the signed payload: timestamp || raw_body.
    let mut signed = Vec::with_capacity(ts_str.len() + body.len());
    signed.extend_from_slice(ts_str.as_bytes());
    signed.extend_from_slice(body);

    if !verify_ecdsa_p256_sha256(&public_key_der, &signed, &signature_der) {
        log_abuse_attempt("sendgrid: signature verification failed");
        return Err(AppError::Validation(
            "SendGrid webhook signature verification failed".into(),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// SNS helpers
// ---------------------------------------------------------------------------

/// Validate an SNS `SigningCertURL`.
///
/// Requirements:
/// - scheme is `https`
/// - host matches `^sns\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$`
/// - path ends in `.pem`
fn is_valid_sns_cert_url(url: &str) -> bool {
    let rest = match url.strip_prefix("https://") {
        Some(r) => r,
        None => return false,
    };
    // Split host from path.
    let (host, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => return false,
    };
    // Reject userinfo / port tricks.
    if host.contains('@') || host.contains(':') {
        return false;
    }
    if !is_valid_sns_host(host) {
        return false;
    }
    // Strip any query/fragment before checking the extension.
    let path_only = path.split(['?', '#']).next().unwrap_or(path);
    path_only.ends_with(".pem")
}

/// Match `^sns\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$`.
fn is_valid_sns_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let rest = match host.strip_prefix("sns.") {
        Some(r) => r,
        None => return false,
    };
    let rest = match rest.strip_suffix(".amazonaws.com.cn") {
        Some(r) => r,
        None => match rest.strip_suffix(".amazonaws.com") {
            Some(r) => r,
            None => return false,
        },
    };
    // `rest` is the region segment: one or more of [a-z0-9-].
    !rest.is_empty()
        && rest
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Build the canonical SNS string-to-sign.
///
/// The field order differs between `Notification` and the confirmation
/// message types, per the AWS documentation.
fn build_sns_string_to_sign(
    envelope: &serde_json::Value,
    msg_type: &str,
) -> Result<String, AppError> {
    let get = |key: &str| -> Result<&str, AppError> {
        envelope[key]
            .as_str()
            .ok_or_else(|| AppError::Validation(format!("SNS message missing {key}")))
    };

    let mut s = String::new();
    let mut push = |label: &str, value: &str| {
        s.push_str(label);
        s.push('\n');
        s.push_str(value);
        s.push('\n');
    };

    match msg_type {
        "Notification" => {
            push("Message", get("Message")?);
            push("MessageId", get("MessageId")?);
            if let Some(subject) = envelope["Subject"].as_str() {
                push("Subject", subject);
            }
            push("Timestamp", get("Timestamp")?);
            push("TopicArn", get("TopicArn")?);
            push("Type", get("Type")?);
        }
        "SubscriptionConfirmation" | "UnsubscribeConfirmation" => {
            push("Message", get("Message")?);
            push("MessageId", get("MessageId")?);
            push("SubscribeURL", get("SubscribeURL")?);
            push("Timestamp", get("Timestamp")?);
            push("Token", get("Token")?);
            push("TopicArn", get("TopicArn")?);
            push("Type", get("Type")?);
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported SNS message type: {other}"
            )))
        }
    }

    Ok(s)
}

/// Per-URL cache of SNS signing certificates (DER).
fn sns_cert_cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Fetch (or retrieve from cache) the SNS signing certificate for `url`.
///
/// The certificate is fetched through the SSRF-safe outbound client so that
/// redirects and DNS rebinding cannot be used to reach internal services.
fn fetch_sns_certificate(url: &str) -> Result<Vec<u8>, AppError> {
    if let Some(cached) = sns_cert_cache()
        .lock()
        .ok()
        .and_then(|c| c.get(url).cloned())
    {
        return Ok(cached);
    }

    let pem = crate::net::outbound::fetch_text(url)
        .map_err(|e| AppError::Internal(format!("failed to fetch SNS certificate: {e}")))?;

    let der = pem_to_der(&pem).ok_or_else(|| {
        AppError::Validation("SNS signing certificate is not valid PEM".into())
    })?;

    if let Ok(mut cache) = sns_cert_cache().lock() {
        cache.insert(url.to_owned(), der.clone());
    }

    Ok(der)
}

/// Extract DER bytes from a PEM-encoded certificate.
fn pem_to_der(pem: &str) -> Option<Vec<u8>> {
    let begin = "-----BEGIN CERTIFICATE-----";
    let end = "-----END CERTIFICATE-----";
    let start = pem.find(begin)? + begin.len();
    let stop = pem[start..].find(end)? + start;
    let b64: String = pem[start..stop]
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    STANDARD.decode(b64).ok()
}

/// Verify an RSA PKCS#1 v1.5 signature over SHA1.
fn verify_rsa_pkcs1_sha1(cert_der: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;
    use rsa::RsaPublicKey;
    use sha1::Digest as _;

    let key = match extract_rsa_public_key(cert_der) {
        Some(k) => k,
        None => return false,
    };
    let _ = RsaPublicKey::default(); // keep import used across feature sets
    let verifying_key = VerifyingKey::<Sha1>::new(key);
    let sig = match Signature::try_from(signature) {
        Ok(s) => s,
        Err(_) => return false,
    };
    let _ = Sha1::new();
    verifying_key.verify(message, &sig).is_ok()
}

/// Verify an RSA PKCS#1 v1.5 signature over SHA256.
fn verify_rsa_pkcs1_sha256(cert_der: &[u8], message: &[u8], signature: &[u8]) -> bool {
    use rsa::pkcs1v15::{Signature, VerifyingKey};
    use rsa::signature::Verifier;

    let key = match extract_rsa_public_key(cert_der) {
        Some(k) => k,
        None => return false,
    };
    let verifying_key = VerifyingKey::<Sha256>::new(key);
    let sig = match Signature::try_from(signature) {
        Ok(s) => s,
        Err(_) => return false,
    };
    verifying_key.verify(message, &sig).is_ok()
}

/// Extract an RSA public key from a DER-encoded X.509 certificate.
fn extract_rsa_public_key(cert_der: &[u8]) -> Option<rsa::RsaPublicKey> {
    use rsa::pkcs1::DecodeRsaPublicKey;
    use rsa::pkcs8::DecodePublicKey;
    use x509_parser::prelude::*;

    let (_, cert) = X509Certificate::from_der(cert_der).ok()?;
    let spki = cert.public_key();
    let der = spki.raw;

    // Try PKCS#8 SubjectPublicKeyInfo first, then PKCS#1.
    if let Ok(key) = rsa::RsaPublicKey::from_public_key_der(der) {
        return Some(key);
    }
    rsa::RsaPublicKey::from_pkcs1_der(der).ok()
}

/// Read the SNS timestamp tolerance window from the environment.
fn sns_timestamp_window_secs() -> u64 {
    std::env::var(SNS_TIMESTAMP_WINDOW_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SNS_TIMESTAMP_WINDOW_SECS)
}

/// Read the SendGrid timestamp tolerance window from the environment.
fn sendgrid_timestamp_window_secs() -> u64 {
    std::env::var(SENDGRID_TIMESTAMP_WINDOW_ENV)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_SENDGRID_TIMESTAMP_WINDOW_SECS)
}

/// Read the SendGrid webhook public key (base64 DER) from the environment.
fn sendgrid_public_key() -> Option<Vec<u8>> {
    let raw = std::env::var(SENDGRID_PUBLIC_KEY_ENV).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    STANDARD.decode(trimmed).ok()
}

/// Verify an ECDSA-P256-SHA256 signature.
fn verify_ecdsa_p256_sha256(public_key_der: &[u8], message: &[u8], signature_der: &[u8]) -> bool {
    use p256::ecdsa::signature::Verifier;
    use p256::ecdsa::{Signature, VerifyingKey};
    use p256::pkcs8::DecodePublicKey;

    let key = match VerifyingKey::from_public_key_der(public_key_der) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let sig = match Signature::from_der(signature_der) {
        Ok(s) => s,
        Err(_) => return false,
    };
    key.verify(message, &sig).is_ok()
}

/// Parse an RFC 3339 timestamp into a Unix timestamp (seconds).
fn parse_rfc3339_unix(s: &str) -> Option<u64> {
    let dt = chrono::DateTime::parse_from_rfc3339(s).ok()?;
    let secs = dt.timestamp();
    if secs < 0 {
        None
    } else {
        Some(secs as u64)
    }
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

    // -----------------------------------------------------------------------
    // Issue #1421: SNS signature verification
    // -----------------------------------------------------------------------

    use axum::http::HeaderMap;

    fn sns_headers(msg_type: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-amz-sns-message-type", msg_type.parse().unwrap());
        h
    }

    #[test]
    fn sns_cert_host_allowlist_accepts_aws_hosts() {
        assert!(is_sns_cert_host("sns.us-east-1.amazonaws.com"));
        assert!(is_sns_cert_host("sns.eu-west-1.amazonaws.com"));
        assert!(is_sns_cert_host("sns.cn-north-1.amazonaws.com.cn"));
        assert!(is_sns_cert_host("SNS.US-EAST-1.AMAZONAWS.COM"));
    }

    #[test]
    fn sns_cert_host_allowlist_rejects_malicious_hosts() {
        assert!(!is_sns_cert_host("evil.com"));
        assert!(!is_sns_cert_host("sns.us-east-1.amazonaws.com.evil.com"));
        assert!(!is_sns_cert_host("sns..amazonaws.com"));
        assert!(!is_sns_cert_host("sns.us_east_1.amazonaws.com"));
        assert!(!is_sns_cert_host("amazonaws.com"));
        assert!(!is_sns_cert_host("sns.us-east-1.amazonaws.com.cn.evil"));
        assert!(!is_sns_cert_host("169.254.169.254"));
    }

    #[test]
    fn sns_cert_url_validation_rejects_http_and_bad_paths() {
        // Non-HTTPS.
        assert!(validate_sns_cert_url("http://sns.us-east-1.amazonaws.com/cert.pem").is_err());
        // Wrong host.
        assert!(validate_sns_cert_url("https://evil.com/cert.pem").is_err());
        // Wrong path suffix.
        assert!(validate_sns_cert_url("https://sns.us-east-1.amazonaws.com/cert.txt").is_err());
        // Valid.
        assert!(
            validate_sns_cert_url("https://sns.us-east-1.amazonaws.com/SimpleNotificationService.pem")
                .is_ok()
        );
    }

    #[test]
    fn sns_timestamp_window_rejects_stale_and_future() {
        let stale = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        assert!(check_sns_timestamp(&stale).is_err());

        let future = (chrono::Utc::now() + chrono::Duration::hours(2)).to_rfc3339();
        assert!(check_sns_timestamp(&future).is_err());

        let fresh = chrono::Utc::now().to_rfc3339();
        assert!(check_sns_timestamp(&fresh).is_ok());

        assert!(check_sns_timestamp("not-a-timestamp").is_err());
    }

    #[test]
    fn sns_string_to_sign_notification_layout() {
        let env = SnsEnvelope {
            type_: "Notification".into(),
            message_id: "mid".into(),
            topic_arn: "arn:aws:sns:us-east-1:1:topic".into(),
            timestamp: "2026-01-01T00:00:00.000Z".into(),
            signature_version: "2".into(),
            signature: "sig".into(),
            signing_cert_url: "https://sns.us-east-1.amazonaws.com/c.pem".into(),
            subject: Some("subj".into()),
            message: Some("msg".into()),
            token: None,
            subscribe_url: None,
        };
        let s = env.string_to_sign().unwrap();
        assert_eq!(
            s,
            "Message\nmsg\nMessageId\nmid\nSubject\nsubj\nTimestamp\n2026-01-01T00:00:00.000Z\nTopicArn\narn:aws:sns:us-east-1:1:topic\nType\nNotification\n"
        );
    }

    #[test]
    fn sns_string_to_sign_subscription_layout() {
        let env = SnsEnvelope {
            type_: "SubscriptionConfirmation".into(),
            message_id: "mid".into(),
            topic_arn: "arn:aws:sns:us-east-1:1:topic".into(),
            timestamp: "2026-01-01T00:00:00.000Z".into(),
            signature_version: "1".into(),
            signature: "sig".into(),
            signing_cert_url: "https://sns.us-east-1.amazonaws.com/c.pem".into(),
            subject: None,
            message: Some("msg".into()),
            token: Some("tok".into()),
            subscribe_url: Some("https://sns.us-east-1.amazonaws.com/?Action=Confirm".into()),
        };
        let s = env.string_to_sign().unwrap();
        assert_eq!(
            s,
            "Message\nmsg\nMessageId\nmid\nSubscribeURL\nhttps://sns.us-east-1.amazonaws.com/?Action=Confirm\nTimestamp\n2026-01-01T00:00:00.000Z\nToken\ntok\nTopicArn\narn:aws:sns:us-east-1:1:topic\nType\nSubscriptionConfirmation\n"
        );
    }

    #[test]
    fn sns_missing_header_rejected() {
        let headers = HeaderMap::new();
        assert!(verify_sns_signature(&headers, b"{}").is_err());
    }

    #[test]
    fn sns_header_body_type_mismatch_rejected() {
        let body = serde_json::json!({
            "Type": "Notification",
            "MessageId": "mid",
            "TopicArn": "arn:aws:sns:us-east-1:1:topic",
            "Timestamp": chrono::Utc::now().to_rfc3339(),
            "SignatureVersion": "2",
            "Signature": "AAAA",
            "SigningCertURL": "https://sns.us-east-1.amazonaws.com/c.pem",
            "Message": "m",
        });
        let headers = sns_headers("SubscriptionConfirmation");
        assert!(verify_sns_signature(&headers, body.to_string().as_bytes()).is_err());
    }

    #[test]
    fn sns_malicious_cert_url_rejected_before_fetch() {
        let body = serde_json::json!({
            "Type": "Notification",
            "MessageId": "mid",
            "TopicArn": "arn:aws:sns:us-east-1:1:topic",
            "Timestamp": chrono::Utc::now().to_rfc3339(),
            "SignatureVersion": "2",
            "Signature": "AAAA",
            "SigningCertURL": "https://169.254.169.254/latest/meta-data/cert.pem",
            "Message": "m",
        });
        let headers = sns_headers("Notification");
        assert!(verify_sns_signature(&headers, body.to_string().as_bytes()).is_err());
    }

    #[test]
    fn sns_stale_message_rejected_before_fetch() {
        let body = serde_json::json!({
            "Type": "Notification",
            "MessageId": "mid",
            "TopicArn": "arn:aws:sns:us-east-1:1:topic",
            "Timestamp": (chrono::Utc::now() - chrono::Duration::hours(3)).to_rfc3339(),
            "SignatureVersion": "2",
            "Signature": "AAAA",
            "SigningCertURL": "https://sns.us-east-1.amazonaws.com/c.pem",
            "Message": "m",
        });
        let headers = sns_headers("Notification");
        assert!(verify_sns_signature(&headers, body.to_string().as_bytes()).is_err());
    }

    #[test]
    fn sns_rsa_signature_round_trip_v2() {
        use base64::engine::general_purpose::STANDARD;
        use rsa::pkcs1v15::SigningKey;
        use rsa::pkcs8::EncodePublicKey;
        use rsa::signature::{SignatureEncoding, Signer};
        use rsa::RsaPrivateKey;

        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("keygen");
        let public_der = private
            .to_public_key()
            .to_public_key_der()
            .expect("public der");
        let pem = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            STANDARD.encode(public_der.as_bytes())
        );

        let msg = "Message\nhello\nMessageId\nmid\nTimestamp\n2026-01-01T00:00:00.000Z\nTopicArn\narn\nType\nNotification\n";
        let key = SigningKey::<sha2::Sha256>::new(private);
        let sig = key.sign(msg.as_bytes());
        let sig_b64 = STANDARD.encode(sig.to_bytes());

        assert!(verify_sns_rsa_signature(&pem, msg, "2", &sig_b64).is_ok());
        // Tampered message must fail.
        assert!(verify_sns_rsa_signature(&pem, "Message\ntampered\n", "2", &sig_b64).is_err());
        // Wrong signature version must fail.
        assert!(verify_sns_rsa_signature(&pem, msg, "1", &sig_b64).is_err());
        // Unsupported version.
        assert!(verify_sns_rsa_signature(&pem, msg, "3", &sig_b64).is_err());
    }

    #[test]
    fn sns_rsa_signature_round_trip_v1_sha1() {
        use base64::engine::general_purpose::STANDARD;
        use rsa::pkcs1v15::SigningKey;
        use rsa::pkcs8::EncodePublicKey;
        use rsa::signature::{SignatureEncoding, Signer};
        use rsa::RsaPrivateKey;

        let mut rng = rand::thread_rng();
        let private = RsaPrivateKey::new(&mut rng, 2048).expect("keygen");
        let public_der = private
            .to_public_key()
            .to_public_key_der()
            .expect("public der");
        let pem = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            STANDARD.encode(public_der.as_bytes())
        );

        let msg = "Message\nhello\nMessageId\nmid\nTimestamp\nt\nTopicArn\narn\nType\nNotification\n";
        let key = SigningKey::<sha1::Sha1>::new(private);
        let sig = key.sign(msg.as_bytes());
        let sig_b64 = STANDARD.encode(sig.to_bytes());

        assert!(verify_sns_rsa_signature(&pem, msg, "1", &sig_b64).is_ok());
        assert!(verify_sns_rsa_signature(&pem, "Message\nother\n", "1", &sig_b64).is_err());
    }

    #[test]
    fn sns_invalid_pem_rejected() {
        assert!(verify_sns_rsa_signature("not a pem", "msg", "2", "AAAA").is_err());
    }

}
