# 0003 — Layered retry, backoff, and circuit breaking for webhook delivery

- **Status:** Accepted
- **Date:** 2026-08-29
- **Owners:** SorobanPulse maintainers
- **Related:** [Webhook endpoint circuit breaker](../webhook_circuit_breaker.md), [Webhook endpoint rate limits](../webhook-endpoint-rate-limits.md), [Webhook failures runbook](../runbooks/webhook-failures.md)

## Context

Webhook subscribers are third-party HTTP endpoints that SorobanPulse does not control. They fail for reasons ranging from transient network blips to sustained outages to permanent misconfiguration (wrong URL, revoked credentials). A delivery strategy that retries too little loses events subscribers should have received; one that retries too aggressively or indefinitely wastes indexer resources, can look like a denial-of-service against a struggling subscriber, and delays detection of endpoints that will never recover without operator intervention.

Different notification channels also have different failure semantics: SMS and email have upstream providers with their own retry semantics, while webhooks are delivered directly to arbitrary subscriber infrastructure with widely varying reliability. A single fixed retry count and delay cannot serve both a transient timeout and a permanently dead endpoint well.

## Decision

Webhook delivery uses a shared, configurable retry policy (`RetryPolicy` in `src/retry_policy.rs`) rather than a hardcoded loop. `RetryPolicy::webhook_default()` is exponential backoff with 5 attempts, a 1-second initial delay, a 2x multiplier, a 10-minute (`600_000` ms) cap, and full jitter enabled — jitter is applied specifically to avoid a thundering herd when many events to the same endpoint fail and retry at the same computed delay. Other channels use different defaults from the same type (`email_default()`: single attempt, no backoff; `sms_default()`: linear backoff, 2 attempts), so the retry shape is a property of the channel, not duplicated per call site.

`deliver_with_retry_policy` in `src/webhook.rs` layers additional, independent controls around this per-attempt retry:

- **Suppression list check** — a URL on the suppression list is skipped before any network call is attempted.
- **Per-endpoint rate limiting and backoff** (`rate_limit_endpoints` table) — independent of the in-request retry policy, this tracks consecutive failures per URL across *different* events and computes its own exponential backoff (`2^failures` seconds, capped at 900s), marking an endpoint `degraded` after failures and `unhealthy` after three or more consecutive failures. When an endpoint is rate-limited or backing off, the event is queued into `webhook_retry_queue` with a future `next_retry_at` instead of being sent immediately.
- **Circuit breaker** (`src/webhook_circuit_breaker.rs`) — a per-endpoint state machine (`Closed` → `Open` → `HalfOpen`) that opens after 5 consecutive failures or a >50% failure rate, stays open for 60 seconds before probing again, and requires 3 consecutive successes in `HalfOpen` to close. This exists specifically to stop sending requests to an endpoint once it is judged failing, rather than continuing to pay the cost of the per-attempt retry policy against it.
- **Dead-letter queue** — once `RetryPolicy::execute_with_retry` exhausts `max_attempts`, the event is written to `webhook_failures` with the error and a `next_retry_at`, and `deliver_with_failover` additionally supports a configured failover URL that is attempted (with its own retry policy run) before falling back to the DLQ.

These layers are deliberately independent: the retry policy handles short-lived transient failures within a single delivery attempt; the endpoint-level backoff and circuit breaker handle sustained failure of an endpoint across many events; the DLQ is the durable record of last resort.

## Alternatives considered

### Fixed delay or no backoff between attempts

Simplest to implement, but produces synchronized retry storms against a recovering endpoint and does not distinguish a slow endpoint from a dead one. Rejected in favor of exponential backoff with jitter as the webhook default, while keeping fixed/linear strategies available in `RetryPolicy` for channels where they are appropriate (e.g., `email_default`, `sms_default`).

### Retry indefinitely without a circuit breaker

Guarantees eventual delivery if the endpoint ever recovers, but an indexer processing many events against a persistently broken endpoint would keep issuing requests (and consuming worker time) for every new event indefinitely. Rejected: `RetryPolicy::webhook_default` bounds attempts to 5 per event, and the circuit breaker independently stops new attempts against an endpoint judged unhealthy regardless of how many distinct events are queued for it.

### Circuit breaker only, no per-attempt retry

Would prevent pile-up on failing endpoints but would treat a single transient timeout the same as a sustained outage, failing events that a short retry would have delivered. Rejected because most webhook failures observed are transient (network blips, momentary subscriber overload), not permanent.

### External message broker for retry scheduling

A dedicated broker (e.g., a delayed-delivery queue service) could offload retry scheduling from the application. Rejected for the current scale: `webhook_retry_queue` and `webhook_failures` as PostgreSQL tables keep retry state in the same transactional store as the rest of the system, avoid an additional infrastructure dependency, and survive failover the same way other replicated tables do (see [0002](0002-multi-replica-indexing.md)).

## Consequences

Transient failures are retried with bounded, jittered exponential backoff instead of hammering a recovering endpoint. Sustained failures are detected and isolated per endpoint by the circuit breaker, reducing wasted requests and giving operators an explicit signal (`/v1/admin/webhook/circuit-breaker`) and a manual reset path. Failed deliveries are never silently dropped: they land in `webhook_failures` for inspection or replay.

The cost is more moving parts to reason about: a delivery can be skipped by suppression, delayed by endpoint-level backoff, rejected by the circuit breaker, retried by the in-request policy, and finally DLQ'd, each with separate configuration and separate state tables (`rate_limit_endpoints`, `webhook_retry_queue`, `webhook_failures`, and the in-memory circuit breaker state). Operators tuning webhook reliability need to know which layer is currently limiting delivery, which is why each layer is independently observable (metrics plus the admin endpoints documented in `docs/webhook_circuit_breaker.md` and `docs/webhook-endpoint-rate-limits.md`).

## Rollout and migration

Not applicable as new schema work: `rate_limit_endpoints`, `webhook_retry_queue`, and `webhook_failures` are existing tables and the circuit breaker is in-memory, keyed by endpoint URL, and rebuilds from a closed state on restart. Operators tuning failure sensitivity should adjust `CircuitBreakerConfig` (`failure_threshold`, `open_duration_secs`) and `RetryPolicy` fields per the guidance in `docs/webhook_circuit_breaker.md`; a circuit stuck open on a since-fixed endpoint can be cleared with `POST /v1/admin/webhook/circuit-breaker/{endpoint}/reset`.

## References

- [`src/webhook.rs`](../../src/webhook.rs)
- [`src/retry_policy.rs`](../../src/retry_policy.rs)
- [`src/webhook_circuit_breaker.rs`](../../src/webhook_circuit_breaker.rs)
- [Webhook endpoint circuit breaker](../webhook_circuit_breaker.md)
- [Webhook endpoint rate limits](../webhook-endpoint-rate-limits.md)
- [Webhook failures runbook](../runbooks/webhook-failures.md)
