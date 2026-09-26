# Multi-Tier Data Retention

This document describes the tiered data retention system implemented in
`src/retention_tiers.rs`, which complements the existing single-window
pruning logic in `src/pruner.rs` and `src/archiver.rs`.

## Overview

- **Retention policy configuration** — `RetentionPolicy` declares
  per-tier age thresholds (`hot_to_warm_after_secs`,
  `warm_to_cold_after_secs`), an optional `delete_after_secs`, and the
  `CompressionAlgorithm` used when archiving into Warm/Cold.
  `RetentionPolicy::standard()` provides sane defaults (7d hot / 30d warm /
  1y cold, then delete).
- **Hot/warm/cold storage tiers** — `StorageTier::{Hot, Warm, Cold}`
  represent where a given unit of data (a partition, batch, or record —
  modeled generically as `RetentionCandidate`) currently lives.
- **Automated archival** — `RetentionPolicy::evaluate(tier, age_secs)`
  decides the `RetentionAction` (`Retain`, `MoveTier`, or `Delete`) for a
  candidate based on its age and current tier.
- **Compression on archive** — each tier transition carries a
  `CompressionAlgorithm` (`None`/`Gzip`/`Zstd`); `RetentionEnforcer`
  estimates resulting byte size using per-algorithm compression ratios
  when applying a `MoveTier` action.
- **Retention metrics** — `RetentionMetrics` tracks records evaluated,
  moved to warm/cold, deleted, and bytes before/after compression,
  exposing `bytes_saved()` and `compression_ratio()`.
- **Retention enforcement** — `RetentionEnforcer::enforce(candidates)` (or
  `enforce_at(candidates, now)` for deterministic testing) evaluates a
  batch of candidates against the configured policy and returns a
  `RetentionEvent` per candidate describing the action taken.

## Usage

```rust
use soroban_pulse::retention_tiers::*;

let policy = RetentionPolicy::standard("events");
let mut enforcer = RetentionEnforcer::new(policy);

let candidates = vec![
    RetentionCandidate { id: "partition_2026_01".into(), tier: StorageTier::Hot, created_at_unix: 1_700_000_000, size_bytes: 5_000_000_000 },
];

let events = enforcer.enforce(&candidates);
for event in events {
    match event.action {
        RetentionAction::MoveTier { to, compression } => {
            // archive `event.candidate_id` into `to` using `compression`
        }
        RetentionAction::Delete => {
            // purge `event.candidate_id`
        }
        RetentionAction::Retain => {}
    }
}

println!("compression ratio: {:.2}", enforcer.metrics().compression_ratio());
```

## Integration points

- Run `RetentionEnforcer::enforce` on a schedule (mirroring
  `src/pruner.rs`'s job pattern) over table partitions
  (`src/partition_manager.rs`) or archived batches (`src/archiver.rs`).
- Publish `RetentionMetrics` via `src/metrics.rs` for dashboards/alerts on
  unexpected growth or archival failures.

## Testing

Unit tests in `src/retention_tiers.rs` cover policy evaluation at each
tier boundary, deletion regardless of tier once past the delete threshold,
end-to-end enforcement with metrics accumulation, and default policy
sanity checks.
