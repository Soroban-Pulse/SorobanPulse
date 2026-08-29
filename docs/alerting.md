# Real-time Alerting Configuration (Issue #897)

Soroban Pulse implements comprehensive real-time alerting for critical operational events using Prometheus AlertManager.

## Overview

The alerting system provides:
- Alert routing based on severity and component
- Alert deduplication and grouping
- Silence management for suppressing alerts
- Integration with PagerDuty, Opsgenie, and VictorOps
- Alert templating with context information
- Alert history and metrics

## Components

### Prometheus Rules
- Location: `docs/alerts.yml`
- Defines alert conditions and thresholds
- Grouped by component (indexer, database, http, notifications)
- Severity levels: warning, critical

### AlertManager Configuration
- Location: `docs/alertmanager.yml`
- Routes alerts to appropriate receivers
- Handles deduplication and grouping
- Manages silence rules

### Alert Templates
- Location: `docs/alertmanager-templates.yml`
- Formats alert messages for different channels
- Includes runbook links and context information
- Supports Slack, PagerDuty, Opsgenie, VictorOps

## Environment Variables

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `ALERTMANAGER_URL` | URL | `http://localhost:9093` | AlertManager API endpoint |
| `PAGERDUTY_SERVICE_KEY` | string | - | PagerDuty integration key |
| `OPSGENIE_API_KEY` | string | - | Opsgenie API key |
| `VICTOROPS_API_KEY` | string | - | VictorOps API key |
| `SLACK_WEBHOOK_URL` | URL | - | Slack webhook for notifications |

## Alert Routing

### Severity-Based Routing

Alerts are routed based on severity:

| Severity | Receiver | Escalation |
|----------|----------|-----------|
| **info** | Slack #alerts | None |
| **warning** | Slack #warnings | 4 hours |
| **critical** | PagerDuty + Slack | 1 hour |

### Component-Based Routing

Alerts are further grouped by component:

| Component | Channel | Responsible Team |
|-----------|---------|------------------|
| **indexer** | #indexer-alerts | Indexing Team |
| **database** | #database-alerts | Database Team |
| **http** | #api-alerts | API Team |
| **notifications** | #notifications-alerts | Notifications Team |

## Alert Rules

### Indexer Alerts

| Rule | Condition | Threshold | Severity |
|------|-----------|-----------|----------|
| **IndexerLagHigh** | Ledger lag > threshold | 100 ledgers | warning |
| **IndexerLagCritical** | Ledger lag > threshold | 500 ledgers | critical |
| **IndexerStall** | No poll for duration | 2 minutes | critical |

### Database Alerts

| Rule | Condition | Threshold | Severity |
|------|-----------|-----------|----------|
| **DBPoolExhaustion** | Connection pool at max | 100% | critical |
| **HighQueryLatency** | Query duration > threshold | 1 second | warning |
| **ReplicationLag** | Replica sync lag > threshold | 30 seconds | critical |

### HTTP API Alerts

| Rule | Condition | Threshold | Severity |
|------|-----------|-----------|----------|
| **HighHTTPErrorRate** | 5xx response rate > threshold | 1% | critical |
| **P99LatencySLOBreach** | p99 latency > SLO | 250ms | critical |
| **HTTPTimeoutRate** | Request timeout rate > threshold | 0.5% | warning |

### Notification Alerts

| Rule | Condition | Threshold | Severity |
|------|-----------|-----------|----------|
| **WebhookDeliveryFailure** | Delivery failure rate > threshold | 5% | warning |
| **EmailDeliveryFailure** | Email delivery failure rate > threshold | 1% | critical |
| **SlackDeliveryFailure** | Slack delivery failure rate > threshold | 5% | warning |

## Silence Management

### Creating a Silence

```bash
curl -X POST http://localhost:3000/v1/admin/alerts/silences \
  -H "Content-Type: application/json" \
  -d '{
    "alert_name": "IndexerLagHigh",
    "duration_minutes": 30,
    "created_by": "ops-user",
    "comment": "Scheduled maintenance"
  }'
```

**Response:**
```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "alert_name": "IndexerLagHigh",
  "matchers": [],
  "starts_at": "2026-08-27T10:30:00Z",
  "ends_at": "2026-08-27T11:00:00Z",
  "created_by": "ops-user",
  "comment": "Scheduled maintenance",
  "created_at": "2026-08-27T10:30:00Z"
}
```

### Listing Active Silences

```bash
curl http://localhost:3000/v1/admin/alerts/silences
```

### Removing a Silence

```bash
curl -X DELETE http://localhost:3000/v1/admin/alerts/silences/{silence_id}
```

## Integration with PagerDuty

### Setup

