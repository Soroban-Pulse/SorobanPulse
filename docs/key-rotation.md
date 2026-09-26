# API key rotation (Issue #939)

Key versioning with a primary/secondary key and a configurable grace
period already existed, fully implemented and tested, in
`src/zero_trust.rs`'s `ApiKeySet` — but had zero callers anywhere in the
codebase. This document covers what it does, what this change added, and
what's still needed to make it the actual live key-validation path.

## `ApiKeySet`

```rust
let mut keys = ApiKeySet::new("initial-key");
keys.rotate_with_metrics("new-key"); // old key demoted to `secondary`, kept
keys.is_valid("new-key");            // true — new primary
keys.is_valid("initial-key");        // true — still valid during grace period
keys.is_in_grace_period(3600);       // true for 1 hour after rotation
```

- `rotate(new_key)`: the current `primary` becomes `secondary`;
  `new_key` becomes `primary`; `rotated_at` is set to now. Both old and
  new keys validate via `is_valid()` until the grace period elapses (the
  caller decides how long that is — `is_in_grace_period` just reports
  elapsed time since the last rotation).
- `is_valid(key)`: constant-time comparison against both `primary` and
  `secondary`, so timing doesn't leak which key (or how much of it)
  matched.
- **Added by this change**: `rotate_with_metrics(new_key)` — identical to
  `rotate()`, plus increments `soroban_pulse_api_key_rotations_total`.
  Kept separate from `rotate()` itself so `rotate()` stays a plain,
  dependency-free, synchronously-testable state transition (see this
  module's existing test coverage, extended here to cover the metrics
  variant too).

## What's not implemented

The live API-key validation path (`src/middleware/auth.rs`'s
`AuthState.api_keys: Vec<String>` and `auth_middleware`) does not use
`ApiKeySet` at all — it's a flat list of always-valid keys with no
rotation concept. Wiring `ApiKeySet` in as the real validation mechanism
would mean:

- Changing `AuthState.api_keys` from `Vec<String>` to something keyed by
  identity → `ApiKeySet` (one key set per logical API consumer, not one
  flat list — a single global primary/secondary pair doesn't make sense
  once there's more than one API consumer).
- A storage/config format for persisting each consumer's `ApiKeySet`
  across restarts (currently nothing persists rotation state — an
  in-memory `ApiKeySet` forgets its rotation on restart).
- An admin endpoint or CLI to actually trigger a rotation.
- Audit logging for rotation events, via `src/audit_logging.rs`
  (`AuditLogEntry`/`log_audit`) — not wired up here.

This is a genuinely larger, riskier change than the pieces above — it
touches the live authentication gate for every API request — and wasn't
attempted as part of this change. What's here (the tested rotation
primitive plus its metrics hook) is the safe, complete building block;
wiring it into `auth_middleware` is deliberately left as a separate,
carefully-reviewed follow-up rather than something to get subtly wrong in
a batch alongside three other issues.
