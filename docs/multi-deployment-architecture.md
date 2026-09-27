# Multi-Deployment Architecture Guide

Guidance for running SorobanPulse across multiple regions or cloud providers to achieve high availability and geo-redundancy.

## Overview

A multi-deployment setup runs two or more SorobanPulse instances in separate failure domains (regions, availability zones, or cloud providers). Each instance maintains its own database replica, but event indexing is coordinated via an advisory lock so only one instance writes new events at a time.

```
┌──────────────────────┐      ┌──────────────────────┐
│   Region A (Primary) │      │  Region B (Standby)  │
│                      │      │                      │
│  ┌────────────────┐  │      │  ┌────────────────┐  │
│  │  SorobanPulse  │  │      │  │  SorobanPulse  │  │
│  │  (Indexer +    │  │      │  │  (HTTP only    │  │
│  │   HTTP)        │  │      │  │   + standby)   │  │
│  └───────┬────────┘  │      │  └───────┬────────┘  │
│          │           │      │          │           │
│  ┌───────▼────────┐  │ Repl │  ┌───────▼────────┐  │
│  │   PostgreSQL   │◄─┼──────┼──│   PostgreSQL   │  │
│  │   (Primary)    │  │      │  │   (Replica)    │  │
│  └────────────────┘  │      │  └────────────────┘  │
└──────────────────────┘      └──────────────────────┘
         ▲                              ▲
         └──────────── DNS / LB ────────┘
```

## Geo-Redundancy Patterns

### Pattern 1: Active-Passive with Streaming Replication

The simplest production setup. One region is active (holds the advisory lock and indexes events); the other is passive (read-only HTTP, ready to promote).

**Setup:**
1. Configure PostgreSQL streaming replication from Region A → Region B.
2. Deploy SorobanPulse in both regions with identical configuration.
3. Both instances connect to their local database. Region B connects to the replica in read-only mode.
4. Region A wins the advisory lock on startup; Region B serves HTTP from the replica.

**Failover:** When Region A becomes unavailable, promote the Region B replica and restart its SorobanPulse instance. It will acquire the advisory lock and begin indexing.

### Pattern 2: Active-Active with Read Distribution

Both regions serve HTTP traffic. Indexing remains in one region (lock holder), but read queries are distributed across both replicas via a load balancer.

**Setup:**
1. Configure streaming replication as above.
2. Deploy SorobanPulse in both regions.
3. Place a global load balancer (AWS Route 53, Cloudflare, GCP GLB) in front of both regions.
4. Use health checks to route write-path traffic only to the primary region.

**Trade-offs:**
- Reads are distributed, improving throughput.
- Replication lag can cause stale reads on the replica. Use `PGRST_DB_USE_LEGACY_GUCS=false` and `SET TRANSACTION READ ONLY` if consistency is critical.

### Pattern 3: Multi-Cloud Active-Passive

Same as Pattern 1 but across different cloud providers (e.g., AWS + GCP) for maximum blast-radius isolation.

**Considerations:**
- Cross-cloud replication incurs egress costs and higher latency (~20–50 ms typical).
- Use a VPN or dedicated interconnect (AWS Direct Connect / GCP Interconnect) for the replication channel.
- Certificate pinning and mutual TLS between the replication endpoints is strongly recommended.

## Failover Documentation

### Automated Failover (Patroni / pg_auto_failover)

