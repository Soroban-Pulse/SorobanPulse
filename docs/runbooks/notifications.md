# Notification Delivery Troubleshooting

## NotificationDeliverySLABreach

**Alert**: Notification delivery p95 latency exceeds 30s SLA
**Severity**: Critical

### Symptoms
- Notifications arriving late to users
- P95 delivery latency above 30 seconds
- Potential user-facing impact on alert delivery

### Investigation Steps

1. **Check notification queue depth**:
   ```sql
   SELECT COUNT(*) FROM notifications WHERE status = 'pending';
   SELECT COUNT(*) FROM notifications WHERE status = 'queued';
   ```

2. **Check notification pipeline metrics**:
   ```bash
   curl -s http://localhost:3000/metrics | grep notification
   ```

3. **Review downstream service health**:
   - Email provider status
   - Webhook endpoint availability
   - Slack/PagerDuty API status

### Common Causes

1. **High notification volume**: Spike in events causing queue backup
2. **Downstream service outage**: Email or webhook service unavailable
3. **Rate limiting**: External API rate limits being hit
4. **Database contention**: Lock contention on notification table

### Resolution

1. **Immediate**: Scale up notification workers
   ```bash
   kubectl scale deployment soroban-pulse-notifications --replicas=5
   ```

2. **Short-term**: Increase worker concurrency
   ```bash
   kubectl set env deployment/soroban-pulse NOTIFICATION_WORKERS=10
   ```

3. **Long-term**: Review notification batching strategy

### Runbook References
- [Email Notifications](../email-notifications.md)
- [Notification Channels](../notification-channels.md)
- [Rate Limiting](../notification-rate-limiting.md)

---

## NotificationDeliveryLatencyHigh

**Alert**: Notification delivery p99 latency exceeds 60s
**Severity**: Warning

### Symptoms
- Occasional delayed notifications
- P99 latency approaching SLA breach

### Investigation Steps

1. **Check for slow consumers**:
   ```sql
   SELECT channel, AVG(delivery_time) 
   FROM notification_delivery_log 
   WHERE created_at > NOW() - INTERVAL '1 hour'
   GROUP BY channel;
   ```

2. **Review webhook endpoint response times**

3. **Check for network issues**

### Resolution
- Optimize slow webhook endpoints
- Consider async processing for heavy payloads
- Implement circuit breakers for unreliable downstream services

---

## NotificationDeliverySuccessRateLow

**Alert**: Notification delivery success rate below 95%
**Severity**: Critical

### Symptoms
- `NotificationDeliverySuccessRateLow` firing for more than 5 minutes
- Users report missing emails, SMS or webhook calls
- `soroban_pulse_notification_delivery_failure_total` rising faster than usual

Follow **Channel failover** below if one channel is clearly down.

---

## Channel failover

Use this procedure when the primary notification channel (for example email
via SMTP) is down and notifications must be routed through a healthy channel.
Target time to complete: **under 5 minutes**.

### Prerequisites

```bash
export PULSE_URL=https://your-host        # Soroban Pulse base URL
export ADMIN_TOKEN=...                    # admin API key
```

### Step 1 — Confirm which channel is failing (≈1 min)

Open the **Soroban Pulse** dashboard in Grafana (`docs/grafana-dashboard.json`)
and check delivery success rate per channel. To add the panel ad hoc, use
**Explore** with the Prometheus data source and run:

```promql
# Success rate per channel over the last 5 minutes (0–1)
sum by (channel) (rate(soroban_pulse_notification_delivery_success_total[5m]))
/
(
  sum by (channel) (rate(soroban_pulse_notification_delivery_success_total[5m]))
  +
  sum by (channel) (rate(soroban_pulse_notification_delivery_failure_total[5m]))
)
```

Supporting queries:

```promql
# Channel health gauge: 1 = healthy, 0 = unhealthy
soroban_pulse_notification_channel_healthy

# p95 delivery latency per channel
histogram_quantile(0.95,
  sum by (channel, le) (rate(soroban_pulse_notification_delivery_latency_seconds_bucket[5m])))

# Failovers already performed
sum by (channel) (increase(soroban_pulse_notification_failover_total[15m]))
```

A channel whose success rate is below 0.95 while others are near 1.0 is the
failing channel. Cross-check in the database:

```sql
SELECT channel_type,
       COUNT(*) FILTER (WHERE status = 'success') AS ok,
       COUNT(*) FILTER (WHERE status = 'failure') AS failed,
       ROUND(100.0 * COUNT(*) FILTER (WHERE status = 'success') / NULLIF(COUNT(*), 0), 1) AS success_pct
FROM notification_deliveries
WHERE delivered_at > NOW() - INTERVAL '15 minutes'
GROUP BY channel_type
ORDER BY success_pct;
```

### Step 2 — Switch the default channel (≈1 min)

Switch the default notification channel to a healthy one (`webhook`, `email`,
`sms` or `pagerduty`):

```bash
curl -sS -X POST "$PULSE_URL/v1/admin/notifications/channel" \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"channel": "webhook"}'
```

Verify the change:

```bash
curl -sS "$PULSE_URL/v1/admin/notifications/dashboard" \
  -H "Authorization: Bearer $ADMIN_TOKEN"
```

Within 1–2 minutes the success rate for the new channel should rise in Grafana
and `soroban_pulse_notification_delivery_failure_total` should stop climbing.

