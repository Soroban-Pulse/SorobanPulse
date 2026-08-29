# Troubleshooting and Debugging Guide

A reference for diagnosing and resolving common issues in Soroban Pulse.

Not sure which section applies? Start at the [Troubleshooting Decision Tree](troubleshooting-guide.md) for a symptom-first path into the sections below.

## Table of Contents

- [Common Errors and Solutions](#common-errors-and-solutions)
- [Logging Configuration](#logging-configuration)
- [Performance Debugging](#performance-debugging)
- [Database Tuning](#database-tuning)
- [Indexer Lag Troubleshooting](#indexer-lag-troubleshooting)
- [Metrics Interpretation](#metrics-interpretation)
- [Contact and Support](#contact-and-support)

---

## Common Errors and Solutions

### No log output after `cargo run`

**Cause**: `RUST_LOG` is not set. Without it, the tracing subscriber produces no output.

**Fix**:
```bash
export RUST_LOG=info
cargo run
```

Or add it to your `.env` file (the `.env.example` template already includes it):
```
RUST_LOG=info
```

---

### `DATABASE_URL` connection refused

**Symptom**: Service panics at startup with `error connecting to database`.

**Causes**:
- PostgreSQL is not running
- `DATABASE_URL` points to the wrong host/port
- Credentials are wrong

**Fix**:
```bash
# Verify PostgreSQL is reachable
psql $DATABASE_URL -c "SELECT 1;"

# Start PostgreSQL if using Docker Compose
make docker-up

# Check your .env
cat .env | grep DATABASE_URL
```

---

### `ADMIN_API_KEY` returns 401 or 403

**Symptom**: Requests to `/v1/admin/*` endpoints are rejected.

| Status | Cause |
|--------|-------|
| `401 Unauthorized` | No key was sent |
| `403 Forbidden` | A regular `API_KEY` was sent instead of `ADMIN_API_KEY` |

**Fix**: Send the admin key via `Authorization: Bearer <ADMIN_API_KEY>` or `X-Api-Key: <ADMIN_API_KEY>`. The admin key is configured separately from `API_KEY`.

---

### Migrations fail on startup

**Symptom**: `error running migrations` in logs.

**Causes**:
- Database user lacks `CREATE TABLE` / `ALTER TABLE` privileges
- A previous migration left the schema in a partial state

**Fix**:
```bash
# Check pending migrations manually
psql $DATABASE_URL -c "SELECT * FROM _sqlx_migrations ORDER BY version;"

# Grant privileges (as superuser)
psql $DATABASE_URL -c "GRANT ALL PRIVILEGES ON SCHEMA public TO <your_user>;"
```

---

### Indexer not processing events / stuck

**Symptom**: `soroban_pulse_indexer_current_ledger` is not advancing. Logs show no activity.

**Causes**:
- Another replica holds the advisory lock (normal in multi-replica setups)
- RPC endpoint is unreachable
- `START_LEDGER` is set to a ledger that no longer exists in the RPC history window

**Fix**:
```bash
# Check if this replica holds the lock
psql $DATABASE_URL -c "SELECT * FROM pg_locks WHERE locktype = 'advisory';"

# Verify RPC connectivity
curl -s $STELLAR_RPC_URL/health | jq .

# Check indexer logs
RUST_LOG=debug cargo run 2>&1 | grep -i "indexer\|rpc\|lock"
```

---

### API returns stale or empty data

**Symptom**: `GET /v1/events` returns an empty array even though events exist on-chain.

**Causes**:
- Indexer lag — the indexer has not caught up yet
- `START_LEDGER` is set to a future ledger
- The contract ID in the request does not match the indexed data

**Fix**:
```bash
# Check current indexer lag
curl http://localhost:3000/metrics | grep soroban_pulse_indexer_lag_ledgers

# Confirm events exist for the contract
psql $DATABASE_URL -c "SELECT COUNT(*) FROM events WHERE contract_id = '<id>';"
```

---

### Rate limit rejections (429)

**Symptom**: Clients receive `429 Too Many Requests`.

**Fix**: Either raise `RATE_LIMIT_PER_MINUTE` in `.env`, or set it to `0` to disable rate limiting for development:
```bash
RATE_LIMIT_PER_MINUTE=0
```

---

### SSE stream disconnects immediately

**Symptom**: `GET /v1/events/stream` closes after a few seconds.

**Causes**:
- Reverse proxy (nginx, ALB) is applying a response timeout
- `SSE_KEEPALIVE_SECS` is set higher than the proxy timeout

**Fix**: Increase the proxy timeout to at least 60 seconds, or reduce `SSE_KEEPALIVE_SECS` below the proxy timeout. Example for nginx:
```nginx
proxy_read_timeout 60s;
proxy_send_timeout 60s;
```

---

## Logging Configuration

### Verbosity levels

Set `RUST_LOG` to control what gets emitted:

| Value | When to use |
|-------|-------------|
| `error` | Production — only failures |
| `warn` | Production — failures + warnings |
| `info` | Default — normal operational events |
| `debug` | Investigating issues — includes query details |
| `trace` | Deep debugging — very verbose, avoid in production |

You can also target a single module:
```bash
# Debug only the indexer
RUST_LOG=soroban_pulse::indexer=debug,info cargo run

# Trace the request handlers
RUST_LOG=soroban_pulse::handlers=trace,info cargo run
```

### Structured JSON logging

Set `RUST_LOG_FORMAT=json` for log aggregation tools (Datadog, Elastic, Splunk):
```bash
RUST_LOG_FORMAT=json cargo run
```

Example output:
```json
{
  "timestamp": "2026-03-14T00:00:00Z",
  "level": "INFO",
  "message": "Event indexed",
  "contract_id": "CABC...",
  "ledger": 1234567,
  "target": "soroban_pulse::indexer"
}
```

### Key log fields

| Field | Description |
|-------|-------------|
| `contract_id` | Soroban contract identifier |
| `ledger` | Ledger sequence number |
| `tx_hash` | Transaction hash |
| `error` | Error message (always `error`, never `err` or `msg`) |
| `correlation_id` | Request trace identifier |
| `attempt` | Retry attempt number |

See [docs/logging.md](logging.md) for the full structured logging convention.

### Filtering logs in production

Suppress noisy modules while keeping application-level info:
```bash
RUST_LOG=soroban_pulse=info,sqlx=warn,hyper=warn cargo run
```

---

## Performance Debugging

### Identify slow HTTP endpoints

Check the `soroban_pulse_http_request_duration_seconds` histogram in Prometheus:
```promql
# p99 latency per route
histogram_quantile(0.99,
  sum(rate(soroban_pulse_http_request_duration_seconds_bucket[5m])) by (le, route, method)
)
```

### Identify slow database queries

Enable `SLOW_QUERY_THRESHOLD_MS` to log queries that exceed a duration threshold:
```bash
SLOW_QUERY_THRESHOLD_MS=200
```

Queries above this threshold are logged at `WARN` level and counted in metrics. You can also query PostgreSQL directly:
```sql
SELECT query, mean_exec_time::int AS mean_ms, calls, max_exec_time::int AS max_ms
FROM pg_stat_statements
WHERE mean_exec_time > 100
ORDER BY mean_exec_time DESC
LIMIT 20;
```

### Profile with flamegraph

```bash
cargo install flamegraph
sudo cargo flamegraph --bin soroban-pulse
# open flamegraph.svg in a browser
```

### Run micro-benchmarks

```bash
# Pagination benchmarks
cargo bench --bench pagination

# Database query benchmarks (requires DATABASE_URL)
cargo bench --bench db_queries

# Compression benchmarks
cargo bench --bench compression
```

Results are written to `target/criterion/`. Compare before and after a change to detect regressions.

### Diagnose memory growth

The `soroban_pulse_process_memory_bytes` metric tracks RSS. If it grows without bound:

1. Check for long-lived SSE connections accumulating in memory (`soroban_pulse_sse_active_connections`)
2. Look for queries fetching large unbounded result sets
3. Run with `RUST_LOG=debug` and watch for `channel lagged` warnings in the SSE ring buffer

---

## Database Tuning

### Connection pool

| Variable | Description | Recommended starting point |
|----------|-------------|---------------------------|
| `DB_MAX_CONNECTIONS` | Max pool size | `10` for single instance; raise to `20–50` under load |
| `DB_MIN_CONNECTIONS` | Min idle connections | `1–2`; raise to reduce cold-start latency |
| `HEALTH_CHECK_TIMEOUT_MS` | Timeout for health check ping | `2000` |

Signs the pool is exhausted: `soroban_pulse_db_pool_size` == `soroban_pulse_db_pool_max` and latency spikes. See [docs/runbooks/db-pool-exhaustion.md](runbooks/db-pool-exhaustion.md).

### Key indexes

The migration files in `migrations/` create all required indexes. If you suspect a missing index:
```sql
-- Check index usage
SELECT schemaname, tablename, indexname, idx_scan, idx_tup_read
FROM pg_stat_user_indexes
ORDER BY idx_scan ASC;

-- Unused indexes (candidates for removal)
SELECT * FROM pg_stat_user_indexes WHERE idx_scan = 0;
```

### Table statistics

Stale statistics cause the query planner to make poor choices. Run after bulk loads:
```sql
ANALYZE events;
ANALYZE subscriptions;
```

### Vacuuming

After high-volume deletes or updates, dead tuples can bloat tables:
```sql
-- Check for bloat
SELECT relname, n_dead_tup, n_live_tup, last_autovacuum
FROM pg_stat_user_tables
ORDER BY n_dead_tup DESC;

-- Manual vacuum if autovacuum is behind
VACUUM ANALYZE events;
```

### PgBouncer (connection pooling middleware)

For high-concurrency deployments, run PgBouncer in front of PostgreSQL. Set `DB_MAX_CONNECTIONS` to the PgBouncer pool size (not the raw Postgres `max_connections`).

---

## Indexer Lag Troubleshooting

The indexer lag is the difference between the latest ledger on the Stellar network and the ledger currently being processed. It is exposed as `soroban_pulse_indexer_lag_ledgers`.

### Thresholds

| Level | Lag | Action |
|-------|-----|--------|
| Normal | < 100 ledgers | No action needed |
| Warning | 100–500 ledgers | Investigate |
| Critical | > 500 ledgers | Immediate attention |

The warning threshold is controlled by `INDEXER_LAG_WARN_THRESHOLD` (default: `100`).

### Step-by-step diagnosis

**1. Is the indexer running?**
```bash
curl http://localhost:3000/healthz/ready | jq .
# Expected: {"status":"ok","db":"ok","indexer":"ok"}
```

**2. Is this replica the leader?**
```bash
curl http://localhost:3000/metrics | grep soroban_pulse_indexer_is_leader
# 1 = active, 0 = standby
```

**3. Check RPC connectivity:**
```bash
curl -s $STELLAR_RPC_URL/health | jq .
```

**4. Check for database slowdowns:**
```bash
psql $DATABASE_URL -c "
  SELECT query, mean_exec_time::int AS mean_ms, calls
  FROM pg_stat_statements
  WHERE query ILIKE '%events%'
  ORDER BY mean_exec_time DESC LIMIT 10;"
```

**5. Check resource pressure:**
```bash
# CPU and memory of the process
top -p $(pgrep soroban-pulse)
```

### Multi-replica advisory lock

In multi-replica deployments, only one replica indexes at a time. If the leader crashes, a standby takes over within `INDEXER_LOCK_RETRY_SECS` (default: 30 seconds). To verify:
```sql
SELECT * FROM pg_locks WHERE locktype = 'advisory';
```

For the full runbook see [docs/runbooks/indexer-lag.md](runbooks/indexer-lag.md).

---

## Metrics Interpretation

All metrics are exposed at `GET /metrics` in Prometheus format.

### Indexer health

| Metric | Meaning |
|--------|---------|
| `soroban_pulse_indexer_is_leader` | `1` = this replica is the active indexer |
| `soroban_pulse_indexer_lag_ledgers` | Ledgers behind the network tip |
| `soroban_pulse_indexer_current_ledger` | Last ledger processed |
| `soroban_pulse_indexer_latest_ledger` | Network tip ledger |
| `soroban_pulse_events_indexed_total` | Cumulative events ingested |
| `soroban_pulse_rpc_errors_total` | Cumulative RPC failures |

**Healthy state**: `lag_ledgers` < 100, `is_leader` == 1 on exactly one replica, `rpc_errors_total` rate near zero.

### API performance

| Metric | Meaning |
|--------|---------|
| `soroban_pulse_http_request_duration_seconds` | Histogram of request durations by route/method/status |
| `soroban_pulse_rate_limit_rejected_total` | Requests rejected by rate limiter (429s) |
| `soroban_pulse_sse_active_connections` | Currently open SSE connections |

**Target SLOs**: p99 latency on `GET /v1/events` < 200 ms at 100 req/s. Error rate < 1%.

### Database health

| Metric | Meaning |
|--------|---------|
| `soroban_pulse_db_pool_size` | Open connections right now |
| `soroban_pulse_db_pool_idle` | Idle connections |
| `soroban_pulse_db_pool_max` | Configured max (`DB_MAX_CONNECTIONS`) |

**Alert**: If `pool_size` / `pool_max` > 0.9 consistently, the pool is near exhaustion.

### Notification delivery

| Metric | Meaning |
|--------|---------|
| `soroban_pulse_webhook_failures_total` | Webhooks exhausted all retries |
| `soroban_pulse_email_failures_total` | Email delivery failures |

A non-zero and rising rate here means subscribers' endpoints are down or misconfigured. See [docs/runbooks/webhook-failures.md](runbooks/webhook-failures.md).

### Memory

| Metric | Meaning |
|--------|---------|
| `soroban_pulse_process_memory_bytes` | RSS memory (Linux only, updated every 30 s) |

The `PodMemoryNearLimit` alert (defined in `docs/alerts.yml`) fires when this exceeds 90% of the pod memory limit.

### Using the Grafana dashboard

Import `docs/grafana-dashboard.json` into Grafana (Dashboards → Import → Upload JSON file) to get a pre-built view covering all of the above metrics with alert thresholds. See [README.md](../README.md#grafana-dashboard) for import instructions.

---

## Contact and Support

### Self-service resources

| Resource | Location |
|----------|----------|
| Architecture overview | [docs/architecture.md](architecture.md) |
| API reference (Swagger UI) | `GET /docs` on a running instance |
| OpenAPI spec | `GET /openapi.json` or [docs/openapi.json](openapi.json) |
| Alert definitions | [docs/alerts.yml](alerts.yml) |
| Grafana dashboard | [docs/grafana-dashboard.json](grafana-dashboard.json) |
| Deployment guide | [docs/deployment.md](deployment.md) |
| Operator runbook | [docs/runbooks/operator-runbook.md](runbooks/operator-runbook.md) |

### Runbooks

| Runbook | Link |
|---------|------|
| Indexer lag | [docs/runbooks/indexer-lag.md](runbooks/indexer-lag.md) |
| DB pool exhaustion | [docs/runbooks/db-pool-exhaustion.md](runbooks/db-pool-exhaustion.md) |
| RPC errors | [docs/runbooks/rpc-errors.md](runbooks/rpc-errors.md) |
| Webhook failures | [docs/runbooks/webhook-failures.md](runbooks/webhook-failures.md) |
| SSE connections | [docs/runbooks/sse-connections.md](runbooks/sse-connections.md) |
| Notifications | [docs/runbooks/notifications.md](runbooks/notifications.md) |

### Filing a bug report

Use the GitHub issue template at `.github/ISSUE_TEMPLATE/bug_report.md`. Include:

1. The version or commit SHA of Soroban Pulse you are running
2. Relevant log output (set `RUST_LOG=debug` before reproducing)
3. The exact request or operation that triggered the issue
4. Environment details (Docker, Kubernetes, bare metal; single-replica or multi-replica)

### Contributing a fix

See [CONTRIBUTING.md](../CONTRIBUTING.md) for branch naming, commit conventions, and the PR process.
