# Runbook: Widespread Service Failure Recovery

**Severity:** SEV-1 / SEV-2  
**Last tested:** See runbook testing log at the bottom of this file.  
**Owner:** Platform Engineering  

---

## Symptoms

- All or most `GET /v1/events` requests returning 5xx
- `GET /healthz/ready` returning `503 Service Unavailable`
- `GET /healthz/live` returning non-200 (process crash)
- All replicas in crash loop (`kubectl get pods` shows `CrashLoopBackOff`)
- Multi-region: one or more regions completely unreachable
- Prometheus alerts firing: `SorobanPulseDown`, `SorobanPulseHighErrorRate`,
  `IndexerStalled`

---

## Triage Decision Tree

```
Service returning 5xx / 503?
│
├─ /healthz/live returns non-200?
│   └─ YES → Process crashed → see "Process Crash" below
│
├─ /healthz/ready returns 503?
│   ├─ "db: error" in response?
│   │   └─ YES → Database issue → see "Database Unreachable" below
│   └─ "indexer: stalled" in response?
│       └─ YES → Indexer stalled → see "Indexer Stalled" below
│
└─ All pods healthy but requests failing?
    └─ Likely load balancer / network issue → see "Network/LB Issue"
```

---

## Immediate Actions (first 15 minutes)

### Step 1 — Declare the incident

```
INCIDENT DECLARED: Widespread service failure on [environment]
Severity: SEV-[1/2]
IC: [your name]
Time: [UTC time]
Channel: #incidents-[date]
```

### Step 2 — Check service status

```bash
# Kubernetes
kubectl get pods -n production -l app=soroban-pulse
kubectl describe pod -n production -l app=soroban-pulse | tail -30

# Docker Compose
docker-compose ps
docker-compose logs --tail=50 app

# Check health endpoints
curl -v https://your-service/healthz/live
curl -v https://your-service/healthz/ready
```

---

## Scenario A: Process Crash / CrashLoopBackOff

### Diagnose

```bash
# Get crash reason
kubectl logs -n production -l app=soroban-pulse --previous | tail -100

# Check for OOM kills
kubectl describe pod -n production -l app=soroban-pulse | grep -A5 "OOMKilled\|Reason\|Exit Code"

# Check recent events
kubectl get events -n production --sort-by='.lastTimestamp' | tail -20
```

### Common causes and fixes

| Exit code / message | Cause | Fix |
|--------------------|-------|-----|
| `OOMKilled` | Memory limit too low | Increase `resources.limits.memory` or reduce `BLOOM_FILTER_CAPACITY` |
| `exit code 1` + config error | Bad configuration | Fix the env var causing the error (check startup logs) |
| `exit code 1` + DB connection error | DB unreachable at startup | See Scenario B |
| `Panic` in logs | Bug | Pin to previous image tag and escalate |

### Rollback to previous version

⚠️ Confirm with IC before rolling back.

```bash
# Kubernetes
kubectl rollout undo deployment/soroban-pulse -n production
kubectl rollout status deployment/soroban-pulse -n production

# Verify
kubectl get pods -n production -l app=soroban-pulse
curl https://your-service/healthz/ready
```

### Increase memory limit (if OOM)

```bash
kubectl set resources deployment/soroban-pulse \
    --limits=memory=2Gi \
    -n production
kubectl rollout restart deployment/soroban-pulse -n production
```

---

## Scenario B: Database Unreachable

### Diagnose

```bash
# Check if DB is accepting connections
psql -h $DB_HOST -U $DB_USER $DB_NAME -c "SELECT 1;"

# AWS RDS: check instance status
aws rds describe-db-instances \
    --db-instance-identifier $DB_INSTANCE \
    --query 'DBInstances[0].{Status:DBInstanceStatus,Endpoint:Endpoint.Address}'

# Check for network/security group issues
aws ec2 describe-security-groups --group-ids $DB_SG_ID \
    --query 'SecurityGroups[0].IpPermissions'
```

### Recovery steps

```bash
# If DB is in maintenance mode, wait for it to complete
aws rds wait db-instance-available --db-instance-identifier $DB_INSTANCE

# If DB failed over to a replica (Multi-AZ), update the endpoint if needed
# RDS Multi-AZ handles failover automatically via the CNAME endpoint.
# If using a static IP, update DATABASE_URL to the new primary.

# Restart the service pods to clear connection pool
kubectl rollout restart deployment/soroban-pulse -n production

# Verify recovery
curl https://your-service/healthz/ready | jq .db
```

### Increase connection pool temporarily

If DB is alive but the pool is exhausted after a failover:

```bash
kubectl set env deployment/soroban-pulse \
    DB_MAX_CONNECTIONS=20 \
    -n production
kubectl rollout restart deployment/soroban-pulse -n production
```

---

## Scenario C: Indexer Stalled

