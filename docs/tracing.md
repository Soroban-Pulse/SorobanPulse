# Distributed Tracing Configuration (Issue #895)

Soroban Pulse implements comprehensive OpenTelemetry distributed tracing for all critical operations.

## Overview

The distributed tracing system provides:
- W3C Trace Context (traceparent / tracestate) propagation
- Span factories for all major operation stages
- Trace context propagation to webhook delivery
- Trace ID injection into response headers
- Database query tracing with query text
- Configurable sampling rates
- Integration with Jaeger, Honeycomb, and other OTel backends

## Configuration

### Environment Variables

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `TRACE_SAMPLE_RATE` | float (0-1) | `1.0` | Sampling probability for new traces |
| `TRACE_SERVICE_NAME` | string | `soroban-pulse` | Service name in traces |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | URL | - | OTLP receiver endpoint (e.g., Jaeger) |
| `OTEL_EXPORTER_OTLP_HEADERS` | string | - | Additional OTLP headers (e.g., auth) |
| `OTEL_TRACES_EXPORTER` | string | `otlp` | Trace exporter type |

## Tracing Feature

The `otel` Cargo feature enables distributed tracing:

```bash
cargo build --features otel
```

When disabled, all tracing calls compile to no-ops (zero overhead).

## Span Hierarchy

### HTTP Request Processing
```
http.request (root)
├── api.handler
│   ├── db.query_detailed (for each query)
│   └── api.query (for search operations)
└── notification.deliver (for async notifications)
```

### Indexer Pipeline
```
indexer.poll_cycle (root)
├── rpc.get_latest_ledger
├── rpc.get_events (per page)
└── event.processing (per event)
    ├── event.validate
    ├── event.dedup_check
    └── db.insert_event
```

### Webhook Delivery
```
webhook.delivery_pipeline (root)
└── webhook.deliver (per attempt)
    └── (HTTP request with injected traceparent header)
```

### Notifications
```
notification.deliver
├── Email
├── SMS
├── Slack
├── Discord
├── PagerDuty
└── Telegram
```

## Trace Context Propagation

### Incoming Requests
1. Extract trace context from HTTP headers (priority order):
   - W3C `traceparent` header (preferred)
   - `X-Trace-ID` header (legacy)
2. If no upstream trace context exists, generate new root trace

### Outgoing Requests
1. Inject `traceparent` header into webhook requests
2. Inject `X-Trace-ID` header for non-compliant downstream services
3. Propagate sampled flag to enable/disable sampling downstream

### Response Headers
All responses include trace ID headers for client-side tracing:
- `X-Trace-ID`: The root trace ID (e.g., `4bf92f3577b34da6a3ce929d0e0e4736`)
- `traceparent`: W3C trace context header

## Sampling

### Sampling Decision
- Made at HTTP request ingestion time
- Applies deterministically to all downstream operations
- Configurable per-deployment via `TRACE_SAMPLE_RATE`

### Sampling Rate Examples
| Rate | Behavior |
|------|----------|
| `0.0` | No sampling (tracing disabled) |
| `0.01` | 1% of traces sampled (high volume) |
| `0.1` | 10% of traces sampled (typical) |
| `1.0` | All traces sampled (development/debugging) |

## Integration with Jaeger

### Local Development
```bash
docker run -d \
  -p 16686:16686 \
  -p 6831:6831/udp \
  jaegertracing/all-in-one

export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4318
export TRACE_SAMPLE_RATE=1.0
cargo run --features otel
```

Then access the UI at `http://localhost:16686`

### Production (Jaeger Collector)
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://jaeger.example.com:4318
export OTEL_EXPORTER_OTLP_HEADERS="Authorization=Bearer ${JAEGER_TOKEN}"
export TRACE_SAMPLE_RATE=0.1
```

## Integration with Honeycomb

### Setup
```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io:443/v1/traces
export OTEL_EXPORTER_OTLP_HEADERS="x-honeycomb-team=${HONEYCOMB_API_KEY}"
export TRACE_SERVICE_NAME=soroban-pulse
export TRACE_SAMPLE_RATE=0.1
```

### Querying
Use Honeycomb's UI to:
1. Filter by service name: `soroban_pulse`
2. Drill down by span type (e.g., `webhook.deliver`)
3. Analyze latency distributions
4. Create alerts on error rates

## Span Attributes

All spans include structured fields:

### HTTP Request Span
- `http.method`: GET, POST, etc.
- `http.target`: Request path
- `http.status_code`: Response code
- `trace.id`: W3C trace ID

### Database Query Span
- `db.system`: postgresql
- `db.operation`: SELECT, INSERT, UPDATE, DELETE
- `db.table`: Table name
- `db.query_text`: Full query (sanitized)
- `db.rows_affected`: Rows modified
- `db.duration_ms`: Query latency

### Webhook Delivery Span
- `webhook.url`: Target URL
- `webhook.contract_id`: Contract ID
- `webhook.attempt`: Attempt number
- `webhook.status_code`: HTTP response code
- `webhook.latency_ms`: Delivery latency

## Metrics

Tracing exports the following metrics:

| Metric | Type | Description |
|--------|------|-------------|
| `soroban_pulse_trace_spans_created_total` | Counter | Spans created (labeled by span_name) |
| `soroban_pulse_trace_samples_total` | Counter | Sampling decisions (labeled by sampled) |
| `soroban_pulse_trace_sample_rate` | Gauge | Current sampling rate (0-1) |
| `soroban_pulse_trace_injection_latency_ms` | Gauge | Header injection latency |

## Best Practices

1. **Always propagate trace context**: Include `traceparent` header in all outgoing HTTP requests
2. **Sample judiciously**: High sampling rates in production can impact performance
3. **Limit query text**: Query text is sampled; sensitive data is redacted
4. **Monitor span count**: Watch for N+1 query patterns in distributed traces
5. **Set meaningful service names**: Use deployment-specific names (e.g., `soroban-pulse-prod`)

## Troubleshooting

### Traces not appearing in backend
- Check `OTEL_EXPORTER_OTLP_ENDPOINT` is reachable
- Verify `TRACE_SAMPLE_RATE > 0`
- Ensure collector is receiving data: check collector logs

### High latency in traces
- Reduce `TRACE_SAMPLE_RATE` to decrease backend load
- Check OTLP endpoint latency
- Verify network connectivity to collector

### Missing traces
- Verify `otel` feature is enabled: `cargo build --features otel`
- Check sampling decision: `TRACE_SAMPLE_RATE` may be too low
- Examine application logs for trace-related errors
