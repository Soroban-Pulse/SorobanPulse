# Issue #1454 - Unify Observability Modules

## Problem Statement

Telemetry setup scattered across 7 overlapping modules with no clear ownership:
- `metrics.rs` (2,089 lines) - Prometheus metrics definitions
- `distributed_tracing.rs` (889 lines) - OpenTelemetry tracing setup
- `observability.rs` (873 lines) - Mixed observability initialization
- `slo_tracker.rs` (866 lines) - SLO tracking and health checks
- `prometheus_remote_write.rs` - Remote write integration
- `stream_statistics.rs` - Stream telemetry helpers
- `tests/observability_unification_tests.rs` - Tests (name suggests unification was planned)

**Issues:**
1. Dependency versions are inconsistent (OpenTelemetry 0.21/0.14, opentelemetry-jaeger deprecated)
2. No clear separation of concerns
3. Duplicate metric definitions
4. Unclear which module to modify for new telemetry
5. Missing dependency declarations (metrics_exporter_prometheus used but not in Cargo.toml)

## Current Module Breakdown

### metrics.rs (2,089 lines)
- Counter definitions (HTTP, RPC, database)
- Histogram definitions (latencies, request sizes)
- Prometheus exporter setup
- Metric collection helpers
- Metric registry management

### distributed_tracing.rs (889 lines)
- OpenTelemetry SDK initialization
- Jaeger exporter setup (deprecated)
- Trace context propagation
- Span builders and helpers
- Baggage management

### observability.rs (873 lines)
- Unified initialization entry point
- Telemetry pipeline setup
- Middleware for automatic instrumentation
- Global tracer/meter access
- Shutdown coordination

### slo_tracker.rs (866 lines)
- SLO threshold definitions
- Health check implementation
- Error rate tracking
- Latency percentile calculations
- Alert condition evaluation

### prometheus_remote_write.rs
- Remote Prometheus write integration
- Push gateway client
- Batch metric submission

### stream_statistics.rs
- Stream event telemetry
- Statistical aggregation
- Throughput tracking

## Proposed Unified Structure

```
src/observability/
├── mod.rs                    -- Public API, initialization
├── metrics/
│   ├── mod.rs               -- Metric registry, definitions
│   ├── http.rs              -- HTTP metrics
│   ├── database.rs          -- Database metrics
│   ├── rpc.rs               -- RPC metrics
│   ├── stream.rs            -- Stream metrics
│   └── exporter.rs          -- Prometheus export
├── tracing/
│   ├── mod.rs               -- Tracing setup, context
│   ├── jaeger.rs            -- Jaeger exporter (deprecated → remove in 0.3)
│   ├── otlp.rs              -- OTLP exporter (recommended)
│   └── propagation.rs       -- Trace context propagation
├── slo/
│   ├── mod.rs               -- SLO definitions, tracking
│   ├── health.rs            -- Health check implementation
│   └── alerts.rs            -- Alert condition evaluation
├── middleware.rs            -- Automatic instrumentation
└── shutdown.rs              -- Graceful shutdown coordination
```

## Migration Strategy

### Phase 1 - Prepare (PR 1)
1. Create new `src/observability/` module structure
2. Add all current observability dependencies to Cargo.toml
3. Update OpenTelemetry to 0.21+ (latest stable)
4. Create compatibility shims in new structure

### Phase 2 - Migrate Metrics (PR 2)
1. Move `metrics.rs` to `observability/metrics/mod.rs`
2. Split by domain: http.rs, database.rs, rpc.rs, stream.rs
3. Consolidate exporter setup (prometheus_remote_write.rs → exporter.rs)
4. Update all imports

### Phase 3 - Migrate Tracing (PR 3)
1. Move `distributed_tracing.rs` to `observability/tracing/mod.rs`
2. Remove deprecated Jaeger exporter (mark deprecated in 0.2, remove in 0.3)
3. Promote OTLP exporter as primary
4. Consolidate propagation logic (propagation.rs)

### Phase 4 - Migrate SLO (PR 4)
1. Move `slo_tracker.rs` to `observability/slo/mod.rs`
2. Split: health.rs, alerts.rs
3. Integrate with metrics (SLO thresholds reference metrics)

### Phase 5 - Consolidate Init (PR 5)
1. Move `observability.rs` to `observability/mod.rs::init()`
2. Coordinate initialization order: metrics → tracing → middleware → slo
3. Create unified config struct

