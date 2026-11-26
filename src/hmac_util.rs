//! Infallible HMAC-SHA256 construction (issue #1246).
//!
//! `Hmac::<Sha256>::new_from_slice` returns a `Result` even though HMAC accepts
//! keys of any length, which led to `expect("HMAC accepts any key length")`
//! calls throughout the crate. [`hmac_sha256`] performs the RFC 2104 key
//! normalisation itself (hash keys longer than the block size, then zero-pad)
//! and hands a block-sized key to the infallible `KeyInit::new`, producing
//! exactly the same MAC as `new_from_slice`.

use hmac::digest::KeyInit;
use hmac::Hmac;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// SHA-256 block size in bytes — the HMAC key length after normalisation.
const SHA256_BLOCK_SIZE: usize = 64;

/// Default tolerance window (in seconds) for webhook signature v2 timestamps.
pub const DEFAULT_SIGNATURE_TOLERANCE_SECS: u64 = 300;

/// Errors returned when verifying a webhook signature v2 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureError {
    /// The header did not contain a `t=` component.
    MissingTimestamp,
    /// The header did not contain a `v1=` component.
    MissingSignature,
    /// The `t=` component was not a valid unsigned integer.
    InvalidTimestamp,
    /// The `v1=` component was not valid lowercase/uppercase hex.
    InvalidSignatureEncoding,
    /// The timestamp is further than the tolerance from `now`.
    TimestampOutOfTolerance,
    /// The recomputed HMAC did not match the provided signature.
    SignatureMismatch,
}

impl std::fmt::Display for SignatureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            SignatureError::MissingTimestamp => "missing timestamp component",
            SignatureError::MissingSignature => "missing signature component",
            SignatureError::InvalidTimestamp => "invalid timestamp component",
            SignatureError::InvalidSignatureEncoding => "invalid signature encoding",
            SignatureError::TimestampOutOfTolerance => "timestamp outside tolerance window",
            SignatureError::SignatureMismatch => "signature mismatch",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for SignatureError {}

/// Compute the raw HMAC-SHA256 of `"<timestamp>.<body>"` under `secret`.
fn v2_mac(secret: &str, timestamp: u64, body: &[u8]) -> [u8; 32] {
    use hmac::Mac;
    let mut mac = hmac_sha256(secret.as_bytes());
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.finalize().into_bytes().into()
}

/// Build the signature v2 header value `t=<unix>,v1=<hex>` for `body`.
///
/// `v1 = HMAC-SHA256(secret, "<timestamp>.<body>")`.
pub fn sign_payload_v2(secret: &str, timestamp: u64, body: &[u8]) -> String {
    let mac = v2_mac(secret, timestamp, body);
    format!("t={timestamp},v1={}", hex::encode(mac))
}

/// Verify a signature v2 header value against `body`.
///
/// Enforces that `timestamp` is within `tolerance_secs` of `now` (in either
/// direction) and compares the recomputed MAC in constant time.
pub fn verify_signature_v2(
    secret: &str,
    header: &str,
    body: &[u8],
    now: u64,
    tolerance_secs: u64,
) -> Result<(), SignatureError> {
    let mut timestamp: Option<u64> = None;
    let mut signature: Option<&str> = None;
    for part in header.split(',') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("t=") {
            timestamp = Some(value.parse().map_err(|_| SignatureError::InvalidTimestamp)?);
        } else if let Some(value) = part.strip_prefix("v1=") {
            signature = Some(value);
        }
    }
    let timestamp = timestamp.ok_or(SignatureError::MissingTimestamp)?;
    let signature = signature.ok_or(SignatureError::MissingSignature)?;

    let provided = hex::decode(signature).map_err(|_| SignatureError::InvalidSignatureEncoding)?;

    let skew = now.abs_diff(timestamp);
    if skew > tolerance_secs {
        return Err(SignatureError::TimestampOutOfTolerance);
    }

    let expected = v2_mac(secret, timestamp, body);
    if provided.len() == expected.len() && bool::from(provided.ct_eq(&expected)) {
        Ok(())
    } else {
        Err(SignatureError::SignatureMismatch)
    }
}

