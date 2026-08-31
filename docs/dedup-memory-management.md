# Bloom Filter Deduplication Memory Management — Issue #996

> **Warning:** Changing `BLOOM_FILTER_CAPACITY` or `BLOOM_FILTER_FP_RATE`
> requires a service restart. The filter is seeded from the database at startup.

## Problem

The original `EventBloomFilter` had no mechanism to limit memory growth.
Over long runtimes — especially deployments with `BLOOM_FILTER_CAPACITY` tuned
for millions of events — the filter would accumulate entries indefinitely,
eventually degrading its false-positive rate toward 100% and silently increasing
unnecessary database lookups.

## Solution — Double-Buffer Rotation

Issue #996 introduces a **double-buffer rotation** strategy:

```
┌──────────────────────────────────────────────────────────────┐
│  EventBloomFilter                                            │
│                                                              │
│  current ─────────────────────────────────────────────────► │
│  │  (new inserts go here)                                    │
│  │                                                           │
│  previous ─────────────────────────────────────────────────► │
│  │  (kept after rotation for duplicate detection)            │
└──────────────────────────────────────────────────────────────┘
```

When `current` fills past `fill_ratio_threshold` (default **80%**):

1. `current` is promoted to `previous` (the old `previous` is dropped).
2. A fresh empty filter becomes `current`.
3. Lookups probe **both** filters so recently-rotated entries are still detected.
4. Metrics are updated: `soroban_pulse_bloom_filter_rotations_total++` and
   `soroban_pulse_bloom_filter_fill_ratio` resets to 0.

**Memory bound:** At most 2 × single-filter memory is used at any time.
Between rotations only 1 × single-filter memory is used.

---

## Memory Estimation

The approximate memory for a single filter is:

```
bytes ≈ capacity × -ln(fp_rate) / ln(2)² / 8
```

| Capacity | fp_rate | Single filter | Peak (during rotation) |
|----------|---------|--------------|------------------------|
| 100 000 | 0.001 | ~180 KB | ~360 KB |
| 1 000 000 | 0.001 | ~1.8 MB | ~3.6 MB |
| 10 000 000 | 0.001 | ~18 MB | ~36 MB |
| 1 000 000 | 0.01 | ~1.2 MB | ~2.4 MB |

Use `EventBloomFilter::estimate_memory_bytes(capacity, fp_rate)` to get the
exact value for your configuration.

---

## Configuration

```dotenv
# Maximum events before the filter rotates (Issue #996).
# Set to the expected number of unique events per rotation cycle.
# Lower values rotate more frequently but use less memory.
BLOOM_FILTER_CAPACITY=1000000

# Target false-positive rate (0.0–1.0).
# Lower values use more memory but produce fewer unnecessary DB lookups.
BLOOM_FILTER_FP_RATE=0.001
```

The fill-ratio threshold is hardcoded to **0.80** (80%).  This is not currently
configurable via environment variable but can be changed by calling
`EventBloomFilter::with_fill_threshold(capacity, fp_rate, threshold)` directly
in code.

