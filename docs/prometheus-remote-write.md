# Prometheus Remote Write Integration

Issue #953: Enable pushing metrics to Prometheus remote write endpoints.

## Overview

The Prometheus Remote Write integration allows SorobanPulse to push metrics to remote Prometheus instances or compatible systems. This enables:

- **Centralized metrics collection** across multiple SorobanPulse instances
- **Custom metric filtering** for selective metric submission
- **Automatic retry logic** with exponential backoff for reliability
- **Batch submission** for efficient resource usage
- **Health monitoring** of remote write endpoints

## Configuration

### Environment Variables

Configure Prometheus remote write via environment variables:

```bash
# Required: Comma-separated list of remote write endpoints
PROMETHEUS_REMOTE_WRITE_ENDPOINTS=https://prometheus.example.com/api/v1/write,https://backup-prometheus.example.com/api/v1/write

# Optional: Batch size for metric submissions (default: 100)
PROMETHEUS_REMOTE_WRITE_BATCH_SIZE=100

# Optional: Flush interval in seconds (default: 60)
PROMETHEUS_REMOTE_WRITE_FLUSH_INTERVAL=60

# Optional: Request timeout in seconds (default: 10)
PROMETHEUS_REMOTE_WRITE_TIMEOUT=10

# Optional: Maximum retry attempts (default: 3)
PROMETHEUS_REMOTE_WRITE_MAX_RETRIES=3

# Optional: Retry delay in milliseconds (default: 100)
PROMETHEUS_REMOTE_WRITE_RETRY_DELAY_MS=100
```

### Metric Filtering

Control which metrics are sent to remote write endpoints:

```rust
use soroban_pulse::prometheus_remote_write::{
    PrometheusRemoteWriteConfig, MetricFilterConfig, PrometheusRemoteWritePublisher
};

let config = PrometheusRemoteWriteConfig {
    endpoints: vec!["https://prometheus.example.com/api/v1/write".to_string()],
    batch_size: 100,
    metric_filter: Some(MetricFilterConfig {
        // Only include metrics matching these patterns
        include_patterns: vec!["soroban_pulse".to_string()],
        // Exclude internal/diagnostic metrics
        exclude_patterns: vec!["internal".to_string(), "debug".to_string()],
    }),
    ..Default::default()
};

let publisher = PrometheusRemoteWritePublisher::new(config);
```

## Usage

### Pushing Metrics

```rust
use soroban_pulse::prometheus_remote_write::{RemoteWriteMetric, RemoteWritePublisher};

// Create metrics
let metrics = vec![
    RemoteWriteMetric::new("soroban_pulse_events_indexed_total".to_string(), 1000.0)
        .with_labels(vec![("source".to_string(), "ledger".to_string())]),
];

// Push to remote endpoints
publisher.push_metrics(metrics).await?;
```

### Health Checks

Monitor the health of remote write endpoints:

```rust
// Check if all endpoints are healthy
match publisher.health_check().await {
    Ok(_) => println!("Remote write endpoints are healthy"),
    Err(e) => eprintln!("Remote write health check failed: {}", e),
}
```

## Metrics

The integration tracks its own health via metrics:

- `soroban_pulse_prometheus_remote_write_success_total` - Counter of successful metric submissions
- `soroban_pulse_prometheus_remote_write_failures_total` - Counter of failed submission attempts
- `soroban_pulse_prometheus_remote_write_health` - Gauge indicating endpoint health (1.0 = healthy, 0.0 = unhealthy)

## Retry Logic

When a metric submission fails, the publisher automatically retries with exponential backoff:

1. Initial delay: `PROMETHEUS_REMOTE_WRITE_RETRY_DELAY_MS` (default: 100ms)
2. Backoff: 2x multiplier on each retry
3. Max retries: `PROMETHEUS_REMOTE_WRITE_MAX_RETRIES` (default: 3)

Example backoff schedule (default config):
- Attempt 1: 0ms
- Attempt 2: 100ms
- Attempt 3: 200ms
- Attempt 4: 400ms

## Multi-Endpoint Support

The integration supports pushing to multiple Prometheus remote write endpoints simultaneously. If any endpoint fails, the operation continues with other endpoints. Only if all endpoints fail is an error returned.

```rust
let config = PrometheusRemoteWriteConfig {
    endpoints: vec![
        "https://primary-prometheus.example.com/api/v1/write".to_string(),
        "https://secondary-prometheus.example.com/api/v1/write".to_string(),
    ],
    ..Default::default()
};
```

## Performance Considerations

- **Batch submission**: Metrics are submitted in configurable batches to reduce network overhead
- **Metric filtering**: Use include/exclude patterns to reduce the volume of metrics sent
- **Timeout handling**: Slow endpoints won't block metric collection with the configurable timeout
- **Async operation**: Remote write operations are non-blocking

## Troubleshooting

### Connection Failures

If connections fail:
1. Verify the remote endpoint URL is accessible
2. Check network connectivity and firewall rules
3. Ensure the endpoint supports the Prometheus remote write protocol (v0.1.0)

### High Latency

If remote write operations are slow:
1. Increase `PROMETHEUS_REMOTE_WRITE_TIMEOUT`
2. Reduce `PROMETHEUS_REMOTE_WRITE_BATCH_SIZE` for smaller payloads
3. Check network latency to the remote endpoint

### Filtered Out Metrics

If expected metrics aren't appearing:
1. Check `include_patterns` and `exclude_patterns` configuration
2. Verify metric names match the patterns exactly
3. Review logs for metric filtering decisions

## Integration with Grafana

Use the remote Prometheus data source in Grafana to visualize SorobanPulse metrics collected via remote write:

1. Add a new Prometheus data source pointing to your remote Prometheus instance
2. Create dashboards using queries like:
   ```promql
   rate(soroban_pulse_events_indexed_total[5m])
   soroban_pulse_indexer_lag_ledgers
   ```

## Testing

The module includes comprehensive tests:

```bash
cargo test prometheus_remote_write
```

Mock implementations are available for testing:

```rust
use soroban_pulse::prometheus_remote_write::mock::MockRemoteWritePublisher;

let publisher = MockRemoteWritePublisher::new();
// Use in tests without network access
```
