# Metrics Counter Design (Issue #993)

> **Status: implementation only, not test-verified.** This change was written
> to satisfy the issue #993 checklist without running the test suite. Treat
> `SafeCounter` and the call sites below as **unreviewed and unverified**
> until someone runs `cargo build` / `cargo test` and exercises them.

## Background

`src/metrics.rs` is the central module for recording Prometheus metrics via
the `metrics` crate's `counter!()` / `gauge!()` / `histogram!()` macros, plus
a handful of counters that are tracked manually in application state (e.g.
`push_notification::DeliveryAnalytics`). Long-running instances (indexers,
notification workers) accumulate these counts indefinitely, so any counter
using unchecked/raw integer arithmetic is a candidate for silent overflow.

## Audit of counter usage in `metrics.rs`

1. **`metrics` crate counters** (`m::counter!(...).increment(n)`) — the vast
   majority of counters in this file. Their internal storage is owned by the
   `metrics`/`metrics-exporter-prometheus` crates, not application code, so
   they are out of scope for an in-process overflow fix here. Prometheus
   counters are conventionally treated as monotonic `f64`-backed series on
   the scrape side and are expected to be reset-detected by the scraper
   (`rate()`/`increase()` handle counter resets), which is the standard
   Prometheus mitigation for this class of series.
2. **Raw `u64` arithmetic inside metrics.rs** — `update_contract_count_cache_hit_ratio`
   computed `hits + misses` directly. On a long enough run this could
   theoretically overflow (panic in debug builds, wrap in release). Fixed to
   use `saturating_add` (see below).
3. **Application-owned atomic counters outside metrics.rs** — the most
   concrete overflow risk, since these are plain `AtomicU64` fields
   incremented with `fetch_add`, which wraps silently on overflow with no
   detection. `push_notification::DeliveryAnalytics` was the clearest
   instance of this pattern and has been migrated to `SafeCounter` (below).
   Other modules with similar hand-rolled atomic counters (e.g. rate
   limiters, connection pools) were **not** touched in this pass — see
   "Follow-up" below.

## Overflow points identified

| Location | Risk | Fix |
|---|---|---|
| `metrics::update_contract_count_cache_hit_ratio` | `hits + misses` unchecked add | `saturating_add` |
| `push_notification::DeliveryAnalytics` (7 fields) | `AtomicU64::fetch_add` wraps silently | Migrated to `metrics::SafeCounter` |

## Safe counter operations

`metrics::SafeCounter` (new in this change) wraps an `AtomicU64` behind a
compare-exchange loop that uses `saturating_add` instead of wrapping
addition:

- `SafeCounter::increment(delta) -> u64` — saturates at `u64::MAX` instead of
  wrapping back toward zero.
- `SafeCounter::get() -> u64` — reads the current value.

## Overflow detection

The first `increment()` call that causes a counter to saturate (i.e. it was
below `u64::MAX` and the new value is `u64::MAX`) emits
`soroban_pulse_counter_overflow_total{counter="<name>"}`, via
`metrics::record_counter_overflow_detected`. This makes saturation an
observable, alertable event rather than a silent data-quality issue.

## Counter state metric

`metrics::record_counter_state(counter_name, value)` publishes a
`soroban_pulse_counter_state{counter="<name>"}` gauge with the counter's
current raw value, so operators can watch how close a long-running counter
is to `u64::MAX` before it would saturate. Wired into
`DeliveryAnalytics::snapshot()` for the two totals most likely to be
consulted operationally (`push_total_sent`, `push_total_failed`).

## Follow-up (explicitly out of scope for this change)

- Long-running / soak tests exercising `SafeCounter` near `u64::MAX` were
  **not** written, per the instruction this change was implemented under
  ("implement only, do not add tests"). Add them before relying on this in
  production — a `#[test]` seeding the internal `AtomicU64` near `u64::MAX`
  (e.g. via a `#[cfg(test)]` constructor) and asserting saturation +
  overflow-metric emission would be the natural first pass.
- Other modules with hand-rolled `AtomicU64` counters (rate limiters,
  connection pool stats, etc.) have not been audited/migrated to
  `SafeCounter` — this pass covered `metrics.rs` and the clearest example
  (`push_notification::DeliveryAnalytics`) called out in the issue.
