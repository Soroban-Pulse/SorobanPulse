# PagerDuty Integration

Soroban Pulse can route Soroban contract events to PagerDuty as incidents, giving on-call engineers immediate visibility into anomalous contract activity.

## Overview

The integration uses the **PagerDuty Events API v2** for event delivery (trigger / acknowledge / resolve) and the **PagerDuty REST API** for on-call schedule lookups and escalation policy management.

Each subscription can have one PagerDuty integration. When a new event matches the configured filters, an incident is triggered. Incidents are deduplicated per `(contract_id, event_type)` pair so a burst of identical events creates exactly one incident rather than a flood.

---

## Quick Start

### 1. Prerequisites

- A PagerDuty service with an **Events API v2 integration** configured.  
  Copy the **Integration Key** (routing key) from the service's *Integrations* tab.
- (Optional) A PagerDuty **REST API key** (from *User Settings → API Access*) if you want on-call lookups or escalation policy management.

### 2. Set environment variables

```bash
# Required for incident delivery
PAGERDUTY_ROUTING_KEY=your_routing_key_here

# Optional — REST API key for on-call lookups
ONCALL_PAGERDUTY_API_KEY=your_api_key_here

# Optional — override defaults
PAGERDUTY_SERVICE_NAME="Soroban Pulse"
PAGERDUTY_AUTO_RESOLVE=true
PAGERDUTY_AUTO_RESOLVE_THRESHOLD_MINUTES=30

# Comma-separated contract IDs to watch (empty = all)
PAGERDUTY_CONTRACT_FILTER=

# Comma-separated event types to watch (empty = all)
# Accepted values: contract, diagnostic, system
PAGERDUTY_EVENT_TYPE_FILTER=

# JSON: event-type to PagerDuty severity mapping
PAGERDUTY_SEVERITY_MAPPING='{"contract":"error","diagnostic":"warning","system":"info"}'
```

### 3. Configure a per-subscription integration

```bash
curl -X POST http://localhost:3000/v1/subscriptions/{subscription_id}/integrations/pagerduty \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $API_KEY" \
  -d '{
    "routing_key": "your_routing_key",
    "service_name": "Soroban Pulse",
    "escalation_policy_id": "P1234ABC",
    "contract_filter": ["CABC..."],
    "event_type_filter": ["contract", "system"],
    "severity_mapping": {
      "contract": "error",
      "diagnostic": "warning",
      "system": "critical"
    },
    "auto_resolve": true,
    "auto_resolve_threshold_min": 30
  }'
```

---

## API Reference

All routes are under `/v1/subscriptions/{subscription_id}/integrations/pagerduty`.

### `POST /v1/subscriptions/{id}/integrations/pagerduty`

Create or update the PagerDuty integration for a subscription.

**Request body:**

| Field | Type | Required | Description |
|---|---|---|---|
| `routing_key` | string | ✓ | Events API v2 routing key |
| `service_name` | string | | Human-readable name shown in incidents. Default: `"Soroban Pulse"` |
| `api_key` | string | | REST API key for on-call / escalation lookups |
| `escalation_policy_id` | string | | PagerDuty escalation policy ID to attach |
| `contract_filter` | string[] | | Only trigger for these contract IDs. Empty = all |
| `event_type_filter` | string[] | | Only trigger for these event types (`contract`, `diagnostic`, `system`). Empty = all |
| `severity_mapping` | object | | Maps event type → PagerDuty severity. Default: `{"contract":"error","diagnostic":"warning","system":"info"}` |
| `auto_resolve` | bool | | Auto-resolve stale incidents. Default: `true` |
| `auto_resolve_threshold_min` | int | | Minutes without new events before auto-resolve. Default: `30` |

**Response:** `201 Created`

```json
{
  "id": "uuid",
  "integration_type": "pagerduty",
  "created_at": "2026-08-31T07:42:45Z"
}
```

---

### `GET /v1/subscriptions/{id}/integrations/pagerduty`

Retrieve integration settings. Note: `routing_key` and `api_key` are never returned for security.

**Response:** `200 OK`

```json
{
  "id": "uuid",
  "integration_type": "pagerduty",
  "service_name": "Soroban Pulse",
  "escalation_policy_id": "P1234ABC",
  "auto_resolve": true,
  "auto_resolve_threshold_min": 30
}
```

---

### `DELETE /v1/subscriptions/{id}/integrations/pagerduty`

Remove the PagerDuty integration. Open incidents are **not** automatically resolved.

**Response:** `204 No Content`

---

### `GET /v1/subscriptions/{id}/integrations/pagerduty/incidents`

List the most recent 100 incidents for this subscription (newest first).

**Response:** `200 OK`

```json
{
  "incidents": [
    {
      "id": "uuid",
      "dedup_key": "soroban-pulse-CABC...-contract",
      "incident_key": "0b856a0bfa784c53be2e21c3...",
      "contract_id": "CABC...",
      "event_type": "contract",
      "status": "triggered",
      "acknowledged_by": null,
      "created_at": "2026-08-31T07:00:00Z"
    }
  ]
}
```

