//! Issue #1162: Encryption at rest for integration credentials.
//!
//! Slack/Telegram bot tokens, Discord and Slack webhook URLs (which embed a
//! secret), GitHub tokens, PagerDuty routing/API keys and provider credentials
//! stored in `notification_channels.config` are encrypted with the AES-256-GCM
//! envelope from [`crate::encryption::secrets`].
//!
//! The key is **separate** from the event-data key:
//! - `INTEGRATION_ENCRYPTION_KEY`     — current key (64 hex chars / 32 bytes)
//! - `INTEGRATION_ENCRYPTION_KEY_OLD` — previous key, used during rotation
//!
//! Writes fail closed: when no key is configured, credentials are refused
//! rather than stored in plaintext. Rows written before this change are
//! encrypted by [`backfill_plaintext`] at startup, and `POST /v1/admin/reencrypt`
//! moves every credential to the current key via [`start_reencrypt_job`].

use serde_json::Value;
use sqlx::PgPool;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::encryption::secrets;
use crate::metrics;

/// Every `TEXT` column that holds a credential or secret-bearing URL,
/// as `(table, column)`. All these tables have a `UUID` primary key `id`.
pub const CREDENTIAL_COLUMNS: &[(&str, &str)] = &[
    ("github_integrations", "access_token"),
    ("discord_integrations", "webhook_url"),
    ("slack_integrations", "webhook_url"),
    ("slack_integrations", "bot_token"),
    ("slack_integrations", "signing_secret"),
    ("telegram_integrations", "bot_token"),
    ("pagerduty_integrations", "routing_key"),
    ("pagerduty_integrations", "api_key"),
];

/// Keys inside `notification_channels.config` (at any depth) whose string
/// values are credentials, e.g. SMS provider `auth_token`s.
///
/// Plain `url` is deliberately excluded: generic webhook URLs are joined
/// against `webhook_failures.url` for delivery statistics.
pub const SENSITIVE_CONFIG_KEYS: &[&str] = &[
    "access_token",
    "api_key",
    "api_secret",
    "auth_token",
    "bot_token",
    "client_secret",
    "password",
    "routing_key",
    "secret",
    "signing_secret",
    "token",
    "webhook_url",
];

#[derive(Debug)]
pub enum SecretError {
    /// `INTEGRATION_ENCRYPTION_KEY` is not set.
    NotConfigured,
    /// Encryption or decryption failed (wrong key, corrupt envelope, ...).
    Crypto(String),
}

impl fmt::Display for SecretError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretError::NotConfigured => write!(
                f,
                "INTEGRATION_ENCRYPTION_KEY is not configured; refusing to store credentials in plaintext"
            ),
            SecretError::Crypto(e) => write!(f, "credential encryption error: {e}"),
        }
    }
}

impl std::error::Error for SecretError {}

struct Keys {
    current: [u8; 32],
    old: Option<[u8; 32]>,
    version: u32,
}

static KEYS: OnceLock<Option<Keys>> = OnceLock::new();
static REENCRYPT_RUNNING: AtomicBool = AtomicBool::new(false);

/// Install the integration keys. Call once at startup; later calls are ignored.
pub fn init_keys(current: Option<[u8; 32]>, old: Option<[u8; 32]>) {
    let keys = current.map(|current| Keys {
        current,
        old,
        // The version is informational (recorded in each envelope): bump it
        // when a rotation key is present so rotated rows are distinguishable.
        version: if old.is_some() { 2 } else { 1 },
    });
    if KEYS.set(keys).is_err() {
        warn!("integration encryption keys already initialised; ignoring re-initialisation");
    }
}

fn keys() -> Result<&'static Keys, SecretError> {
    KEYS.get()
        .and_then(Option::as_ref)
        .ok_or(SecretError::NotConfigured)
}

/// Whether credential encryption is available.
pub fn is_configured() -> bool {
    keys().is_ok()
}

/// Whether both a current and an old key are configured, i.e. re-encryption can run.
pub fn rotation_configured() -> bool {
    keys().map(|k| k.old.is_some()).unwrap_or(false)
}

/// Encrypt a credential for storage.
pub fn seal(plaintext: &str) -> Result<String, SecretError> {
    let k = keys()?;
    secrets::encrypt_secret(&k.current, k.version, plaintext).map_err(SecretError::Crypto)
}

