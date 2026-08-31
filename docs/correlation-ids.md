# Correlation IDs

SorobanPulse propagates an `X-Correlation-ID` header across every service
boundary so a single logical operation (an inbound API request, an indexer
poll cycle, a webhook delivery) can be traced end to end, independent of
whether full W3C trace-context (`traceparent`) propagation is enabled.

## How it works

- `src/middleware/request_id.rs::correlation_id_middleware` runs on every
  inbound HTTP request. If the caller already supplied `X-Correlation-ID`, it
  is preserved; otherwise a fresh ID is minted using the same hex generator
  used for trace IDs (`distributed_tracing::new_trace_id`).
- The resolved correlation ID is stored in a thread-local
  (`distributed_tracing::set_correlation_id` / `get_correlation_id`) for the
  lifetime of the request, and mirrored onto the outgoing response so
  browser/CLI callers can log it.
- Outbound calls to other SorobanPulse services or webhooks should forward
  the same header so downstream services join the same correlation group.

## Correlation-based debugging

`distributed_tracing::record_correlation_log(service, message)` appends an
entry to a bounded in-memory ring buffer tagged with the current thread's
correlation ID. `distributed_tracing::search_by_correlation_id(id)` returns
every recorded entry across all services for that ID, and
`filter_correlation_logs(service, id)` narrows by service name. This gives a
minimal, dependency-free "correlation search interface" for local debugging;
production deployments should also ship these entries to the centralized log
backend (see `docs/runbooks`) for cross-instance search.

## Metrics

`distributed_tracing::record_correlation_metrics(had_incoming_id)` increments
`soroban_pulse_correlation_ids_total{source="propagated"|"generated"}`, and
`record_correlation_log` increments
`soroban_pulse_correlation_log_entries_total{service=...}`. Use these to
monitor what fraction of traffic arrives with a correlation ID already
attached (indicating a well-instrumented caller) versus falling back to a
freshly generated one.

## Usage example

```rust
use soroban_pulse::distributed_tracing::{
    record_correlation_log, search_by_correlation_id, set_correlation_id,
};

set_correlation_id("abc123".to_string());
record_correlation_log("indexer", "picked up event for contract CABC...");

// Later, from a debugging session or admin endpoint:
let timeline = search_by_correlation_id("abc123");
for entry in timeline {
    println!("[{}] {}: {}", entry.timestamp_ms, entry.service, entry.message);
}
```

## Testing

Unit tests covering header propagation/generation live in
`src/middleware/request_id.rs`; tests covering the log ring buffer, search,
filtering, and metrics live in `src/distributed_tracing.rs`.
