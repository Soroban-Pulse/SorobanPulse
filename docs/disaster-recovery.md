# Disaster Recovery Runbook (Issue #908)

This document consolidates SorobanPulse's disaster recovery (DR) posture: recovery targets, procedures per failure scenario, automated testing, and communication templates. It ties together two docs that already cover pieces of this — [multi-deployment-architecture.md](multi-deployment-architecture.md) (replication/failover mechanics) and [backup-verification.md](backup-verification.md) (backup/restore integrity testing) — into a single incident-response reference.

## RTO / RPO Targets

**Recovery Time Objective (RTO)** — how long the system may be down:

| Scenario | RTO (manual) | RTO (automated) |
|---|---|---|
| App process crash | < 30 s (restart) | < 10 s (Kubernetes) |
| Single AZ outage | 5–10 min | 1–2 min (Patroni) |
| Full region outage | 10–20 min | 2–5 min |
| Cloud provider outage | 20–60 min | 10–15 min |
| Full database loss (restore from backup) | 30–90 min (depends on DB size — see [backup-verification.md](backup-verification.md) timing metrics) | Not currently automated |

**Recovery Point Objective (RPO)** — how much data may be lost:

| Failure mode | RPO | Basis |
|---|---|---|
| Replica failover (streaming replication) | Seconds to low single-digit minutes | Depends on `pg_replication_lag_seconds` at time of failure |
| Backup/restore (full data loss) | Up to 24 hours | Daily encrypted backup schedule (`backup-ci.yml`, 03:00 UTC) |
| In-flight webhook deliveries | Zero (durable queue) | Retried from the replicated `webhook_retry_queue` table after promotion |
| Indexed event data since last checkpoint | Zero (idempotent replay) | Indexer resumes from `indexer_checkpoints`, not from an arbitrary point |

If the daily-backup RPO of up to 24 hours is too coarse for a given deployment, reduce it by increasing backup frequency in `backup-ci.yml`'s cron schedule, or by relying on the replica-failover path (seconds-level RPO) for anything short of full data loss.

## Recovery Procedures by Failure Scenario

