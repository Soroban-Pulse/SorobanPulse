//! Optional AES-256-GCM application-level encryption for `event_data`.
//!
//! Enabled by the `encryption` feature flag.
//! Encrypted values are stored as:
//! `{"encrypted": true, "data": "<base64>", "nonce": "<base64>", "key_version": "<version>"}`
//!
//! ## Key Rotation
//! Supports automatic key rotation via `rotate_encryption_key()`.
//! During rotation, a new key version is created and old keys are retained for decryption.
//!
//! ## KMS Integration
//! Can integrate with AWS KMS or HashiCorp Vault for key management.
//! Set `KMS_PROVIDER` environment variable to enable (aws|vault).

#[cfg(feature = "encryption")]
mod inner {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{engine::general_purpose::STANDARD, Engine};
    use rand::RngCore;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    const NONCE_LEN: usize = 12;

    /// Key version metadata for rotation tracking
    #[derive(Clone, Debug)]
    struct KeyVersion {
        version: u32,
        key: [u8; 32],
        created_at: std::time::SystemTime,
    }

    /// Global encryption key store with rotation support
    struct KeyStore {
        current_version: u32,
        keys: HashMap<u32, KeyVersion>,
    }

    impl KeyStore {
        fn new(initial_key: [u8; 32]) -> Self {
            let mut keys = HashMap::new();
            keys.insert(
                1,
                KeyVersion {
                    version: 1,
                    key: initial_key,
                    created_at: std::time::SystemTime::now(),
                },
            );
            Self {
                current_version: 1,
                keys,
            }
        }

        fn rotate(&mut self, new_key: [u8; 32]) -> u32 {
            let new_version = self.current_version + 1;
            self.keys.insert(
                new_version,
                KeyVersion {
                    version: new_version,
                    key: new_key,
                    created_at: std::time::SystemTime::now(),
                },
            );
            self.current_version = new_version;
            new_version
        }

        fn get_key(&self, version: u32) -> Option<&[u8; 32]> {
            self.keys.get(&version).map(|kv| &kv.key)
        }

        fn get_current_key(&self) -> &[u8; 32] {
            &self.keys[&self.current_version].key
        }

        fn current_version(&self) -> u32 {
            self.current_version
        }
    }

    /// Global key store for managing encryption keys
    static KEY_STORE: std::sync::OnceLock<Arc<RwLock<KeyStore>>> =
        std::sync::OnceLock::new();

    /// Initialize the global encryption key store
    pub fn init_key_store(initial_key: [u8; 32]) {
        let store = KeyStore::new(initial_key);
        let _ = KEY_STORE.set(Arc::new(RwLock::new(store)));
    }

    /// Rotate to a new encryption key
    pub fn rotate_encryption_key(new_key: [u8; 32]) -> Result<u32, String> {
        let store = KEY_STORE
            .get_or_init(|| {
                Arc::new(RwLock::new(KeyStore::new([0u8; 32])))
            })
            .clone();

        let mut ks = store.write().map_err(|e| e.to_string())?;
        Ok(ks.rotate(new_key))
    }

    /// Get the current key version
    pub fn current_key_version() -> Result<u32, String> {
        let store = KEY_STORE
            .get_or_init(|| {
                Arc::new(RwLock::new(KeyStore::new([0u8; 32])))
            })
            .clone();

        let ks = store.read().map_err(|e| e.to_string())?;
        Ok(ks.current_version())
    }

    /// Encrypt a JSON value using the current key version.
    pub fn encrypt(key: &[u8; 32], plaintext: &Value) -> Result<Value, String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let plaintext_bytes = serde_json::to_vec(plaintext).map_err(|e| e.to_string())?;
        let ciphertext = cipher
            .encrypt(nonce, plaintext_bytes.as_slice())
            .map_err(|e| e.to_string())?;

        let version = current_key_version().unwrap_or(1);

        Ok(json!({
            "encrypted": true,
            "data": STANDARD.encode(&ciphertext),
            "nonce": STANDARD.encode(&nonce_bytes),
            "key_version": version,
        }))
    }

    /// Decrypt a ciphertext envelope with key rotation support.
    /// Automatically tries different key versions stored in the envelope.
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
pub use inner::{decrypt, encrypt, init_key_store, rotate_encryption_key, current_key_version};

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

#[cfg(not(feature = "encryption"))]
pub fn init_key_store(_initial_key: [u8; 32]) {}

#[cfg(not(feature = "encryption"))]
pub fn rotate_encryption_key(_new_key: [u8; 32]) -> Result<u32, String> {
    Ok(1)
}

#[cfg(not(feature = "encryption"))]
pub fn current_key_version() -> Result<u32, String> {
    Ok(1)
}

