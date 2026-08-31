# Webhook Signing

Soroban Pulse signs every outgoing webhook delivery using HMAC-SHA256 so
subscribers can verify authenticity and integrity. This document covers the
sender-side signing implementation
([`src/webhook_signing.rs`](../src/webhook_signing.rs)); see
[`src/webhook_verification.rs`](../src/webhook_verification.rs) for the
subscriber-facing verification guide.

## Algorithm

`HMAC-SHA256(secret, "{timestamp}.{payload}")`, hex-encoded. Binding the
timestamp into the signed material (rather than sending it unsigned) prevents
an attacker from replaying an old payload with a fresh, unsigned timestamp.

## Headers

Every signed delivery carries:

| Header | Description |
|--------|-------------|
| `X-Signature-256` | `sha256=<hex digest>` |
| `X-Signature-Key-Id` | Identifier of the key used to sign |
| `X-Signature-Timestamp` | Unix timestamp (seconds) included in the signed material |
| `X-Signature-Algorithm` | Currently always `hmac-sha256` |

## Multiple Keys & Rotation

`WebhookKeyManager` holds a set of `SigningKey`s, each with an id and
active/inactive flag:

```rust
use soroban_pulse::webhook_signing::{WebhookKeyManager, SigningKey};

let keys = WebhookKeyManager::new();
keys.add_key(SigningKey::new("key-2024-01", secret_bytes));
```

To rotate:

```rust
keys.rotate(SigningKey::new("key-2024-02", new_secret_bytes));
```

`rotate` marks all existing keys inactive (so they stop being used to sign
new deliveries) but keeps them available for **verification** — giving
subscribers a grace period to switch over before you call `remove_key` to
retire the old key entirely.

## Verification Example (Rust)

```rust
manager.verify(key_id, timestamp, &raw_body, &signature_header, /* max_skew_secs */ 300)?;
```

Verification enforces a timestamp skew window (default guidance: 300s) to
mitigate replay attacks, and uses constant-time comparison (`subtle::ConstantTimeEq`)
to avoid timing side channels.

## Metrics

- `soroban_pulse_webhook_signatures_created_total{key_id}`
- `soroban_pulse_webhook_signature_verifications_total{key_id,result}`
