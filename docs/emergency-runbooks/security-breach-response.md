# Runbook: Security Breach Response

**Severity:** SEV-1  
**Last tested:** See runbook testing log at the bottom of this file.  
**Owner:** Security Team + Platform Engineering  

> **⚠️ Do not publicly disclose the incident** before completing Step 1 and
> notifying the Security Lead.

---

## Symptoms

- Unexpected API key usage from unknown IP addresses
- Unusual spike in `/v1/admin/*` calls
- Webhook payloads with forged HMAC signatures being accepted
- Database credentials appearing in logs, public repositories, or third-party systems
- Anomaly detection alerts: `soroban_pulse_anomaly_*` firing unexpectedly
- Audit log (`audit_logs` table) showing unexpected admin actions
- PagerDuty incident from an unknown actor
- Git history showing committed secrets

---

## Immediate Actions (first 30 minutes)

### Step 1 — Engage Security Team

Do NOT investigate alone. Notify immediately:

- Security Lead (PagerDuty: `@security-oncall`)
- Engineering Manager
- Document everything in a **private** incident channel (#sec-incident-YYYY-MM-DD)

### Step 2 — Preserve evidence before any remediation

⚠️ Do not delete logs, rotate keys, or restart services until evidence is captured.

```bash
# Export recent audit logs
psql -h $DB_HOST -U $DB_USER $DB_NAME \
    -c "COPY (SELECT * FROM audit_logs ORDER BY created_at DESC LIMIT 10000) TO STDOUT CSV HEADER" \
    > /tmp/audit-evidence-$(date +%Y%m%d%H%M%S).csv

# Export recent access logs (if using nginx/ALB)
# AWS ALB:
aws s3 cp s3://$ALB_LOG_BUCKET/ /tmp/alb-logs/ --recursive \
    --include "*.gz" \
    --no-progress

# Capture current DB connections
psql -h $DB_HOST -U $DB_USER $DB_NAME \
    -c "SELECT * FROM pg_stat_activity;" \
    > /tmp/pg-connections-$(date +%Y%m%d%H%M%S).txt
```

### Step 3 — Assess the breach scope

```sql
-- Check recent admin API calls
SELECT * FROM audit_logs
WHERE action LIKE '%admin%'
ORDER BY created_at DESC
LIMIT 100;

-- Check for unexpected API keys in use
SELECT DISTINCT api_key_prefix, ip_address, COUNT(*) as calls
FROM audit_logs
WHERE created_at > NOW() - INTERVAL '24 hours'
GROUP BY 1, 2
ORDER BY 3 DESC;

-- Check for data exfiltration indicators (large bulk exports)
SELECT * FROM audit_logs
WHERE action = 'bulk_export'
   OR action = 'events_list'
   AND metadata->>'limit' = '1000'
ORDER BY created_at DESC
LIMIT 50;
```

---

## Credential Rotation (execute in order)

### Step 4 — Rotate API keys

⚠️ This will break all clients using the current key. Coordinate with API consumers.

```bash
# Generate new keys (use a secrets manager in production)
NEW_API_KEY=$(openssl rand -base64 48 | tr -d '/+=' | head -c 64)
NEW_ADMIN_API_KEY=$(openssl rand -base64 48 | tr -d '/+=' | head -c 64)

# Update environment (Kubernetes)
kubectl create secret generic soroban-pulse-secrets \
    --from-literal=API_KEY="$NEW_API_KEY" \
    --from-literal=ADMIN_API_KEY="$NEW_ADMIN_API_KEY" \
    --dry-run=client -o yaml | kubectl apply -f -

# Rolling restart to pick up new keys
kubectl rollout restart deployment/soroban-pulse -n production
kubectl rollout status deployment/soroban-pulse -n production

# Immediately verify old key is rejected
curl https://your-service/v1/events \
     -H "Authorization: Bearer $OLD_API_KEY" \
     -o /dev/null -w "%{http_code}"  # Expected: 401
```

### Step 5 — Rotate database credentials

```bash
# AWS RDS: rotate via Secrets Manager
aws secretsmanager rotate-secret \
    --secret-id $DB_SECRET_ARN

# Self-hosted PostgreSQL:
psql -h $DB_HOST -U postgres -c "ALTER USER $DB_USER PASSWORD '$NEW_DB_PASSWORD';"

# Update DATABASE_URL in the service
kubectl set env deployment/soroban-pulse \
    DATABASE_URL="postgres://$DB_USER:$NEW_DB_PASSWORD@$DB_HOST/$DB_NAME" \
    -n production
kubectl rollout restart deployment/soroban-pulse -n production
```

### Step 6 — Rotate webhook secret

```bash
NEW_WEBHOOK_SECRET=$(openssl rand -base64 48 | tr -d '/+=' | head -c 64)

kubectl set env deployment/soroban-pulse \
    WEBHOOK_SECRET="$NEW_WEBHOOK_SECRET" \
    -n production
kubectl rollout restart deployment/soroban-pulse -n production

# Notify webhook consumers of the new signing secret
echo "New webhook secret: $NEW_WEBHOOK_SECRET"
# (share securely — do NOT send over email or Slack)
```

### Step 7 — Revoke any compromised encryption keys

If `EVENT_DATA_ENCRYPTION_KEY` was exposed:

```bash
# Generate new key
NEW_ENC_KEY=$(openssl rand -hex 32)

# Set old key as the rotation key (so existing data can still be decrypted)
kubectl set env deployment/soroban-pulse \
    EVENT_DATA_ENCRYPTION_KEY="$NEW_ENC_KEY" \
    EVENT_DATA_ENCRYPTION_KEY_OLD="$COMPROMISED_KEY" \
    -n production

# Trigger re-encryption of existing data
curl -X POST https://your-service/v1/admin/reencrypt \
     -H "Authorization: Bearer $NEW_ADMIN_API_KEY"

# Monitor re-encryption progress in logs
kubectl logs -f deployment/soroban-pulse -n production | grep reencrypt
```

---

## Network Isolation (if breach is ongoing)

```bash
# Block suspicious IP addresses
kubectl apply -f - <<EOF
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: block-attacker
  namespace: production
spec:
  podSelector:
    matchLabels:
      app: soroban-pulse
  ingress:
  - from:
    - ipBlock:
        except:
        - "$ATTACKER_IP/32"
        cidr: 0.0.0.0/0
EOF

# Or via AWS WAF
aws wafv2 create-ip-set \
    --name block-attacker-$(date +%Y%m%d) \
    --scope REGIONAL \
    --ip-address-version IPV4 \
    --addresses "$ATTACKER_IP/32"
```

---

## Post-Breach Audit

After containment, perform a thorough audit:

```sql
-- List all events accessed by the attacker (correlate with audit logs)
SELECT e.* FROM events e
JOIN audit_logs al ON al.metadata->>'event_id' = e.id::text
WHERE al.ip_address = '$ATTACKER_IP'
ORDER BY al.created_at;

-- Check for any data modifications
SELECT * FROM audit_logs
WHERE action IN ('delete_event', 'anonymize', 'gdpr_erasure')
  AND created_at > '$BREACH_START_TIME'
ORDER BY created_at;
```

---

## Communication Templates

### Internal (Security + Engineering only, first 30 min)

```
[CONFIDENTIAL] Security incident in progress on [environment]
Type: [credential compromise / data exfiltration / unauthorized access]
IC: [name]
Scope: [brief description]
Current action: [evidence preservation / credential rotation / investigation]
Updates: every 15 minutes in #sec-incident-YYYY-MM-DD
```

### Customer notification (after legal review, if data was accessed)

```
[SECURITY NOTICE] Unauthorized access to Soroban Pulse

We are writing to inform you that on [date], we detected unauthorized
access to our systems.

What happened: [brief, non-technical description]
What data was accessed: [scope]
What we have done: [actions taken]
What you should do: [e.g., rotate your API keys, review webhook endpoints]

We take security seriously and are working to prevent recurrence.
Contact: security@your-company.com
```

---

## Escalation Procedure

| Trigger | Action |
|---------|--------|
| Credentials committed to public GitHub | Immediately rotate + notify GitHub security |
| Customer data exfiltrated | Engage Legal + DPO within 1 hour |
| Encryption key compromised | Begin re-encryption immediately |
| Breach ongoing / attacker still active | Network isolation + engage cloud provider security |
| Regulatory notification required (GDPR: 72 hrs) | Notify DPO immediately |

---

## Secrets Committed to Git — Special Procedure

If secrets were committed to git history:

```bash
# 1. Rotate ALL secrets immediately (Steps 4-7 above)
# 2. Remove from git history (⚠️ this rewrites history — coordinate with team)
git filter-repo --path .env --invert-paths
git push origin --force --all

# 3. Notify GitHub to scan for the exposed secret
# GitHub will automatically scan pushes; also file a report at:
# https://docs.github.com/en/code-security/secret-scanning/about-secret-scanning
```

---

## Runbook Testing Log

Test quarterly by simulating a credential leak in staging, executing
Steps 4–6, and verifying:
- Old credentials are rejected.
- New credentials work correctly.
- Audit logs captured the simulated access.

| Date | Tester | Scenario tested | Time to rotate all credentials | Notes |
|------|--------|----------------|-------------------------------|-------|
| | | | | |