### Phase 6 - Update Middleware (PR 6)
1. Create `observability/middleware.rs`
2. Auto-instrument HTTP, database, RPC operations
3. Attach trace context to requests

### Phase 7 - Final Cleanup (PR 7)
1. Remove old module files from root
2. Update all imports across codebase
3. Remove compatibility shims
4. Add observability documentation

## Cargo.toml Updates

```toml
[dependencies]
# OpenTelemetry (unified to 0.21.x)
opentelemetry = "0.21"
opentelemetry-jaeger = { version = "0.20", optional = true }  # deprecated
opentelemetry-otlp = "0.14"  # recommended
opentelemetry_sdk = "0.21"

# Metrics
prometheus = "0.13"
metrics = "0.21"
metrics-exporter-prometheus = "0.12"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.21"

[features]
observability-jaeger = ["opentelemetry-jaeger"]
observability-otlp = ["opentelemetry-otlp"]
```

## Code Changes Outline

### New observability/mod.rs structure
```rust
pub mod metrics;
pub mod tracing;
pub mod slo;
pub mod middleware;
pub mod shutdown;

pub struct ObservabilityConfig {
    pub metrics_enabled: bool,
    pub traces_enabled: bool,
    pub otlp_endpoint: Option<String>,
    pub prometheus_port: u16,
    pub slo_enabled: bool,
}

pub async fn init(config: ObservabilityConfig) -> Result<ObservabilityHandle> {
    let metrics_guard = metrics::init(&config)?;
    let tracing_guard = tracing::init(&config)?;
    let slo_guard = slo::init(&config)?;
    
    Ok(ObservabilityHandle {
        metrics: metrics_guard,
        tracing: tracing_guard,
        slo: slo_guard,
    })
}
```

### Updated imports
```rust
// Before
use crate::metrics::{HTTP_REQUESTS_TOTAL, RPC_DURATION};
use crate::distributed_tracing::create_span;
use crate::observability::init_telemetry;

// After
use crate::observability::metrics::{http, rpc};
use crate::observability::tracing::create_span;
use crate::observability::init;
```

## Testing Strategy

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn metrics_initialized() {
        let config = test_config();
        init(&config).unwrap();
        // Verify meter is registered
        assert!(metrics::registry().is_registered());
    }
    
    #[test]
    fn traces_propagate_correctly() {
        let config = test_config();
        init(&config).unwrap();
        // Verify trace context in headers
    }
    
    #[test]
    fn slo_evaluates_correctly() {
        let config = test_config();
        init(&config).unwrap();
        // Simulate metrics, verify SLO alerts trigger
    }
}
```

## Deprecation Timeline

| Version | Action |
|---------|--------|
| 0.2 | Mark Jaeger exporter deprecated, promote OTLP |
| 0.3 | Remove Jaeger exporter, make OTLP default |
| 0.4 | Remove deprecated stream_statistics, consolidate into metrics |

## Files to Delete (after migration)

- ~~src/metrics.rs~~
- ~~src/distributed_tracing.rs~~
- ~~src/observability.rs~~
- ~~src/slo_tracker.rs~~
- ~~src/prometheus_remote_write.rs~~ (merge into metrics/exporter.rs)
- ~~src/stream_statistics.rs~~ (merge into metrics/stream.rs)
- ~~tests/observability_unification_tests.rs~~ (move to src/observability/tests/)

## Benefits

✅ Single source of truth for observability  
✅ Clear ownership and structure  
✅ Unified dependency versions  
✅ Easier to add new telemetry  
✅ Better code organization  
✅ Clearer initialization order  
✅ Proper cascade shutdown  

## Checklist

- [ ] Design final structure (get team approval)
- [ ] Create src/observability/ directory
- [ ] Update Cargo.toml with consistent dependency versions
- [ ] Create Phase 1 PR (structure + shims)
- [ ] Create Phase 2 PR (metrics migration)
- [ ] Create Phase 3 PR (tracing migration)
- [ ] Create Phase 4 PR (SLO migration)
- [ ] Create Phase 5 PR (init consolidation)
- [ ] Create Phase 6 PR (middleware)
- [ ] Create Phase 7 PR (cleanup + docs)
- [ ] Update architecture.md
- [ ] Add observability troubleshooting guide
