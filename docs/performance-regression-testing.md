# Performance Regression Testing (Issue #591)

## Overview

Performance Regression Testing tracks and alerts on performance regressions in key queries and operations. This ensures the system maintains acceptable latency and throughput as the codebase evolves.

## Performance Baselines

The system establishes and maintains performance baselines for critical operations:

### Query Operations

| Operation           | p50 Latency | p95 Latency | p99 Latency | Throughput  |
| ------------------- | ----------- | ----------- | ----------- | ----------- |
| List events         | 50ms        | 150ms       | 300ms       | 200 ops/sec |
| Filter events       | 75ms        | 200ms       | 400ms       | 150 ops/sec |
| Create subscription | 25ms        | 75ms        | 150ms       | 400 ops/sec |
| Webhook delivery    | 100ms       | 300ms       | 600ms       | 100 ops/sec |

## Regression Detection

### Alert Threshold

Regressions are detected when:

- **p95 latency increases > 10%** above baseline
- **Throughput decreases > 10%** below baseline
- **p99 latency increases > 15%** above baseline

### Example

Baseline: p95 = 150ms
Current: p95 = 165ms (10% increase)
Result: Alert triggered

## Running Performance Tests

```bash
# Run performance regression tests
cargo test performance_regression

# Run specific baseline test
cargo test baseline_list_events_query

# Run regression detection
cargo test detect_regression_above_threshold
```

## Performance Monitoring

### Continuous Monitoring

Performance metrics are automatically collected from:

1. **Application Metrics** - Instrumented handlers and database queries
2. **Database Query Plans** - EXPLAIN ANALYZE results
3. **External Service Calls** - Webhook delivery, external APIs
4. **System Resources** - CPU, memory, disk I/O

### Metrics Collection

Metrics are collected every:

- **Query operations**: Per-request basis (aggregated)
- **Webhooks**: Per-delivery basis (aggregated)
- **Database connections**: Per-second basis

## Performance Report

Generate daily/weekly performance reports:

```bash
# Daily report
./scripts/generate_performance_report.sh --period daily

# Weekly report
./scripts/generate_performance_report.sh --period weekly

# Custom date range
./scripts/generate_performance_report.sh --from 2024-01-01 --to 2024-01-31
```

### Report Contents

- Baseline vs. current performance
- Percentile distributions (p50, p95, p99)
- Throughput analysis
- Regressions detected
- Optimization recommendations

## Automatic Alerts

### Alert Channels

Regressions trigger alerts via:

1. **Application Logs** - WARN level in structured logs
2. **Metrics** - Performance regression counters incremented
3. **Email** - On critical regressions (p95 > 20%)
4. **Slack** - Real-time notifications for High urgency

### Alert Example

```
REGRESSION DETECTED
Operation: query_events
Baseline p95: 150ms
Current p95: 165ms
Increase: +10%
Recommendation: Check query optimization, database indices
```

## Investigation Workflow

### When a Regression is Detected

1. **Verify** - Run test again to confirm
2. **Isolate** - Identify which commit introduced regression
3. **Root Cause** - Profile and analyze the problematic code
4. **Fix** - Implement optimization
5. **Verify** - Confirm performance is restored
6. **Document** - Add optimization notes to code

### Profiling Commands

```bash
# Profile with perf
cargo flamegraph --bin soroban-pulse

# Profile with criterion
cargo bench

# Analyze query plans
EXPLAIN ANALYZE SELECT * FROM events WHERE ledger > 1000;
```

## Common Regressions and Fixes

### N+1 Query Problem

**Symptom**: Linear increase in query time with more results
**Fix**: Batch queries or use JOIN

### Missing Database Index

**Symptom**: Sudden jump in query latency
**Fix**: Analyze query plans, add index if needed

### Memory Leak

**Symptom**: Gradual increase in latency over time
**Fix**: Profile memory allocation, fix leak

### Inefficient Filter

**Symptom**: Filter operations slower after code change
**Fix**: Review filter implementation, optimize

## Performance Testing Best Practices

1. **Test on Consistent Hardware** - Same server specs for valid comparisons
2. **Run Multiple Times** - Account for system variance
3. **Clear Cache Between Runs** - Avoid cache pollution
4. **Realistic Data** - Use production-like data volumes
5. **Test Under Load** - Simulate real concurrency

## Configuration

Performance thresholds can be adjusted in `config.toml`:

```toml
[performance]
# Regression alert threshold (percentage)
regression_threshold = 10.0

# Percentiles to track
percentiles = [50, 95, 99]

# Alert severity for high latency
high_latency_p95_ms = 300.0
critical_latency_p95_ms = 500.0

# Sample rate for latency collection
sample_rate = 1.0  # 1.0 = 100%, collect all
```

## Related Issues

- #590 - Contract Event Simulation
- #589 - Capacity Planning Automation
- #588 - Performance Monitoring Dashboard