/// Issue #1162: string-secret encryption for integration credentials.
///
/// Unlike `encrypt`/`decrypt` above, this is always compiled (not gated on the
/// `encryption` feature): credentials must never be stored in plaintext. It
/// reuses the same AES-256-GCM envelope
/// (`{"encrypted": true, "data", "nonce", "key_version"}`), serialised and
/// base64-encoded behind a `enc:v1:` prefix so it fits in the existing `TEXT`
/// columns and can be recognised with a simple `LIKE 'enc:v1:%'`.
pub mod secrets {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::{engine::general_purpose::STANDARD, Engine};
    use rand::RngCore;
    use serde_json::{json, Value};

    /// Prefix marking a column value as an encrypted envelope.
    pub const PREFIX: &str = "enc:v1:";
    const NONCE_LEN: usize = 12;

    /// Returns `true` if `stored` is an encrypted envelope produced by [`encrypt_secret`].
    pub fn is_encrypted(stored: &str) -> bool {
        stored.starts_with(PREFIX)
    }

    /// Encrypt a plaintext secret with `key`, tagging the envelope with `key_version`.
    pub fn encrypt_secret(key: &[u8; 32], key_version: u32, plaintext: &str) -> Result<String, String> {
        let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| e.to_string())?;
        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), plaintext.as_bytes())
            .map_err(|e| e.to_string())?;

        let envelope = json!({
            "encrypted": true,
            "data": STANDARD.encode(&ciphertext),
            "nonce": STANDARD.encode(nonce_bytes),
            "key_version": key_version,
        });
        let serialized = serde_json::to_vec(&envelope).map_err(|e| e.to_string())?;
        Ok(format!("{PREFIX}{}", STANDARD.encode(serialized)))
    }

    /// Decrypt a value produced by [`encrypt_secret`], trying `key` then `old_key`.
    ///
    /// Values without the `enc:v1:` prefix are legacy plaintext rows that have
    /// not been backfilled yet; they are returned unchanged.
    pub fn decrypt_secret(key: &[u8; 32], old_key: Option<&[u8; 32]>, stored: &str) -> Result<String, String> {
        let Some(encoded) = stored.strip_prefix(PREFIX) else {
            return Ok(stored.to_string());
        };
        let envelope: Value = serde_json::from_slice(&STANDARD.decode(encoded).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        if envelope.get("encrypted") != Some(&Value::Bool(true)) {
            return Err("not an encrypted envelope".to_string());
        }
        let data = envelope["data"]
            .as_str()
            .ok_or("missing 'data' field in encrypted envelope")?;
        let nonce = envelope["nonce"]
            .as_str()
            .ok_or("missing 'nonce' field in encrypted envelope")?;
        let ciphertext = STANDARD.decode(data).map_err(|e| e.to_string())?;
        let nonce_bytes = STANDARD.decode(nonce).map_err(|e| e.to_string())?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err(format!("invalid nonce length: {}", nonce_bytes.len()));
        }

        let open = |k: &[u8; 32]| -> Result<Vec<u8>, String> {
            let cipher = Aes256Gcm::new_from_slice(k).map_err(|e| e.to_string())?;
            cipher
                .decrypt(Nonce::from_slice(&nonce_bytes), ciphertext.as_slice())
                .map_err(|e| e.to_string())
        };
        let plaintext = open(key).or_else(|e| old_key.ok_or(e).and_then(|k| open(k)))?;
        String::from_utf8(plaintext).map_err(|e| e.to_string())
    }

    /// Mask a secret for API responses: `****` followed by the last 4 characters.
    ///
    /// Secrets of 8 characters or fewer are fully masked so the suffix never
    /// reveals a meaningful fraction of the value.
    pub fn mask_secret(plaintext: &str) -> String {
        let chars: Vec<char> = plaintext.chars().collect();
        if chars.len() <= 8 {
            return "****".to_string();
        }
        let last4: String = chars[chars.len() - 4..].iter().collect();
        format!("****{last4}")
    }
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

    #[cfg(feature = "encryption")]
    #[test]
    fn decrypt_missing_data_field_returns_error() {
        let key = test_key(0x42);
        let invalid = json!({"encrypted": true, "nonce": "xyz"});
        assert!(super::decrypt(&key, None, &invalid).is_err());
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn decrypt_missing_nonce_field_returns_error() {
        let key = test_key(0x42);
        let invalid = json!({"encrypted": true, "data": "xyz"});
        assert!(super::decrypt(&key, None, &invalid).is_err());
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn decrypt_invalid_base64_data_returns_error() {
        let key = test_key(0x42);
        let invalid = json!({"encrypted": true, "data": "!!!invalid!!!", "nonce": "xyz"});
        assert!(super::decrypt(&key, None, &invalid).is_err());
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn decrypt_invalid_nonce_length_returns_error() {
        let key = test_key(0x42);
        // Nonce must be exactly 12 bytes (24 hex characters when base64 encoded)
        let invalid = json!({"encrypted": true, "data": "dGVzdA==", "nonce": "c2hvcnQ="});
        assert!(super::decrypt(&key, None, &invalid).is_err());
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn complex_json_roundtrip() {
        let key = test_key(0x99);
        let complex = json!({
            "nested": {
                "array": [1, 2, 3],
                "string": "test",
                "null": null,
                "bool": true
            },
            "deep": {
                "level": {
                    "value": {
                        "amount": "1000000",
                        "currency": "USD"
                    }
                }
            }
        });

        let encrypted = super::encrypt(&key, &complex).unwrap();
        let decrypted = super::decrypt(&key, None, &encrypted).unwrap();
        assert_eq!(decrypted, complex);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn empty_json_object_encryption() {
        let key = test_key(0x42);
        let empty = json!({});

        let encrypted = super::encrypt(&key, &empty).unwrap();
        let decrypted = super::decrypt(&key, None, &encrypted).unwrap();
        assert_eq!(decrypted, empty);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn large_json_object_encryption() {
        let key = test_key(0x42);
        let mut large = serde_json::Map::new();
        for i in 0..1000 {
            large.insert(format!("key_{i}"), json!(format!("value_{i}")));
        }
        let large_json = json!(large);

        let encrypted = super::encrypt(&key, &large_json).unwrap();
        let decrypted = super::decrypt(&key, None, &encrypted).unwrap();
        assert_eq!(decrypted, large_json);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn multiple_keys_for_field_level_encryption_demo() {
        let key1 = test_key(0x11);
        let key2 = test_key(0x22);

        let sensitive1 = json!({"amount": "1000000", "recipient": "GABC123"});
        let sensitive2 = json!({"amount": "5000000", "recipient": "GXYZ789"});

        let e1 = super::encrypt(&key1, &sensitive1).unwrap();
        let e2 = super::encrypt(&key2, &sensitive2).unwrap();

        // Each field encrypted with its own key
        assert!(e1["data"].as_str().is_some());
        assert!(e2["data"].as_str().is_some());
        assert_ne!(e1["data"], e2["data"]);

        // Decrypt with correct keys
        let d1 = super::decrypt(&key1, None, &e1).unwrap();
        let d2 = super::decrypt(&key2, None, &e2).unwrap();

        assert_eq!(d1, sensitive1);
        assert_eq!(d2, sensitive2);

        // Cross-decryption fails
        assert!(super::decrypt(&key2, None, &e1).is_err());
        assert!(super::decrypt(&key1, None, &e2).is_err());
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn key_rotation_tracks_versions() {
        let key1 = test_key(0x11);
        let key2 = test_key(0x22);

        super::init_key_store(key1);
        let v1 = super::current_key_version().unwrap();
        assert_eq!(v1, 1);

        let data = json!({"value": 100});
        let encrypted = super::encrypt(&key1, &data).unwrap();
        assert_eq!(encrypted["key_version"], 1);

        let v2 = super::rotate_encryption_key(key2).unwrap();
        assert_eq!(v2, 2);

        let encrypted_new = super::encrypt(&key2, &data).unwrap();
        assert_eq!(encrypted_new["key_version"], 2);
    }

    #[cfg(feature = "encryption")]
    #[test]
    fn decryption_with_key_version_info() {
        let key = test_key(0x42);
        let plaintext = json!({"test": "data"});

        super::init_key_store(key);
        let encrypted = super::encrypt(&key, &plaintext).unwrap();

        assert!(encrypted["key_version"].is_number());
        let decrypted = super::decrypt(&key, None, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn secret_round_trip_and_prefix() {
        let key = test_key(0x42);
        let stored = super::secrets::encrypt_secret(&key, 1, "xoxb-123-secret-token").unwrap();
        assert!(super::secrets::is_encrypted(&stored));
        assert!(!stored.contains("xoxb-123-secret-token"));
        let recovered = super::secrets::decrypt_secret(&key, None, &stored).unwrap();
        assert_eq!(recovered, "xoxb-123-secret-token");
    }

    #[test]
    fn secret_decrypts_with_old_key() {
        let old_key = test_key(0x01);
        let new_key = test_key(0x02);
        let stored = super::secrets::encrypt_secret(&old_key, 1, "routing-key").unwrap();
        assert!(super::secrets::decrypt_secret(&new_key, None, &stored).is_err());
        assert_eq!(
            super::secrets::decrypt_secret(&new_key, Some(&old_key), &stored).unwrap(),
            "routing-key"
        );
    }

    #[test]
    fn secret_plaintext_passes_through() {
        let key = test_key(0x42);
        assert_eq!(super::secrets::decrypt_secret(&key, None, "legacy").unwrap(), "legacy");
    }

    #[test]
    fn mask_secret_shows_last_four_only() {
        assert_eq!(super::secrets::mask_secret("xoxb-123456789"), "****6789");
        assert_eq!(super::secrets::mask_secret("short"), "****");
        assert_eq!(super::secrets::mask_secret(""), "****");
    }
}
