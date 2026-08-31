# Runbook: Database Corruption Recovery

**Severity:** SEV-1  
**Last tested:** See runbook testing log at the bottom of this file.  
**Owner:** Platform Engineering  

---

## Symptoms

- PostgreSQL error: `invalid page in block X of relation Y`
- `ERROR: could not read block X in file "base/NNN/NNN"`
- `FATAL: database file appears to be corrupted`
- Unexpected NULL values or truncated rows in the `events` table
- Failed health check (`GET /healthz/ready` returns 503)
- `soroban_pulse_rpc_errors_total` spiking without RPC issues

---

## Immediate Actions (first 15 minutes)

### Step 1 — Declare the incident

```
INCIDENT DECLARED: Database corruption detected on [environment]
Severity: SEV-1
IC: [your name]
Time: [UTC time]
Channel: #incidents-[date]
```

Page the on-call engineer via PagerDuty if not already paged.

### Step 2 — Assess the scope

```bash
# Check PostgreSQL logs for corruption messages
# On RDS:
aws rds describe-events --source-identifier <db-instance-id> --duration 60

# On self-hosted:
sudo grep -i "corrupt\|invalid page\|checksum" /var/log/postgresql/postgresql-*.log | tail -50
```

```sql
-- Check table integrity
SELECT schemaname, tablename, pg_relation_size(schemaname||'.'||tablename) AS size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY size DESC;

-- Check for corrupt pages (will error if pages are corrupt)
SELECT COUNT(*) FROM events;

-- Check for the specific corrupted range
SELECT MIN(ledger), MAX(ledger) FROM events;
```

### Step 3 — Stop writes immediately

⚠️ **This will pause event indexing.** Coordinate with stakeholders.

```bash
# Pause the indexer via admin API
curl -X POST https://your-service/v1/admin/indexer/pause \
     -H "Authorization: Bearer $ADMIN_API_KEY"

# Verify
curl https://your-service/v1/admin/indexer/status \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

### Step 4 — Take a snapshot before any repair

⚠️ **Do not skip this step.** You may need to roll back the repair.

```bash
# AWS RDS
aws rds create-db-snapshot \
    --db-instance-identifier $DB_INSTANCE \
    --db-snapshot-identifier corruption-incident-$(date +%Y%m%d%H%M%S)

# Self-hosted (stop writes first via Step 3, then)
pg_basebackup -h $DB_HOST -U postgres -D /backup/corruption-snapshot-$(date +%Y%m%d)
```

---

## Recovery Procedures

### Option A — Restore from backup (preferred for severe corruption)

Use when: more than a handful of pages are corrupt, or the corruption is in
system catalogs.

```bash
# 1. List available backups
aws rds describe-db-snapshots \
    --db-instance-identifier $DB_INSTANCE \
    --query 'DBSnapshots[?Status==`available`].[DBSnapshotIdentifier,SnapshotCreateTime]' \
    --output table

# 2. Determine acceptable data loss window (RPO)
#    Check the latest event in the DB and compare with latest RPC ledger.
curl https://your-service/v1/events?limit=1 \
     -H "Authorization: Bearer $API_KEY" | jq .data[0].ledger

# 3. Restore to the most recent snapshot before corruption
aws rds restore-db-instance-from-db-snapshot \
    --db-instance-identifier $DB_INSTANCE-restored \
    --db-snapshot-identifier <snapshot-id>

# 4. Verify restored instance
psql -h $RESTORED_DB_HOST -U $DB_USER $DB_NAME -c "SELECT COUNT(*) FROM events;"

# 5. Point the service at the restored DB (update DATABASE_URL + restart)
# 6. Re-index missing events using the replay feature:
curl -X POST https://your-service/v1/admin/replay \
     -H "Authorization: Bearer $ADMIN_API_KEY" \
     -H "Content-Type: application/json" \
     -d '{"from_ledger": <last_good_ledger>, "to_ledger": <latest_ledger>}'
