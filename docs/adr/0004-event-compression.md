# 0004 — Optional gzip compression for stored event data

- **Status:** Accepted
- **Date:** 2026-08-29
- **Owners:** SorobanPulse maintainers
- **Related:** Issue #610, [`migrations/20260628000004_event_data_gzip.sql`](../../migrations/20260628000004_event_data_gzip.sql)

## Context

`event_data` stores the decoded JSON payload of every indexed Soroban event and is the largest per-row column in the `events` table. At high event volume this column dominates table and index size, storage cost, and I/O for full-row reads. Compressing it can materially reduce storage footprint, but any compression scheme has to coexist with rows already written before compression was enabled, must not block reads when compression is misconfigured or fails, and must not be a one-way door — turning compression off must not strand data.

## Decision

SorobanPulse compresses `event_data` at rest using gzip, applied at the application layer rather than relying on PostgreSQL's built-in TOAST compression alone. `src/event_compression.rs` provides `compress`/`decompress` over the JSON bytes using `flate2` at the default compression level. Storage uses two columns added by `migrations/20260628000004_event_data_gzip.sql`: `event_data_compressed BYTEA` and `compression_algo TEXT`, added alongside the existing plain `event_data` column rather than replacing it.

Compression is applied at ingest time in the indexer (`src/indexer.rs`) when `config.event_compression_enabled` is set: the plain `event_data` is still computed and size-checked against `max_event_data_bytes` first, and only then is the compressed form additionally computed and stored, with a metric (`record_compression_ratio`) recording the size reduction achieved. Existing rows are migrated out-of-band by `migrate_existing_events`, which batches over rows where `event_data_compressed IS NULL` (an index, `idx_events_uncompressed`, exists specifically to make that scan cheap) rather than compressing everything in a single blocking pass.

Reads go through `read_event_data`, which prefers the compressed column when `compression_algo = "gzip"` and falls back to the plain `event_data` column if the compressed bytes are absent or fail to decompress — a decompression failure is logged and counted (`record_decompression_failure`) but does not fail the read. The plain column is therefore retained as the fallback of record, not deleted once compression is enabled.

## Alternatives considered

### No compression

Simplest option and avoids any CPU cost or fallback logic, but leaves `event_data` as the dominant contributor to storage growth with no mitigation. Rejected given the storage and I/O cost at scale that motivated Issue #610.

### zstd instead of gzip

zstd generally offers a better compression ratio and speed trade-off than gzip. Rejected for this iteration in favor of gzip via `flate2`, which was already a dependency-compatible, well-understood choice requiring no new decoder for external tooling that may need to inspect `event_data_compressed` directly; nothing in the schema (`compression_algo TEXT`) prevents adding a `"zstd"` variant later without another migration.

### Rely on PostgreSQL TOAST compression only

PostgreSQL automatically compresses large column values via TOAST, requiring no application code. Rejected as the sole mechanism because TOAST compression ratio and behavior are less predictable and less observable (no per-row compression-ratio metric, no explicit algorithm tagging) than compressing explicitly at the application layer before storage, and application-level compression also reduces bytes transferred between the application and the database, not just on-disk size.

### Replace `event_data` outright instead of adding a parallel column

Compressing in place would avoid storing both a compressed and (for migrated rows, historically) plain copy. Rejected because it removes the fallback path: a bug in `decompress` or a corrupted compressed value would make the row's data permanently unreadable. Keeping `event_data_compressed` and `compression_algo` as additive columns next to the original lets decompression failures degrade to the plain column instead of failing the read outright, and lets compression be disabled without a rollback migration.

### Compress synchronously for all existing rows in one migration

A single migration that rewrites every row would guarantee immediate storage savings but would hold locks and run for an unbounded time on a large `events` table. Rejected in favor of the batched `migrate_existing_events` background job driven off the `idx_events_uncompressed` partial index, so migration progress is incremental and does not block ingestion.

## Consequences

Storage and I/O for `event_data` shrink in proportion to the achieved compression ratio, which is measured per write via `record_compression_ratio` rather than assumed. Because compression is additive and feature-flagged, it can be enabled, disabled, or rolled back per environment without a schema change or data loss: disabling it simply stops writing to `event_data_compressed`, and existing compressed rows remain readable through `read_event_data`.

The cost is doubled storage for rows that have both a plain and a compressed copy (older rows before the plain column is eventually dropped, if ever, and any row where compression failed and fell back), CPU overhead for compression on the write path and decompression on the read path, and an additional failure mode (decompression failure) that must degrade gracefully rather than error — which `read_event_data` does by design, at the cost of silently returning stale-looking plain data if the plain column was itself out of date relative to the compressed one (it should not be, since both are written from the same `event_data` value at insert time).

## Rollout and migration

The forward migration (`migrations/20260628000004_event_data_gzip.sql`) adds `event_data_compressed` and `compression_algo` as nullable columns plus the `idx_events_uncompressed` partial index; it has a matching down migration. New writes are compressed automatically once `event_compression_enabled` is turned on in configuration; there is no need to backfill before enabling it, since only new rows are affected until `migrate_existing_events` is run. Rollback is to stop calling `migrate_existing_events`, disable `event_compression_enabled`, and leave the compressed columns in place (or drop them via the down migration) — no data is lost because `event_data` remains the authoritative plain-text fallback for any row that was never compressed or whose compressed form fails to decode.

## References

- [`src/event_compression.rs`](../../src/event_compression.rs)
- [`src/indexer.rs`](../../src/indexer.rs) (compression call site during event insert)
- [`migrations/20260628000004_event_data_gzip.sql`](../../migrations/20260628000004_event_data_gzip.sql)
- [`migrations/20260628000004_event_data_gzip.down.sql`](../../migrations/20260628000004_event_data_gzip.down.sql)
