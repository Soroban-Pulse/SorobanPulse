# Runbook: Complete Data Loss Recovery

**Severity:** SEV-1  
**Last tested:** See runbook testing log at the bottom of this file.  
**Owner:** Platform Engineering  

---

## Symptoms

- `SELECT COUNT(*) FROM events` returns 0 or a suspiciously low number
- The `events` table is missing entirely
- Database instance is destroyed or inaccessible
- All replicas are unavailable
- Accidental `DROP TABLE events` or `TRUNCATE events`

---

## Immediate Actions (first 15 minutes)

### Step 1 — Declare the incident

```
INCIDENT DECLARED: Data loss detected on [environment]
Severity: SEV-1
IC: [your name]
Time: [UTC time]
Channel: #incidents-[date]
```

Page the on-call IC and Engineering Manager.

### Step 2 — Stop all writes immediately

⚠️ Writing more data after a loss event can overwrite WAL needed for recovery.

```bash
# Pause the indexer
curl -X POST https://your-service/v1/admin/indexer/pause \
     -H "Authorization: Bearer $ADMIN_API_KEY"

# If the DB is gone, scale down all service replicas first
kubectl scale deployment soroban-pulse --replicas=0 -n production
# or
docker-compose stop app
```

### Step 3 — Determine scope

```sql
-- How many events remain (if DB is accessible)?
SELECT COUNT(*), MIN(ledger), MAX(ledger), MAX(created_at) FROM events;

-- When was the last successful indexer checkpoint?
SELECT * FROM indexer_checkpoints ORDER BY id DESC LIMIT 5;
```

---

## Recovery Procedures

### Option A — Restore from the latest snapshot (primary path)

```bash
# 1. List available snapshots
aws rds describe-db-snapshots \
    --db-instance-identifier $DB_INSTANCE \
    --query 'DBSnapshots[?Status==`available`].[DBSnapshotIdentifier,SnapshotCreateTime]' \
    --output table | sort -k2

# 2. Choose the most recent snapshot before the loss event
SNAPSHOT_ID="rds:soroban-pulse-prod-2026-08-30-05-00"

# 3. Restore
aws rds restore-db-instance-from-db-snapshot \
    --db-instance-identifier $DB_INSTANCE-recovery \
    --db-snapshot-identifier $SNAPSHOT_ID \
    --db-instance-class db.t3.medium \
    --no-publicly-accessible

# 4. Wait for the instance to be available
aws rds wait db-instance-available \
    --db-instance-identifier $DB_INSTANCE-recovery

# 5. Update the service DATABASE_URL to point at the recovery instance
export DATABASE_URL="postgres://user:pass@$RECOVERY_HOST:5432/soroban_pulse"

# 6. Restart the service
kubectl set env deployment/soroban-pulse DATABASE_URL="$DATABASE_URL" -n production
kubectl rollout restart deployment/soroban-pulse -n production
```

### Option B — Continuous backup / WAL restore (self-hosted)

```bash
# Using the restore.sh script (see scripts/restore.sh)
./scripts/restore.sh \
    --backup-dir /backup/base \
    --wal-dir /backup/wal \
    --target-time "2026-08-31 09:00:00 UTC" \
    --data-dir /var/lib/postgresql/data
```

Or with Barman:

```bash
barman recover --target-time "2026-08-31 09:00:00" \
    $BARMAN_SERVER latest /var/lib/postgresql/data
```

### Option C — Re-index from Stellar RPC (no backup available)

Use this only as a last resort. It re-indexes all events from scratch by
replaying from the Stellar RPC. Depending on how many ledgers need to be
re-indexed, this can take hours to days.

```bash
# Recreate the schema
cargo run -- migrate  # or: make migrate

# Set START_LEDGER to the oldest ledger you want to recover
export START_LEDGER=<oldest_ledger_to_recover>

# Start the service; the indexer will replay from START_LEDGER
make run
```

