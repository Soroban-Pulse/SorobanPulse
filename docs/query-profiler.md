# Query Profiler

A profiling tool for identifying slow queries, built as an extension to
[`query_optimizer`](../src/query_optimizer.rs). Implemented in
[`src/query_profiler.rs`](../src/query_profiler.rs).

## Why

`query_optimizer` focuses on rewriting and optimizing individual queries
ahead of execution. The profiler complements it by measuring what actually
happened during execution — per-stage timing, rows examined vs. returned,
and aggregate latency percentiles across repeated executions of the same
query — so that slow queries can be identified and root-caused in
production or during load testing.

## Core types

- **`QueryProfile`** — a timing recorder for a single query execution.
  Call `record_stage(name, duration, rows_examined, rows_returned)` for each
  logical stage (parse, plan, scan, sort, ...), then `into_plan()` to get an
  `ExecutionPlan`.
- **`ExecutionPlan`** — an ordered list of `PlanStage`s with helpers for
  `total_duration()` and per-stage `selectivity()` (rows returned / rows
  examined — low selectivity signals a missing index).
- **`QueryProfiler`** — analyzes an `ExecutionPlan` to find bottlenecks and
  produce recommendations. Configurable via `bottleneck_threshold` (fraction
  of total time a stage must consume to be flagged, default 30%) and
  `low_selectivity_threshold` (default 5%).
- **`ProfilingMetrics`** — aggregates timings across many executions of the
  same query (keyed by a normalized query id), exposing `p50()`, `p95()`,
  and `slowest_by_total_time()` for ranking which queries are worth
  optimizing first.

## Usage

```rust
use soroban_pulse::query_profiler::{QueryProfile, QueryProfiler};
use std::time::Duration;

let mut profile = QueryProfile::start("SELECT * FROM events WHERE type = 'payment'");
profile.record_stage("scan", Duration::from_millis(400), 100_000, 12);
profile.record_stage("sort", Duration::from_millis(20), 12, 12);
let plan = profile.into_plan();

let profiler = QueryProfiler::new();
println!("{}", profiler.report("payments query", &plan));
```

Example report output:

```
Query profile: payments query
Total duration: 420ms
Stages:
  - scan: 400ms (examined=100000, returned=12, selectivity=0.01%)
  - sort: 20ms (examined=12, returned=12, selectivity=100.00%)
Bottlenecks:
  - stage 'scan' accounts for 95.2% of total query time
Recommendations:
  - [scan] stage 'scan' examined 100000 rows but returned only 12 (0.01% selectivity) — consider adding an index
  - [scan] stage 'scan' accounts for 95.2% of total query time — consider caching, batching, or narrowing the query for this stage
```

## Tracking slow queries over time

```rust
use soroban_pulse::query_profiler::ProfilingMetrics;

let mut metrics = ProfilingMetrics::new();
metrics.record("payments_query", plan.total_duration());
// ... more executions accumulate over time ...

for (query_id, total) in metrics.slowest_by_total_time() {
    println!("{query_id}: {total:?} cumulative, p95={:?}", metrics.p95(&query_id));
}
```

## Testing

```
cargo test query_profiler
```

Covers: total duration summation, selectivity calculation (including the
zero-rows-examined edge case), bottleneck identification, index
recommendations for low-selectivity stages, empty-plan handling, report
formatting, and `ProfilingMetrics` percentile/ranking behavior.