1. Create an integration in PagerDuty:
   - Service → Integrations → Add Integration
   - Type: Prometheus
   - Copy the integration key

2. Set environment variable:
   ```bash
   export PAGERDUTY_SERVICE_KEY=<your-key>
   ```

3. Critical alerts will automatically:
   - Create an incident in PagerDuty
   - Trigger on-call rotation
   - Include runbook links

### Alert Format

```
Alert: IndexerLagCritical
Severity: Critical
Component: indexer
Description: Indexer lag is 523 ledgers (threshold: 500)
Runbook: https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/runbooks/indexer-lag.md
```

## Integration with Opsgenie

### Setup

1. Get API key from Opsgenie:
   - Settings → API Key Management
   - Copy the API key

2. Set environment variable:
   ```bash
   export OPSGENIE_API_KEY=<your-key>
   ```

3. Critical alerts will automatically:
   - Create an alert in Opsgenie
   - Route to on-call team
   - Include escalation policies

## Integration with VictorOps

### Setup

1. Create routing key in VictorOps:
   - Integrations → Prometheus
   - Copy the routing key

2. Set environment variable:
   ```bash
   export VICTOROPS_API_KEY=<your-routing-key>
   ```

3. Alerts will be sent with:
   - Alert name and severity
   - Component and description
   - Runbook link for remediation

## Alert Deduplication

AlertManager automatically deduplicates alerts:

| Setting | Value | Purpose |
|---------|-------|---------|
| **group_wait** | 30s | Wait before sending first notification |
| **group_interval** | 5m | Wait before sending grouped alerts |
| **repeat_interval** | 4h | Resend resolved alerts after interval |

## Alert Templating

### Available Variables

```
{{ .Status }}                     # firing or resolved
{{ .Alerts | len }}              # number of alerts
{{ .GroupLabels.alertname }}     # alert rule name
{{ .GroupLabels.component }}     # component label
{{ .Alerts[0].Labels.severity }} # alert severity
{{ .Alerts[0].Annotations.summary }}      # alert summary
{{ .Alerts[0].Annotations.description }}  # detailed description
{{ .Alerts[0].Annotations.runbook_url }}  # runbook link
```

### Custom Templates

Add custom templates to `alertmanager-templates.yml`:

```
{{ define "custom.format" -}}
Alert: {{ .GroupLabels.alertname }}
Severity: {{ .Alerts[0].Labels.severity }}
Component: {{ .Alerts[0].Labels.component }}
{{ .Alerts[0].Annotations.description }}
{{- end }}
```

## Metrics

Alert-related metrics exported by Soroban Pulse:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `soroban_pulse_alerts_fired_total` | Counter | alert_name, severity | Alerts fired |
| `soroban_pulse_alerts_resolved_total` | Counter | alert_name | Alerts resolved |
| `soroban_pulse_alerts_silenced_total` | Counter | alert_name | Alerts silenced |
| `soroban_pulse_active_alerts` | Gauge | component | Currently active alerts |
| `soroban_pulse_alert_silence_duration_minutes` | Gauge | alert_name | Silence duration |

## Best Practices

### 1. Set Appropriate Thresholds
- Use SLO targets as baseline for critical alerts
- Leave headroom for transient spikes
- Adjust thresholds based on historical data

### 2. Use Silence Management Wisely
- Document why alerts are silenced
- Use comments to explain maintenance windows
- Automatically remove silences after maintenance

### 3. Keep Runbooks Updated
- Include troubleshooting steps
- Link to relevant dashboards
- Document known issues and workarounds

### 4. Monitor Alert Fatigue
- Track alert fired vs. resolved ratio
- Remove alerts with low signal-to-noise ratio
- Tune thresholds to reduce false positives

### 5. Test Alert Routing
- Regularly test PagerDuty integration
- Verify team notifications are working
- Check runbook link accessibility

## Troubleshooting

### Alerts Not Firing
- Check Prometheus scrape targets are healthy
- Verify alert rules are configured correctly
- Check AlertManager configuration for syntax errors

### Alerts Not Routing
- Verify `ALERTMANAGER_URL` is correct
- Check AlertManager is running and accessible
- Review routing configuration in `alertmanager.yml`

### Missing Notifications
- Verify webhook URLs are correct
- Check API keys for PagerDuty/Opsgenie/VictorOps
- Review AlertManager logs for routing errors

### Duplicate Notifications
- Increase `group_wait` to deduplicate more
- Add matchers to silence noisy alerts
- Check if multiple AlertManager instances are configured

## Related Documentation

- [SLI/SLO Dashboard](sli-slo.md)
- [Alert Rules](alerts.yml)
- [AlertManager Configuration](alertmanager.yml)
- [Distributed Tracing](tracing.md)
