# Webhook Retry Policies

Webhook (and other outbound) delivery uses `RetryPolicy` (`src/retry_policy.rs`)
to control exponential backoff, jitter, and retry limits, plus `RetryMetrics`
for observability and a dashboard view of retry health.

## Exponential Backoff

`RetryStrategy::Exponential` computes `initial_backoff_ms * multiplier^(attempt - 1)`,
capped at `max_backoff_ms`. `RetryStrategy::Linear` and `RetryStrategy::Fixed`
are also available for less aggressive backoff curves (e.g. SMS, email).

## Jitter

When `use_jitter` is enabled, `RetryPolicy::apply_jitter` applies "full
jitter" — a random value between `0` and the computed backoff — so that many
clients retrying after the same failure don't all retry at the exact same
instant (the thundering herd problem).

## Configurable Retry Policies

Policies are plain, serializable config:

```rust
use sorobanpulse::retry_policy::{RetryPolicy, RetryPolicyRegistry};

let webhook_policy = RetryPolicy::webhook_default(); // 5 attempts, exponential, jittered
let email_policy = RetryPolicy::email_default();     // 1 attempt, no backoff
let sms_policy = RetryPolicy::sms_default();         // 2 attempts, linear, jittered

let mut registry = RetryPolicyRegistry::with_defaults();
registry.register("custom-integration", RetryPolicy {
    max_attempts: 8,
    initial_backoff_ms: 500,
    backoff_multiplier: 1.8,
    max_backoff_ms: 120_000,
    strategy: Some(sorobanpulse::retry_policy::RetryStrategy::Exponential),
    use_jitter: true,
});
```

`RetryPolicyRegistry` lets policies be selected by name (e.g. from per-tenant
configuration) instead of hardcoding a policy per call site.

## Max Retry Limits

`max_attempts` bounds the total number of attempts (including the first).
Once exhausted, `execute_with_retry` / `execute_with_retry_metrics` return the
last error to the caller instead of retrying indefinitely.

## Retry Metrics & Status Dashboard

`execute_with_retry_metrics(policy_name, metrics, operation)` records, per
named policy:

- `attempts` — total attempts made
- `successes` — attempts that ultimately succeeded
- `retries` — attempts that failed and were retried, plus accumulated backoff
- `exhausted` — times a policy ran out of attempts without succeeding

`RetryMetrics::dashboard_snapshot()` returns a `Vec<RetryDashboardEntry>` —
one row per policy — with derived `success_rate` and `avg_backoff_ms`, ready
to render as a retry status dashboard (e.g. a Grafana table panel backed by
a metrics endpoint that serializes this snapshot).

## Testing

See the `tests` module in `src/retry_policy.rs` for coverage of each backoff
strategy, jitter bounds, the policy registry, max-attempt enforcement, and
metrics/dashboard snapshot correctness.