```bash
# Check the stall reason
curl https://your-service/healthz/ready | jq .
curl https://your-service/metrics | grep indexer

# Check indexer logs
kubectl logs -n production -l app=soroban-pulse | grep -i "indexer\|stall\|rpc"
```

### Common causes

| Symptom | Cause | Fix |
|---------|-------|-----|
| No RPC response | RPC endpoint down | Check `STELLAR_RPC_URL`; try fallback URL |
| `soroban_pulse_indexer_is_leader = 0` on all pods | Advisory lock lost | Restart one pod |
| `soroban_pulse_indexer_lag_ledgers` growing fast | RPC falling behind | Nothing to do; wait for RPC to catch up |
| Indexer paused | Admin paused it | Resume via admin API |

```bash
# Check if indexer is paused
curl https://your-service/v1/admin/indexer/status \
     -H "Authorization: Bearer $ADMIN_API_KEY"

# Resume if paused
curl -X POST https://your-service/v1/admin/indexer/resume \
     -H "Authorization: Bearer $ADMIN_API_KEY"

# Force leader election by restarting all pods (advisory lock will be re-acquired)
kubectl rollout restart deployment/soroban-pulse -n production
```

---

## Scenario D: Network / Load Balancer Issue

```bash
# Check ALB target health
aws elbv2 describe-target-health \
    --target-group-arn $TARGET_GROUP_ARN

# Check if pods are reachable directly (bypass LB)
POD_IP=$(kubectl get pods -n production -l app=soroban-pulse -o jsonpath='{.items[0].status.podIP}')
kubectl run tmp-curl --rm -it --image=curlimages/curl -- curl http://$POD_IP:3000/healthz/live

# Check ingress / service
kubectl get ingress -n production
kubectl describe service soroban-pulse -n production
```

---

## Multi-Region Failover

If a region is completely unavailable:

### Step 1 — Verify secondary region is operational

```bash
curl https://your-service-secondary-region/healthz/ready
```

### Step 2 — Update DNS to point at secondary region

```bash
# Route 53 failover record (if already configured, this is automatic)
aws route53 change-resource-record-sets \
    --hosted-zone-id $HOSTED_ZONE_ID \
    --change-batch '{
      "Changes": [{
        "Action": "UPSERT",
        "ResourceRecordSet": {
          "Name": "api.your-domain.com",
          "Type": "A",
          "AliasTarget": {
            "HostedZoneId": "'$SECONDARY_ALB_ZONE_ID'",
            "DNSName": "'$SECONDARY_ALB_DNS'",
            "EvaluateTargetHealth": true
          }
        }
      }]
    }'
```

### Step 3 — Verify traffic is routing to secondary

```bash
dig api.your-domain.com
curl https://api.your-domain.com/healthz/ready
```

---

## Verification Checklist

Before declaring the incident resolved:

- [ ] `GET /healthz/ready` returns `{"status":"ok","db":"ok","indexer":"ok"}`
- [ ] `GET /v1/events` returns 200 with data
- [ ] `soroban_pulse_indexer_lag_ledgers` is decreasing or < 100
- [ ] `soroban_pulse_http_request_duration_seconds` p99 is below SLO (200 ms)
- [ ] Error rate (`5xx / total`) is below 1%
- [ ] SSE stream (`GET /v1/events/stream`) delivers events
- [ ] Webhooks are being delivered (`soroban_pulse_webhook_failures_total` not growing)

---

## Communication Templates

### Initial (within 15 min)

```
[SEV-1] Service outage on [environment]
All or most API requests are failing with [error code].
We are investigating. Updates every 15 minutes in #incidents.
Estimated impact: [scope, e.g., "all API consumers"]
```

### Progress update (every 30 min)

```
[UPDATE] Service outage on [environment]
Current status: [Investigating cause / Rollback in progress / DB failover complete]
Estimated resolution: [time or TBD]
```

### Resolution

```
[RESOLVED] Service outage on [environment] has been resolved.
Duration: [X minutes/hours]
Root cause: [brief description]
Impact: [N% of requests failed during the window]
Post-mortem scheduled for [date].
```

---

## Escalation Procedure

| Trigger | Action |
|---------|--------|
| Outage > 30 min | Escalate to Engineering Manager |
| Outage > 1 hour | Escalate to VP Engineering |
| Multi-region outage | Invoke business continuity plan |
| Root cause is security | Switch to security-breach-response.md |
| DB corruption discovered | Switch to database-corruption-recovery.md |

---

## Runbook Testing Log

Test quarterly via chaos engineering:

```bash
# Kill all pods simultaneously
kubectl delete pods -n production -l app=soroban-pulse

# Verify automatic recovery
kubectl rollout status deployment/soroban-pulse -n production
curl https://your-service/healthz/ready
```

| Date | Tester | Scenario | Recovery time | Notes |
|------|--------|----------|--------------|-------|
| | | | | |
