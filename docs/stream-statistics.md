# Real-Time Event Stream Statistics

## Overview

The Stream Statistics API (Issue #929) provides live metrics about the
Server-Sent Event (SSE) broadcast pipeline.  It tracks throughput, payload
size distribution, per-contract event counts, moving averages, and anomaly
signals — all updated on every event broadcast.

Use this API to:
- Monitor how many events are flowing through the system per second.
- Detect traffic spikes or unexpected drops in event activity.
- Identify contracts that are dominating the stream.
- Feed dashboards and alerting systems with real-time data.

---

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/v1/stats/stream` | Point-in-time snapshot of all stream metrics |
| `GET` | `/v1/stats/stream/throughput` | Time-series of events-per-second |
| `GET` | `/v1/stats/stream/{contract_id}` | Per-contract stream statistics |

---

## `GET /v1/stats/stream`

Returns a snapshot of current stream statistics.

**Response `200 OK`**

```json
{
  "events_broadcast_total": 1048576,
  "events_per_second": 42.3,
  "moving_averages": {
    "one_min": 38.1,
    "five_min": 35.7,
    "fifteen_min": 33.2
  },
  "payload_size_distribution": {
    "min": 128,
    "max": 4096,
    "avg": 512.4,
    "p50": 480.0,
    "p95": 1200.0,
    "p99": 2800.0,
    "count": 1000
  },
  "broadcast_latency_ms": {
    "min": 0.5,
    "max": 180.0,
    "avg": 12.4,
    "p50": 8.0,
    "p95": 45.0,
    "p99": 120.0,
    "count": 1000
  },
  "event_type_distribution": {
    "contract": 920000,
    "diagnostic": 100000,
    "system": 28576
  },
  "unique_contracts_total": 314,
  "active_sse_connections": 7,
  "anomalies": [],
  "last_event_at": "2026-08-30T10:59:00Z",
  "uptime_secs": 86400
}
```

---

## `GET /v1/stats/stream/throughput`

Returns a time-series of events-per-second readings covering up to the last
60 one-second buckets.

**Response `200 OK`**

```json
{
  "active_sse_connections": 7,
  "throughput_series": [
    { "timestamp": "2026-08-30T10:58:00Z", "events_per_second": 38.0, "event_count": 38 },
    { "timestamp": "2026-08-30T10:58:01Z", "events_per_second": 41.0, "event_count": 41 }
  ]
}
```

---

## `GET /v1/stats/stream/{contract_id}`

Returns stream statistics for a single contract.

**Response `200 OK`**

```json
{
  "event_count": 50000,
  "event_type_distribution": {
    "contract": 49800,
    "diagnostic": 200
  },
  "avg_payload_bytes": 480.5,
  "last_event_at": "2026-08-30T10:59:00Z",
  "events_per_second_1m": 12.3
}
```

**Response `404 Not Found`** — no events have been broadcast for this contract.

---

## StreamStats Object Reference

| Field | Type | Description |
|-------|------|-------------|
| `events_broadcast_total` | `integer` | Cumulative events broadcast since service start |
| `events_per_second` | `float` | Instantaneous EPS (rolling 60-second window) |
| `moving_averages.one_min` | `float` | 1-minute EMA of EPS |
| `moving_averages.five_min` | `float` | 5-minute EMA of EPS |
| `moving_averages.fifteen_min` | `float` | 15-minute EMA of EPS |
| `payload_size_distribution` | `Distribution` | Stats over event payload sizes (bytes) |
| `broadcast_latency_ms` | `Distribution` | Latency from indexing to broadcast (ms) |
| `event_type_distribution` | `object` | Count by event type string |
| `per_contract` | `object` | Per-contract `ContractStats` (top 100) |
| `unique_contracts_total` | `integer` | Unique contracts seen since start |
| `active_sse_connections` | `integer` | Current SSE client count |
| `anomalies` | `AnomalySignal[]` | Detected anomalies (empty when all is well) |
| `last_event_at` | `string (ISO-8601)` | Timestamp of the most recent broadcast event |
| `uptime_secs` | `integer` | Seconds since service start |

### Distribution Object

| Field | Type | Description |
|-------|------|-------------|
| `min` | `float` | Minimum observed value |
| `max` | `float` | Maximum observed value |
| `avg` | `float` | Arithmetic mean |
| `p50` | `float` | 50th percentile (median) |
| `p95` | `float` | 95th percentile |
| `p99` | `float` | 99th percentile |
| `count` | `integer` | Sample count (bounded at 1000) |

### AnomalySignal Object

| Field | Type | Description |
|-------|------|-------------|
| `kind` | `string` | One of: `spike`, `drop`, `contract_domination`, `large_payload` |
| `message` | `string` | Human-readable description |
| `severity` | `float (0–1)` | Normalised severity score |
| `detected_at` | `string (ISO-8601)` | When the anomaly was first detected |

**Anomaly rules**

| Kind | Condition |
|------|-----------|
| `spike` | Current EPS > 3× the 5-minute EMA |
| `drop` | 5-minute EMA > 1 EPS but current EPS < 0.01 |
| `contract_domination` | One contract accounts for > 80% of total events |
| `large_payload` | p99 payload size > 50 KB (configurable) |

---

## Integrating with Grafana

The stream statistics endpoints can be polled by Grafana's JSON datasource
plugin to create real-time panels.

**Polling interval**: 5 seconds is recommended.

**Useful PromQL-equivalent queries over the JSON datasource:**

- Current EPS: `events_per_second`
- 1-minute average: `moving_averages.one_min`
- Active connections: `active_sse_connections`
- p99 broadcast latency: `broadcast_latency_ms.p99`

A pre-built Grafana dashboard JSON that includes a *Stream Statistics* row is
available at [`docs/grafana-dashboard.json`](grafana-dashboard.json).

---

## Alert Thresholds

The following Prometheus alert rules are provided in
[`docs/alerts.yml`](alerts.yml):

| Alert | Condition | Severity |
|-------|-----------|----------|
| `SorobanPulseStreamStatsStale` | `sse_active_connections` not updated in > 60 s | warning |
| `SorobanPulseEpsSpikeDetected` | anomaly kind = `spike` present in last snapshot | warning |
| `SorobanPulseEpsDropDetected` | anomaly kind = `drop` present in last snapshot | critical |
| `SorobanPulseBroadcastLatencyHigh` | p99 broadcast latency > 500 ms | warning |

Adjust thresholds in `alerts.yml` to match your traffic patterns.