```

### Option B — Page-level repair (minor corruption, system online)

Use when: a small number of data pages are corrupt, system catalogs are intact.

```bash
# Connect to the database
psql -h $DB_HOST -U $DB_USER $DB_NAME

-- Identify the corrupt ledger range
DO $$
DECLARE
  r RECORD;
BEGIN
  FOR r IN SELECT ledger FROM events ORDER BY ledger LOOP
    -- This will raise if the row is unreadable
    PERFORM 1 FROM events WHERE ledger = r.ledger;
  END LOOP;
END $$;

-- Delete corrupt rows (⚠️ data loss — confirm with IC)
DELETE FROM events WHERE ledger BETWEEN <start> AND <end>;

-- Vacuum and reindex
VACUUM ANALYZE events;
REINDEX TABLE events;
```

After page-level repair, re-index the deleted range:

```bash
curl -X POST https://your-service/v1/admin/replay \
     -H "Authorization: Bearer $ADMIN_API_KEY" \
     -H "Content-Type: application/json" \
     -d '{"from_ledger": <deleted_start>, "to_ledger": <deleted_end>}'
```

### Option C — Point-in-time recovery (PITR) on RDS

Use when: you know the exact time corruption was introduced.

```bash
aws rds restore-db-instance-to-point-in-time \
    --source-db-instance-identifier $DB_INSTANCE \
    --target-db-instance-identifier $DB_INSTANCE-pitr \
    --restore-time <ISO8601 timestamp before corruption>
```

---

## Verification

After recovery, verify data integrity:

```sql
-- Check event count is plausible
SELECT COUNT(*) FROM events;

-- Check no ledger gaps larger than expected (RPC sometimes skips ranges)
SELECT ledger, COUNT(*) FROM events GROUP BY ledger ORDER BY ledger;

-- Check latest indexed ledger matches approximately the latest known ledger
SELECT MAX(ledger) FROM events;
```

```bash
# Verify service health
curl https://your-service/healthz/ready

# Verify events are flowing again
curl https://your-service/v1/events?limit=5 | jq .data[].ledger
```

Resume the indexer:

```bash
curl -X POST https://your-service/v1/admin/indexer/resume \
     -H "Authorization: Bearer $ADMIN_API_KEY"
```

---

## Root Cause Investigation

After the service is restored, file a post-mortem within 24 hours:

- [ ] Check storage I/O errors in OS/hypervisor logs
- [ ] Check PostgreSQL `pg_stat_database` for corruption counters
- [ ] Review backup age and verify backup/restore SLA is met
- [ ] If on RDS, check for storage issues in AWS Health Dashboard
- [ ] Enable `data_checksums` if not already enabled (requires `pg_dumpall` + restore)

---

## Communication Templates

### Initial notification

```
[SEV-1] Database corruption detected on [environment]
We are investigating data integrity issues affecting [scope].
Users may experience [symptom, e.g., missing historical events].
Updates every 30 minutes in #incidents.
ETA for resolution: [time or TBD]
```

### Resolution

```
[RESOLVED] Database corruption on [environment] has been resolved.
Root cause: [brief description]
Impact: [events from ledger X to Y were re-indexed / data loss of N rows]
Post-mortem scheduled for [date].
```

---

## Escalation Procedure

| Escalation trigger | Action |
|-------------------|--------|
| Corruption in system catalogs | Engage PostgreSQL DBA |
| Data loss > 1 hour of events | Notify VP Engineering |
| Corruption on all replicas | Invoke disaster recovery plan |
| Root cause is cloud storage | Open support ticket with cloud provider |

---

## Runbook Testing Log

Run this runbook quarterly in staging:

```bash
# Simulate corruption by truncating a data file (staging ONLY)
# Then execute Option B above to verify recovery steps work
```

| Date | Tester | Outcome | Notes |
|------|--------|---------|-------|
| | | | |
