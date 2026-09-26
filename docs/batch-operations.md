# Batch Event Operations

## Overview

The Batch Event Operations API (Issue #931) lets clients perform high-throughput
create, read, update, and delete operations across multiple events in a single
HTTP round-trip.  All write operations are transactional where possible and
produce an audit log entry.

Batch endpoints complement the single-event REST API.  Use them when you need to:
- Retrieve dozens or hundreds of events by UUID without issuing N+1 requests.
- Bulk-delete events for GDPR compliance or data-retention enforcement.
- Apply tags or metadata to a set of events atomically.
- Update multiple subscriptions in one call.
- Apply a transformation pipeline to event payloads (masking, renaming, dropping fields).

All batch endpoints are versioned under `/v1/`.

---

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/v1/events/batch/retrieve` | Fetch up to 500 events by UUID |
| `POST` | `/v1/events/batch/delete` | Soft-delete events with audit trail |
| `POST` | `/v1/events/batch/tag` | Apply key-value tags to events |
| `POST` | `/v1/events/batch/subscriptions` | Bulk-update subscriptions |
| `POST` | `/v1/events/batch/transform` | Apply transformation pipeline |
| `GET`  | `/v1/events/batch/progress/{job_id}` | Poll long-running job progress |

---

## Request / Response Examples

### `POST /v1/events/batch/retrieve`

Fetch a specific set of events in one round-trip.

**Request**

```json
{
  "ids": [
    "018e2c7a-1234-7abc-9abc-000000000001",
    "018e2c7a-1234-7abc-9abc-000000000002"
  ],
  "page": 1,
  "limit": 100
}
```

**Response `200 OK`**

```json
{
  "data": [
    {
      "id": "018e2c7a-1234-7abc-9abc-000000000001",
      "contract_id": "CABC...",
      "event_type": "contract",
      "tx_hash": "abc123",
      "ledger": 5000000,
      "timestamp": "2026-08-01T12:00:00Z",
      "event_data": { "amount": 100 },
      "created_at": "2026-08-01T12:00:01Z"
    }
  ],
  "found": 1,
  "not_found": ["018e2c7a-1234-7abc-9abc-000000000002"],
  "page": 1,
  "limit": 100
}
```

**Limits**

| Parameter | Default | Maximum |
|-----------|---------|---------|
| `ids` count | — | 500 |
| `limit` | 100 | 500 |

---

### `POST /v1/events/batch/delete`

Soft-delete a set of events and record an audit trail entry.

> Requires `ADMIN_API_KEY`.

**Request**

```json
{
  "ids": [
    "018e2c7a-1234-7abc-9abc-000000000001"
  ],
  "reason": "GDPR erasure request #REQ-4291",
  "operator": "alice@example.com"
}
```

**Response `200 OK`**

```json
{
  "deleted": 1,
  "not_found": [],
  "audit_id": "9b2e5c01-aaaa-bbbb-cccc-111111111111",
  "deleted_at": "2026-08-01T14:30:00Z"
}
```

**Limits**

| Parameter | Maximum |
|-----------|---------|
| `ids` count | 200 |

---

### `POST /v1/events/batch/tag`

Apply key-value tags to a set of events.  Tags are stored under `event_data._tags`.

**Request**

```json
{
  "ids": ["018e2c7a-..."],
  "tags": {
    "reviewed": "true",
    "priority": "high"
  },
  "replace": false
}
```

Set `"replace": true` to overwrite all existing tags.  Default is `false`
(merge — existing tags are preserved).

**Response `200 OK`**

```json
{
  "updated": 1,
  "not_found": []
}
```

---

### `POST /v1/events/batch/subscriptions`

Update multiple subscriptions in one request.  Only the fields you include in
each update entry are modified; omitted fields are left unchanged.

**Request**

```json
{
  "updates": [
    {
      "id": "sub-uuid-1",
      "webhook_url": "https://new.example.com/hook",
      "active": true
    },
    {
      "id": "sub-uuid-2",
      "active": false
    }
  ]
}
```

**Response `200 OK`**

```json
{
  "updated": 2,
  "not_found": [],
  "failed": []
}
```

---

### `POST /v1/events/batch/transform`

Apply an ordered transformation pipeline to a set of events.

**Supported transform ops**

| Op | Description |
|----|-------------|
| `mask_field` | Replace field value with `"***"` |
| `drop_field` | Remove the field entirely |
| `rename_field` | Rename a field (provide new name in `value`) |
| `set_field` | Set a field to a new value |

**Request**

```json
{
  "ids": ["018e2c7a-..."],
  "pipeline": [
    { "op": "mask_field", "field": "user_address" },
    { "op": "rename_field", "field": "amount", "value": "transfer_amount" }
  ],
  "persist": false
}
```

When `persist` is `false` (default), the transformed data is returned in the
response without modifying the database.  Set `persist: true` to write changes
back.

**Response `200 OK`**

```json
{
  "transformed": 1,
  "data": [
    {
      "id": "018e2c7a-...",
      "event_data": {
        "user_address": "***",
        "transfer_amount": 100
      }
    }
  ],
  "not_found": [],
  "errors": []
}
```

---

### `GET /v1/events/batch/progress/{job_id}`

Poll the progress of a long-running batch job.

**Response `200 OK`**

```json
{
  "job_id": "9b2e5c01-...",
  "total": 5000,
  "processed": 3200,
  "succeeded": 3195,
  "failed": 5,
  "status": "running",
  "created_at": "2026-08-01T12:00:00Z",
  "updated_at": "2026-08-01T12:01:30Z",
  "message": null
}
```

**Status values**

| Status | Meaning |
|--------|---------|
| `pending` | Accepted, not started |
| `running` | Actively processing |
| `completed` | All items succeeded |
| `partial_success` | Finished with some failures |
| `failed` | Catastrophic failure — no items processed |

---

## Streaming Response Format

For very large batch retrieve operations the server will eventually support
chunked `Transfer-Encoding: chunked` with NDJSON streaming.  Send:

```
Accept: application/x-ndjson
```

Each line will be a complete JSON event object.  Current implementation returns
a standard JSON response.

---

## Audit Logging Behavior

The `batch_delete_events` endpoint writes a row to the `audit_logs` table
with the following fields:

| Field | Value |
|-------|-------|
| `action` | `"batch_delete"` |
| `entity` | `"event"` |
| `entity_ids` | JSON array of deleted UUIDs |
| `reason` | Caller-provided reason string |
| `operator` | Caller-provided operator identity |
| `created_at` | Server UTC timestamp |

Audit log entries are append-only and cannot be deleted via the API.

---

## Progress Tracking

Long-running batch jobs return a `job_id` that you can poll via
`GET /v1/events/batch/progress/{job_id}`.  Progress objects include:
- `total` — total items submitted
- `processed` — items attempted so far
- `succeeded` / `failed` — outcome counts
- `status` — overall job state
- `updated_at` — timestamp of the last update

---

## Performance Benchmarks

| Operation | Dataset | Mean | p99 |
|-----------|---------|------|-----|
| `batch_retrieve_events` | 100 IDs, 10k events | ~3 ms | ~8 ms |
| `batch_delete_events` | 50 IDs | ~4 ms | ~10 ms |
| `batch_tag_events` | 100 IDs, merge | ~2 ms | ~6 ms |
| `batch_transform_events` | 50 events, 3-step pipeline | ~5 ms | ~12 ms |

Benchmarks are run by `cargo bench --bench batch_operations` against a
local PostgreSQL instance pre-seeded with 10,000 events.

---

## Error Handling

All batch endpoints return standard JSON error responses:

```json
{
  "error": "too many ids; maximum is 500",
  "code": "VALIDATION_ERROR",
  "correlation_id": "..."
}
```

| HTTP Status | When |
|-------------|------|
| `400 Bad Request` | Empty or oversized ID list, malformed body |
| `401 Unauthorized` | Missing auth header |
| `403 Forbidden` | Insufficient privileges (delete/admin ops) |
| `422 Unprocessable Entity` | Body parses but fails validation |
| `500 Internal Server Error` | Database failure |

Partial failures within a batch (e.g. some IDs not found) are **not** 4xx
errors — they are reported inline in the `not_found` / `errors` arrays within
a `200 OK` response.
