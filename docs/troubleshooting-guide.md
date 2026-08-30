# Troubleshooting Decision Tree

A symptom-first entry point for operational issues in Soroban Pulse. Start at [What are you seeing?](#what-are-you-seeing) and follow the branch that matches; each leaf links to the detailed fix in [docs/troubleshooting.md](troubleshooting.md) or a dedicated [runbook](#runbook-index), plus what to check, how to keep it from recurring, and when to stop self-service and escalate.

This document is the *navigation* layer. [docs/troubleshooting.md](troubleshooting.md) remains the detailed reference (exact commands, error message text, config variables) — this guide exists to get you to the right section of it faster when you're not sure what's actually wrong yet.

## Table of Contents

- [What are you seeing?](#what-are-you-seeing)
- [Decision Tree](#decision-tree)
- [Prevention Strategies](#prevention-strategies)
- [Escalation Procedures](#escalation-procedures)
- [Searchable Issue Index](#searchable-issue-index)
- [Runbook Index](#runbook-index)

---

## What are you seeing?

Pick the closest match:

| Symptom | Go to |
|---|---|
| Service won't start / crashes on startup | [Branch A: Startup failures](#branch-a-startup-failures) |
| Requests return errors (4xx/5xx) or unexpected data | [Branch B: API behaving incorrectly](#branch-b-api-behaving-incorrectly) |
| Everything responds, but slowly | [Branch C: Latency and throughput](#branch-c-latency-and-throughput) |
| Events are missing, delayed, or the indexer looks stuck | [Branch D: Indexing and data freshness](#branch-d-indexing-and-data-freshness) |
| Webhooks, SSE, or email notifications aren't arriving | [Branch E: Delivery and notifications](#branch-e-delivery-and-notifications) |
| Memory or CPU usage climbing over time | [Branch F: Resource growth](#branch-f-resource-growth) |

---

## Decision Tree

### Branch A: Startup failures

```
Service fails to start
├─ Panic mentions "database" / "connecting to database"
│   → Is DATABASE_URL correct and Postgres reachable?
│       YES → docs/troubleshooting.md § DATABASE_URL connection refused
│       NO  → fix DATABASE_URL, retry
├─ "error running migrations"
│   → docs/troubleshooting.md § Migrations fail on startup
│   → Check: `psql $DATABASE_URL -c "SELECT * FROM _sqlx_migrations ORDER BY version;"`
├─ Panic during `cargo build` specifically (not at runtime)
│   → docs/onboarding.md § Why does cargo build need a database?
└─ Nothing above matches
    → Run with RUST_LOG=debug and re-attempt startup; the first ERROR line
      almost always names the failing subsystem. Escalate if not (see below).
```

**Metrics/logs to check**: startup log lines at `RUST_LOG=debug`; `GET /healthz/ready` will not respond at all if the process never came up, so absence of a response *is* the signal here, not a metric.

### Branch B: API behaving incorrectly

```
Unexpected API response
├─ 401 Unauthorized
│   → No API key sent. docs/troubleshooting.md § ADMIN_API_KEY returns 401 or 403
├─ 403 Forbidden
│   → Wrong key tier (regular key on an admin endpoint). Same section as above.
├─ 429 Too Many Requests
│   → docs/troubleshooting.md § Rate limit rejections (429)
├─ 404 on a resource you expect to exist
│   → Check contract_id / tx_hash formatting first (most common cause),
│     then confirm the indexer has actually reached that ledger — see Branch D.
├─ Empty array / stale data on GET /v1/events
│   → docs/troubleshooting.md § API returns stale or empty data
│   → Check: soroban_pulse_indexer_lag_ledgers — if non-trivial, this is really Branch D.
└─ 500 / 503
    → 503 usually means /healthz/ready is failing — check that endpoint directly.
      500 → RUST_LOG=debug and reproduce; check for a stack trace naming the failing handler.
```

**Metrics/logs to check**: `soroban_pulse_http_request_duration_seconds` (by route/status), `GET /healthz/ready`, `RUST_LOG=soroban_pulse::handlers=debug`.

**Runbook**: none dedicated — most API-behavior issues resolve via [docs/troubleshooting.md](troubleshooting.md) directly.

### Branch C: Latency and throughput

```
Requests are slow
├─ One specific route is slow, others are fine
│   → Check per-route p99: docs/troubleshooting.md § Identify slow HTTP endpoints
│   → Check for a slow query behind that route: pg_stat_statements
├─ Everything is slow, including /healthz/live
│   → Likely resource contention (CPU/memory) or DB pool exhaustion, not app logic
│   → docs/runbooks/db-pool-exhaustion.md
├─ Slow only under load / during traffic spikes
│   → docs/performance-tuning.md § Connection Pool Tuning
│   → docs/load-testing-runbook.md to reproduce and quantify
└─ Slow specifically on large result sets / bulk export
    → docs/bulk_export.md and docs/query-caching.md
```

**Metrics/logs to check**: `soroban_pulse_http_request_duration_seconds` histogram, `soroban_pulse_db_pool_size` vs. `soroban_pulse_db_pool_max`, `pg_stat_statements.mean_exec_time`.

**Runbook**: [docs/runbooks/db-pool-exhaustion.md](runbooks/db-pool-exhaustion.md)

### Branch D: Indexing and data freshness

```
Events missing or delayed
├─ soroban_pulse_indexer_lag_ledgers is climbing
│   → docs/troubleshooting.md § Indexer Lag Troubleshooting (full step-by-step)
│   → docs/runbooks/indexer-lag.md
├─ Indexer not advancing at all (lag flat, current_ledger frozen)
│   → Check which replica holds the advisory lock — is this even the leader?
│   → docs/troubleshooting.md § Indexer not processing events / stuck
├─ RPC errors climbing (soroban_pulse_rpc_errors_total)
│   → docs/runbooks/rpc-errors.md
└─ Events exist on-chain but never appear, even after lag clears
    → Confirm contract_id filter matches exactly (case-sensitive, full address)
    → docs/troubleshooting.md § API returns stale or empty data
```

**Metrics/logs to check**: `soroban_pulse_indexer_lag_ledgers`, `soroban_pulse_indexer_is_leader`, `soroban_pulse_rpc_errors_total`, `soroban_pulse_indexer_current_ledger` vs. `_latest_ledger`.

**Runbook**: [docs/runbooks/indexer-lag.md](runbooks/indexer-lag.md), [docs/runbooks/rpc-errors.md](runbooks/rpc-errors.md)

### Branch E: Delivery and notifications

```
Notifications not arriving
├─ Webhooks specifically
│   ├─ All webhooks for one endpoint failing
│   │   → Check delivery_logs for that webhook_id; is the endpoint reachable at all?
│   │   → docs/runbooks/webhook-failures.md
│   ├─ Webhooks failing intermittently
│   │   → docs/subscription-best-practices.md § Webhook retry schedule (server-side)
│   │   → Is the endpoint responding within WEBHOOK_TIMEOUT_MS?
│   └─ Signature verification failing on the receiving end
│       → docs/webhook_signing.md — confirm raw body (not re-serialized JSON) is what's hashed
├─ SSE specifically
│   ├─ Connection drops after ~30-60s
│   │   → docs/troubleshooting.md § SSE stream disconnects immediately (proxy timeout)
│   └─ Connection stays open but events stop arriving
│       → docs/runbooks/sse-connections.md
└─ Email specifically
    → docs/email-notifications.md and docs/runbooks/notifications.md
```

**Metrics/logs to check**: `soroban_pulse_webhook_failures_total`, `soroban_pulse_email_failures_total`, `soroban_pulse_sse_active_connections`, `delivery_logs` table (`status = 'failed'`).

**Runbook**: [docs/runbooks/webhook-failures.md](runbooks/webhook-failures.md), [docs/runbooks/sse-connections.md](runbooks/sse-connections.md), [docs/runbooks/notifications.md](runbooks/notifications.md)

### Branch F: Resource growth

```
Memory or CPU climbing
├─ Memory (soroban_pulse_process_memory_bytes climbing without bound)
│   → docs/troubleshooting.md § Diagnose memory growth
│   → Check soroban_pulse_sse_active_connections first — the most common cause
│     is accumulating long-lived SSE connections, not a leak in request handling
├─ CPU pegged on one core
│   → Usually the indexer's decode/transform path — profile with flamegraph
│   → docs/development-setup.md § Performance Profiling Setup
└─ Both climbing together, correlated with traffic
    → This is capacity, not a bug — docs/capacity-planning.md
```

**Metrics/logs to check**: `soroban_pulse_process_memory_bytes`, `soroban_pulse_sse_active_connections`, `top -p $(pgrep soroban-pulse)`.

---

## Prevention Strategies

Recurring root causes and how to avoid hitting them again:

| Root cause | Prevention |
|---|---|
| DB pool exhaustion under load | Set `DB_MAX_CONNECTIONS` using the sizing formula in [docs/performance-tuning.md](performance-tuning.md#connection-pool-tuning) *before* going to production, and alert at 90% utilization (`soroban_pulse_db_pool_size` / `_max`). |
| Indexer lag from RPC flakiness | Point `STELLAR_RPC_URL` at a provider with an SLA, and alert on `soroban_pulse_rpc_errors_total` rate rather than waiting for lag to become visible to users. |
| Reverse proxy killing SSE connections | Set proxy idle timeouts (≥60s) to exceed `SSE_KEEPALIVE_SECS` at deploy time, not after the first complaint — see [docs/sse_reverse_proxy_configuration.md](sse_reverse_proxy_configuration.md). |
| Webhook endpoint overload | Follow the endpoint sizing guidance in [docs/subscription-best-practices.md](subscription-best-practices.md#webhook-delivery) *before* subscribing at production event volume, not after deliveries start failing. |
| Silent memory growth from SSE clients that never disconnect | Alert on `soroban_pulse_sse_active_connections` trending upward with no matching traffic increase — this is almost always clients that stopped reading but never closed the connection. |
| Migration ordering conflicts | `make check-migrations` runs in CI, but run it locally before committing — see [CONTRIBUTING.md § Database Migrations](../CONTRIBUTING.md#database-migrations). |
| Stale table statistics after bulk loads | Run `ANALYZE` after any bulk import job — see [docs/troubleshooting.md § Table statistics](troubleshooting.md#table-statistics) — rather than waiting for autovacuum to catch up on its own schedule. |

---

## Escalation Procedures

1. **Self-service first**: work the relevant [decision tree branch](#decision-tree) and its linked runbook. Most issues in this system are covered by an existing runbook.
2. **If the runbook's steps don't resolve it, or the symptom doesn't match any branch**: file an issue using `.github/ISSUE_TEMPLATE/bug_report.md`, including:
   - The version or commit SHA running
   - `RUST_LOG=debug` output covering the incident window
   - The exact request/operation that triggered it
   - Environment (Docker/Kubernetes/bare metal, single- or multi-replica)
   - Which decision tree branch and runbook you already tried, and what you observed
3. **If this is affecting a production instance with active user impact** (not a dev/staging investigation): treat it as an incident rather than a routine bug report — page whoever holds operational ownership for the deployment per your organization's on-call process, and open the bug report in parallel rather than instead of paging. This document does not define who is on-call; that's specific to each deployment and isn't something the open-source repo can prescribe.
4. **If the fix requires a schema or API change**, don't patch around it locally — open the issue first so the change goes through the review path in [CONTRIBUTING.md § Contribution Workflow](../CONTRIBUTING.md#contribution-workflow), since operational fixes that touch the schema need a migration and rollback plan, not a hotfix.

---

## Searchable Issue Index

Keyword → section, for quick lookup (Ctrl+F this table):

| Keyword | Section |
|---|---|
| `401`, `403`, unauthorized, forbidden | [Branch B](#branch-b-api-behaving-incorrectly) |
| `429`, rate limit | [Branch B](#branch-b-api-behaving-incorrectly) |
| `503`, healthz | [Branch A](#branch-a-startup-failures), [Branch B](#branch-b-api-behaving-incorrectly) |
| database, postgres, connection refused | [Branch A](#branch-a-startup-failures) |
| migration, migrations | [Branch A](#branch-a-startup-failures), [Prevention](#prevention-strategies) |
| slow, latency, p99, timeout | [Branch C](#branch-c-latency-and-throughput) |
| pool exhaustion, connection pool | [Branch C](#branch-c-latency-and-throughput) |
| lag, indexer stuck, ledger | [Branch D](#branch-d-indexing-and-data-freshness) |
| RPC error, stellar rpc | [Branch D](#branch-d-indexing-and-data-freshness) |
| missing events, stale data, empty response | [Branch B](#branch-b-api-behaving-incorrectly), [Branch D](#branch-d-indexing-and-data-freshness) |
| webhook, delivery failed, HMAC, signature | [Branch E](#branch-e-delivery-and-notifications) |
| SSE, stream disconnect, EventSource | [Branch E](#branch-e-delivery-and-notifications) |
| email notification | [Branch E](#branch-e-delivery-and-notifications) |
| memory growth, leak, RSS | [Branch F](#branch-f-resource-growth) |
| CPU, flamegraph, profiling | [Branch F](#branch-f-resource-growth) |
| capacity, scaling | [Branch F](#branch-f-resource-growth), [docs/capacity-planning.md](capacity-planning.md) |

For anything not listed here, [docs/troubleshooting.md](troubleshooting.md) has a broader table of contents and is the more exhaustive reference.

---

## Runbook Index

| Runbook | When to use it |
|---|---|
| [Indexer lag](runbooks/indexer-lag.md) | Branch D |
| [DB pool exhaustion](runbooks/db-pool-exhaustion.md) | Branch C |
| [RPC errors](runbooks/rpc-errors.md) | Branch D |
| [Webhook failures](runbooks/webhook-failures.md) | Branch E |
| [SSE connections](runbooks/sse-connections.md) | Branch E |
| [Notifications](runbooks/notifications.md) | Branch E |
| [Feature flag rollback](runbooks/feature-flag-rollback.md) | A recent feature-flagged change is suspected as the cause of any branch above |
| [Operator runbook](runbooks/operator-runbook.md) | General operational reference, not symptom-specific |
| [Schema consolidation](runbooks/schema-consolidation.md) | Migration/schema-related incidents |
| [Query plan cache](runbooks/query-plan-cache.md) | Branch C, query-plan-specific slowdowns |

---

## Related Documentation

- [Troubleshooting and Debugging Guide](troubleshooting.md) — the detailed reference this guide points into
- [Development Environment Setup](development-setup.md) — profiling and debugging tool setup
- [Performance Tuning Guide](performance-tuning.md)
- [Capacity Planning](capacity-planning.md)
- [CONTRIBUTING.md](../CONTRIBUTING.md)
