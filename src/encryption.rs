//! Optional AES-256-GCM application-level encryption for `event_data`.
//!
//! Enabled by the `encryption` feature flag.
//! Encrypted values are stored as:
//! `{"encrypted": true, "data": "<base64>", "nonce": "<base64>"}`

#[cfg(feature = "encryption")]
mod inner {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{engine::general_purpose::STANDARD, Engine};
    use rand::RngCore;
    use serde_json::{json, Value};

    const NONCE_LEN: usize = 12;

    /// Encrypt a JSON value. Returns the ciphertext envelope on success.
    pub fn encrypt(key: &[u8; 32], plaintext: &Value) -> Result<Value, String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext_bytes = serde_json::to_vec(plaintext).map_err(|e| e.to_string())?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext_bytes.as_slice())
            .map_err(|e| e.to_string())?;

        Ok(json!({
            "encrypted": true,
            "data": STANDARD.encode(&ciphertext),
            "nonce": STANDARD.encode(&nonce_bytes),
        }))
    }

    /// Decrypt a ciphertext envelope. Tries `key` first, then `old_key` if provided.
    /// Returns the original JSON value, or the envelope unchanged if it is not encrypted.
    pub fn decrypt(
        key: &[u8; 32],
        old_key: Option<&[u8; 32]>,
        value: &Value,
    ) -> Result<Value, String> {
        // Not an encrypted envelope — pass through.
        if value.get("encrypted") != Some(&Value::Bool(true)) {
            return Ok(value.clone());
        }

        let data_b64 = value["data"]
            .as_str()
            .ok_or("missing 'data' field in encrypted envelope")?;
        let nonce_b64 = value["nonce"]
            .as_str()
            .ok_or("missing 'nonce' field in encrypted envelope")?;

        let ciphertext = STANDARD.decode(data_b64).map_err(|e| e.to_string())?;
        let nonce_bytes = STANDARD.decode(nonce_b64).map_err(|e| e.to_string())?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err(format!("invalid nonce length: {}", nonce_bytes.len()));
        }

        // Try current key first, then fall back to old key.
        let plaintext_bytes =
            try_decrypt_with_key(key, &nonce_bytes, &ciphertext).or_else(|e| {
                old_key
                    .ok_or(e)
                    .and_then(|k| try_decrypt_with_key(k, &nonce_bytes, &ciphertext))
            })?;

        serde_json::from_slice(&plaintext_bytes).map_err(|e| e.to_string())
    }

    fn try_decrypt_with_key(
        key: &[u8; 32],
        nonce_bytes: &[u8],
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
        let nonce = Nonce::from_slice(nonce_bytes);
        cipher.decrypt(nonce, ciphertext).map_err(|e| e.to_string())
    }
}

#[cfg(feature = "encryption")]
pub use inner::{decrypt, encrypt};

/// No-op stubs when the feature is disabled — callers compile cleanly either way.
#[cfg(not(feature = "encryption"))]
pub fn encrypt(
    _key: &[u8; 32],
    plaintext: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    Ok(plaintext.clone())
}

#[cfg(not(feature = "encryption"))]
pub fn decrypt(
    _key: &[u8; 32],
    _old_key: Option<&[u8; 32]>,
    value: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    fn test_key(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn round_trip_encrypt_decrypt() {
        let key = test_key(0x42);
        let plaintext = json!({"value": {"amount": 100}, "topic": ["transfer"]});

        let envelope = super::encrypt(&key, &plaintext).unwrap();
        assert_eq!(envelope["encrypted"], true);
        assert!(envelope["data"].is_string());
        assert!(envelope["nonce"].is_string());

        let recovered = super::decrypt(&key, None, &envelope).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn decrypt_with_old_key_on_rotation() {
        let old_key = test_key(0x01);
        let new_key = test_key(0x02);

        // Data encrypted with old key
        let plaintext = json!({"value": null, "topic": null});
        let envelope = super::encrypt(&old_key, &plaintext).unwrap();

        // Decrypting with new key alone fails
        assert!(super::decrypt(&new_key, None, &envelope).is_err());

        // Decrypting with new key + old key succeeds
        let recovered = super::decrypt(&new_key, Some(&old_key), &envelope).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn non_encrypted_value_passes_through() {
        let key = test_key(0x42);
        let plain = json!({"value": {"foo": "bar"}, "topic": []});
        let result = super::decrypt(&key, None, &plain).unwrap();
        assert_eq!(result, plain);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn wrong_key_returns_error() {
        let key = test_key(0xAA);
        let wrong_key = test_key(0xBB);
        let plaintext = json!({"x": 1});
        let envelope = super::encrypt(&key, &plaintext).unwrap();
        assert!(super::decrypt(&wrong_key, None, &envelope).is_err());
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn each_encryption_produces_unique_nonce() {
        let key = test_key(0x42);
        let plaintext = json!({"v": 1});
        let e1 = super::encrypt(&key, &plaintext).unwrap();
        let e2 = super::encrypt(&key, &plaintext).unwrap();
        // Different nonces (probabilistically certain)
        assert_ne!(e1["nonce"], e2["nonce"]);
    }

    #[cfg(not(feature = "encryption"))]
    #[test]
    fn stubs_are_identity() {
        let key = [0u8; 32];
        let v = json!({"a": 1});
        assert_eq!(super::encrypt(&key, &v).unwrap(), v);
        assert_eq!(super::decrypt(&key, None, &v).unwrap(), v);
    }
}
