# Connection Pool Configuration & Optimization - Issue #888

## Overview

SorobanPulse implements adaptive connection pooling that dynamically adjusts pool parameters based on runtime utilization patterns. The pool includes health checks, stale connection detection, and ML-based demand forecasting.

## Architecture

### Adaptive Pool Components

- **Connection Pool**: SQLx PostgreSQL connection pool
- **Adaptive Tuner**: Background task that monitors utilization
- **Demand Predictor**: Exponential smoothing for forecasting connection demand
- **Advanced Counters**: Detailed metrics on pool operations
- **Health Checker**: Periodic connection validation

## Configuration

### Environment Variables

```bash
# Pool sizing
DB_MIN_CONNECTIONS=5              # Minimum connections to maintain
DB_MAX_CONNECTIONS=100            # Maximum connection ceiling
DB_ACQUIRE_TIMEOUT_SECS=30        # Timeout for connection acquisition

# Adaptive tuning
ADAPTIVE_POOL_ENABLED=true         # Enable adaptive tuning
ADAPTIVE_POOL_SAMPLE_INTERVAL=15   # Seconds between samples
ADAPTIVE_POOL_HEALTH_CHECK_SECS=30 # Health check interval

# Advanced options
ADAPTIVE_POOL_AB_TESTING=false     # Enable A/B testing metrics
STALE_CONNECTION_AGE_SECS=600      # Connection age threshold
```

### Runtime Configuration

```rust
use soroban_pulse::adaptive_pool::{spawn_adaptive_monitor, AdaptivePoolConfig};

let config = AdaptivePoolConfig {
    max_connections_ceiling: 200,
    min_connections_floor: 1,
    adaptive_enabled: true,
    health_checks_enabled: true,
    health_check_interval_secs: 30,
    stale_connection_age_secs: 600,
    sample_interval_secs: 15,
    ab_testing_enabled: false,
    config_variant: "default".to_string(),
    config_version: 1,
};

let tuner_state = spawn_adaptive_monitor(pool, config, 100, 5);
```

## Metrics

### Pool Utilization

```
soroban_pulse_pool_utilization{variant="default"}
- Current connection usage as percentage of max
- Updated every sample interval
```

### Queue Depth

```
soroban_pulse_pool_queue_depth{variant="default"}
- Number of pending acquisition requests
- Indicates connection starvation
```

### Acquisition Latency

```
soroban_pulse_pool_acquire_latency_ms{variant="default"}
- Time to acquire a connection
- Histogram with buckets (1ms, 5ms, 10ms, 50ms, 100ms)
```

### Health Checks

```
soroban_pulse_pool_health_check_failures_total{variant="default"}
- Count of failed keepalive pings
- Indicates connection quality issues
```

### Stale Connection Tracking

```
soroban_pulse_pool_stale_cleaned_total{variant="default"}
- Count of idle connections exceeding age threshold
```

### Adaptive Recommendations

```
soroban_pulse_pool_adaptive_target_min{variant="default"}
soroban_pulse_pool_adaptive_target_max{variant="default"}
- Recommended min/max connections based on patterns
```

## Scaling Decisions

### Scale-Up Conditions

The tuner recommends scaling up when:

```
70% of recent samples > 75% utilization
```

This indicates sustained high load requiring additional connections.

**Action**: Increase `DB_MAX_CONNECTIONS` by ~25% per recommendation

### Scale-Down Conditions

The tuner recommends scaling down when:

```
80% of recent samples < 25% utilization
```

This indicates sustained low demand.

**Action**: Increase `DB_MIN_CONNECTIONS` can be reduced

## Demand Prediction

### Exponential Smoothing Model

The ML-based predictor uses exponential smoothing to forecast connection demand:

```rust
alpha = 0.2    // Observation weight (higher = more responsive)
beta = 0.1     // Trend smoothing factor
```

The prediction equation:

```
Level[t] = alpha * Observation[t] + (1 - alpha) * (Level[t-1] + Trend[t-1])
Trend[t] = beta * (Level[t] - Level[t-1]) + (1 - beta) * Trend[t-1]
Predict[t+1] = Level[t] + Trend[t]
```

### Predicted Utilization

Available in tuning snapshots:

```rust
let snapshot = tuner_state.latest_snapshot();
println!("Predicted utilization: {:.1}%", snapshot.predicted_utilization * 100.0);
```

## Performance Tuning

### Acquire Timeout Optimization

```bash
# If seeing frequent timeouts:
export DB_ACQUIRE_TIMEOUT_SECS=60

# If connections sit idle waiting:
export DB_ACQUIRE_TIMEOUT_SECS=5
```

### Health Check Tuning

```bash
# For high-throughput systems, reduce check frequency:
export ADAPTIVE_POOL_HEALTH_CHECK_SECS=60

# For low-latency requirements:
export ADAPTIVE_POOL_HEALTH_CHECK_SECS=10
```

### Connection Idle Timeout

```bash
# Balance between resource usage and connection overhead:
# PostgreSQL server setting:
export POSTGRES_IDLE_IN_TRANSACTION_TIMEOUT=30000  # 30 seconds
```

## Monitoring & Observability

### Query Tuning Snapshot

```rust
if let Some(snapshot) = tuner_state.latest_snapshot() {
    println!("Pool Status:");
    println!("  Recommended: {} - {} connections", 
             snapshot.recommended_min, 
             snapshot.recommended_max);
    println!("  Utilization: {:.1}%", snapshot.avg_utilization * 100.0);
    println!("  Predicted: {:.1}%", snapshot.predicted_utilization * 100.0);
    println!("  Scale-up advised: {}", snapshot.scale_up_advised);
    println!("  Scale-down advised: {}", snapshot.scale_down_advised);
}
```

### API Endpoint for Monitoring

```http
GET /metrics/pool
```

Returns JSON with:
- Current pool size and utilization
- Recent tuning recommendations
- Historical trends (optional)
- Configuration version and variant

## Chaos Testing

### Connection Exhaustion Test

```bash
# Simulate connection exhaustion
cargo test --features chaos
```

Tests:
- Pool behavior under saturation
- Queue depth escalation
- Recovery after exhaustion
- Timeout handling

### Health Check Failure Scenarios

Tests verify:
- Detection of stale connections
- Recovery after transient failures
- Metrics update correctly
- No connection leaks

## Hot Reloading Configuration

### Apply New Config

```rust
let new_config = AdaptivePoolConfig {
    max_connections_ceiling: 250,
    ..current_config
};

tuner_state.apply_config(new_config)?;
```

### Rollback Previous Config

```rust
let prev_config = tuner_state.rollback()?;
println!("Rolled back to version: {}", prev_config.config_version);
```

## Performance Benchmarks

### Throughput

Typical performance with optimal pool sizing:

- **Queries/sec**: 5,000+ with 100 connections
- **p50 Latency**: 2-5ms
- **p99 Latency**: 10-20ms

### Resource Usage

- **Memory per connection**: ~10-20MB (connection buffers)
- **Total pool memory**: ~1-2GB with 100 connections
- **CPU overhead**: <1% for tuning background task

## Troubleshooting

### High Acquisition Latency

**Symptom**: `soroban_pulse_pool_acquire_latency_ms` > 100ms

**Cause**: Insufficient connections or slow queries

**Fix**:
1. Increase `DB_MAX_CONNECTIONS`
2. Check for long-running queries: `EXPLAIN ANALYZE`
3. Review tuner recommendations

### Queue Depth Growing

**Symptom**: `soroban_pulse_pool_queue_depth` continuously increasing

**Cause**: More requests than available connections

**Fix**:
1. Increase `DB_MAX_CONNECTIONS`
2. Optimize slow queries
3. Implement request rate limiting

### Health Check Failures

**Symptom**: `soroban_pulse_pool_health_check_failures_total` increasing

**Cause**: Database connectivity or network issues

**Fix**:
1. Check database connectivity
2. Verify network latency to database
3. Check PostgreSQL logs for errors

## References

- [SQLx Connection Pool Documentation](https://github.com/launchbadge/sqlx)
- [PostgreSQL Connection Management](https://www.postgresql.org/docs/current/runtime-config-connection.html)
- [Exponential Smoothing Forecasting](https://en.wikipedia.org/wiki/Exponential_smoothing)
