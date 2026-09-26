# IP allow/deny list enforcement (Issue #942)

CIDR-aware IP allow/deny list middleware. Implemented as `IpDenyListRule` /
`IpAllowListRule` in `src/zero_trust.rs`, wired in by
`src/middleware/ip_access.rs`'s `ip_access_control_middleware`.

## What existed before this change

`IpDenyListRule` already existed but had two problems: it was never
called from anywhere (no middleware, no route layer — completely
unreachable), and it only supported exact-string IP matches — a `/24`
block had to be listed one address at a time, which isn't practical for
real network-level policy. Both are fixed here: real CIDR support (IPv4
and IPv6, via `IpFilterEntry`) plus an `IpAllowListRule` counterpart, and
an actual middleware layer registered in `routes.rs`.

## Configuration

| Env var | Effect |
|---|---|
| `IP_DENYLIST` | Comma-separated IPs/CIDR blocks to block |
| `IP_ALLOWLIST` | Comma-separated IPs/CIDR blocks to exclusively allow |

Accepts both IPv4 and IPv6, exact addresses or CIDR notation, e.g.
`203.0.113.5,198.51.100.0/24,2001:db8::/32`. Malformed entries are
skipped (logged nowhere currently — a config typo silently drops that
one entry rather than blocking startup or panicking).

If both are set, `IP_DENYLIST` takes precedence — it's checked first, so
an explicit block is never silently overridden by being also present on
an allow list. If neither is set, the middleware is a no-op.

Registered in the middleware stack to run *before* rate limiting (a
blocked IP shouldn't consume a rate-limit quota check).

## Response

A blocked request gets `403 Forbidden` with an `X-Access-Denied-Reason:
ip-policy` header. The specific reason (which rule, which IP) is logged
via `tracing::warn!`, not returned to the caller — matches this
codebase's existing convention elsewhere of not giving a blocked caller
enough detail to probe the policy.

## Metrics

`soroban_pulse_ip_access_blocked_total` — incremented on every block,
regardless of which list (allow or deny) caused it.

## What's not implemented

The issue's checklist also asked for: IP list management *endpoints*
(currently config/env-only — changing the list requires a redeploy, not
an API call), IP lookup caching (not needed yet — the check here is an
in-memory linear scan over a small parsed list, not a database query),
geographic IP enrichment, and IP reputation checking (both would need a
third-party data source/API this change doesn't introduce). Left as
follow-up work; what's here is a complete, real, correctly-CIDR-matching
enforcement layer for a statically-configured list.