### 1. App process / pod crash
See [operator-runbook.md § Emergency Response Procedures](runbooks/operator-runbook.md#emergency-response-procedures) (SEV-1 triage → restart).

### 2. Single AZ / database primary outage
Promote the standby replica and let the SorobanPulse instance re-acquire the indexer advisory lock. Full steps: [operator-runbook.md § Replica Failover Procedures](runbooks/operator-runbook.md#replica-failover-procedures) and [multi-deployment-architecture.md § Manual Failover Procedure](multi-deployment-architecture.md#manual-failover-procedure).

### 3. Full region outage
1. Confirm the region is unreachable (health checks failing from Global Accelerator — see [multi-region.md](multi-region.md#load-balancing-and-failover-routing)).
2. Global Accelerator automatically routes traffic away from the unhealthy region's endpoint group — no manual DNS action needed for read/HTTP traffic.
3. Promote a standby region's database replica per the manual failover procedure above.
4. Update that region's `DATABASE_URL` to the promoted primary and restart the SorobanPulse deployment there so it wins the advisory lock.
5. Verify with `curl <region>/healthz/ready` and confirm `soroban_pulse_indexer_is_leader` is set on exactly one region.

### 4. Full database loss (backup restore required)
1. Provision a fresh PostgreSQL instance in the target region.
2. Restore from the most recent verified encrypted backup: `DATABASE_URL="..." BACKUP_ENCRYPTION_KEY="..." bash scripts/restore.sh <file>.dump.gpg` (see [backup-verification.md](backup-verification.md)).
3. Apply any migrations committed after the backup was taken: `for f in migrations/*.sql; do psql "$DATABASE_URL" -f "$f"; done`.
4. Point the application at the restored database and restart.
5. Expect the indexer to replay from its last checkpoint in the restored data — verify `soroban_pulse_indexer_lag` converges back to normal rather than stalling.

### 5. Corrupted or bad data deployed (not infra failure)
See [operator-runbook.md § Data Corruption Recovery](runbooks/operator-runbook.md#data-corruption-recovery) — this is a data-integrity incident, not a capacity/availability one, and does not require a regional failover.

## Automated DR Testing

**Currently automated:** backup/restore integrity is verified continuously, not just at DR time:

- [`.github/workflows/backup-ci.yml`](../.github/workflows/backup-ci.yml) — daily at 03:00 UTC, full backup → restore → row-count and checksum comparison against a seeded dataset.
- [`.github/workflows/backup-verify.yml`](../.github/workflows/backup-verify.yml) — weekly on Sundays at 02:00 UTC, an overlapping backup/restore integrity check.

Both are documented in detail in [backup-verification.md](backup-verification.md). Together they validate the backup-restore leg of scenario 4 above on every run — this is already a strong signal for "can we restore," but it doesn't exercise application-level failover (scenario 3) or run against production-scale data volumes.

**Not yet automated — the checklist gap:** a scheduled, end-to-end DR game day that exercises regional/replica failover (not just backup restore) on a recurring cadence. To close this:

1. Add a scheduled workflow (e.g., monthly, `cron: '0 4 1 * *'`) that stands up a throwaway staging environment, triggers a simulated primary failure (e.g., scale the primary ASG to 0 or kill the DB connection), and asserts the standby acquires the advisory lock and resumes indexing within the RTO targets above.
2. Record pass/fail and timing per run, similar to the timing metrics already captured in `backup-ci.yml`, so RTO/RPO drift is caught before a real incident.
3. Treat a failed DR drill the same as a failed test suite run — it should page, not just log.

## Backup Restoration Validation

Fully covered by [backup-verification.md](backup-verification.md) — row-count matching, `md5(string_agg(tx_hash ...))` checksum comparison, and timing capture for RTO tracking. Use `gh workflow run backup-ci.yml` to trigger an on-demand validation before a planned maintenance window.

## Replica Failover Procedures

See [operator-runbook.md § Replica Failover Procedures](runbooks/operator-runbook.md#replica-failover-procedures) for the operational checklist and [multi-deployment-architecture.md § Failover Documentation](multi-deployment-architecture.md#failover-documentation) for both the Patroni-automated and manual paths, including the exact `pg_ctl promote` / `aws rds failover-db-cluster` commands.

## Disaster Communication Templates

Use these as a starting point for incident channels (Slack/status page); adapt severity language to match [operator-runbook.md](runbooks/operator-runbook.md#emergency-response-procedures)'s SEV levels.

**Initial notification (within 5 minutes of declaring an incident):**
```
[SEV-<n>] SorobanPulse — investigating {symptom, e.g. "indexer lag spiking in us-east-1"}
Impact: {who/what is affected}
Status: Investigating. Next update in 15 minutes.
Incident lead: {name}
```

**Update (every 15–30 minutes until resolved):**
```
[SEV-<n>] SorobanPulse — update
Since last update: {what was found/done}
Current status: {e.g. "failover to eu-west-1 in progress"}
ETA: {best estimate or "unknown, next update in 15 min"}
```

**Resolution:**
```
[SEV-<n>] SorobanPulse — resolved
Root cause: {one line}
Resolution: {what fixed it, e.g. "failed over to eu-west-1 replica"}
Duration: {start} – {end} ({total downtime})
Follow-up: {link to postmortem doc/issue}
```

**Postmortem** should record actual RTO/RPO achieved against the targets in this document — persistent gaps between target and actual are a signal to revisit the architecture, not just the runbook.

## Related Documentation

- [multi-deployment-architecture.md](multi-deployment-architecture.md) — replication and failover mechanics
- [multi-region.md](multi-region.md) — cross-region infrastructure and routing
- [backup-verification.md](backup-verification.md) — backup/restore integrity testing
- [runbooks/operator-runbook.md](runbooks/operator-runbook.md) — incident response procedures and severity levels
