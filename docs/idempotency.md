# Idempotency Keys / Request Deduplication

`src/idempotency.rs` implements request deduplication for client-initiated
write requests (e.g. webhook registration, subscription creation) using the
standard `Idempotency-Key` request header pattern.

This is distinct from `src/dedup.rs`, which deduplicates *on-chain events*
by content fingerprint. Idempotency keys deduplicate *API requests* so a
client's retried POST after a timeout doesn't create a duplicate resource.

## How it works

1. The client sends `Idempotency-Key: <uuid-or-similar>` on a write request.
2. The handler calls `idempotency::dedup_or_execute(store, key, ttl, || { ... })`,
   wrapping the actual side-effecting operation in the closure.
3. If a non-expired record exists for that key, the cached
   `(status_code, body)` is returned immediately and the closure never runs.
4. Otherwise the closure runs, and its result is cached under the key for
   `ttl` (default `idempotency::DEFAULT_TTL`, 24 hours).

Keys are scoped per-route via `idempotency::scoped_key(route, key)` so the
same idempotency key value cannot be replayed against a different endpoint
to retrieve an unrelated cached response.

## Key expiration

`InMemoryStore::remove_expired()` purges records whose `created_at + ttl`
has passed, and increments
`soroban_pulse_idempotency_keys_expired_total`. Call this periodically
(e.g. from a background maintenance task) to bound memory usage.

## Metrics

`soroban_pulse_idempotency_requests_total{outcome="hit"|"miss"|"stored"}`
tracks cache effectiveness — a high hit rate on a given route often
indicates a client retrying aggressively on transient errors.

## Distributed deduplication

`IdempotencyStore` is a trait so the default `InMemoryStore` (single
instance, in-process) can be swapped for a shared backend when running
multiple SorobanPulse instances behind a load balancer:

- **Postgres-backed**: a table `idempotency_keys(key PRIMARY KEY,
  status_code, body, created_at, ttl_secs)` with the same get/put/remove
  semantics as `InMemoryStore`.
- **Redis-backed**: `SET key value NX EX <ttl>` for `put`, `GET key` for
  `get`, relying on Redis's native TTL for expiration instead of
  `remove_expired()`.

Either implementation just needs to satisfy the `IdempotencyStore` trait;
call sites (`dedup_or_execute`) do not change.

## Testing

See the `tests` module in `src/idempotency.rs`: first-call execution,
cache hit on retry, expired-key re-execution, expired-entry purging,
per-route key scoping, and hit/miss counters.
