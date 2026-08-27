# Webhook Endpoint Circuit Breaker

Issue #879: Webhook endpoint circuit breaker implementation

## Overview

The circuit breaker pattern prevents cascading failures and reduces wasted retries for failing webhook endpoints. This implementation automatically manages endpoint health and backs off from endpoints that are not responding.

## States

The circuit breaker has three states:

### Closed (Normal Operation)
- **Description**: Circuit is closed, requests pass through normally
- **Behavior**: All requests are sent to the webhook endpoint
- **Transition**: Opens when failure threshold is exceeded or failure rate exceeds threshold
- **Metrics**: Tracks all requests normally

### Open (Circuit Broken)
- **Description**: Circuit is open, requests are rejected immediately
- **Behavior**: Requests are rejected without attempting delivery
- **Transition**: Transitions to Half-Open after configured duration
- **Purpose**: Prevents wasted network requests to failing endpoints
- **Metrics**: Records rejections as circuit breaker events

### Half-Open (Testing)
- **Description**: Circuit is half-open, testing if service recovered
- **Behavior**: A limited number of requests are allowed through
- **Transition**: 
  - Closes if success threshold is reached
  - Opens immediately on any failure
- **Purpose**: Allows gradual recovery without immediately flooding failing endpoints

## Configuration

Default configuration with reasonable defaults:

```rust
CircuitBreakerConfig {
    failure_threshold: 5,              // Open after 5 consecutive failures
    open_duration_secs: 60,            // Stay open for 60 seconds
    success_threshold_half_open: 3,    // Close after 3 successes in half-open
    failure_rate_threshold: 0.5,       // Open if failure rate > 50%
}
```

## Exponential Backoff

When the circuit is open, the retry strategy uses exponential backoff:

```
Backoff = 2^(failures-1) seconds, capped at 3600 seconds (1 hour)

Examples:
- 1 failure:  1 second
- 2 failures: 2 seconds
- 3 failures: 4 seconds
- 4 failures: 8 seconds
- 5 failures: 16 seconds
- ...
- 12 failures: 2048 seconds (~34 minutes)
- 13+ failures: 3600 seconds (1 hour, capped)
```

## Admin Endpoints

### Get All Circuit Breaker Statistics

```
GET /v1/admin/webhook/circuit-breaker
```

Response:
```json
[
  {
    "endpoint": "https://example.com/webhook",
    "state": "CLOSED",
    "failure_count": 0,
    "success_count": 0,
    "total_requests": 1000,
    "total_failures": 50,
    "failure_rate": 0.05,
    "backoff_seconds": 0,
    "opened_at": null,
    "half_open_at": null
  },
  {
    "endpoint": "https://unreliable.com/webhook",
    "state": "OPEN",
    "failure_count": 5,
    "success_count": 0,
    "total_requests": 100,
    "total_failures": 75,
    "failure_rate": 0.75,
    "backoff_seconds": 16,
    "opened_at": "2024-08-27T10:00:00Z",
    "half_open_at": null
  }
]
```

### Get Endpoint Circuit Breaker Status

```
GET /v1/admin/webhook/circuit-breaker/{endpoint}
```

Returns the same format as above for a single endpoint.

### Reset Circuit Breaker

```
POST /v1/admin/webhook/circuit-breaker/{endpoint}/reset
```

Manually reset a circuit breaker to the closed state, clearing all counters.

Response:
```json
{
  "status": "reset",
  "endpoint": "https://example.com/webhook"
}
```

## Integration with Webhook Delivery

The circuit breaker integrates with webhook delivery in two ways:

### 1. Pre-Check Before Delivery
Before attempting to deliver to an endpoint:
```rust
if !circuit_breaker.allow_request(&endpoint) {
    // Circuit is open, don't send
    queue_retry_with_backoff(&endpoint);
    return;
}
```

### 2. Record Result After Delivery
After delivery attempt:
```rust
match delivery_result {
    Ok(_) => circuit_breaker.record_success(&endpoint),
    Err(_) => circuit_breaker.record_failure(&endpoint),
}
```

## Metrics Recorded

