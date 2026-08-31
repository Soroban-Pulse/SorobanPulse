# Feature Flags

The feature flags system (`src/feature_flags.rs`) supports flag evaluation
against a `FeatureFlagContext`, user/contract/IP/region targeting,
percentage-based rollout, A/B testing variant assignment, and per-flag
metrics — on top of the existing auto-rollback watcher that disables flags
when error rates spike.

## Flag Evaluation

```rust
use sorobanpulse::feature_flags::{is_feature_enabled, FeatureFlagContext};

let context = FeatureFlagContext {
    contract_id: Some("CABC123...".to_string()),
    user_id: None,
    ip_address: None,
    region: Some("us-east-1".to_string()),
};

let enabled = is_feature_enabled(&pool, "new-checkout-flow", &context).await?;
```

`is_feature_enabled` loads the flag row (`enabled`, `rollout_percentage`,
`target_contract_ids`, `target_user_ids`, `target_ips`, `target_regions`)
from the `feature_flags` table and evaluates it against the context.

## User-Based Targeting

If a flag has any targeting lists set (`target_contract_ids`,
`target_user_ids`, `target_ips`, `target_regions`), the context must match at
least one of them to be eligible at all — otherwise the flag evaluates to
`false` regardless of rollout percentage.

## Percentage Rollout

Once a context passes targeting, `compute_rollout_hash` derives a
deterministic hash from the flag name and the context's contract/user/IP
identifier, buckets it into `0..100`, and compares against
`rollout_percentage`. The same identifier always lands in the same bucket for
a given flag, so a user's flag state doesn't flicker between requests.

## A/B Testing Support

`assign_variant(flag_name, context, variants)` extends the same deterministic
hashing to assign a context to one of several named, weighted variants:

```rust
use sorobanpulse::feature_flags::{assign_variant, FlagVariant};

let variants = vec![
    FlagVariant { name: "control".into(), weight: 50 },
    FlagVariant { name: "treatment".into(), weight: 50 },
];

let variant = assign_variant("checkout-experiment", &context, &variants);
```

Weights are relative (they don't need to sum to 100) and the assignment is
stable for a given flag name + context identifier, which is what makes it
usable for A/B tests: the same user consistently sees the same variant.

## Flag Metrics

`FlagMetrics` tracks, per flag name:

- `evaluations` — total evaluation calls
- `enabled` / `disabled` — outcome split
- `enabled_rate` — derived enabled/evaluations ratio
- `variant_assignments` — counts per A/B variant name

Use `is_feature_enabled_with_metrics(pool, flag_name, context, metrics)` in
place of `is_feature_enabled` to automatically record outcomes, and call
`metrics.record_variant_assignment(flag_name, variant_name)` after
`assign_variant` to track A/B distribution. `FlagMetrics::snapshot()` returns
a `Vec<FlagMetricsSnapshot>` suitable for exposing on a metrics/dashboard
endpoint.

## Auto-Rollback (existing behavior)

`FeatureFlagWatcher` continues to poll the recent request error rate and
automatically disables any flag with `auto_rollback = TRUE` when the error
rate exceeds `DEFAULT_ROLLBACK_THRESHOLD` (5% by default), recording an audit
row and a `soroban_pulse_feature_flag_rollback` metric.
