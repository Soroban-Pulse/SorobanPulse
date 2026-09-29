# Helm Migration Job

> **Issue #1147** — Run database migrations as a Helm pre-install/pre-upgrade Job.

## Overview

SorobanPulse ships a Helm hook Job that runs database migrations **before** any
application pod is started or replaced.  The Job uses the
`pre-install,pre-upgrade` Helm hooks, so `helm install` and `helm upgrade` both
run it automatically.

**Why this matters:**

- With multiple replicas, having every pod run migrations on startup is a race
  condition: two pods may start simultaneously and try to apply the same
  migration.  SQLx's advisory-lock guard mitigates this, but a dedicated Job is
  the cleaner, more explicit solution.
- Long-running migrations (e.g., table partitioning, adding GIN indexes on large
  tables) can time-out the pod's liveness probe and cause a restart loop.  A
  dedicated Job has its own `activeDeadlineSeconds` budget.
- A failing migration **blocks the rollout entirely** — Helm returns a non-zero
  exit code and leaves the existing release untouched.

## How it works

```
helm upgrade soroban-pulse ./helm/soroban-pulse
  └─ [hook: pre-upgrade, weight: -5]
       └─ Job: soroban-pulse-migrate
            └─ container runs: /app/soroban-pulse migrate
                 └─ runs sqlx migrations, then exits 0 (success) or 1 (failure)
  └─ [only if Job succeeded]
       └─ Deployment rolling update
```

1. Helm creates the `soroban-pulse-migrate` Job.
2. The container invokes the binary with the `migrate` subcommand (equivalent
   to setting `MIGRATE_ONLY=true`).
3. The binary connects to PostgreSQL, acquires a session-level advisory lock,
   and applies any pending migrations using `sqlx::migrate!`.
4. On success the process exits 0 and Helm proceeds with the rest of the
   release.
5. On failure the process exits 1, Helm marks the release as failed, and **no
   pods are replaced**.

## Configuration

```yaml
# values.yaml (excerpt)
migrationJob:
  enabled: true          # set to false to disable the hook

  backoffLimit: 2        # retries before the Job is considered failed
  activeDeadlineSeconds: 600   # hard deadline (all retries combined)
  ttlSecondsAfterFinished: 86400  # keep Job/logs for 24 h after completion

  resources:
    requests:
      cpu: "100m"
      memory: "128Mi"
    limits:
      cpu: "500m"
      memory: "256Mi"

  extraEnv: []           # extra env vars injected into the migration container
  podAnnotations: {}     # e.g., Vault Agent sidecar injection
  nodeSelector: {}       # override node placement for the Job
  tolerations: []
```

### Disabling startup migrations in the app

When using the migration Job you should also **disable startup migrations** in
the application pods so that they skip the migration step entirely:

```yaml
env:
  RUN_MIGRATIONS_ON_STARTUP: "false"
```

This prevents the app pods from racing the Job on very fast deployments and
makes the deployment model explicit.  The default value is `true` for backward
compatibility — existing deployments without the Job continue to work unchanged.

### Long-running migrations

Increase `activeDeadlineSeconds` to allow more time:

```yaml
migrationJob:
  activeDeadlineSeconds: 3600   # 1 hour for heavy migrations
  backoffLimit: 0               # no retries for non-idempotent migrations
```

---

## Down-migrations and rollback

> **Important:** SQLx does not run down-migrations automatically.  Helm's
> `helm rollback` command re-deploys the previous chart version (code) but does
> **not** reverse any schema changes that were applied by the migration Job.

### Expectations

| Scenario | What happens |
|---|---|
| `helm upgrade` — migration succeeds | Deployment rolls out normally |
| `helm upgrade` — migration fails | Helm aborts; old pods keep running; schema unchanged |
| `helm rollback` | Old code re-deployed; schema stays at the **newer** version |
| Manual down-migration + `helm rollback` | Old code re-deployed with schema correctly reverted |

### When a rollback requires schema reversal

If you need to revert a schema change:

1. Run the down-migration manually before or immediately after the rollback:

   ```bash
   # Using sqlx-cli
   sqlx migrate revert --database-url "$DATABASE_URL"
   ```

   Or apply the corresponding `.down.sql` file directly:

   ```bash
   psql "$DATABASE_URL" -f migrations/<version>_<name>.down.sql
   ```

2. Then execute `helm rollback`:

   ```bash
   helm rollback soroban-pulse
   ```

### Additive-only migrations (recommended)

The safest approach is to write only **additive** migrations (add columns, add
tables, add indexes) and handle feature removal in a separate, later migration.
This way a code rollback is always safe because the old code simply ignores the
new columns/tables it does not know about.

Example pattern:

```
Release N:   add column  users.new_field     ← additive, rollback-safe
Release N+1: backfill    users.new_field     ← data migration, rollback-safe
Release N+2: drop column users.old_field     ← destructive, only after N+1 stable
```

### Advisory-lock protection

`db::run_migrations` acquires a session-level Postgres advisory lock
(`0xD0C0_1234`) before running.  This ensures that if two migration Job pods
start simultaneously (e.g., during a cluster incident) only one applies
migrations; the other waits and then finds nothing to do.

---

## Troubleshooting

### Inspect migration Job logs

```bash
kubectl logs -l app.kubernetes.io/component=migration -n <namespace>
```

Or by Job name:

```bash
kubectl logs job/soroban-pulse-migrate -n <namespace>
```

### Job left in a failed state

If a previous Job is stuck in a failed state and preventing a new upgrade, you
can delete it manually (the `before-hook-creation` delete policy handles this
automatically on the next `helm upgrade`):

```bash
kubectl delete job soroban-pulse-migrate -n <namespace>
helm upgrade soroban-pulse ./helm/soroban-pulse
```

### Migration timed out

Increase `activeDeadlineSeconds` in values and retry:

```bash
helm upgrade soroban-pulse ./helm/soroban-pulse \
  --set migrationJob.activeDeadlineSeconds=3600
```

### Skipping the Job entirely (emergency)

If you need to bypass the migration Job (not recommended):

```bash
helm upgrade soroban-pulse ./helm/soroban-pulse \
  --set migrationJob.enabled=false
```

Migrations will then fall back to the startup migration path (assuming
`RUN_MIGRATIONS_ON_STARTUP` is not set to `false`).
