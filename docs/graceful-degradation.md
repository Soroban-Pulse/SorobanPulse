# Graceful Degradation

In addition to graceful *shutdown* (see `src/graceful_shutdown.rs`), the
service degrades gracefully while running when a dependency becomes
unhealthy, rather than failing every request outright.

## Degradation levels

`graceful_shutdown::DegradationLevel` has four states:

- `Normal` — all dependencies healthy.
- `Degraded` — a non-critical dependency (e.g. webhook delivery, an
  optional upstream RPC) is failing; core read/write paths are unaffected.
- `ReadOnly` — the primary database is unreachable for writes; only reads
  (optionally served from cache) are accepted.
- `Unavailable` — nothing is healthy enough to serve; return 503.

## Fallback strategies

`DegradationController::apply_fallback_strategy(dependency, healthy)` is the
single entry point health checks and circuit breakers call into:

- `"database"` unhealthy → `ReadOnly` mode is enabled
  (`DegradationController::is_read_only()`).
- `"all"` unhealthy → `Unavailable`.
- any other dependency name → `Degraded`.
- `healthy = true` for any dependency resets to `Normal` and clears
  read-only mode.

## Read-only mode & cached response serving

While `is_read_only()` is true, write endpoints should reject requests (503
or 409) while read endpoints continue to serve. To keep serving reads even
if the database itself is the unhealthy dependency,
`DegradationController::cache_response(key, body)` stores the last-known-good
response body per cache key (e.g. request path), and
`serve_cached(key, max_age_ms)` returns it if it is still within the
allowed staleness window.

## Circuit breaker integration

`graceful_shutdown::on_circuit_breaker_state_change(controller, dependency,
is_open)` bridges the existing webhook circuit breaker
(`src/webhook_circuit_breaker.rs`) into the degradation controller: an open
circuit for a dependency is treated as that dependency being unhealthy.

## Metrics

Every state transition increments
`soroban_pulse_degradation_transitions_total{dependency, level}`, and the
current level is exposed as a gauge, `soroban_pulse_degradation_level`
(0=normal, 1=degraded, 2=read_only, 3=unavailable), for alerting and
dashboards.

## Testing

See the `tests` module at the bottom of `src/graceful_shutdown.rs` for
coverage of: database-failure → read-only transitions, recovery back to
normal, non-database degraded mode, full unavailability, cached response
serving, and circuit-breaker integration.
