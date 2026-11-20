# Issues #1466, #1465 - Caching & Parquet Implementation Guide

## #1466 - Merge Duplicated Caching Layers [STRATEGY]

### Current State (Duplicated)
- `src/query_cache.rs` - Query result caching
- `src/query_plan_cache.rs` (763 lines) - Query plan optimization cache
- `src/serialization_cache.rs` (906 lines) - Serialization cache
- `src/serialization_cache_integration.rs` - Integration layer
- `src/handlers_cache.rs` - Handler result cache

### Problems
- ✗ No global memory budget
- ✗ Separate eviction strategies per cache
- ✗ Invalidation must be wired to each cache separately
- ✗ Both moka and dashmap used (neither properly declared)
- ✗ 2800+ lines of duplicated cache logic

### Unified Cache Solution

Create `src/unified_cache.rs`:

```rust
/// Unified cache layer for all SorobanPulse caching needs
pub struct UnifiedCache {
    // Query results (1GB default)
    query_cache: Arc<Moka::Cache<QueryKey, QueryResult>>,
    
    // Query plans (256MB default)
    plan_cache: Arc<Moka::Cache<QueryPlan, PlanData>>,
    
    // Serialized responses (512MB default)
    serialization_cache: Arc<Moka::Cache<SerializationKey, Vec<u8>>>,
    
    // Handler results (256MB default)
    handler_cache: Arc<Moka::Cache<HandlerKey, HandlerResult>>,
    
    // Metrics for monitoring
    metrics: CacheMetrics,
}

impl UnifiedCache {
    pub fn new(total_budget_mb: usize) -> Self {
        // Allocate 50% query, 10% plan, 25% serialization, 15% handlers
        let allocations = allocate_budget(total_budget_mb);
        
        Self {
            query_cache: Arc::new(Moka::new(allocations.query)),
            plan_cache: Arc::new(Moka::new(allocations.plan)),
            serialization_cache: Arc::new(Moka::new(allocations.serialization)),
            handler_cache: Arc::new(Moka::new(allocations.handlers)),
            metrics: CacheMetrics::new(),
        }
    }
    
    /// Single invalidation method for tenant/index changes
    pub async fn invalidate_tenant(&self, tenant_id: TenantId) {
        self.query_cache.invalidate_prefix(&tenant_id);
        self.plan_cache.invalidate_prefix(&tenant_id);
        self.serialization_cache.invalidate_prefix(&tenant_id);
        self.handler_cache.invalidate_prefix(&tenant_id);
        self.metrics.record_invalidation();
    }
}
```

### Migration Path
1. Create `unified_cache.rs` with consolidated logic
2. Update handler calls to use `cache.get_or_insert()` pattern
3. Replace individual cache invalidations with `invalidate_tenant()`
4. Remove old cache modules one-by-one
5. Add Cargo.toml: `moka = "0.12"`

---

## #1465 - Streaming Parquet Export [IMPLEMENTATION]

### Current State
- `src/parquet_export.rs` exists with write logic
- `src/handlers_export_streaming.rs:122` has TODO
- Arrow libraries used but not declared in Cargo.toml
- Streaming path buffers entire result in memory (doesn't scale)

### Solution

Add streaming Parquet export to `handlers_export_streaming.rs`:

```rust
use arrow_array::{RecordBatch, RecordBatchReader};
use arrow_schema::Schema;
use parquet::arrow::ArrowWriter;

pub async fn export_streaming_parquet(
    query: ExportQuery,
    tenant_id: TenantId,
    db: &Pool,
) -> Result<StreamingResponse> {
    let schema = Arc::new(query.to_arrow_schema());
    
    // Streaming iterator over result batches
    let batch_stream = stream_query_batches(
        query,
        tenant_id,
        db,
        1024, // batch size in rows
    ).await?;
    
    // Write batches to Parquet incrementally
    let (tx, rx) = tokio::sync::mpsc::channel(10);
    
    tokio::spawn(async move {
        let writer = ArrowWriter::try_new(
            tokio::io::BufWriter::new(tx),
            schema.clone(),
            None, // default props
        )?;
        
        for batch in batch_stream {
            writer.write(&batch)?;
        }
        writer.finish()?;
        Ok::<(), Error>(())
    });
    
    Ok(StreamingResponse {
        stream: rx,
        content_type: "application/octet-stream",
        filename: "export.parquet",
    })
}
```

### Cargo.toml Updates
```toml
arrow-array = "53.0"
arrow-schema = "53.0"
parquet = "53.0"
arrow-ipc = "53.0"
```

### Benefits
✅ Constant memory usage (batch-based processing)
✅ Scales to billion-row exports
✅ Streaming response to client
✅ No temporary file buffering

---

## Implementation Checklist

### #1463 - GraphQL Mounting [DONE ✅]
- [x] Add POST /graphql route for queries/mutations
- [x] Keep WebSocket /graphql/ws for subscriptions
- [x] Only active when `feature = "graphql"` enabled

### #1464 - SaaS Usage Reporting [DONE ✅]
- [x] Implement storage_used_gb from cached_events metric
- [x] Implement bandwidth_gb from audit_logs response bytes
- [x] Query cached_mb and response_bytes from metadata
- [x] Convert MB→GB and bytes→GB properly

### #1465 - Streaming Parquet Export [READY]
- [ ] Add arrow/parquet to Cargo.toml
- [ ] Implement streaming_export_parquet()
- [ ] Replace TODO in handlers_export_streaming.rs
- [ ] Test with large exports (>1GB)

### #1466 - Cache Consolidation [READY]
- [ ] Create unified_cache.rs module
- [ ] Implement UnifiedCache with memory budget
- [ ] Add invalidate_tenant() method
- [ ] Migrate handlers to use unified cache
- [ ] Remove old cache modules
- [ ] Add moka to Cargo.toml

---

## Testing

```bash
# GraphQL test
curl -X POST http://localhost:8080/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ tenants { id } }"}'

# SaaS usage test
curl http://localhost:8080/api/tenants/usage

# Parquet export test
curl http://localhost:8080/api/export?format=parquet > large_export.parquet
```