/// Encrypt an optional credential for storage.
pub fn seal_opt(plaintext: Option<&str>) -> Result<Option<String>, SecretError> {
    plaintext.map(seal).transpose()
}

/// Decrypt a stored credential (legacy plaintext passes through unchanged).
pub fn open(stored: &str) -> Result<String, SecretError> {
    if !secrets::is_encrypted(stored) {
        return Ok(stored.to_string());
    }
    let k = keys()?;
    secrets::decrypt_secret(&k.current, k.old.as_ref(), stored).map_err(SecretError::Crypto)
}

/// Masked representation (`****last4`) of a stored credential for API responses.
/// Never returns the plaintext, even if decryption fails.
pub fn mask_stored(stored: &str) -> String {
    match open(stored) {
        Ok(plaintext) => secrets::mask_secret(&plaintext),
        Err(_) => "****".to_string(),
    }
}

/// Masked representation of an optional stored credential.
pub fn mask_stored_opt(stored: Option<&str>) -> Option<String> {
    stored.map(mask_stored)
}

fn is_sensitive_key(key: &str) -> bool {
    SENSITIVE_CONFIG_KEYS.contains(&key.to_ascii_lowercase().as_str())
}

fn map_sensitive(
    value: &Value,
    f: &mut dyn FnMut(&str) -> Result<String, SecretError>,
) -> Result<Value, SecretError> {
    Ok(match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let new_v = match v {
                    Value::String(s) if is_sensitive_key(k) => Value::String(f(s)?),
                    other => map_sensitive(other, f)?,
                };
                out.insert(k.clone(), new_v);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| map_sensitive(v, f))
                .collect::<Result<_, _>>()?,
        ),
        other => other.clone(),
    })
}

/// Encrypt every sensitive string in a channel `config` JSON object.
/// Values that are already encrypted are left untouched.
pub fn seal_config(config: &Value) -> Result<Value, SecretError> {
    map_sensitive(config, &mut |s| {
        if secrets::is_encrypted(s) {
            Ok(s.to_string())
        } else {
            seal(s)
        }
    })
}

/// Decrypt every sensitive string in a channel `config` JSON object.
pub fn open_config(config: &Value) -> Result<Value, SecretError> {
    map_sensitive(config, &mut |s| open(s))
}

/// Mask every sensitive string in a channel `config` JSON object.
pub fn mask_config(config: &Value) -> Value {
    map_sensitive(config, &mut |s| Ok(mask_stored(s))).unwrap_or_else(|_| Value::Null)
}

/// Re-encrypt one stored credential with the current key.
fn reseal(stored: &str) -> Result<String, SecretError> {
    seal(&open(stored)?)
}

/// Rewrite credential columns in batches.
///
/// With `only_plaintext = true` this is the backfill: rows not yet carrying an
/// `enc:v1:` envelope are encrypted. Otherwise every credential is decrypted
/// (current or old key) and re-encrypted with the current key.
async fn rewrite_all(pool: &PgPool, batch_size: i64, only_plaintext: bool) -> anyhow::Result<u64> {
    keys()?;
    let mut rewritten = 0u64;

    for (table, column) in CREDENTIAL_COLUMNS {
        let filter = if only_plaintext {
            format!("{column} IS NOT NULL AND {column} NOT LIKE 'enc:v1:%'")
        } else {
            format!("{column} IS NOT NULL")
        };
        // Table and column names come from the constant list above, never from input.
        let select = format!(
            "SELECT id, {column} FROM {table} WHERE {filter} AND id > $1 ORDER BY id LIMIT $2"
        );
        let update = format!("UPDATE {table} SET {column} = $1 WHERE id = $2");

        let mut last_id = Uuid::nil();
        loop {
            let rows: Vec<(Uuid, String)> = sqlx::query_as(&select)
                .bind(last_id)
                .bind(batch_size)
                .fetch_all(pool)
                .await?;
            let Some((id, _)) = rows.last() else { break };
            last_id = *id;

            for (id, stored) in rows {
                match reseal(&stored) {
                    Ok(sealed) => {
                        sqlx::query(&update).bind(&sealed).bind(id).execute(pool).await?;
                        rewritten += 1;
                    }
                    Err(e) => {
                        error!(table, column, id = %id, error = %e, "Failed to encrypt integration credential");
                        metrics::record_reencrypt_error();
                    }
                }
            }
        }
    }

    // notification_channels.config: provider credentials nested in JSONB.
    let mut last_id = Uuid::nil();
    loop {
        let rows: Vec<(Uuid, Value)> = sqlx::query_as(
            "SELECT id, config FROM notification_channels WHERE id > $1 ORDER BY id LIMIT $2",
        )
        .bind(last_id)
        .bind(batch_size)
        .fetch_all(pool)
        .await?;
        let Some((id, _)) = rows.last() else { break };
        last_id = *id;

        for (id, config) in rows {
            let result = if only_plaintext {
                seal_config(&config)
            } else {
                map_sensitive(&config, &mut |s| reseal(s))
            };
            match result {
                Ok(sealed) if sealed != config => {
                    sqlx::query("UPDATE notification_channels SET config = $1 WHERE id = $2")
                        .bind(&sealed)
                        .bind(id)
                        .execute(pool)
                        .await?;
                    rewritten += 1;
                }
                Ok(_) => {}
                Err(e) => {
                    error!(id = %id, error = %e, "Failed to encrypt notification channel credentials");
                    metrics::record_reencrypt_error();
                }
            }
        }
    }

    Ok(rewritten)
}

