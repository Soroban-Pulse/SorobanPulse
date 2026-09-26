# Scoped API Keys

API keys should be stored hashed, scoped by capability, and optionally bound to contract IDs.

Minimum fields:

- `prefix`
- `hash`
- `scopes`
- `created_at`
- `last_used_at`
- `revoked_at`
