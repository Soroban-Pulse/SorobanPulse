# Runbook: Query Plan Cache

**Related issues:** #802, #803  
**Related files:** `src/query_plan_cache.rs`, `src/metrics.rs`, `docs/QUERY_PLAN_CACHING.md`

---

## 1. Overview

The query plan cache (`QueryPlanCache`) stores PostgreSQL EXPLAIN outputs for
frequently executed parameterized queries.  On a cache hit the service skips
the `EXPLAIN (FORMAT JSON, ANALYZE OFF)` round-trip to PostgreSQL, saving
0.5–2 ms per query at high load.

### Key metrics to watch

| Metric | Type | Purpose |
|--------|------|---------|
| `soroban_pulse_query_plan_cache_hits_total` | counter | Cumulative cache hits |
| `soroban_pulse_query_plan_cache_misses_total` | counter | Cumulative cache misses |
| `soroban_pulse_query_plan_cache_hit_ratio` | gauge | Rolling hit/(hit+miss), 0–1 |
| `soroban_pulse_query_plan_cache_evictions_total` | counter | LRU/TTL evictions |
| `soroban_pulse_query_plan_cache_entry_count` | gauge | Live entries in the cache |
| `soroban_pulse_query_plans_cached_total` | counter | Cumulative inserts |
| `soroban_pulse_query_planning_time_ms` | histogram | Planning time per query |

**Target:** `soroban_pulse_query_plan_cache_hit_ratio` ≥ 0.80 for stable
workloads.  Alert if it drops below 0.60 for more than 5 minutes.

---

## 2. Configuration Reference

All values are read from environment variables (or `config.toml`).

| Variable | Default | Description |
|----------|---------|-------------|
| — | `1000` | `max_plans` — maximum entries in the LRU cache |
| — | `3600` | `ttl_secs` — base TTL in seconds (1 hour) |

The cache is created with `QueryPlanCacheConfig::default()` in `main.rs`.
To override, edit `QueryPlanCacheConfig` construction in
`query_plan_cache::QueryPlanCache::with_defaults()`.

### Adaptive TTL thresholds

The per-entry TTL is multiplied based on request frequency:

| Frequency (`f`) | TTL multiplier | Effective TTL (default) |
|-----------------|----------------|------------------------|
| `f < 10` | 1× | 1 hour |
| `10 ≤ f < 100` | 2× | 2 hours |
| `f ≥ 100` | 4× | 4 hours |

**Recommendation:** Keep these defaults unless you observe significant cache
churn on hot queries.  If `soroban_pulse_query_plan_cache_evictions_total` is
rising faster than `soroban_pulse_query_plans_cached_total`, increase
`max_plans` first before adjusting multipliers.

---

## 3. Diagnosing Low Hit Ratios

A hit ratio below 0.60 typically means one of:

1. **Cache too small** — entries are evicted before they can be reused.
2. **Too many distinct query strings** — un-parameterized queries each get
   their own cache slot.
3. **Schema changes invalidating plans** — stale plans TTL out and are not
   immediately replaced.

### Step-by-step diagnosis

**Step 1 — Check the current ratio**

```promql
soroban_pulse_query_plan_cache_hit_ratio
```

**Step 2 — Check the eviction rate vs insert rate**

```promql
# Eviction rate (per minute)
rate(soroban_pulse_query_plan_cache_evictions_total[5m]) * 60

# Insert rate (per minute)
rate(soroban_pulse_query_plans_cached_total[5m]) * 60
```

If evictions ≈ inserts, the cache is thrashing — increase `max_plans`.

**Step 3 — Check entry count vs capacity**

```promql
soroban_pulse_query_plan_cache_entry_count
```

If this is consistently at `max_plans` (default 1000), the cache is full.

**Step 4 — Verify parameterized queries are in use**

All queries in `src/handlers.rs` should use `$1`, `$2`, etc. placeholders.
Search for literal values being interpolated:

```bash
grep -n "format!.*SELECT\|format!.*WHERE" src/handlers.rs
```

Any hits indicate un-parameterized queries that will create a separate cache
entry for every unique value.

**Step 5 — Check for recent schema migrations**

```promql
soroban_pulse_migrations_applied_total
```

A bump here means plans may have been invalidated.  Manual flush:

```rust
cache.clear().await;
```

---

## 4. Eviction Troubleshooting

### Symptoms

- `soroban_pulse_query_plan_cache_evictions_total` growing at a steady rate.
- `soroban_pulse_query_plan_cache_hit_ratio` declining over time.
- `soroban_pulse_query_plan_cache_entry_count` consistently near `max_plans`.

### Resolution steps

1. **Increase `max_plans`** — edit `QueryPlanCacheConfig::default()` in
   `src/query_plan_cache.rs`:
   ```rust
   max_plans: 5000,   // was 1000
   ```
   Restart the service to pick up the change.

2. **Verify hot queries reach the FREQ_HIGH threshold (100)** — once they do,
   their TTL becomes 4× base, reducing churn for the most common queries.

3. **Reduce distinct query count** — consolidate similar queries in
   `src/handlers.rs` to share the same parameterized form.

---

## 5. Manual Cache Flush

Flush the plan cache when:
- A major schema migration has been applied and you want fresh plans
  immediately (rather than waiting for TTL expiry).
- A query is producing unexpected plans (possible after `ANALYZE` re-computes
  statistics on a large table).

The cache is not exposed via an admin HTTP endpoint today.  To flush,
restart the service — the `warm_cache()` call at startup will re-prime the
five canonical queries automatically.

For an emergency in-process flush (future admin API or manual Rust test):

```rust
let cache: &QueryPlanCache = /* handle from AppState */;
cache.clear().await;
```

---

## 6. Performance Baseline

The following baselines are drawn from `benches/query_planning.rs` and
correspond to the five queries primed by `warm_cache()` at startup.

| Query pattern | Bench name | Typical planning time |
|---|---|---|
| Paginated list (`ORDER BY ledger DESC LIMIT $1 OFFSET $2`) | `query_planning_simple_select` | ~0.5 ms |
| Contract filter (`WHERE contract_id = $1`) | `query_planning_complex_join` | ~0.8 ms |
| Tx hash lookup (`WHERE tx_hash = $1`) | `query_cache_hashmap_lookup` | ~0.7 ms |
| Ledger range (`WHERE ledger >= $1 AND ledger <= $2`) | `query_planning_parameterized` | ~1.0 ms |
| Exact count (`SELECT COUNT(*)`) | `query_explain_parsing` | ~0.3 ms |

Run benchmarks to detect regressions after schema changes:

```bash
cargo bench --bench query_planning
```

A planning-time increase > 50% for a previously warm query warrants investigation
(possible plan regression after `ANALYZE` updated statistics).

---

## See Also

- `docs/QUERY_PLAN_CACHING.md` — feature overview and PromQL snippets
- `docs/runbooks/db-pool-exhaustion.md` — pool exhaustion affects EXPLAIN latency
- `src/query_plan_cache.rs` — full implementation
