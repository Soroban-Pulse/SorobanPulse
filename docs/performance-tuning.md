# Performance Tuning Guide

Guidance on configuring Soroban Pulse for optimal throughput and latency.

## Table of Contents

- [Connection Pool Tuning](#connection-pool-tuning)
- [Query Optimization](#query-optimization)
- [Index Recommendations](#index-recommendations)
- [Caching Strategies](#caching-strategies)
- [Rate Limiting Configuration](#rate-limiting-configuration)
- [Indexer Performance Tuning](#indexer-performance-tuning)
- [Benchmark Interpretation](#benchmark-interpretation)
- [Load Testing Runbook](#load-testing-runbook)

---

## Connection Pool Tuning

The connection pool is managed by SQLx and controls how many simultaneous PostgreSQL connections Soroban Pulse holds open.

### Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DB_MAX_CONNECTIONS` | `10` | Maximum open connections |
| `DB_MIN_CONNECTIONS` | `1` | Minimum idle connections kept warm |

### Sizing formula

A rough starting point:

```
DB_MAX_CONNECTIONS = (number of CPU cores on DB host × 2) + effective_spindle_count
```

For most cloud databases (1–4 cores, SSD), values between `10` and `30` are appropriate. Avoid setting this too high — more connections than the database can efficiently serve increases contention rather than throughput.

### Signs the pool is too small

- `soroban_pulse_db_pool_size` == `soroban_pulse_db_pool_max` consistently
- HTTP latency spikes during traffic peaks
- Logs show `PoolTimedOut` or `connection timed out` errors

### Signs the pool is too large

- PostgreSQL `max_connections` is hit (`FATAL: sorry, too many clients already`)
- High memory usage on the database host (each connection uses ~5–10 MB)
- `pg_stat_activity` shows many idle connections

### PgBouncer for high concurrency

For deployments with many application replicas, use PgBouncer in transaction-pooling mode. Set `DB_MAX_CONNECTIONS` to the PgBouncer pool size (not the raw PostgreSQL `max_connections`):

```ini
# pgbouncer.ini
[pgbouncer]
pool_mode = transaction
max_client_conn = 200
default_pool_size = 20
```

```bash
# Point the app at PgBouncer
DATABASE_URL=postgres://user:pass@pgbouncer:5432/soroban_pulse
DB_MAX_CONNECTIONS=20
```

### Monitoring

```promql
# Pool utilisation (alert if > 0.9)
soroban_pulse_db_pool_size / soroban_pulse_db_pool_max

# Idle connections (should be > 0 at low load)
soroban_pulse_db_pool_idle
```

---

## Query Optimization

### Slow query logging

Enable `SLOW_QUERY_THRESHOLD_MS` to automatically log and count queries that exceed a duration threshold:

```bash
SLOW_QUERY_THRESHOLD_MS=200  # log queries taking > 200 ms
```

Slow queries appear at `WARN` level in structured logs with the `query` field populated. They are also counted in internal metrics.

### pg_stat_statements

For deeper analysis, enable `pg_stat_statements` in PostgreSQL:

```sql
-- One-time setup (requires superuser)
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- Top slow queries by mean execution time
SELECT
    left(query, 80)   AS query_snippet,
    calls,
    mean_exec_time::int AS mean_ms,
    max_exec_time::int  AS max_ms,
    total_exec_time::int AS total_ms
FROM pg_stat_statements
WHERE query NOT ILIKE '%pg_stat%'
ORDER BY mean_exec_time DESC
LIMIT 20;
```

### EXPLAIN ANALYZE

Profile individual queries before adding indexes:

```sql
-- Check the execution plan for the main events query
EXPLAIN (ANALYZE, BUFFERS, FORMAT TEXT)
SELECT * FROM events
WHERE contract_id = 'CABC...'
ORDER BY ledger DESC
LIMIT 20;
```

Look for:
- **Seq Scan** on large tables — usually means a missing index
- **Hash Join** vs **Nested Loop** — hash join is better for large result sets
- **Buffers: shared hit** — data served from cache (good); **read** — disk I/O (expensive)

### Pagination

Always use `limit` + `page` (or cursor-based) pagination. Never fetch an unbounded result set:

```bash
# Correct — page through results
GET /v1/events?page=1&limit=100

# Avoid — may return millions of rows
GET /v1/events?limit=100000
```

For very large datasets prefer `exact_count=false` (the default). This returns an approximate count via PostgreSQL statistics, saving an expensive `COUNT(*)` on every request:

```bash
GET /v1/events?exact_count=false  # fast (default)
GET /v1/events?exact_count=true   # slow — forces full table scan
```

### N+1 queries

Avoid issuing one query per item. If you are building a consumer that fans out on individual events, batch them:

```sql
-- Bad: one query per event
SELECT * FROM events WHERE id = $1;  -- × 1000

-- Good: one query for all
SELECT * FROM events WHERE id = ANY($1::uuid[]);
```

---

## Index Recommendations

All standard indexes are created by the migrations in `migrations/`. This section covers when to add custom indexes and how to verify existing ones are being used.

### Verify index usage

```sql
-- Indexes with zero scans are candidates for removal
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
ORDER BY idx_scan ASC;
```

### Core indexes on the `events` table

| Index | Columns | Supports |
|-------|---------|----------|
| Primary key | `id` | Lookup by UUID |
| `idx_events_contract_id` | `contract_id` | `GET /v1/events/{contract_id}` |
| `idx_events_ledger` | `ledger` | `from_ledger` / `to_ledger` filters |
| `idx_events_tx_hash` | `tx_hash` | `GET /v1/events/tx/{tx_hash}` |
| `idx_events_timestamp` | `timestamp` | Time-range queries |
| Composite | `(contract_id, ledger)` | Contract + ledger range |
| GIN | `event_data` | Full-text / JSONB path queries |

### Adding a custom index

If profiling reveals a specific query pattern is slow, add a targeted index via a migration:

```sql
-- Example: index on event_type for frequent type-filter queries
CREATE INDEX CONCURRENTLY idx_events_event_type ON events (event_type);
```

Always use `CONCURRENTLY` in production to avoid locking the table.

### Partial indexes

If a query pattern applies to only a fraction of rows, a partial index is smaller and faster:

```sql
-- Only index contract events (skip diagnostic/system)
CREATE INDEX CONCURRENTLY idx_events_contract_type
ON events (contract_id, ledger)
WHERE event_type = 'contract';
```

### VACUUM and ANALYZE

After heavy writes or bulk deletes, refresh statistics and reclaim dead space:

```sql
-- Update planner statistics
ANALYZE events;

-- Reclaim bloat (can run concurrently)
VACUUM ANALYZE events;
```

Autovacuum handles this automatically at low load. Check it is running:

```sql
SELECT relname, last_autovacuum, last_autoanalyze, n_dead_tup
FROM pg_stat_user_tables
WHERE relname = 'events';
```

---

## Caching Strategies

### Query result cache (`query_cache`)

Soroban Pulse includes a built-in in-memory query cache (`src/query_cache.rs`) for repeated identical requests. It is lightweight and zero-configuration — no Redis required.

The cache is most effective for:
- The same paginated `GET /v1/events` query repeated by monitoring dashboards
- High-traffic contract pages where many clients fetch the same contract

The cache is automatically invalidated when new events are indexed.

### HTTP conditional GET (ETag / Last-Modified)

The API supports conditional GET. Clients can cache responses and re-validate cheaply:

```bash
# First request — get the ETag
curl -i http://localhost:3000/v1/events
# Response headers include: ETag: "abc123"

# Subsequent request — 304 Not Modified if nothing changed
curl -H 'If-None-Match: "abc123"' http://localhost:3000/v1/events
# → 304 No Content (zero bandwidth)
```

Implement this in your consumer to reduce unnecessary data transfer and improve perceived latency.

### Response compression (gzip)

HTTP response compression is enabled by default via `tower-http`'s `CompressionLayer`. Clients that send `Accept-Encoding: gzip` receive compressed responses automatically.

Compression ratios for typical event payloads:

| Events in response | Uncompressed | Compressed | Ratio |
|--------------------|-------------|------------|-------|
| 10 | ~1.5 KB | ~0.6 KB | ~2.5× |
| 100 | ~15 KB | ~2.5 KB | ~6× |
| 1 000 | ~150 KB | ~12 KB | ~12× |

No configuration is needed. The `Accept-Encoding` header is handled transparently.

### Materialized views

For high-frequency aggregation queries, PostgreSQL materialized views provide pre-computed results. The migrations include:
- `20260428000002_matview_daily_summary.sql` — daily event counts
- `20260428000003_matview_contract_summary.sql` — per-contract statistics
- `20260530000001_mv_contract_summary.sql` — enriched contract summary

Refresh materialized views on a schedule if autovacuum is not refreshing them fast enough:

```sql
REFRESH MATERIALIZED VIEW CONCURRENTLY mv_contract_summary;
```

---

## Rate Limiting Configuration

### Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `RATE_LIMIT_PER_MINUTE` | `60` | Requests per IP per minute; `0` = unlimited |

The rate limiter uses a sliding-window token bucket per IP address. Rejected requests return `429 Too Many Requests`.

### Tuning for your workload

| Workload | Recommended setting |
|----------|-------------------|
| Development / internal | `RATE_LIMIT_PER_MINUTE=0` (disable) |
| Public API | `60–120` |
| Dashboard / monitoring | `300–600` |
| Batch export client | `0` (disable) or per-IP exemption |

### Monitoring rate limiting

```promql
# Request rejection rate (429s per second)
rate(soroban_pulse_rate_limit_rejected_total[1m])
```

If legitimate clients are being rate-limited, raise the limit. If you see a single IP driving the counter, investigate abuse.

### Rate limiting and SSE

SSE connections count as one long-lived request (not one per event). Rate limiting applies to the connection establishment, not the event delivery rate. Clients should reconnect gracefully when the stream closes.

---

## Indexer Performance Tuning

### Poll interval behaviour

The indexer polls the Soroban RPC `getEvents` method:
- Every **~5 seconds** when the ledger is current
- Every **10 seconds** after an RPC error (back-off)

These intervals are not currently configurable. If the indexer is consistently behind, the bottleneck is usually the database write path or the RPC endpoint, not the poll interval.

### Batch insert tuning

Events are inserted with `ON CONFLICT DO NOTHING`, which is safe for concurrent replicas. The bottleneck at high ledger rates is typically write throughput. To improve it:

1. **Increase `DB_MAX_CONNECTIONS`** so the indexer can pipeline inserts
2. **Tune `shared_buffers`** and `wal_buffers` in `postgresql.conf`
3. **Use faster storage** — NVMe over HDD; dedicated IOPS on cloud

### Multi-replica advisory lock

In multi-replica deployments only one replica indexes at a time (leader election via `pg_try_advisory_lock`). Standbys retry every `INDEXER_LOCK_RETRY_SECS` seconds (default: 30).

To reduce failover time after a leader crash:
```bash
INDEXER_LOCK_RETRY_SECS=10  # retry every 10 s instead of 30 s
```

Monitor which replica is the leader:
```promql
soroban_pulse_indexer_is_leader == 1
```

Exactly one replica should show `1`. Zero means the indexer is down; more than one means a split-brain scenario (requires immediate investigation).

### Indexer lag alert threshold

```bash
INDEXER_LAG_WARN_THRESHOLD=100  # warn when > 100 ledgers behind
```

Lower this during SLA-sensitive periods to catch degradation earlier.

### Indexer bloom filter

The indexer uses a Bloom filter to skip already-processed event IDs before hitting the database. The filter state is persisted in the `indexer_bloom_state` table (migration `20260527000000_indexer_bloom_state.sql`). It is maintained automatically — no tuning is needed under normal operation.

---

## Benchmark Interpretation

### Running benchmarks

```bash
# Micro-benchmarks (no database required)
cargo bench --bench pagination
cargo bench --bench compression

# Database query benchmarks (requires DATABASE_URL)
cargo bench --bench db_queries

# All benchmarks
cargo bench
```

Results are written to `target/criterion/`. Open `target/criterion/report/index.html` in a browser for charts and regression detection.

### Baseline numbers (10 000-event dataset, local PostgreSQL)

| Benchmark | Mean | p99 | Notes |
|-----------|------|-----|-------|
| `db/get_events_no_filter` | ~1.5 ms | ~2.5 ms | Page 1, no filters |
| `db/get_events_ledger_range` | ~1.8 ms | ~3.0 ms | `from_ledger` + `to_ledger` |
| `db/get_events_exact_count` | ~3.5 ms | ~6.0 ms | Forces `COUNT(*)` |
| `db/get_events_by_contract` | ~1.2 ms | ~2.0 ms | 500-event contract |

These baselines were measured on a local development machine. Your production numbers will vary with hardware and dataset size. Use them as **regression references** — a significant increase after a schema or query change warrants investigation.

### Target SLOs

| Metric | Target |
|--------|--------|
| p99 latency `GET /v1/events` | < 200 ms at 100 req/s |
| Error rate | < 1% |

Track these in the Grafana dashboard (`docs/grafana-dashboard.json`).

### Interpreting Criterion output

```
db/get_events_no_filter
  time: [1.4872 ms 1.5031 ms 1.5201 ms]
  change: [+0.0213% +0.5127% +1.0108%] (p = 0.06 > 0.05)
  No change in performance detected.
```

- The three numbers are the lower bound, mean, and upper bound of the 95% confidence interval
- If Criterion reports a **regression** (red output), compare the change against the SLO targets
- A 10% or greater increase in p95/p99 warrants investigation — see [docs/performance-regression-testing.md](performance-regression-testing.md)

### Load testing

A k6 script for `GET /v1/events` is at `tests/load/events.js`. It runs a 30-second constant-arrival-rate scenario at 100 req/s and asserts the SLOs above:

```bash
k6 run tests/load/events.js
k6 run -e BASE_URL=http://staging:3000 tests/load/events.js
```

For SSE load testing:
```bash
k6 run tests/load/sse_stream.js
```

See [README.md](../README.md#load-testing) for full details and thresholds.

---

## Related Documentation

- [Database Configuration Tuning](database-configuration-tuning.md) — PostgreSQL server-level `postgresql.conf` recommendations (memory, cache sizing, parallelism) and the `pg_tuning_advisor` CLI
- [Database Schema](schema.md) — index definitions and table structure
- [Performance Regression Testing](performance-regression-testing.md) — automated regression detection
- [Deployment Guide](deployment.md) — resource sizing and Kubernetes tuning
- [Troubleshooting Guide](troubleshooting.md) — diagnosing latency and slow queries
- [Capacity Planning](capacity-planning.md) — forecasting and scaling
- [Runbook: DB Pool Exhaustion](runbooks/db-pool-exhaustion.md)
- [Runbook: Indexer Lag](runbooks/indexer-lag.md)

---

## Load Testing Runbook

This section describes all k6 load test scenarios, how to run them locally, how to interpret results, and how the CI regression gate works.

### Prerequisites

```bash
# Install k6 (https://k6.io/docs/get-started/installation/)
# macOS
brew install k6

# Ubuntu / Debian
sudo gpg --no-default-keyring \
  --keyring /usr/share/keyrings/k6-archive-keyring.gpg \
  --keyserver hkp://keyserver.ubuntu.com:80 \
  --recv-keys C5AD17C747E3415A3642D57D77C6C491D6AC1D69
echo "deb [signed-by=/usr/share/keyrings/k6-archive-keyring.gpg] https://dl.k6.io/deb stable main" \
  | sudo tee /etc/apt/sources.list.d/k6.list
sudo apt-get update && sudo apt-get install k6

# Node.js ≥ 18 (for regression_check.js)
node --version
```

### Scenario overview

| Script | Purpose | Default duration | CI trigger |
|---|---|---|---|
| `tests/load/events.js` | Steady-state 100 req/s baseline | 30 s | Every PR + push |
| `tests/load/sse_stream.js` | SSE connections — sustained + churn | ~1 m | Every PR + push |
| `tests/load/stress.js` | Ramp 2×→10× (200→1 000 req/s) | ~18 m | Push to main |
| `tests/load/spike.js` | Instant 10× burst + recovery | ~3.5 m | Push to main |
| `tests/load/soak.js` | 24-hour sustained load | 24 h (overridable) | Weekly schedule |
| `tests/load/multi_contract.js` | Mixed real-world query patterns | ~9 m | Every PR + push |
| `tests/load/webhook_delivery.js` | Webhook pipeline under load | 5 m | Push to main |

### Running scenarios locally

```bash
# 1. Start a local Soroban Pulse instance
cargo run

# 2. Run any individual scenario
k6 run tests/load/events.js
k6 run tests/load/sse_stream.js
k6 run tests/load/stress.js
k6 run tests/load/spike.js
k6 run tests/load/multi_contract.js
k6 run tests/load/webhook_delivery.js

# 3. Soak test with shortened duration for local validation
k6 run -e SOAK_DURATION=30m tests/load/soak.js

# Override the base URL or inject an API key
k6 run -e BASE_URL=http://staging.example.com -e API_KEY=mykey tests/load/stress.js

# Save JSON summary for regression check
mkdir -p tests/load/results
k6 run --out json=tests/load/results/stress_raw.json tests/load/stress.js
```

### Regression detection

After any run that produces a JSON summary, compare against the baseline library:

```bash
# node tests/load/regression_check.js <scenario> <summary.json> [baselines.json] [db_size]
node tests/load/regression_check.js events_steady \
  tests/load/results/events_steady_raw.json \
  tests/load/baselines.json \
  1M
```

Exit codes:
- `0` — all checks pass
- `1` — one or more regressions detected (CI gate fails)
- `2` — bad arguments or missing files

**Regression thresholds** (configured in `tests/load/baselines.json`):

| Dimension | Threshold |
|---|---|
| Latency (any percentile) | > 20 % above baseline |
| Error rate | > 2 percentage points above baseline |
| Memory growth (soak) | > 50 % above baseline |

### Baseline library (`tests/load/baselines.json`)

Baselines are stored per scenario and per database size (`100k`, `1M`, `10M` events).

**When to update baselines:**
- After a deliberate performance improvement (lower latency expected).
- After a capacity change (larger machine, more DB connections).
- Never after an unintentional regression — fix the code instead.

**How to update:**

```bash
# 1. Run the scenario and confirm the new numbers are intentional
k6 run --out json=tests/load/results/events_raw.json tests/load/events.js

# 2. Edit tests/load/baselines.json, update the relevant p50/p95/p99 values
#    under scenarios.<name>.baselines.<db_size>

# 3. Commit with a message explaining why the baseline changed
git add tests/load/baselines.json
git commit -m "perf(load): update events_steady baseline after index optimisation"
```

### Interpreting results

#### Stress test

Look for the **breaking point** — the request rate at which p99 latency exceeds 1 000 ms or the error rate exceeds 5 %. The recovery section (final 2 min) should show p99 returning to within 20 % of the pre-stress baseline. A slow recovery indicates connection pool exhaustion or memory pressure.

#### Spike test

The spike scenario has three distinct phases in the output:
1. **Pre-spike baseline** (`baseline` executor) — should match `events_steady` numbers.
2. **Spike** — p99 may reach several seconds; error rate may spike briefly. The SLO allows up to 10 % errors and 5 s p99 during the burst.
3. **Recovery** (`recovery` executor) — p99 must return below 200 ms within the recovery window. A slow recovery suggests the connection pool is not releasing fast enough; increase `DB_MAX_CONNECTIONS` or tune `DB_IDLE_TIMEOUT_SECS`.

#### Soak test

Compare `p95_ms_hour1` vs `p95_ms_hour12` vs `p95_ms_hour24` from the summary. More than a 10 % increase over 24 hours indicates latency drift, likely caused by:
- Gradual memory growth (check `soroban_pulse_process_memory_bytes` in Prometheus)
- Table bloat (check `pg_stat_user_tables.n_dead_tup`)
- Index fragmentation (check `docs/index-maintenance.md`)

#### Multi-contract query test

The per-query-type p99 values (`mc_contract_latency_ms`, `mc_filter_latency_ms`, etc.) make it easy to isolate which query type regressed. A high `mc_filter_latency_ms` with low `mc_contract_latency_ms` suggests a missing or unused composite index on `(event_type, ledger, id)`.

### CI/CD integration

The workflow `.github/workflows/load-tests.yml` runs:

| Job | Triggers |
|---|---|
| `load-events-steady` | Every PR and push to main |
| `load-multi-contract` | Every PR and push to main |
| `load-stress` | Push to main + weekly schedule + manual |
| `load-spike` | Push to main + weekly schedule + manual |
| `load-soak` | Weekly schedule + manual (with `SOAK_DURATION` override) |
| `load-test-summary` | PR comment with key metrics from all completed jobs |

To run all scenarios manually:

```
GitHub → Actions → Load Tests → Run workflow
```

The `db_size` input selects which baseline row to compare against (default `1M`). The `soak_duration` input lets you run a shortened soak (e.g. `30m`) for quick validation.

### Webhook delivery setup

The `webhook_delivery.js` scenario drives the admin replay endpoint and observes `soroban_pulse_webhook_failures_total` via `/metrics`. For end-to-end delivery latency measurement, point a stub receiver at `WEBHOOK_URL` before starting the service:

```bash
# Option 1: WireMock stub
docker run -p 8080:8080 wiremock/wiremock --global-response-templating
curl -X POST http://localhost:8080/__admin/mappings \
  -d '{"request":{"method":"POST","url":"/webhook"},"response":{"status":200}}'

# Option 2: webhook.site — copy the unique URL and set it as WEBHOOK_URL
export WEBHOOK_URL="https://webhook.site/<your-uuid>"

# Start service with webhook enabled
WEBHOOK_URL=$WEBHOOK_URL WEBHOOK_SECRET=test-secret cargo run

# Run the test
k6 run -e ADMIN_API_KEY=your-admin-key \
       -e WEBHOOK_RECEIVER=http://localhost:8080/webhook \
       tests/load/webhook_delivery.js
```

### Performance budgets

| Scenario | p99 budget | Error budget |
|---|---|---|
| Steady-state (100 req/s) | 200 ms | 1 % |
| Stress peak (1 000 req/s) | 2 000 ms | 5 % |
| Spike burst | 5 000 ms | 10 % |
| Recovery (post-spike) | 200 ms | 1 % |
| Soak (24 h) | 500 ms | 1 % |
| Multi-contract queries | 400 ms | 1 % |
| Webhook delivery | 1 000 ms | 5 % |

If any CI job reports a regression, the workflow exits with a non-zero code and blocks the merge. Investigate using the artifact JSON and the Grafana dashboard (`docs/grafana-dashboard.json`) before updating baselines.