### Per-Endpoint Metrics
- `soroban_pulse_circuit_breaker_opened_total`: Total times circuit opened
- `soroban_pulse_circuit_breaker_closed_total`: Total times circuit closed
- `soroban_pulse_circuit_breaker_half_open_total`: Total times circuit went half-open
- `soroban_pulse_circuit_breaker_success_total`: Total successful requests per endpoint
- `soroban_pulse_circuit_breaker_failure_total`: Total failed requests per endpoint
- `soroban_pulse_circuit_breaker_rejection_total`: Total rejected requests (circuit open)

### Dashboard Queries
```promql
# Current circuit states
group by (endpoint) (soroban_pulse_circuit_breaker_state)

# Failure rate by endpoint
soroban_pulse_circuit_breaker_failure_total / soroban_pulse_circuit_breaker_requests_total

# Circuit opens per hour
increase(soroban_pulse_circuit_breaker_opened_total[1h])
```

## Failure Detection

The circuit breaker considers a request failed if:
- HTTP response status is not 2xx
- Network timeout occurs
- Connection refused
- Request sending fails
- Invalid response received

Failures are tracked in a time-windowed list (last hour).

## State Transitions

```
CLOSED
├─ On 5 consecutive failures → OPEN
└─ On 50%+ failure rate → OPEN

OPEN
├─ After 60 seconds → HALF_OPEN
└─ On manual reset → CLOSED

HALF_OPEN
├─ On 3 consecutive successes → CLOSED
├─ On any failure → OPEN
└─ (No explicit timeout)
```

## Best Practices

### 1. Monitoring
- Monitor the `/v1/admin/webhook/circuit-breaker` endpoint regularly
- Alert when circuits open
- Track circuit state transitions

### 2. Manual Intervention
- Manually reset circuits after fixing underlying issues
- Don't reset circuits for endpoints that are still failing
- Use circuit state as an early warning system

### 3. Configuration Tuning
- Adjust `failure_threshold` based on your tolerance:
  - Lower (e.g., 3) for critical endpoints
  - Higher (e.g., 10) for less critical endpoints
- Adjust `open_duration_secs` based on typical recovery time:
  - Shorter (e.g., 30s) for frequently failing endpoints
  - Longer (e.g., 300s) to give endpoints more time to recover

### 4. Integration
- Implement retries with exponential backoff at the application level
- Use circuit breaker state to adjust alerting sensitivity
- Store circuit breaker metrics for historical analysis

## Example Scenarios

### Scenario 1: Temporary Outage
1. Endpoint stops responding (5+ failures)
2. Circuit opens, stops sending requests
3. Exponential backoff prevents wasted requests
4. Endpoint recovers after 60 seconds
5. Circuit transitions to half-open, tests with 3 requests
6. All 3 requests succeed, circuit closes
7. Normal operation resumes

### Scenario 2: Persistent Failure
1. Endpoint has configuration issue (incorrect API key)
2. All requests fail, circuit opens
3. Stays open even after timeout (all test requests in half-open fail)
4. Returns to open state
5. Operator investigates and manually resets after fixing config
6. Circuit closes and operation resumes

### Scenario 3: Intermittent Failures
1. Endpoint experiences intermittent failures (50%+ failure rate)
2. Circuit opens due to failure rate threshold
3. Exponential backoff reduces load
4. Endpoint recovers with lighter load
5. Circuit transitions to half-open
6. Some test requests succeed
7. Circuit closes, normal operation resumes

## Testing

```bash
# Run circuit breaker tests
cargo test --lib webhook_circuit_breaker

# Test scenarios:
# - State transitions
# - Failure rate calculation
# - Exponential backoff
# - Lock ID generation (determinism)
```

## Troubleshooting

### Circuit Won't Close
- Check if endpoint is actually healthy
- Verify network connectivity
- Check endpoint logs for errors
- Manually reset if appropriate

### Circuit Opens Too Frequently
- Check endpoint health
- Increase `failure_threshold`
- Increase `open_duration_secs`
- Investigate why failures are occurring

### Too Many Rejections
- Reduce `failure_threshold`
- Increase `success_threshold_half_open`
- Reduce `open_duration_secs`
- Investigate underlying endpoint issues

## Future Enhancements

1. Dynamic threshold adjustment based on metrics
2. Per-endpoint configuration
3. Circuit breaker histograms
4. Integration with alerts
5. Webhook health check endpoint
6. Circuit breaker visualization dashboard
