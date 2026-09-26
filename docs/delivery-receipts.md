# Notification Delivery Receipts

_Issue #933_ — Extended delivery receipt tracking for SMS, webhook, and other
notification channels, with retention management and an aggregated stats view.

## Overview

Every notification delivery attempt is recorded in the
`notification_deliveries` table. This gives operators a durable audit trail of
exactly which events triggered a notification, which channels were used, whether
delivery succeeded, and how long it took.

Migration `20260830000003_notification_delivery_receipts.sql` extends the
original table (created by `20260625000001_notification_deliveries.sql`) with
three new columns:

| Column | Type | Description |
|---|---|---|
| `channel_metadata` | `JSONB` | Provider-specific context (e.g. HTTP status code, Twilio SID, SMTP message-ID). |
| `retry_count` | `INT` | Number of prior attempts before this final outcome (0 = first attempt). |
| `latency_ms` | `INT` | Wall-clock milliseconds from initial dispatch to final outcome. |

A `notification_delivery_stats` view aggregates these columns per
`(channel_type, status)` for efficient dashboard queries.

## Data Model

```
notification_deliveries
├── id                UUID  PK
├── channel_type      TEXT   -- "webhook", "email", "sms", ...
├── channel_config_id UUID?  -- FK to notification_channels
├── event_id          UUID?  -- FK to events (NULL for batch notifications)
├── status            TEXT   -- "success" | "failure"
├── delivered_at      TIMESTAMPTZ
├── error             TEXT?  -- populated on failure
├── channel_metadata  JSONB? -- NEW: provider context
├── retry_count       INT    -- NEW: attempts before this outcome
└── latency_ms        INT?   -- NEW: end-to-end delivery latency
```

### Stats View

```sql
SELECT * FROM notification_delivery_stats;
-- channel_type | status  | count | avg_latency_ms | last_delivery
-- -------------|---------|-------|----------------|---------------------
-- webhook      | success | 1042  | 83.4           | 2026-08-30 12:35:00
-- webhook      | failure | 12    | 250.1          | 2026-08-29 08:00:00
-- sms          | success | 304   | 412.0          | 2026-08-30 12:30:00
```

## API (Rust)

All functions live in `src/notification_delivery.rs`.

### Record a delivery (basic)

```rust
use soroban_pulse::notification_delivery::{record_delivery, DeliveryStatus};

record_delivery(
    &pool,
    "webhook",
    Some(channel_config_id),
    Some(event_id),
    DeliveryStatus::Success,
    None,          // error string
)
.await;
```

### Record a delivery with metadata (Issue #933)

```rust
use soroban_pulse::notification_delivery::{
    record_delivery_with_metadata, DeliveryStatus,
};
use serde_json::json;

let metadata = json!({
    "http_status": 200,
    "response_body": "OK",
    "provider": "twilio"
});

record_delivery_with_metadata(
    &pool,
    "sms",
    Some(channel_config_id),
    Some(event_id),
    DeliveryStatus::Success,
    None,                    // error
    Some(&metadata),         // channel_metadata
    2,                       // retry_count (0-indexed: this was the 3rd attempt)
    Some(412),               // latency_ms
)
.await;
```

### Fetch aggregated stats

```rust
use soroban_pulse::notification_delivery::get_receipt_stats;

let stats = get_receipt_stats(&pool).await?;
for s in &stats {
    println!(
        "{}: {}/{} success ({:.0}%), avg latency: {:.0} ms",
        s.channel_type,
        s.successful,
        s.total_deliveries,
        s.success_rate * 100.0,
        s.avg_latency_ms.unwrap_or(0.0),
    );
}
```

### Fetch all receipts for a specific event

```rust
use soroban_pulse::notification_delivery::get_receipts_by_event;

let receipts = get_receipts_by_event(&pool, event_id).await?;
for r in &receipts {
    println!(
        "  {} via {} — {} (retry #{}, {}ms)",
        r.id,
        r.channel_type,
        r.status,
        r.retry_count,
        r.latency_ms.unwrap_or(0),
    );
}
```

### Purge old receipts

```rust
use soroban_pulse::notification_delivery::purge_old_receipts;

// Delete receipts older than 90 days and log the count.
let deleted = purge_old_receipts(&pool, 90).await?;
println!("Purged {} old receipts", deleted);
```

## Struct Reference

### `DeliveryReceipt`

The original lightweight receipt. Returned by `query_deliveries`.

```rust
pub struct DeliveryReceipt {
    pub id:               Uuid,
    pub channel_type:     String,
    pub channel_config_id: Option<Uuid>,
    pub event_id:         Option<Uuid>,
    pub status:           String,
    pub delivered_at:     DateTime<Utc>,
    pub error:            Option<String>,
}
```

### `DeliveryReceiptExtended`

Full receipt with the new Issue #933 columns. Returned by
`get_receipts_by_event`.

```rust
pub struct DeliveryReceiptExtended {
    // ... all DeliveryReceipt fields, plus:
    pub channel_metadata: Option<serde_json::Value>,
    pub retry_count:      i32,
    pub latency_ms:       Option<i32>,
}
```

### `ReceiptStats`

Per-channel aggregate returned by `get_receipt_stats`.

```rust
pub struct ReceiptStats {
    pub channel_type:      String,
    pub total_deliveries:  i64,
    pub successful:        i64,
    pub failed:            i64,
    pub success_rate:      f64,    // 0.0 – 1.0
    pub avg_latency_ms:    Option<f64>,
}
```

## Indexes

The migration adds two composite indexes to keep common query patterns fast:

| Index | Columns | Purpose |
|---|---|---|
| `idx_notification_deliveries_retention` | `delivered_at` | Range scan for `purge_old_receipts` |
| `idx_notification_deliveries_channel` | `(channel_type, status, delivered_at DESC)` | Per-channel filtered history queries |

## Retention Policy

Call `purge_old_receipts(pool, retention_days)` periodically to remove old
receipts. A reasonable starting policy is **90 days**, matching the default
replay max age. Adjust based on your compliance requirements.

```bash
# Example: run the pruner daily via a cron job or the pruner task
RETENTION_DAYS=90
```

## Metrics Integration

`record_delivery` and `record_delivery_with_metadata` both increment the
matching Prometheus counters defined in `src/metrics.rs`:

- `soroban_pulse_notification_deliveries_total{status="success"}`
- `soroban_pulse_notification_deliveries_total{status="failure"}`

These counters are always incremented **before** the DB write, so Prometheus
metrics remain accurate even if the DB insert fails.