For large re-index operations, increase the pool and poll rate temporarily:

```dotenv
DB_MAX_CONNECTIONS=30
INDEXER_POLL_INTERVAL_MS=1000
```

---

## Data Validation After Recovery

```sql
-- Verify approximate event count (compare with pre-incident metrics)
SELECT
    DATE_TRUNC('hour', created_at) AS hour,
    COUNT(*) AS events
FROM events
GROUP BY 1
ORDER BY 1 DESC
LIMIT 24;

-- Check for any obvious gaps in ledger sequence
SELECT
    ledger,
    LAG(ledger) OVER (ORDER BY ledger) AS prev_ledger,
    ledger - LAG(ledger) OVER (ORDER BY ledger) AS gap
FROM (SELECT DISTINCT ledger FROM events) t
ORDER BY ledger
LIMIT 100;

-- Verify bloom filter reflects current state
-- (will be re-seeded on next restart)
```

```bash
# Verify service is healthy
curl https://your-service/healthz/ready | jq .

# Verify events are flowing
curl https://your-service/v1/events?limit=1 | jq .data[0]

# Check indexer is catching up
curl https://your-service/metrics | grep soroban_pulse_indexer_lag
```

---

## Data Loss Assessment

Quantify the data loss for the post-mortem and stakeholder communication:

```bash
# Events recovered vs expected
RECOVERED=$(psql -h $DB_HOST -U $DB_USER -t -c "SELECT COUNT(*) FROM events;")
echo "Recovered events: $RECOVERED"

# Ledger gap (compare with Stellar network)
LATEST_INDEXED=$(psql -h $DB_HOST -U $DB_USER -t -c "SELECT MAX(ledger) FROM events;")
LATEST_NETWORK=$(curl -s "$STELLAR_RPC_URL" -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger","params":{}}' | jq .result.sequence)
echo "Ledger gap: $((LATEST_NETWORK - LATEST_INDEXED))"
```

---

## Communication Templates

### Initial (within 15 min of detection)

```
[SEV-1] Data loss incident on [environment]
We have detected a data loss event affecting the events database.
Impact: Events indexed between approximately [start time] and [end time] may
        be unavailable.
We are executing the data recovery runbook. Updates every 15 minutes.
Estimated recovery time: [time or TBD]
```

### Progress update (every 30 min)

```
[UPDATE] Data recovery in progress on [environment]
Current status: [Restoring from snapshot / Re-indexing from ledger X / Validating]
Events recovered so far: [count]
Estimated completion: [time]
```

### Resolution

```
[RESOLVED] Data recovery complete on [environment]
Data loss: Events between ledger [X] and ledger [Y] ([N hours] of data)
Recovery method: [snapshot restore / WAL replay / RPC re-index]
All events re-indexed. Service is fully operational.
Post-mortem scheduled for [date/time].
```

---

## Prevention Checklist

After the incident, verify these are in place:

- [ ] Automated backups enabled with retention ≥ 7 days
- [ ] Point-in-time recovery enabled (RDS: automated backups; self-hosted: WAL archiving)
- [ ] Backup restore tested in the last 30 days
- [ ] Database deletion protection enabled
- [ ] Staging environment used to test schema changes before production
- [ ] Access controls reviewed: only the application user has write access

---

## Escalation Procedure

| Trigger | Action |
|---------|--------|
| No usable backup found | Escalate to VP Engineering; begin RPC re-index |
| Recovery time > 2 hours | Notify affected customers |
| Data loss > 24 hours | Engage cloud provider support |
| Root cause is security incident | Switch to security-breach-response.md |

---

## Runbook Testing Log

Run quarterly in staging by executing a `TRUNCATE events;` and verifying
full recovery from the most recent backup.

| Date | Tester | Recovery time | Data loss (ledgers) | Notes |
|------|--------|--------------|---------------------|-------|
| | | | | |
