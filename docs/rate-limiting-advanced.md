# Advanced per-API-key rate limiting (Issue #941)

Sliding-window quotas per API key across four tiers: minute, hour, day,
and a rolling 30-day month. Implemented in `src/rate_limiter.rs`, enforced
by `src/middleware/rate_limit.rs`'s `rate_limit_headers_middleware`.

## Important: this used to be enforcement-only in name

Before this change, `check_rate_limit` (the function that actually
increments counters and decides whether to allow a request) had zero
callers anywhere in the codebase. The middleware only ever called
`get_rate_limit_status` — a read-only lookup — **after** the request had
already run, purely to attach informational `X-RateLimit-*` headers. No
request was ever actually blocked, regardless of configuration.
`check_rate_limit` also had an inverted return value bug (`Ok((is_rate_limited,
status))` where the doc comment promised `Ok((is_allowed, status))`) that
would have silently reversed enforcement the moment it was wired up. Both
are fixed as part of this change; the middleware now calls
`check_rate_limit` before running the request and returns `429 Too Many
Requests` when a key is over quota.

## Configuration

| Env var | Tier |
|---|---|
| `RATE_LIMIT_KEY_PER_MINUTE` | Sliding 1-minute window |
| `RATE_LIMIT_KEY_PER_HOUR` | Sliding 1-hour window |
| `RATE_LIMIT_KEY_PER_DAY` | Sliding 1-day window |
| `RATE_LIMIT_KEY_PER_MONTH` | Sliding 30-day window |

Any subset may be set. When none are set, the middleware is a no-op (no
headers, no enforcement) — existing deployments are unaffected unless
they opt in.

## Response headers

`X-RateLimit-Limit-{Minute,Hour,Day,Month}` and
`X-RateLimit-Remaining-{Minute,Hour,Day,Month}` on every response (for
whichever tiers are configured), plus `X-RateLimit-Reset` and
`Retry-After` when a request is actually blocked (429).

## Storage

Counters live in the `rate_limit_counters` table, keyed by
`sha256(api_key)` (the raw key is never persisted) and a per-minute
`window_start` bucket; each tier's check sums buckets covering that
tier's lookback window. `cleanup_old_counters(pool, hours_to_keep)`
prunes buckets older than the longest configured window — schedule it
periodically (e.g. daily) to keep the table from growing unbounded; it
isn't run automatically by anything yet.

## What's not implemented

- **Burst allowance**: the checklist for this issue also asked for a
  configurable burst allowance (a short-lived overflow above the steady
  rate, token-bucket style). This is a meaningfully different algorithm
  from the sliding-window counters here and wasn't added — a real design
  decision (how burst interacts with each of four tiers) rather than a
  small addition, left for a follow-up.
- **Quota reset scheduling**: `cleanup_old_counters` exists but has no
  scheduler calling it automatically.

## Testing

`src/rate_limiter.rs`'s test module includes both pure-logic tests and
`#[sqlx::test]`-backed tests against a real (ephemeral, migrated)
Postgres database exercising actual enforcement: allowing requests
within a limit, blocking once exhausted, enforcing the monthly tier
independently of other tiers, and confirming different API keys have
independent quotas.