/// Encrypt credentials that were stored in plaintext before Issue #1162.
///
/// Idempotent; run at startup. Does nothing (with a warning) when no key is
/// configured.
pub async fn backfill_plaintext(pool: &PgPool) -> anyhow::Result<u64> {
    if !is_configured() {
        warn!("INTEGRATION_ENCRYPTION_KEY not set: integration credentials cannot be stored or backfilled");
        return Ok(0);
    }
    let n = rewrite_all(pool, 500, true).await?;
    if n > 0 {
        info!(rows = n, "Encrypted plaintext integration credentials");
    }
    Ok(n)
}

/// Whether an integration re-encryption job is currently running.
pub fn reencrypt_running() -> bool {
    REENCRYPT_RUNNING.load(Ordering::Relaxed)
}

/// Start a background job re-encrypting every integration credential with the
/// current key. Returns `false` if a job is already running.
pub fn start_reencrypt_job(pool: PgPool, batch_size: i64) -> bool {
    if REENCRYPT_RUNNING.swap(true, Ordering::AcqRel) {
        warn!("Integration credential re-encryption already running");
        return false;
    }
    tokio::spawn(async move {
        match rewrite_all(&pool, batch_size, false).await {
            Ok(n) => info!(rows = n, "Integration credential re-encryption completed"),
            Err(e) => error!(error = %e, "Integration credential re-encryption failed"),
        }
        REENCRYPT_RUNNING.store(false, Ordering::Release);
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_KEY: [u8; 32] = [0x5a; 32];

    fn init() {
        init_keys(Some(TEST_KEY), None);
    }

    #[test]
    fn seal_never_contains_plaintext() {
        init();
        let token = "xoxb-1234567890-supersecret";
        let sealed = seal(token).unwrap();
        assert!(sealed.starts_with(secrets::PREFIX));
        assert!(!sealed.contains(token));
        assert_eq!(open(&sealed).unwrap(), token);
    }

    #[test]
    fn mask_stored_hides_all_but_last_four() {
        init();
        let sealed = seal("https://discord.com/api/webhooks/1/abcdefWXYZ").unwrap();
        assert_eq!(mask_stored(&sealed), "****WXYZ");
    }

    #[test]
    fn config_sensitive_keys_are_sealed_and_others_kept() {
        init();
        let config = json!({
            "url": "https://example.com/hook",
            "provider": "twilio",
            "auth_token": "twilio-auth-token-value",
            "nested": {"api_key": "nested-api-key-value"},
        });
        let sealed = seal_config(&config).unwrap();
        let dump = sealed.to_string();
        assert!(!dump.contains("twilio-auth-token-value"));
        assert!(!dump.contains("nested-api-key-value"));
        assert_eq!(sealed["url"], "https://example.com/hook");
        assert_eq!(sealed["provider"], "twilio");
        assert_eq!(open_config(&sealed).unwrap(), config);
        // Sealing twice does not double-encrypt.
        assert_eq!(seal_config(&sealed).unwrap(), sealed);
        assert_eq!(mask_config(&sealed)["auth_token"], "****alue");
    }
}