### Step 3 — Clear the notification DLQ (≈2 min)

Notifications that exhausted their retries while the primary channel was down
are parked in the dead-letter queue. Once the new channel is delivering:

1. **Inspect the backlog**:
   ```bash
   curl -sS "$PULSE_URL/v1/admin/webhook-dlq" \
     -H "Authorization: Bearer $ADMIN_TOKEN"
   ```
   Or check the `soroban_pulse_webhook_dlq_depth` gauge.

2. **Replay all pending entries** through the healthy channel (use `limit` to
   replay in smaller batches if the backlog is large):
   ```bash
   curl -sS -X POST "$PULSE_URL/v1/admin/webhook-dlq/replay" \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"limit": 500}'
   ```

3. **Discard entries that should not be resent** (for example, stale alerts):
   ```bash
   curl -sS -X DELETE "$PULSE_URL/v1/admin/webhook-dlq/<id>" \
     -H "Authorization: Bearer $ADMIN_TOKEN"
   ```

4. **Confirm the DLQ is drained**:
   ```sql
   SELECT COUNT(*) FROM webhook_dlq WHERE replayed_at IS NULL;
   ```
   The result should be `0` and `soroban_pulse_webhook_dlq_depth` should drop to 0.

### Step 4 — Fail back once the primary recovers

1. Confirm the primary provider is healthy (provider status page, and
   `POST /v1/admin/notifications/channels/{id}/test` returns success).
2. Switch the default back:
   ```bash
   curl -sS -X POST "$PULSE_URL/v1/admin/notifications/channel" \
     -H "Authorization: Bearer $ADMIN_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"channel": "email"}'
   ```
3. Watch the per-channel success rate panel for 10 minutes and confirm
   `NotificationDeliverySuccessRateLow` has resolved.
4. Record the incident timeline, affected window and DLQ replay counts in the
   incident ticket.

---

## Webhook Signature Verification Failures

Inbound email provider webhooks are authenticated before they are allowed to
change suppression state or delivery analytics. When verification fails the
request is rejected with `401`/`403` and **no** suppression or delivery rows are
written. Use this section when you see a spike in rejected webhooks.

### SNS / SES bounce and complaint notifications

`verify_sns_signature` validates the SNS envelope before processing:

- `SigningCertURL` must be HTTPS, its host must match
  `^sns\.[a-z0-9-]+\.amazonaws\.com(\.cn)?$` and its path must end in `.pem`.
- The certificate is fetched through the SSRF-safe outbound client and cached by
  URL.
- The canonical string-to-sign (different for `Notification` vs
  `SubscriptionConfirmation`) is verified with RSA PKCS#1 v1.5 for
  `SignatureVersion` 1 (SHA1) and 2 (SHA256).
- Messages whose `Timestamp` is older than the configured window (default
  **1 hour**) are rejected as replays.

Common causes and actions:

| Symptom | Likely cause | Action |
| --- | --- | --- |
| `SigningCertURL` rejected | Non-AWS host or non-`.pem` path | Confirm the request really comes from AWS SNS; treat as an attack otherwise |
| Certificate fetch failure | Egress blocked to `sns.*.amazonaws.com` | Allow egress to the SNS certificate hosts, or pre-warm the cache |
| Signature mismatch | Tampered body or wrong cert | Do not relax verification; capture the raw request and escalate |
| Timestamp rejected | Clock skew or replay | Check node clock (`timedatectl`); genuine retries older than the window must be re-sent by SNS |

### SendGrid event webhooks

`verify_sendgrid_signature` verifies
`ECDSA-P256-SHA256(timestamp || raw_body)` using the public key from
`SENDGRID_WEBHOOK_PUBLIC_KEY` (base64 DER, as shown in the SendGrid UI) and
enforces a timestamp tolerance window to stop replays.

- **Key missing while SendGrid is enabled**: the service fails closed. The
  endpoint rejects all events and a startup error is logged. Set
  `SENDGRID_WEBHOOK_PUBLIC_KEY` and restart.
- **Signature mismatch**: the body was tampered with or the key was rotated in
  SendGrid without updating config. Re-copy the key from the SendGrid UI.
- **Stale timestamp**: check clock skew; events outside the tolerance window are
  rejected as replays.

### Fail-closed startup behavior

If a webhook provider is configured but signature verification cannot be
performed (for example SNS verification is unavailable, or SendGrid is enabled
without a public key), the service **refuses to start** or disables the affected
endpoint rather than accepting unauthenticated requests. Check the startup logs
for `signature verification` errors and fix the configuration before rolling
out.

---

## Common Remediation Commands

### Flush stuck notifications
```sql
UPDATE notifications SET status = 'pending' 
WHERE status = 'queued' AND created_at < NOW() - INTERVAL '1 hour';
```

### Check notification worker health
```bash
kubectl get pods -l app=soroban-pulse -o jsonpath='{.items[*].status.conditions[?(@.type=="Ready")].status}'
```

### Restart notification workers
```bash
kubectl rollout restart deployment/soroban-pulse-notifications
```

### Inspect rejected webhooks
```bash
# Count signature verification rejections in the last 15 minutes
curl -s http://localhost:3000/metrics | grep -E 'webhook_(signature|verification)_(failure|rejected)'
```