/// Build an HMAC-SHA256 instance keyed with `key` (any length).
pub fn hmac_sha256(key: &[u8]) -> Hmac<Sha256> {
    let mut block = [0u8; SHA256_BLOCK_SIZE];
    if key.len() > SHA256_BLOCK_SIZE {
        let digest = Sha256::digest(key);
        block[..digest.len()].copy_from_slice(&digest);
    } else {
        block[..key.len()].copy_from_slice(key);
    }
    <Hmac<Sha256> as KeyInit>::new(&block.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hmac::Mac;

    fn reference(key: &[u8], msg: &[u8]) -> Vec<u8> {
        let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key).unwrap();
        mac.update(msg);
        mac.finalize().into_bytes().to_vec()
    }

    fn ours(key: &[u8], msg: &[u8]) -> Vec<u8> {
        let mut mac = hmac_sha256(key);
        mac.update(msg);
        mac.finalize().into_bytes().to_vec()
    }

    #[test]
    fn matches_new_from_slice_for_all_key_lengths() {
        for len in [0usize, 1, 32, 63, 64, 65, 128, 1000] {
            let key = vec![0xA5u8; len];
            assert_eq!(ours(&key, b"payload"), reference(&key, b"payload"), "key len {len}");
        }
    }

    #[test]
    fn rfc4231_test_case_2() {
        let mut mac = hmac_sha256(b"Jefe");
        mac.update(b"what do ya want for nothing?");
        assert_eq!(
            hex::encode(mac.finalize().into_bytes()),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn v2_signature_round_trips() {
        let header = sign_payload_v2("s3cret", 1_700_000_000, b"{\"a\":1}");
        assert!(header.starts_with("t=1700000000,v1="));
        assert_eq!(
            verify_signature_v2("s3cret", &header, b"{\"a\":1}", 1_700_000_000, 300),
            Ok(())
        );
    }

    #[test]
    fn v2_signature_covers_timestamp_and_body() {
        let header = sign_payload_v2("s3cret", 1_700_000_000, b"body");
        // Tampered body.
        assert_eq!(
            verify_signature_v2("s3cret", &header, b"other", 1_700_000_000, 300),
            Err(SignatureError::SignatureMismatch)
        );
        // Tampered timestamp (signature no longer matches).
        let tampered = header.replacen("t=1700000000", "t=1700000001", 1);
        assert_eq!(
            verify_signature_v2("s3cret", &tampered, b"body", 1_700_000_001, 300),
            Err(SignatureError::SignatureMismatch)
        );
    }

    #[test]
    fn v2_signature_rejects_stale_and_future_timestamps() {
        let header = sign_payload_v2("s3cret", 1_700_000_000, b"body");
        assert_eq!(
            verify_signature_v2("s3cret", &header, b"body", 1_700_000_301, 300),
            Err(SignatureError::TimestampOutOfTolerance)
        );
        assert_eq!(
            verify_signature_v2("s3cret", &header, b"body", 1_699_999_699, 300),
            Err(SignatureError::TimestampOutOfTolerance)
        );
        // Exactly at the boundary is accepted.
        assert_eq!(
            verify_signature_v2("s3cret", &header, b"body", 1_700_000_300, 300),
            Ok(())
        );
    }

    #[test]
    fn v2_signature_rejects_malformed_headers() {
        assert_eq!(
            verify_signature_v2("s3cret", "v1=abcd", b"body", 0, 300),
            Err(SignatureError::MissingTimestamp)
        );
        assert_eq!(
            verify_signature_v2("s3cret", "t=123", b"body", 123, 300),
            Err(SignatureError::MissingSignature)
        );
        assert_eq!(
            verify_signature_v2("s3cret", "t=abc,v1=00", b"body", 0, 300),
            Err(SignatureError::InvalidTimestamp)
        );
        assert_eq!(
            verify_signature_v2("s3cret", "t=123,v1=zz", b"body", 123, 300),
            Err(SignatureError::InvalidSignatureEncoding)
        );
    }
}
