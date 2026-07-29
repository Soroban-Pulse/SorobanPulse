# Multi-Tenancy Deployment

SorobanPulse supports isolating event data between multiple tenants in a single deployment (issue #583). Each tenant sees only their own events; queries, streams, and exports are automatically scoped by the resolved tenant identity.

## Architecture

```
Client (API key A) ──► auth middleware ──► resolves tenant-a ──► events WHERE tenant_id = 'tenant-a'
Client (API key B) ──► auth middleware ──► resolves tenant-b ──► events WHERE tenant_id = 'tenant-b'
Admin  (admin key) ──► auth middleware ──► no tenant scope  ──► all events
```

Tenant identity is derived from the API key: the SHA-256 hash of the raw API key is looked up in the `api_key_tenants` table, which maps key hashes to tenant identifiers. The plaintext key is never stored in the database.

## Schema

The `events` table gains a `tenant_id TEXT` column (migration `20260430000000_add_tenant_id.sql`). A `NULL` value means the row belongs to the default single-tenant deployment.

The `api_key_tenants` table maps key hashes to tenants:

```sql
CREATE TABLE api_key_tenants (
    key_hash  TEXT PRIMARY KEY,   -- SHA-256(raw_api_key) hex
    tenant_id TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

## Configuration

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `MULTI_TENANT` | `false` | Set to `true` to enable multi-tenant mode |
| `TENANT_RATE_LIMIT_PER_MINUTE` | `RATE_LIMIT_PER_MINUTE` | Independent per-tenant request quota |
| `INDEXER_TENANT_ID` | — | Tenant stamped on events by this indexer instance |
| `TENANT_CONTRACT_FILTER` | — | Per-tenant contract allowlist for the indexer |

### Enable multi-tenant mode

```bash
MULTI_TENANT=true
```

When enabled:
- Every non-admin API key must have a row in `api_key_tenants`.
- A key with no tenant mapping returns `403 Forbidden`.
- All event queries are automatically filtered to the resolved `tenant_id`.
- The indexer stamps every inserted event with `INDEXER_TENANT_ID`.

### Register a tenant API key

Insert the SHA-256 hash of the raw key (SorobanPulse uses the same hashing function):

```bash
KEY="your-tenant-api-key"
KEY_HASH=$(echo -n "$KEY" | sha256sum | awk '{print $1}')

psql "$DATABASE_URL" -c "
  INSERT INTO api_key_tenants (key_hash, tenant_id)
  VALUES ('$KEY_HASH', 'tenant-a')
  ON CONFLICT (key_hash) DO UPDATE SET tenant_id = EXCLUDED.tenant_id;
"
```

After inserting, restart the service so the in-memory tenant map is reloaded (or set `MULTI_TENANT=true` before the first start to load from the database at startup).

## Tenant routing middleware

The `auth_middleware` in `src/middleware.rs` handles tenant resolution:

1. Extracts the raw API key from `Authorization: Bearer <key>` or `X-Api-Key`.
2. Computes `SHA-256(key)`.
3. Looks up the hash in the in-memory `tenant_map` (loaded from `api_key_tenants` at startup).
4. Injects a `TenantId` extension into the request for downstream handlers.

Admin keys (configured via `ADMIN_API_KEY`) bypass tenant resolution and can access all tenant data.

## Tenant isolation validation

All event query handlers read the `TenantId` extension and append a `tenant_id = $N` condition to every SQL query. This is enforced in:

- `GET /v1/events`
- `GET /v1/events/contract/:id`
- `GET /v1/events/tx/:hash`
- `GET /v1/events/stream` (SSE)
- `GET /v1/events/stream/multi` (SSE)
- `GET /v1/events/export`
- `GET /v1/events/stats`

Events broadcast on SSE channels are additionally filtered in the streaming loop: events whose `tenant_id` does not match the subscriber's resolved tenant are dropped before delivery.

## Per-tenant rate limiting

When `MULTI_TENANT=true`, the HTTP rate limiter keys on the API key rather than the client IP address. This gives each tenant an independent quota so one tenant cannot exhaust the shared IP-based limit.

Set the per-tenant quota independently from the global IP rate limit:

```bash
MULTI_TENANT=true
TENANT_RATE_LIMIT_PER_MINUTE=120   # each tenant gets 120 req/min
RATE_LIMIT_PER_MINUTE=60           # fallback if TENANT_RATE_LIMIT_PER_MINUTE is unset
```

When `TENANT_RATE_LIMIT_PER_MINUTE` is unset, the value of `RATE_LIMIT_PER_MINUTE` is used for each tenant bucket.

## Per-tenant contract filtering (indexer)

Use `TENANT_CONTRACT_FILTER` to restrict which contracts the indexer stores per tenant:

```bash
TENANT_CONTRACT_FILTER=tenant-a:CABC...,CDEF...;tenant-b:CXYZ...
```

Events whose `contract_id` is not in the tenant's allowlist are dropped before storage. An empty allowlist means all contracts are indexed for that tenant.

## Multi-indexer deployment

Run one indexer instance per tenant, each writing to the same shared database:

```yaml
# indexer-tenant-a
env:
  - name: MULTI_TENANT
    value: "true"
  - name: INDEXER_TENANT_ID
    value: tenant-a
  - name: TENANT_CONTRACT_FILTER
    value: "tenant-a:CABC...,CDEF..."

# indexer-tenant-b
env:
  - name: MULTI_TENANT
    value: "true"
  - name: INDEXER_TENANT_ID
    value: tenant-b
  - name: TENANT_CONTRACT_FILTER
    value: "tenant-b:CXYZ..."
```

A single API tier serves all tenants; the indexers write to isolated rows in the shared `events` table.

## Kubernetes example

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: soroban-pulse-tenant-keys
stringData:
  tenant-a-key: "your-tenant-a-api-key"
  tenant-b-key: "your-tenant-b-api-key"
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: soroban-pulse-api
spec:
  template:
    spec:
      containers:
        - name: api
          env:
            - name: MULTI_TENANT
              value: "true"
            - name: TENANT_RATE_LIMIT_PER_MINUTE
              value: "120"
            - name: API_KEY
              valueFrom:
                secretKeyRef:
                  name: soroban-pulse-tenant-keys
                  key: tenant-a-key
```

## Security considerations

- API key hashes (SHA-256) are stored instead of plaintext keys. A compromised database does not expose raw keys.
- The `tenant_id` filter is applied unconditionally at the SQL layer; a missing or invalid `TenantId` extension results in no results returned (not an error) to prevent data leakage.
- Admin keys are the only path to cross-tenant data; protect them with `ADMIN_API_KEY` and restrict access to admin routes.
- Combine multi-tenancy with event encryption (`EVENT_DATA_ENCRYPTION_KEY`) for defence-in-depth: even if a tenant's `event_data` rows are accessed by another tenant via a misconfiguration, the payload remains encrypted.

---

## Isolation Guarantees

SorobanPulse enforces tenant isolation at **three independent layers**, so a bug in any single layer does not expose cross-tenant data.

### Layer 1 — Application middleware (primary control)

Every non-admin HTTP request passes through `auth_middleware` in `src/middleware.rs`:

1. The raw API key is extracted from `Authorization: Bearer` or `X-Api-Key`.
2. `SHA-256(key)` is computed and looked up in the in-memory `tenant_map`.
3. On a hit, a `TenantId` extension is injected into the request.
4. On a miss in multi-tenant mode, `403 Forbidden` is returned immediately — the handler is never called.

All event query handlers (`GET /v1/events`, `/v1/events/contract/:id`, `/v1/events/tx/:hash`, `/v1/events/stream`, `/v1/events/stream/multi`, `/v1/events/export`, `/v1/events/stats`) read the `TenantId` extension and append:

```sql
AND tenant_id = $N
```

to every SQL query before execution. There is no code path that skips this filter for a request that has a resolved `TenantId`.

### Layer 2 — PostgreSQL Row-Level Security (defence-in-depth)

Migration `20260527000001_rls_events.sql` enables RLS on the `events` table:

```sql
CREATE POLICY tenant_isolation ON events
    USING (
        tenant_id IS NULL
        OR current_setting('app.current_tenant_id', TRUE) = ''
        OR tenant_id = current_setting('app.current_tenant_id', TRUE)
    );
```

The application sets `app.current_tenant_id` via `SET LOCAL` at the start of each transaction. Even if a query bug omits the `WHERE tenant_id = $N` clause, the Postgres-level policy blocks cross-tenant row access before data leaves the database.

- `NULL` rows are visible to all (single-tenant deployments where `tenant_id` was never stamped).
- An empty setting (admin or pre-RLS connection) bypasses the policy, matching the admin access model.

RLS on the audit table (`tenant_access_audit`) follows the same pattern — a tenant cannot read another tenant's audit trail.

### Layer 3 — Event encryption (optional, defence-in-depth)

When `EVENT_DATA_ENCRYPTION_KEY` is configured, each `event_data` payload is AES-256-GCM encrypted before storage. Even if RLS and the application layer were both bypassed, a cross-tenant attacker would only see ciphertext. See [docs/event-encryption.md](event-encryption.md).

### Isolation properties (summary)

| Property | Enforced by |
|---|---|
| No cross-tenant event reads | Middleware + RLS |
| No cross-tenant audit reads | RLS on `tenant_access_audit` |
| API keys never stored in plaintext | SHA-256 hash in `api_key_tenants` |
| Constant-time key comparison | `subtle::ConstantTimeEq` |
| SSE events filtered per tenant | Broadcast loop in `src/routes.rs` |
| Per-tenant rate limiting | Key-bucketed rate limiter |

---

## Audit Trail for Tenant Access

Every authenticated request is written to the `tenant_access_audit` table (migration `20260729000001_tenant_access_audit.sql`). This provides a tamper-evident log of who accessed what and when.

### Schema

```sql
tenant_access_audit (
    id               BIGSERIAL PRIMARY KEY,
    tenant_id        TEXT,           -- resolved tenant (NULL = admin)
    api_key_hash     TEXT NOT NULL,  -- SHA-256(raw key)
    http_method      TEXT NOT NULL,
    http_path        TEXT NOT NULL,
    query_string     TEXT,           -- truncated to 512 chars
    client_ip        INET,
    response_status  SMALLINT NOT NULL,
    duration_us      INTEGER,        -- handler latency in microseconds
    trace_id         TEXT,           -- W3C trace ID for cross-correlation
    accessed_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
)
```

The table is:
- **Partitioned by month** (`RANGE` on `accessed_at`) — old partitions can be archived without locking live data.
- **Append-only** — the application role has `INSERT` and `SELECT` only; `UPDATE` and `DELETE` are revoked.
- **RLS-protected** — tenants can only query their own rows.

### Querying the audit trail

```sql
-- Last 100 accesses for a specific tenant
SELECT http_method, http_path, response_status, duration_us, accessed_at
FROM tenant_access_audit
WHERE tenant_id = 'tenant-a'
ORDER BY accessed_at DESC
LIMIT 100;

-- Failed requests (auth failures, 4xx/5xx) in the last hour
SELECT api_key_hash, http_path, response_status, client_ip, accessed_at
FROM tenant_access_audit
WHERE accessed_at > NOW() - INTERVAL '1 hour'
  AND response_status >= 400
ORDER BY accessed_at DESC;

-- Correlate with a distributed trace
SELECT *
FROM tenant_access_audit
WHERE trace_id = '4bf92f3577b34da6a3ce929d0e0e4736';
```

### Retention

Audit log retention follows the `NOTIFICATION_AUDIT_LOG_RETENTION_DAYS` policy (default 90 days). The background purge job in `src/pruner.rs` detaches and drops expired monthly partitions. See [docs/data-retention.md](data-retention.md) for full policy details.

### Tenant quota table

The `tenant_quotas` table (same migration) allows operators to set per-tenant limits:

| Column | Description |
|---|---|
| `rate_limit_per_minute` | Override global `RATE_LIMIT_PER_MINUTE` for this tenant |
| `rate_limit_per_hour` | Rolling hourly quota |
| `max_sse_connections` | Max concurrent SSE streams |
| `max_page_size` | Max events per page |
| `max_export_bytes_day` | Daily export budget |
| `suspended` | If `true`, all requests return `403` immediately |

---

## Security Audit Procedures

### Pre-deployment checklist

- [ ] `MULTI_TENANT=true` is set.
- [ ] `ADMIN_API_KEY` is set and distinct from all tenant keys.
- [ ] No tenant key appears in `ADMIN_API_KEY` (and vice versa).
- [ ] All tenant keys have entries in `api_key_tenants` before the service starts.
- [ ] `EVENT_DATA_ENCRYPTION_KEY` is configured (recommended for sensitive tenants).
- [ ] Database application role has `UPDATE`/`DELETE` revoked on `tenant_access_audit`.
- [ ] RLS is enabled on both `events` and `tenant_access_audit`.

### Isolation verification tests

Run the isolation test suite before every release:

```bash
cargo test --test multi_tenant_isolation_tests
```

The suite (in `tests/multi_tenant_isolation_tests.rs`) covers 30 scenarios including:

- Correct tenant ID resolution per key (tests 1–2)
- Auth failures: missing key, wrong key, unmapped key, empty bearer (tests 3–8)
- Admin key cross-tenant bypass and no-TenantId injection (tests 9–10)
- `/health` bypass (test 11)
- Constant-time key comparison (test 12)
- Hash determinism, collision resistance, no-plaintext leak (tests 13–15)
- Edge cases: empty tenant ID, prefix attacks, multi-key tenants, unicode keys (tests 16–20)
- Cross-tenant data leak prevention (tests 21–22)
- Concurrent request isolation (test 23)
- Audit trail: TenantId extension availability (tests 24–25)
- SQL filter and RLS policy correctness (tests 26–27)
- Bearer vs X-Api-Key parity (test 28)
- Key rotation no-bleed (test 29)
- Revoked key rejection (test 30)

### Periodic security review

Perform the following checks quarterly:

1. **Audit log review** — Query `tenant_access_audit` for anomalies: unexpected IP addresses, burst access patterns, cross-tenant probing attempts (a key hash appearing against multiple `tenant_id` values).

2. **Key rotation** — Rotate all tenant API keys. Insert new key hashes into `api_key_tenants` before removing old ones to avoid downtime. Both old and new keys are active during the rotation window (test 29).

3. **RLS verification** — After any schema migration, confirm RLS is still enabled:
   ```sql
   SELECT relname, relrowsecurity
   FROM pg_class
   WHERE relname IN ('events', 'tenant_access_audit');
   ```
   Both rows must show `relrowsecurity = true`.

4. **Privilege audit** — Confirm the application role cannot `UPDATE` or `DELETE` audit rows:
   ```sql
   SELECT grantee, privilege_type
   FROM information_schema.role_table_grants
   WHERE table_name = 'tenant_access_audit';
   ```

### Incident response: suspected isolation breach

1. **Immediately suspend** the affected tenant via `tenant_quotas`:
   ```sql
   UPDATE tenant_quotas
   SET suspended = TRUE,
       suspended_reason = 'Suspected isolation breach — under investigation',
       suspended_at = NOW()
   WHERE tenant_id = '<affected-tenant>';
   ```
   Suspended tenants receive `403 Forbidden` on all requests.

2. **Collect evidence** from `tenant_access_audit`:
   ```sql
   SELECT *
   FROM tenant_access_audit
   WHERE accessed_at > NOW() - INTERVAL '24 hours'
     AND (tenant_id = '<affected-tenant>' OR api_key_hash = '<suspected-hash>')
   ORDER BY accessed_at;
   ```

3. **Rotate** the affected tenant's API key.

4. **Review RLS** policy for the events table (see Periodic security review above).

5. **Notify** affected tenants per your data breach notification policy.

---

## Multi-Region Isolation Strategy

### Replication isolation

When running primary + read replicas:
- All writes (event ingestion) go through the primary; RLS policies are enforced at write time.
- Read replicas inherit the same RLS policies; the application sets `app.current_tenant_id` on replica connections identically.
- Cross-region replicas should be configured with the same application role privileges.

### Backup isolation

Each backup should be treated as containing all tenants' data. Recommended controls:
- Encrypt backups at rest using a key managed separately from the database encryption key.
- Restrict backup restore access to the DBA team; do not grant tenant-level access to backup files.
- Test partial restores (single-tenant data extract) using `COPY (SELECT ... WHERE tenant_id = '...') TO STDOUT`.

### Disaster recovery

In a failover scenario:
- The promoted replica already has RLS enabled; no manual re-application is needed.
- Verify `api_key_tenants` was replicated correctly before accepting tenant traffic.
- Re-run the isolation test suite against the promoted primary before switching DNS.