---

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `soroban_pulse_bloom_filter_hits_total` | Counter | Entries found in filter (potential dedup) |
| `soroban_pulse_bloom_filter_size` | Gauge | Legacy — number of items seeded |
| `soroban_pulse_bloom_filter_fill_ratio` | Gauge | **New** — current / capacity (0–1) |
| `soroban_pulse_bloom_filter_memory_bytes` | Gauge | **New** — estimated heap usage in bytes |
| `soroban_pulse_bloom_filter_rotations_total` | Counter | **New** — total rotation events |
| `soroban_pulse_bloom_filter_memory_resets_total` | Counter | **New** — alias for rotations (memory freed) |
| `soroban_pulse_session_bloom_hits_total` | Counter | Session-level filter hits (Issue #615) |
| `soroban_pulse_session_bloom_resets_total` | Counter | Session filter resets (new ledger detected) |

### Grafana Panels to Add

```json
{
  "title": "Bloom Filter Fill Ratio",
  "type": "timeseries",
  "targets": [{ "expr": "soroban_pulse_bloom_filter_fill_ratio" }],
  "fieldConfig": { "defaults": { "max": 1, "min": 0, "unit": "percentunit" }}
}
```

### Alert Rule

```yaml
- alert: BloomFilterFillHigh
  expr: soroban_pulse_bloom_filter_fill_ratio > 0.95
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "Bloom filter nearly full"
    description: >
      Fill ratio is {{ $value | humanizePercentage }}.
      A rotation is imminent. If rotations are happening too frequently,
      increase BLOOM_FILTER_CAPACITY.
```

---

## Deduplication Layers

The deduplication pipeline has three layers, each acting as a safety net for
the one below it:

```
RPC event
    │
    ▼
┌─────────────────────────────────┐
│ 1. Session Bloom Filter          │  Reset on every new ledger.
│    (SessionBloomFilter)          │  Catches same-ledger duplicates.
└─────────────┬───────────────────┘
              │ Not seen in this ledger
              ▼
┌─────────────────────────────────┐
│ 2. Persistent Bloom Filter       │  Double-buffer, bounded memory.
│    (EventBloomFilter)            │  Checks current + previous buffer.
└─────────────┬───────────────────┘
              │ Not seen in filter
              ▼
┌─────────────────────────────────┐
│ 3. Database INSERT               │  ON CONFLICT DO NOTHING.
│    ON CONFLICT DO NOTHING        │  Authoritative dedup guard.
└─────────────────────────────────┘
```

False positives in layers 1 or 2 cause a missed insert (events may be skipped).
The DB constraint in layer 3 is the authoritative guard and prevents actual
duplicates. The bloom filter exists purely as a performance optimization.

---

## Periodic Cleanup

The rotation happens automatically as the filter fills. If you prefer a
time-based cleanup instead of a fill-ratio-based one, use the admin replay
endpoint to reset the indexer state and force a re-seed from the DB:

```bash
# Pause, then resume the indexer (triggers a re-seed on resume)
curl -X POST http://localhost:3000/v1/admin/indexer/pause \
     -H "Authorization: Bearer $ADMIN_API_KEY"

curl -X POST http://localhost:3000/v1/admin/indexer/resume \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

This is rarely necessary because the rotation strategy bounds memory without
requiring manual intervention.

---

## Testing

Unit tests in `src/bloom_filter.rs` cover:

- `rotation_triggered_when_fill_threshold_exceeded` — rotation fires at 80%
- `entries_still_found_after_rotation` — old entries detectable via `previous`
- `fill_ratio_resets_after_rotation` — fill counter resets to 0 post-rotation
- `memory_bytes_positive` — memory estimate is non-zero
- `estimate_memory_bytes_scales_with_capacity` — larger capacity → more memory
- `estimate_memory_bytes_invalid_inputs_return_zero` — edge-case safety

Run them with:

```bash
cargo test -p soroban_pulse bloom_filter
```

---

## Troubleshooting

**Q: False positive rate seems high / legitimate events are being skipped.**

Check `soroban_pulse_bloom_filter_fill_ratio`. If it's near 1.0 for extended
periods without rotation, the `BLOOM_FILTER_CAPACITY` may be too low. Increase
it so rotations happen less frequently:

```dotenv
BLOOM_FILTER_CAPACITY=5000000
```

**Q: Memory usage keeps growing after many rotations.**

Each rotation replaces `previous` with the old `current`. If memory appears to
grow, check whether the OS is slow to reclaim freed pages (common on Linux with
`jemalloc`). The `soroban_pulse_process_memory_bytes` metric tracks RSS; a slow
decrease after a rotation is normal.

**Q: The filter rotates too frequently.**

Rotations are cheap (microseconds) but each one discards the
`previous` buffer's entries, meaning events from before the last-but-one
rotation must fall through to the DB. If rotations happen more than once per
hour, increase `BLOOM_FILTER_CAPACITY`.

---

## References

- `src/bloom_filter.rs` — implementation
- `src/metrics.rs` — metric definitions
- `docs/event-deduplication.md` — overall dedup architecture
- `docs/event_deduplication_replicas.md` — cross-replica dedup
