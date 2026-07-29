# Database Configuration Tuning (Issue #824)

This guide covers PostgreSQL *server*-level configuration (`postgresql.conf`) — memory
allocation, cache sizing, and parallelism. For the application-side SQLx connection
pool (`DB_MAX_CONNECTIONS` / `DB_MIN_CONNECTIONS`, PgBouncer, pool health monitoring),
see [Performance Tuning Guide § Connection Pool Tuning](performance-tuning.md#connection-pool-tuning)
and `src/pool_management.rs`.

## The `pg_tuning_advisor` tool

`src/db_config_advisor.rs` implements a PGTune-style advisor: given a host's memory,
CPU count, expected connection count, and storage type, it computes a starting
`postgresql.conf` configuration and explains the reasoning behind each value.

```bash
cargo run --bin pg_tuning_advisor -- \
  --memory-mb 16384 \
  --cpu-count 8 \
  --max-connections 200 \
  --ssd
```

This prints a ready-to-review config snippet, e.g.:

```
# ~25% of RAM: PostgreSQL's dedicated shared memory cache for table and index pages.
shared_buffers = 4096MB

# ~75% of RAM: tells the planner how much total caching (shared_buffers + OS page
# cache) to expect when choosing between an index scan and a sequential scan.
effective_cache_size = 12288MB
...
```

Options:

| Flag | Default | Description |
|------|---------|-------------|
| `--memory-mb` | `8192` | Total host RAM in MB |
| `--cpu-count` | `4` | Number of CPU cores |
| `--max-connections` | `100` | Expected max concurrent connections |
| `--ssd` / `--hdd` | `--ssd` | Storage type (affects `random_page_cost`, `effective_io_concurrency`) |

## Formulas used

These follow the well-established PGTune heuristics for a "Mixed" OLTP workload,
matching Soroban Pulse's read-heavy event indexing/API traffic:

| Parameter | Formula | Rationale |
|-----------|---------|-----------|
| `shared_buffers` | 25% of RAM | Dedicated cache for hot table/index pages |
| `effective_cache_size` | 75% of RAM | Planner hint for total available caching (shared_buffers + OS page cache) |
| `maintenance_work_mem` | RAM / 16, capped at 2 GB | `VACUUM`, `CREATE INDEX`, `ALTER TABLE` |
| `work_mem` | Remaining RAM / (max_connections × 3) | Per sort/hash node, sized so concurrent connections don't collectively exhaust RAM |
| `wal_buffers` | shared_buffers / 32, capped at 16 MB | WAL write buffer |
| `random_page_cost` | `1.1` (SSD) / `4.0` (HDD) | Relative cost of random vs. sequential I/O |
| `effective_io_concurrency` | `200` (SSD) / `2` (HDD) | Concurrent I/O requests the storage can serve efficiently |
| `max_worker_processes` / `max_parallel_workers` | = CPU core count | Background worker budget |
| `max_parallel_workers_per_gather` | CPU cores / 2, capped at 4 | Prevents one query from monopolizing all cores |
| `checkpoint_completion_target` | `0.9` (fixed) | Spreads checkpoint I/O to avoid write spikes |

**These are starting points, not a final answer.** A generic formula cannot know your
actual query mix, data size, or contention patterns. After applying a recommended
configuration:

1. Re-run the benchmarks in `benches/` (see [Benchmark Interpretation](performance-tuning.md#benchmark-interpretation))
   and the k6 load test in `tests/load/events.js` to confirm the change helps.
2. Watch `pg_stat_statements` and `EXPLAIN ANALYZE` output for the queries that
   actually run in production — see [Query Optimization](performance-tuning.md#query-optimization).
3. Re-tune iteratively. A configuration that's right for a 10k-event test database
   may be wrong once the `events` table holds hundreds of millions of rows.

## Applying a recommendation

`postgresql.conf` changes generally require one of:

- A config reload (`SELECT pg_reload_conf();` or `pg_ctl reload`) for parameters like
  `work_mem`, `random_page_cost`, `effective_io_concurrency`.
- A full PostgreSQL restart for parameters like `shared_buffers`, `max_connections`,
  `max_worker_processes`.

Check which category a parameter falls into before assuming a reload is sufficient:

```sql
SELECT name, context FROM pg_settings WHERE name = 'shared_buffers';
-- context = 'postmaster' means a restart is required
```

On managed PostgreSQL (RDS, Cloud SQL, etc.), apply these through the provider's
parameter group / flag mechanism rather than editing `postgresql.conf` directly —
see [Terraform modules/rds](../terraform/modules/rds) for how this project provisions
managed instances.

## Related documentation

- [Performance Tuning Guide](performance-tuning.md) — connection pooling, query
  optimization, indexing, caching, rate limiting, indexer tuning, benchmark
  interpretation.
- [Performance Regression Testing](performance-regression-testing.md) — how
  benchmark results are tracked over time and gated in CI.
- [Capacity Planning](capacity-planning.md) — forecasting growth and scaling needs.
- [Database Schema](schema.md) — table structure and index definitions.
