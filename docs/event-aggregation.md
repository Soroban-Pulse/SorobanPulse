# Event Aggregation

SorobanPulse can aggregate Soroban contract events in real time, grouping them
into time windows and computing statistics over arbitrary numeric fields.  This
document describes the data model, the API, and the operational concerns for
the aggregation subsystem (issue #934).

---

## Overview

The aggregation pipeline runs alongside the main indexer.  When a subscription
has one or more **aggregation rules** attached to it, each indexed event is
eligible for inclusion in a window.  At the end of every window the aggregator
evaluates the configured operations and stores the results in
`aggregation_results` and, for group-by queries, in `group_metrics`.

```
Events
  │
  ▼
Aggregation Rule
  ├─ Window Type  (Tumbling / Sliding / Session)
  ├─ Field Selectors  (paths + operations)
  ├─ Group By  (optional field path)
  └─ Filter Condition  (optional JSONPath predicate)
        │
        ▼
  AggregationResult  ──►  aggregation_results table
  GroupMetrics       ──►  group_metrics table
```

---

## Window Types

| Type       | Description |
|------------|-------------|
| `tumbling` | Fixed-size, non-overlapping windows.  Every event belongs to exactly one window. |
| `sliding`  | Fixed-size windows that advance by a configurable slide interval.  Events can appear in multiple windows. |
| `session`  | Activity-driven windows that close after a configurable gap of inactivity. |

### Configuration

```json
{
  "window_type": "tumbling",
  "window_size_secs": 3600,
  "slide_interval_secs": null
}
```

For sliding windows `slide_interval_secs` must be ≥ 1 and < `window_size_secs`.

---

## Field Selectors and Operations

A **field selector** picks a value from the event JSON and applies an
aggregation operation.

```json
{
  "path": "/event_data/amount",
  "operation": "sum",
  "alias": "total_transferred"
}
```

### Supported operations

| Operation      | Description |
|----------------|-------------|
| `count`        | Number of events in the window (ignores `path`). |
| `sum`          | Sum of all numeric values at `path`. |
| `avg`          | Arithmetic mean of numeric values at `path`. |
| `min`          | Minimum numeric value at `path`. |
| `max`          | Maximum numeric value at `path`. |
| `distinct_count` | Number of unique serialised values at `path`. |

---

## Group By

Adding a `group_by` clause causes the aggregator to partition the window by the
distinct values of the specified field.  Results are stored per group in the
`group_metrics` table.

```json
{
  "group_by": [{ "field": "contract_id" }]
}
```

### `GroupConfiguration` struct

| Field | Type | Description |
|-------|------|-------------|
| `group_key` | `String` | JSON field path used for grouping (e.g. `"contract_id"`). |
| `aggregation_ops` | `Vec<AggregationOp>` | Operations applied to each group. |
| `window_type` | `WindowType` | Window type for this configuration. |

### `GroupMetrics` struct

| Field | Type | Description |
|-------|------|-------------|
| `group_key` | `String` | Serialised group-by key value. |
| `rule_id` | `Uuid` | The aggregation rule. |
| `subscription_id` | `Uuid` | The owning subscription. |
| `window_start` | `DateTime<Utc>` | Window start (inclusive). |
| `window_end` | `DateTime<Utc>` | Window end (exclusive). |
| `event_count` | `i64` | Number of events in this group+window. |
| `avg_value` | `Option<f64>` | Mean numeric value (if applicable). |
| `min_value` | `Option<f64>` | Minimum numeric value (if applicable). |
| `max_value` | `Option<f64>` | Maximum numeric value (if applicable). |
| `sum_value` | `Option<f64>` | Sum of numeric values (if applicable). |
| `distinct_count` | `Option<i64>` | Distinct value count (if applicable). |

---

## Batch Processing with `AggregationOptimizer`

The `AggregationOptimizer` reduces DB round-trips when many windows need to be
evaluated in sequence.

```rust
let mut optimizer = AggregationOptimizer::new(50); // process 50 windows per batch

optimizer.enqueue(rule_id, subscription_id, window_start, window_end);
// … enqueue more windows …

let results = optimizer.flush(&pool).await;
for result in results {
    match result {
        Ok(agg) => println!("Window OK: {} events", agg.event_count),
        Err(e)  => eprintln!("Window failed: {}", e),
    }
}
```

`batch_size` is clamped to `[1, 1000]`.  The optimizer drains its queue
entirely on each call to `flush`.

---

## Database Schema

### `aggregation_rules`

Stores the configuration for each aggregation pipeline.

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID | PK |
| `subscription_id` | UUID | FK → `subscriptions` |
| `name` | TEXT | Human-readable label |
| `window_type` | TEXT | `tumbling` / `sliding` / `session` |
| `window_size_secs` | INT | Window duration |
| `slide_interval_secs` | INT | Sliding window step (nullable) |
| `fields` | JSONB | Array of `FieldSelector` |
| `group_by` | JSONB | Array of `GroupBy` (nullable) |
| `filter_condition` | TEXT | JSONPath filter (nullable) |
| `aggregation_ops` | JSONB | Per-rule operation config (added in #934) |
| `batch_size` | INT | Optimizer batch hint (default 100) |
| `enabled` | BOOL | Whether the rule is active |

### `aggregation_results`

Stores per-window aggregation output.

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID | PK |
| `rule_id` | UUID | FK → `aggregation_rules` |
| `subscription_id` | UUID | FK → `subscriptions` |
| `window_start` | TIMESTAMPTZ | |
| `window_end` | TIMESTAMPTZ | |
| `group_values` | JSONB | Group-by values (nullable) |
| `aggregated_data` | JSONB | Field → value map |
| `event_count` | BIGINT | |

### `group_metrics`

Stores pre-computed statistics per group key + window (added in #934).

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID | PK |
| `rule_id` | UUID | FK → `aggregation_rules` |
| `subscription_id` | UUID | FK → `subscriptions` |
| `group_key` | TEXT | Serialised group value |
| `window_start` | TIMESTAMPTZ | |
| `window_end` | TIMESTAMPTZ | |
| `event_count` | BIGINT | |
| `avg_value` | DOUBLE PRECISION | Nullable |
| `min_value` | DOUBLE PRECISION | Nullable |
| `max_value` | DOUBLE PRECISION | Nullable |
| `sum_value` | DOUBLE PRECISION | Nullable |
| `distinct_count` | BIGINT | Nullable |
| `extra_metrics` | JSONB | Extension point |
| `computed_at` | TIMESTAMPTZ | |

### Indexes

| Index | Covers |
|-------|--------|
| `idx_aggregation_rules_subscription` | Subscription-based rule lookup |
| `idx_aggregation_results_rule` | Per-rule result retrieval |
| `idx_aggregation_results_window` | Time-range queries |
| `idx_group_metrics_rule_window` | Per-rule, windowed group queries |
| `idx_group_metrics_group_key` | Cross-rule group key comparisons |
| `idx_group_metrics_rule_group_time` | Rule + group + time range (primary access pattern) |

---

## API Reference (internal Rust functions)

### `create_aggregation_rule`

```rust
pub async fn create_aggregation_rule(
    pool: &PgPool,
    subscription_id: Uuid,
    req: CreateAggregationRuleRequest,
) -> Result<AggregationRuleResponse, String>
```

Creates and persists a new rule.  Returns the rule ID and a `"created"` status.

### `evaluate_aggregation_window`

```rust
pub async fn evaluate_aggregation_window(
    pool: &PgPool,
    rule_id: Uuid,
    subscription_id: Uuid,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<AggregationResult, String>
```

Fetches events for the window, applies field selectors and stores the result.

### `compute_group_metrics`

```rust
pub fn compute_group_metrics(
    rule_id: Uuid,
    subscription_id: Uuid,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    events: &[(Uuid, serde_json::Value)],
    config: &GroupConfiguration,
) -> Vec<GroupMetrics>
```

Pure (no DB I/O) — computes per-group statistics over a slice of events.
Persist the result with `save_group_metrics`.

### `save_group_metrics`

```rust
pub async fn save_group_metrics(
    pool: &PgPool,
    metrics: &[GroupMetrics],
) -> Result<(), String>
```

### `get_group_statistics`

```rust
pub async fn get_group_statistics(
    pool: &PgPool,
    rule_id: Uuid,
    group_key: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<GroupMetrics>, String>
```

Retrieves stored group metrics, optionally filtered by group key and time range.
Results are ordered by `window_start DESC`.

### `get_aggregation_results`

```rust
pub async fn get_aggregation_results(
    pool: &PgPool,
    rule_id: Uuid,
    limit: i64,
) -> Result<Vec<AggregationResult>, String>
```

---

## Operational Notes

- **Deduplication**: `evaluate_aggregation_window` stores results without a
  uniqueness constraint on `(rule_id, window_start, window_end)`.  Callers are
  responsible for ensuring windows are not evaluated twice (the indexer task
  does this via its own state tracking).

- **Filter language**: `filter_condition` is treated as a JSON Pointer.  The
  event is included if the pointer resolves to a truthy value.  Full JSONPath
  support may be added in a future release.

- **Performance**: The `AggregationOptimizer` batches DB writes.  For high-
  volume subscriptions, increase `batch_size` to reduce per-window overhead at
  the cost of higher memory usage per flush cycle.

- **Retention**: Aggregation results and group metrics are retained indefinitely
  by default.  Apply the same retention policy as for raw events (see
  [data-retention.md](data-retention.md)) or add a separate pruning task.
