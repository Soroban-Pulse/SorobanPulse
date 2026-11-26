# Issue #1456 - Split src/handlers/events.rs into Cohesive Modules

## Problem

handlers/events.rs (4,768 lines) mixes 8 distinct concerns:
- Event listing with pagination
- Event search with filters
- SSE streaming (real-time push)
- WebSocket streaming
- Batch event retrieval
- Event anonymization
- Event deletion
- Ad-hoc SQL building (duplicated query logic)

## Proposed Structure

```
src/handlers/events/
├── mod.rs                        -- Re-exports, router setup
├── list.rs                       -- List events with pagination (cursor-based)
├── search.rs                     -- Search events with filters, facets
├── sse.rs                        -- Server-Sent Events streaming
├── websocket.rs                  -- WebSocket bidirectional streaming
├── batch.rs                      -- Batch retrieval (get_events_batch)
├── anonymize.rs                  -- Anonymization and PII handling
├── delete.rs                     -- Event deletion and purge
└── query_builder.rs              -- Shared SQL construction (DRY)
```

## Responsibilities per Module

| Module | Handler(s) | Lines | Responsibility |
|--------|-----------|-------|-----------------|
| list.rs | GET /events, /by_contract | ~400 | Cursor pagination, sorting |
| search.rs | POST /search, /facets | ~600 | Filtering, full-text search, aggregations |
| sse.rs | GET /events/stream (SSE) | ~350 | Reconnect, event buffering, multicast |
| websocket.rs | WS /events/ws | ~450 | Bidirectional flow, backpressure, heartbeat |
| batch.rs | POST /batch | ~250 | Bulk retrieve by IDs |
| anonymize.rs | PATCH /anonymize | ~300 | PII redaction, audit logging |
| delete.rs | DELETE /events/{id}, /purge | ~300 | Soft/hard delete, cascade |
| query_builder.rs | (internal) | ~200 | SQL construction, parameter binding |

## Migration Steps

### Phase 1 - Create Module (PR 1)
1. Create src/handlers/events/ directory
2. Move handlers/events.rs → handlers/events/mod.rs
3. Create empty domain modules

### Phase 2 - Extract Listing (PR 2)
1. Move list_events handler → list.rs
2. Move pagination helpers → list.rs
3. Extract query builder → query_builder.rs

### Phase 3 - Extract Search (PR 3)
1. Move search_events, facets handlers → search.rs
2. Move filter logic → search.rs
3. Share query builder

### Phase 4 - Extract Streaming (PR 4)
1. Move SSE handler → sse.rs
2. Move WebSocket handler → websocket.rs
3. Move stream helpers → shared module

### Phase 5 - Extract Batch, Anonymize, Delete (PR 5)
1. Move batch_get → batch.rs
2. Move anonymize → anonymize.rs
3. Move delete operations → delete.rs

### Phase 6 - Final Consolidation (PR 6)
1. Update mod.rs to re-export handlers
2. Update router setup
3. Add module documentation

## Expected File Sizes

After split:
- list.rs: ~400 lines
- search.rs: ~600 lines
- sse.rs: ~350 lines
- websocket.rs: ~450 lines
- batch.rs: ~250 lines
- anonymize.rs: ~300 lines
- delete.rs: ~300 lines
- query_builder.rs: ~200 lines (shared)
- mod.rs: ~100 lines (router + re-exports)

**Total: ~2,950 lines** (vs 4,768) - 38% reduction

## Query Builder Consolidation

Create shared query_builder.rs to eliminate duplication:

```rust
// src/handlers/events/query_builder.rs
pub struct EventQueryBuilder {
    base: String,
    filters: Vec<String>,
    params: Vec<Value>,
}

impl EventQueryBuilder {
    pub fn new() -> Self { ... }
    pub fn with_tenant(&mut self, tenant_id: UUID) -> &mut Self { ... }
    pub fn with_date_range(&mut self, start: DateTime, end: DateTime) -> &mut Self { ... }
    pub fn with_contract(&mut self, contract_id: &str) -> &mut Self { ... }
    pub fn with_event_type(&mut self, event_type: &str) -> &mut Self { ... }
    pub fn with_full_text_search(&mut self, query: &str) -> &mut Self { ... }
    pub fn with_pagination(&mut self, limit: i64, offset: i64) -> &mut Self { ... }
    pub fn with_cursor(&mut self, cursor: Option<&str>) -> &mut Self { ... }
    
    pub fn build(&self) -> (String, Vec<Value>) { ... }
    pub fn count(&self) -> (String, Vec<Value>) { ... }
}
```

Used by all modules:
```rust
// list.rs
let mut builder = EventQueryBuilder::new()
    .with_tenant(tenant_id)
    .with_pagination(limit, offset);
let (sql, params) = builder.build();

// search.rs
let mut builder = EventQueryBuilder::new()
    .with_tenant(tenant_id)
    .with_full_text_search(search_query)
    .with_date_range(start, end);
let (sql, params) = builder.build();
```

## Testing Strategy

Each module has focused tests:
```rust
// events/list.rs
#[test]
fn pagination_cursor_roundtrips() { ... }

#[test]
fn list_respects_tenant_isolation() { ... }

// events/search.rs
#[test]
fn full_text_search_matches_indexing() { ... }

#[test]
fn facets_aggregation_correct() { ... }

// events/sse.rs
#[test]
fn sse_reconnect_resumes_at_cursor() { ... }

#[test]
fn sse_respects_backpressure() { ... }
```

## Checklist

- [ ] Design module structure
- [ ] Create Phase 1 PR (directory + modules)
- [ ] Create Phase 2-5 PRs (handler extraction)
- [ ] Create Phase 6 PR (consolidation)
- [ ] Create query_builder tests
- [ ] Update router registration
- [ ] Add module documentation
- [ ] Performance test (no regression)

## Benefits

✅ Clear single responsibility per module  
✅ Easier to modify streaming without affecting search  
✅ Reusable query builder eliminates duplication  
✅ Better error handling per handler type  
✅ Focused test suites  
✅ Easier to add new event handlers  
✅ Reduced cyclomatic complexity  

## Related Issues

Coordinates with:
- #1455 (config refactoring) - Config drives handler timeouts, batch sizes
- #1457 (indexer refactoring) - Indexer produces events this module serves
- #1458 (lib.rs cleanup) - Handlers will be better organized for export