---

### `POST /v1/subscriptions/{id}/integrations/pagerduty/incidents/acknowledge`

Acknowledge an open incident via the Events API v2. The `status` in the database is updated to `acknowledged`.

**Request body:**

```json
{
  "dedup_key": "soroban-pulse-CABC...-contract",
  "acknowledged_by": "alice@example.com"
}
```

**Response:** `200 OK`

```json
{
  "status": "acknowledged",
  "dedup_key": "soroban-pulse-CABC...-contract"
}
```

---

### `POST /v1/subscriptions/{id}/integrations/pagerduty/incidents/resolve`

Resolve an open or acknowledged incident via the Events API v2.

**Request body:**

```json
{
  "dedup_key": "soroban-pulse-CABC...-contract"
}
```

**Response:** `200 OK`

```json
{
  "status": "resolved",
  "dedup_key": "soroban-pulse-CABC...-contract"
}
```

---

## Incident Lifecycle

```
[New event] → should_trigger? ──No──→ (ignored)
                    │
                   Yes
                    │
              dedup_key = "soroban-pulse-{contract_id}-{event_type}"
                    │
              Events API v2 POST (trigger)
                    │
            ┌───────▼────────┐
            │   triggered    │
            └───────┬────────┘
                    │  acknowledge API
            ┌───────▼────────┐
            │  acknowledged  │
            └───────┬────────┘
                    │  resolve API  OR  auto-resolve after threshold
            ┌───────▼────────┐
            │    resolved    │
            └────────────────┘
```

### Deduplication

The deduplication key is `soroban-pulse-{contract_id}-{event_type}`. A burst of identical events for the same contract + event type creates exactly one incident. The `ON CONFLICT DO UPDATE` in the database ensures idempotent delivery.

### Auto-resolve

When `auto_resolve = true` (default), a background task periodically queries for incidents in `triggered` or `acknowledged` state whose contract has produced no new events of the matching type within `auto_resolve_threshold_min` minutes. Those incidents are resolved automatically via the Events API.

---

## Escalation Policies

When an `escalation_policy_id` is configured, Soroban Pulse attaches it to every triggered incident, ensuring the correct escalation path is used without manual configuration in PagerDuty.

### Listing available policies

Use the [PagerDuty REST API](https://developer.pagerduty.com/api-reference/YXBpOjI3NDgyNjU-pa-ger-duty-api) directly:

```bash
curl -H "Accept: application/vnd.pagerduty+json;version=2" \
     -H "Authorization: Token token=$ONCALL_PAGERDUTY_API_KEY" \
     https://api.pagerduty.com/escalation_policies
```

Copy the `id` of the desired policy into `escalation_policy_id` when creating the integration.

---

## On-call Lookup

When `ONCALL_PROVIDER=pagerduty` and `ONCALL_PAGERDUTY_API_KEY` are configured, Soroban Pulse can resolve the current on-call engineer:

```rust
// Example: get current on-call contact
let scheduler = OnCallScheduler::from_config(&config)?;
let contact = scheduler.current_oncall().await;
```

The result is cached for `oncall_schedule_cache_ttl_secs` (default 5 minutes) to avoid hammering the PagerDuty API during high-volume periods.

---

## Database Schema

Three tables are used (created by migration `20260831000001_pagerduty_integration.sql`):

| Table | Purpose |
|---|---|
| `pagerduty_integrations` | Per-subscription configuration (routing key, filters, escalation policy, auto-resolve settings) |
| `pagerduty_incidents` | Incident lifecycle tracking (dedup key, status, acknowledged_by, resolved_at) |
| `pagerduty_escalation_policies` | Cached escalation policy JSON from the REST API |

---

## Metrics

| Metric | Description |
|---|---|
| `soroban_pulse_pagerduty_failures_total` | Incidents that failed delivery after all retries |

---

## Troubleshooting

**No incidents appearing in PagerDuty**
- Verify `PAGERDUTY_ROUTING_KEY` matches the *Integration Key* on the PagerDuty service.
- Check whether `contract_filter` or `event_type_filter` is accidentally excluding your events.
- Inspect logs for `PagerDuty API non-success response` messages.

**Incidents not auto-resolving**
- Confirm `auto_resolve = true` is set on the integration.
- Ensure the background auto-resolve task is running (check for `Auto-resolved stale PagerDuty incident` log entries).
- Increase `auto_resolve_threshold_min` if events are arriving intermittently.

**`403` on escalation policy endpoints**
- The REST API key requires the `Read` scope on escalation policies. Regenerate a key with adequate permissions.

**Duplicate incidents**
- Each `(contract_id, event_type)` pair maps to a single dedup key. If you see duplicates, confirm all replicas are using the same dedup key derivation (no customisation of `make_dedup_key`).

---

## Related documentation

- [Subscription best practices](subscription-best-practices.md)
- [Webhook delivery and retry](webhook_signing.md)
- [On-call rotation](../src/oncall.rs) — OpsGenie and VictorOps are also supported
- [Alert rules](alerts.yml)
