# Serialization Caching - Issue #959

## Overview

`src/serialization_cache.rs` caches serialized JSON payloads so the same entity
is not turned into bytes over and over. A single contract event is commonly
serialized once per subscriber, again on replay, and again on export - producing
identical bytes every time.

The cache is a moka `future::Cache<String, Vec<u8>>` bounded by both entry count
and TTL, wrapped in versioned keys and lock-free counters.

## Configuration

```rust
use soroban_pulse::serialization_cache::SerializedEventCache;

// explicit
let cache = SerializedEventCache::new(10_000, 300);

// module defaults
let cache = SerializedEventCache::with_defaults();
```

| Constant | Value | Meaning |
|---|---|---|
| `DEFAULT_MAX_CAPACITY` | 10 000 | Entries held before capacity eviction |
| `DEFAULT_TTL_SECS` | 300 | Seconds a payload may be served before re-serialization |
| `DEFAULT_ENTITY_TYPE` | `event` | Entity type attributed to raw-key lookups |

## Lookup

Two entry points:

```rust
// versioned: participates in entity-type invalidation
let bytes = cache
    .get_or_serialize_entity("event", &event_id, &value, serde_json::to_vec)
    .await?;

// raw key, for callers that build their own
let bytes = cache
    .get_or_serialize(&my_key, &value, serde_json::to_vec)
    .await?;
```

Prefer `get_or_serialize_entity`. The raw-key form is kept for existing callers
and does not take part in versioned invalidation.

`peek(entity_type, id)` reads without serializing on a miss.

A fallback that returns an error propagates it and caches nothing, so a
transient serialization failure cannot be memoized.

## Invalidation strategies

The hard part of a cache is not the hit; it is knowing when the hit is wrong.
Three mechanisms cover it.

### 1. TTL and capacity

Bounds on how long a stale entry can survive and how much memory the cache can
take. Always on. Evictions fire an eviction listener that attributes the entry to
its entity type before counting it.

### 2. Explicit invalidation

```rust
cache.invalidate("event", &event_id).await;
```

Removes one entry immediately. Use when a single entity is known to have changed.

### 3. Versioning

```rust
let version = cache.invalidate_entity_type("event").await;  // one type
let version = cache.invalidate_all().await;                 // everything
```

Keys are built as:

```
v{global_version}:{entity_type}:e{entity_version}:{id}
```

Bumping either version changes the prefix, so every key built before the bump is
unreachable. This costs the same whether the entity type has ten entries or ten
thousand - which is what makes bulk invalidation cheap enough to run on every
schema change or redeploy. Walking a ten-thousand-entry cache to drop half of it
would cost more than the serializations it saves.

Orphaned entries are not leaked, only deferred: they are unreachable and age out
through TTL and capacity eviction.

`invalidate_all` also calls moka's `invalidate_all`, so the memory comes back
immediately rather than at TTL.

### `invalidate_all` versus `clear`

- `invalidate_all()` empties the cache and keeps the counters, so the effect of
  the invalidation stays visible in metrics.
- `clear()` empties the cache and resets every counter. For tests and for a
  deliberate statistics reset.

## Pre-warming

```rust
let inserted = cache.prewarm("event", entries).await;
```

Serializes and caches a batch up front. Worth running after a restart or a
version bump, where the alternative is that the first request for each hot
entity pays full serialization cost.

Entries already present are skipped, so a pre-warm pass is safe against a warm
cache and cannot evict something newer than itself. A value that fails to
serialize is skipped rather than aborting the batch - it will surface on the real
request path, where there is a caller to return the error to.

## Statistics

```rust
let stats = cache.get_metrics().await;

stats.hit_rate();                 // 0.0 - 1.0
stats.avg_serialization_time_us();
stats.estimated_time_saved_us();
```

`get_metrics` runs moka's pending maintenance first, so eviction counts and the
entry count reflect what has actually happened rather than lagging behind it.

| Field | Meaning |
|---|---|
| `cache_hits` / `cache_misses` | Lookups served from cache versus serialized |
| `total_serializations` | Real serialization passes, including pre-warm |
| `total_serialization_time_us` | Cumulative time in those passes |
| `bytes_served_from_cache` | Bytes handed back without work |
| `bytes_serialized` | Bytes produced by real passes |
| `evictions` | Entries dropped by TTL or capacity |
| `invalidations` | Deliberate invalidations, all strategies |
| `prewarmed_entries` | Entries loaded by pre-warm passes |
| `entry_count` | Live entries |
| `version` | Current global version |

### Reading effectiveness

Hit rate alone does not tell you whether the cache earns its memory. A 99% hit
rate on payloads that take a microsecond to build saves nothing worth having.
`estimated_time_saved_us()` charges each hit the mean cost of a miss, which is an
estimate - the cost of a serialization that never happened is not knowable - but
it is the figure that answers the question actually being asked.

## Metrics

| Metric | Type | Labels |
|---|---|---|
| `soroban_pulse_serialization_cache_hits_total` | counter | `entity_type` |
| `soroban_pulse_serialization_cache_misses_total` | counter | `entity_type` |
| `soroban_pulse_serialization_time_us` | histogram | `entity_type` |
| `soroban_pulse_serialization_cache_evictions_total` | counter | `entity_type` |
| `soroban_pulse_serialization_cache_invalidations_total` | counter | `entity_type`, `strategy` |
| `soroban_pulse_serialization_cache_prewarmed_total` | counter | `entity_type` |
| `soroban_pulse_serialization_cache_bytes_saved_total` | counter | `entity_type` |
| `soroban_pulse_serialization_cache_entry_count` | gauge | - |
| `soroban_pulse_serialization_cache_hit_rate` | gauge | `entity_type` |
| `soroban_pulse_serialization_cache_version` | gauge | `entity_type` |

`strategy` is `key`, `entity_type`, or `all`.

## Tuning guidance

- **Hit rate low and evictions high**: the working set does not fit. Raise
  `max_capacity` before touching TTL.
- **Hit rate low and evictions low**: entities are not being requested twice
  inside the TTL. Raising capacity will not help; the cache may not be earning
  its place for that entity type.
- **Hit rate high, `bytes_saved` low**: payloads are small. Check
  `estimated_time_saved_us()` before assuming the cache is doing useful work.
- **Version gauge climbing steadily**: something is invalidating an entity type
  in a loop. Every bump throws away a warm cache.

## Related

- Streaming those payloads to clients: `docs/streaming-optimization.md`
- Query-side batching: `docs/query-streaming.md`