For production deployments, manage promotion automatically with a tool like [Patroni](https://patroni.readthedocs.io/) or [pg_auto_failover](https://pg-auto-failover.readthedocs.io/).

```yaml
# Example Patroni config snippet
bootstrap:
  dcs:
    ttl: 30
    loop_wait: 10
    retry_timeout: 10
    maximum_lag_on_failover: 1048576  # 1 MB
```

When Patroni promotes the replica, SorobanPulse's database connection pool will see a connection error, reconnect, and re-attempt the advisory lock. The first instance to reconnect to the new primary will acquire the lock and resume indexing.

### Manual Failover Procedure

1. **Confirm primary is down:**
   ```bash
   psql $DATABASE_URL_REGION_A -c "SELECT 1"
   ```

2. **Promote the replica:**
   ```bash
   # On the Region B PostgreSQL host
   pg_ctl promote -D /var/lib/postgresql/data
   # Or for managed databases:
   aws rds failover-db-cluster --db-cluster-identifier soroban-pulse
   ```

3. **Update `DATABASE_URL` in Region B** to point to the now-promoted instance.

4. **Restart the Region B SorobanPulse instance.** It will acquire the advisory lock and start indexing from the last checkpoint stored in `indexer_checkpoints`.

5. **Verify recovery:**
   ```bash
   curl https://region-b.pulse.example.com/healthz/ready
   curl https://region-b.pulse.example.com/v1/metrics | grep soroban_pulse_indexer_lag
   ```

6. **Update DNS** (if not managed automatically) to point traffic to Region B.

### Recovery Time Objectives

| Scenario | RTO (manual) | RTO (automated) |
|---|---|---|
| App process crash | < 30 s (restart) | < 10 s (Kubernetes) |
| Single AZ outage | 5–10 min | 1–2 min (Patroni) |
| Full region outage | 10–20 min | 2–5 min |
| Cloud provider outage | 20–60 min | 10–15 min |

## Cross-Region Sync

### Database Replication

SorobanPulse relies on standard PostgreSQL logical or physical replication. Physical (streaming) replication is recommended for most deployments:

```sql
-- On the primary, create a replication slot
SELECT pg_create_physical_replication_slot('region_b_slot');

-- On the replica, set recovery.conf / postgresql.auto.conf
primary_conninfo = 'host=db-primary.region-a.internal port=5432 user=replicator password=...'
primary_slot_name = 'region_b_slot'
```

**Replication lag monitoring:** The `soroban_pulse_indexer_lag` metric tracks how far behind the indexer is from the chain tip. A separate lag metric from the replica itself (`pg_wal_lsn_diff`) should be monitored via the Prometheus job scraping the replica's `pg_stat_replication`.

### Configuration Sync

Sync these configuration items across regions:

| Item | Sync method |
|---|---|
| `API_KEY` / `ADMIN_API_KEY` | Secret manager (AWS Secrets Manager, GCP Secret Manager) |
| `WEBHOOK_SECRET` | Secret manager |
| `EVENT_DATA_ENCRYPTION_KEY` | Secret manager — **must be identical across regions** |
| `config.toml` | Git (deploy via CI/CD) |
| Notification channel config | Stored in the database; replicated automatically |

### Subscription and Webhook Consistency

Subscriptions and webhook channel registrations are stored in the PostgreSQL database and replicated to all standby nodes. When failing over:

- Active SSE connections to the failed region will drop and clients will reconnect (standard EventSource retry).
- Webhook deliveries in-flight at the time of failure will be retried from the `webhook_retry_queue` table, which is replicated and becomes writable on the new primary.
- No manual intervention is required for subscriptions.

## Kubernetes Multi-Region Deployment

```yaml
# Region A deployment (indexer + HTTP)
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soroban-pulse
  namespace: soroban-pulse
spec:
  replicas: 2
  template:
    spec:
      containers:
      - name: soroban-pulse
        image: soroban-pulse:latest
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: soroban-pulse-secrets
              key: database-url-region-a
        - name: STELLAR_RPC_URL
          value: "https://soroban-mainnet.stellar.org"
```

```yaml
# Region B deployment (HTTP-only standby)
# Same spec but DATABASE_URL points to the read replica.
# When Region A fails, update DATABASE_URL to the promoted primary and scale replicas.
```

## Health Checks and Observability

Both regions should scrape the same Prometheus metrics. Use an alert to detect replication lag exceeding a threshold:

```yaml
# docs/alerts.yml addition
- alert: ReplicationLagHigh
  expr: pg_replication_lag_seconds > 30
  for: 5m
  labels:
    severity: warning
  annotations:
    summary: "PostgreSQL replication lag is {{ $value }}s"
```

Monitor cross-region readiness with:
```bash
# Check both regions are healthy
curl https://region-a.pulse.example.com/healthz/ready
curl https://region-b.pulse.example.com/healthz/ready

# Confirm only one region is actively indexing
curl https://region-a.pulse.example.com/v1/metrics | grep soroban_pulse_indexer_is_leader
curl https://region-b.pulse.example.com/v1/metrics | grep soroban_pulse_indexer_is_leader
```

## Data Consistency

SorobanPulse should use a single writable PostgreSQL primary per network. Cross-region replicas are eventually consistent and can lag behind the primary, so route admin operations, subscription changes, webhook registration, and replay jobs to the primary region.

For read traffic, choose a consistency mode per workload:

| Workload | Recommended routing | Consistency expectation |
|---|---|---|
| Dashboard analytics | Nearest healthy replica | Eventual consistency is acceptable |
| Alerting and webhook delivery | Primary region | Avoid missed or duplicated delivery decisions |
| Replay/backfill jobs | Primary region | Reads the latest checkpoint and writes deterministic output |
| Public event search | Replica with lag checks | Reject or reroute when replica lag exceeds the SLA |

Use idempotent consumers and ledger checkpoints to handle duplicate reads after failover. After promotion, verify the new primary has replayed all WAL up to the last known `indexer_checkpoints` row before enabling webhook delivery.

## Split-Mode: Separately Scalable API and Indexer

> **New in this release.** Previously every pod ran both the HTTP API and the
> indexer together, which forced the HPA to scale both workloads as a unit.
> The new `ROLE` environment variable separates them cleanly.

### The Problem with Combined Mode

In combined mode (`ROLE=all`) every replica competes for the advisory lock.
Only one becomes the active indexer; the rest park the indexer goroutine in a
retry loop while still serving HTTP traffic.  This creates two problems:

1. **Over-provisioned indexer** — adding more API replicas also adds more
   standby indexer goroutines that do nothing useful.
2. **Under-provisioned indexer** — the HPA cannot scale the indexer separately
   if RPC lag grows; it can only add more combined pods.

### ROLE Environment Variable

| Value     | Starts indexer? | HTTP server | Readiness check |
|-----------|-----------------|-------------|-----------------|
| `all`     | ✓ (default)     | Full API    | DB reachable **and** indexer not stalled |
| `api`     | ✗               | Full API    | DB reachable only |
| `indexer` | ✓               | Health + metrics only | Advisory lock held (503 on standby) |

The default (`all`) preserves complete backward compatibility — a `helm
upgrade` with no value changes is a no-op.

### Architecture in Split Mode

```
                 ┌──────────────────────────────────────────┐
                 │  Kubernetes cluster                       │
                 │                                           │
  HTTP traffic   │  ┌───────────────────┐                   │
  ─────────────► │  │  API Deployment   │  ROLE=api          │
                 │  │  (HPA: 2–10 pods) │  - no indexer     │
                 │  │  /v1/events       │  - ready: DB ok   │
                 │  └────────┬──────────┘                   │
                 │           │  SQL                          │
                 │  ┌────────▼──────────┐                   │
                 │  │   PostgreSQL      │                    │
                 │  └────────▲──────────┘                   │
                 │           │  INSERT                       │
                 │  ┌────────┴──────────────┐               │
  Stellar RPC ───┼─►│  Indexer Deployment   │ ROLE=indexer  │
                 │  │  (1 active + 1 standby│ - ready: lock │
                 │  │   advisory lock)      │   held        │
                 │  └───────────────────────┘               │
                 └──────────────────────────────────────────┘
```

### Helm Chart: Enabling Split Mode

Set `splitMode: true` in your values to activate the split-mode deployment:

```yaml
# values-production.yaml
splitMode: true

api:
  autoscaling:
    enabled: true
    minReplicas: 3
    maxReplicas: 20
    targetCPUUtilizationPercentage: 70
  resources:
    requests:
      cpu: "200m"
      memory: "256Mi"
    limits:
      cpu: "1000m"
      memory: "512Mi"
  # Spread api pods across zones
  topologySpreadConstraints:
    - maxSkew: 1
      topologyKey: topology.kubernetes.io/zone
      whenUnsatisfiable: ScheduleAnyway
      labelSelector:
        matchLabels:
          app.kubernetes.io/component: api

indexer:
  # 1 active + 1 warm standby
  replicaCount: 2
  resources:
    requests:
      cpu: "100m"
      memory: "256Mi"
    limits:
      cpu: "500m"
      memory: "512Mi"
  podDisruptionBudget:
    enabled: true
    minAvailable: 1
```

Then upgrade:

```bash
helm upgrade soroban-pulse ./helm/soroban-pulse \
  -f values-production.yaml
```

### Migrating from Combined Mode

A zero-downtime migration path:

1. **Upgrade with `splitMode: true`** — Helm creates the new api and indexer
   Deployments while keeping the old combined Deployment in place until it is
   replaced.
2. **Verify** the new Deployments are healthy:
   ```bash
   kubectl get deploy -l app.kubernetes.io/instance=soroban-pulse
   kubectl rollout status deploy/soroban-pulse-api
   kubectl rollout status deploy/soroban-pulse-indexer
   ```
3. **Confirm only one indexer is active**:
   ```bash
   kubectl exec -it deploy/soroban-pulse-indexer -- \
     curl -s localhost:3000/healthz/ready
   # Expect: {"status":"ok","role":"indexer","lock":"held"}
   ```
4. **Rollback** at any time with `--set splitMode=false`.

### Readiness Semantics per Role

The `/healthz/ready` endpoint behaves differently per role:

**`ROLE=api`**
```json
{ "status": "ok", "role": "api", "db": "ok" }
```
Returns `200` when the database is reachable. An indexer stall on another pod
does **not** make API pods unready.

**`ROLE=indexer` (active leader)**
```json
{ "status": "ok", "role": "indexer", "lock": "held" }
```
Returns `200` only when this pod holds the advisory lock and is the active
indexer.

**`ROLE=indexer` (standby)**
```json
{ "status": "degraded", "role": "indexer", "lock": "standby",
  "reason": "advisory lock not held; this replica is on standby" }
```
Returns `503`. This is intentional — the standby is not in the ready state so
Kubernetes does not route traffic to it, but it remains running and will
promote automatically when the leader is lost.

**`ROLE=all` (default / combined mode)**
Unchanged from before: `200` when DB is reachable and the indexer is not
stalled. Backward compatible.

### Indexer Standby and High Availability

With `indexer.replicaCount: 2`, one pod holds the advisory lock (active) and
the other waits in a retry loop (`INDEXER_LOCK_RETRY_SECS`, default 30 s).
When the leader pod is lost (crash, OOM, node eviction):

1. Postgres automatically releases the session-level advisory lock.
2. The standby pod acquires the lock within one retry interval.
3. Indexing resumes from the last persisted checkpoint with no manual
   intervention.

A `PodDisruptionBudget` with `minAvailable: 1` ensures node maintenance
cannot evict both pods simultaneously.

### Operational Notes

- The `Service` always selects on `selectorLabels` (combined mode) or
  `apiSelectorLabels` (split mode via Ingress). Indexer pods are not
  reachable from the public Ingress.
- The `ServiceMonitor` (when enabled) scrapes `/metrics` on all pods
  regardless of role. The `soroban_pulse_indexer_is_leader` gauge is `1` on
  the active indexer pod and `0` on all others.
- The `NetworkPolicy` (when enabled) controls egress from all pods to
  Postgres and RPC regardless of role.

## Split-Mode: Separately Scalable API and Indexer

> **New in this release.** Previously every pod ran both the HTTP API and the
> indexer together, which forced the HPA to scale both workloads as a unit.
> The new `ROLE` environment variable separates them cleanly.

### The Problem with Combined Mode

In combined mode (`ROLE=all`) every replica competes for the advisory lock.
Only one becomes the active indexer; the rest park the indexer goroutine in a
retry loop while still serving HTTP traffic. This creates two problems:

1. **Over-provisioned indexer** — adding more API replicas also adds more
   standby indexer goroutines that do nothing useful.
2. **Coupled scaling** — the HPA cannot scale the indexer separately
   if RPC lag grows; it can only add more combined pods.

### ROLE Environment Variable

| Value     | Starts indexer? | HTTP server        | Readiness check |
|-----------|-----------------|--------------------|-----------------|
| `all`     | ✓ (default)     | Full API           | DB reachable **and** indexer not stalled |
| `api`     | ✗               | Full API           | DB reachable only |
| `indexer` | ✓               | Health + metrics   | Advisory lock held (503 on standby) |

The default (`all`) preserves complete backward compatibility — a `helm upgrade` with no value changes is a no-op.

### Architecture in Split Mode

```
                 ┌──────────────────────────────────────────┐
                 │  Kubernetes cluster                       │
                 │                                           │
  HTTP traffic   │  ┌───────────────────┐                   │
  ─────────────► │  │  API Deployment   │  ROLE=api          │
                 │  │  (HPA: 2–10 pods) │  · no indexer     │
                 │  │  /v1/events       │  · ready: DB ok   │
                 │  └────────┬──────────┘                   │
                 │           │  SQL reads                    │
                 │  ┌────────▼──────────┐                   │
                 │  │   PostgreSQL      │                    │
                 │  └────────▲──────────┘                   │
                 │           │  INSERT events                │
                 │  ┌────────┴──────────────┐               │
  Stellar RPC ───┼─►│  Indexer Deployment   │ ROLE=indexer  │
                 │  │  (1 active + 1 standby│ · ready: lock │
                 │  │   advisory lock)      │   held        │
                 │  └───────────────────────┘               │
                 └──────────────────────────────────────────┘
```

### Helm Chart: Enabling Split Mode

Set `splitMode: true` in your values:

```yaml
# values-production.yaml
splitMode: true

api:
  autoscaling:
    enabled: true
    minReplicas: 3
    maxReplicas: 20
    targetCPUUtilizationPercentage: 70
  resources:
    requests:
      cpu: "200m"
      memory: "256Mi"
    limits:
      cpu: "1000m"
      memory: "512Mi"
  topologySpreadConstraints:
    - maxSkew: 1
      topologyKey: topology.kubernetes.io/zone
      whenUnsatisfiable: ScheduleAnyway
      labelSelector:
        matchLabels:
          app.kubernetes.io/component: api

indexer:
  # 1 active + 1 warm standby; advisory lock ensures only one indexes at a time
  replicaCount: 2
  resources:
    requests:
      cpu: "100m"
      memory: "256Mi"
    limits:
      cpu: "500m"
      memory: "512Mi"
  podDisruptionBudget:
    enabled: true
    minAvailable: 1
```

Then upgrade:

```bash
helm upgrade soroban-pulse ./helm/soroban-pulse \
  -f values-production.yaml
```

### Migrating from Combined Mode

Zero-downtime migration path:

1. **Upgrade** with `splitMode: true` — Helm replaces the single combined
   Deployment with separate `soroban-pulse-api` and `soroban-pulse-indexer`
   Deployments.
2. **Verify** both are healthy:
   ```bash
   kubectl rollout status deploy/soroban-pulse-api
   kubectl rollout status deploy/soroban-pulse-indexer
   ```
3. **Confirm** only one indexer pod is active (lock held):
   ```bash
   kubectl exec deploy/soroban-pulse-indexer -- \
     wget -qO- localhost:3000/healthz/ready
   # Active leader:  {"status":"ok","role":"indexer","lock":"held"}
   # Standby:        {"status":"degraded","role":"indexer","lock":"standby",...}
   ```
4. **Rollback** at any time: `helm upgrade ... --set splitMode=false`

### Readiness Semantics per Role

**`ROLE=api`** — `200` when DB is reachable; indexer health is irrelevant:
```json
{ "status": "ok", "role": "api", "db": "ok" }
```

**`ROLE=indexer` (active leader)** — `200` when advisory lock is held:
```json
{ "status": "ok", "role": "indexer", "lock": "held" }
```

**`ROLE=indexer` (standby)** — `503` intentionally (auto-promotes on leader loss):
```json
{ "status": "degraded", "role": "indexer", "lock": "standby",
  "reason": "advisory lock not held; this replica is on standby" }
```

**`ROLE=all` (default)** — unchanged; `200` when DB reachable and indexer not stalled.

### Indexer High Availability

With `indexer.replicaCount: 2`, one pod holds the advisory lock and the other
waits in a standby retry loop (`INDEXER_LOCK_RETRY_SECS`, default 30 s). When
the leader is lost:

1. Postgres releases the session-level advisory lock automatically.
2. The standby acquires the lock within one retry interval.
3. Indexing resumes from the last persisted checkpoint — no manual action needed.

A `PodDisruptionBudget` with `minAvailable: 1` prevents simultaneous eviction
of both pods during node maintenance.

### Operational Notes

- The Kubernetes `Service` selects API pods in split mode; indexer pods are
  not exposed through the Ingress.
- The `ServiceMonitor` (when `serviceMonitor.enabled: true`) scrapes `/metrics`
  on all pods. The `soroban_pulse_indexer_is_leader` gauge is `1` on the
  active indexer and `0` on all others.
- The `NetworkPolicy` (when `networkPolicy.enabled: true`) enforces egress
  rules on all pods regardless of role.
- The `ROLE` variable is set automatically by the Helm chart when
  `splitMode: true`; do not set it manually in `env`.
