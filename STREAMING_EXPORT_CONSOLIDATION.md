# Issue #1453 - Consolidate Streaming/Export Modules

## Problem Statement

Streaming and export functionality scattered across overlapping modules with duplicate query building and unclear responsibilities.

**Affected Modules:**
1. `export_service.rs` - Event export to file formats
2. `streaming_service.rs` - Real-time event streaming (WebSocket/SSE)
3. `query_builder.rs` - SQL construction (duplicated in export + streaming)
4. `format_converter.rs` - Format conversion (JSON, CSV, Parquet)
5. `cache_layer.rs` - Caching for stream performance
6. `pagination.rs` - Cursor-based pagination (duplicated implementations)

## Current Issues

1. **Duplicate Query Builders**: Both export and streaming build similar queries independently
2. **Unclear Responsibilities**: Not clear which module handles filtering, sorting, pagination
3. **Code Duplication**: Pagination logic in both modules
4. **Inconsistent APIs**: Export uses callbacks, streaming uses channels
5. **Cache Integration**: Cache layer integrated differently in each module
6. **Format Support**: Format conversion duplicated in export + streaming

## Proposed Unified Structure

```
src/data_export/
├── mod.rs                    -- Public API
├── query.rs                  -- Unified query builder (deduplicated)
├── formats/
│   ├── mod.rs               -- Format registry
│   ├── json.rs              -- JSON serialization
│   ├── csv.rs               -- CSV export
│   ├── parquet.rs           -- Parquet columnar format
│   └── arrow.rs             -- Arrow IPC format
├── pagination/
│   ├── mod.rs               -- Cursor + offset pagination
│   └── cursor.rs            -- Cursor encoding/decoding
├── streaming/
│   ├── mod.rs               -- Stream trait + implementations
│   ├── websocket.rs         -- WebSocket streaming
│   ├── sse.rs               -- Server-Sent Events
│   └── channel.rs           -- Channel-based buffering
├── export/
│   ├── mod.rs               -- File export API
│   ├── file.rs              -- File writing
│   └── batch.rs             -- Batch export coordination
├── cache.rs                 -- Unified cache layer
└── types.rs                 -- Shared type definitions
```

## Deduplication Strategy

### 1. Unified Query Builder (query.rs)

```rust
pub struct DataQuery {
    pub select: Vec<String>,
    pub from: String,
    pub filters: QueryFilters,
    pub order_by: Vec<OrderBy>,
    pub pagination: Pagination,
}

impl DataQuery {
    pub fn build_sql(&self) -> (String, Vec<SqlValue>) {
        // Single, optimized SQL builder used by all consumers
    }
    
    pub fn explain(&self) -> String {
        // For debugging/optimization
    }
}

pub struct QueryFilters {
    pub tenant_id: UUID,
    pub date_range: Option<(DateTime, DateTime)>,
    pub event_types: Option<Vec<String>>,
    pub contract_ids: Option<Vec<String>>,
    pub custom: Option<String>,  // User-provided filter
}

pub enum Pagination {
    Cursor(CursorPagination),
    Offset(OffsetPagination),
}
```

### 2. Unified Pagination (pagination/mod.rs)

```rust
pub trait Paginator: Send + Sync {
    type Cursor: Serialize;
    
    fn encode_cursor(&self, row: &Row) -> Self::Cursor;
    fn decode_cursor(&self, cursor: &str) -> Result<Self::Cursor>;
    fn apply_to_query(&self, query: &mut String, cursor: Option<&str>);
}

pub struct CursorPagination {
    pub cursor: Option<String>,
    pub limit: i64,
    pub fields: Vec<String>,  // Which fields define the cursor
}

impl Paginator for CursorPagination { ... }
```

### 3. Unified Format Support (formats/mod.rs)

```rust
pub trait Formatter: Send + Sync {
    fn format(&self, rows: Vec<Row>) -> Result<Bytes>;
    fn supports_streaming(&self) -> bool;
    fn stream_format(&self, row: Row) -> Result<Bytes>;
}

pub struct FormatRegistry {
    formatters: HashMap<String, Arc<dyn Formatter>>,
}

impl FormatRegistry {
    pub fn get(&self, format: &str) -> Option<Arc<dyn Formatter>> { ... }
    pub fn register(&mut self, name: &str, formatter: Arc<dyn Formatter>) { ... }
}
```

### 4. Unified Streaming (streaming/mod.rs)

```rust
pub trait EventStream: Send + Sync {
    async fn send(&self, event: StreamEvent) -> Result<()>;
    async fn close(&self) -> Result<()>;
}

pub struct WebSocketStream {
    tx: UnboundedSender<StreamEvent>,
}

pub struct ServerSentEventStream {
    tx: UnboundedSender<StreamEvent>,
}

pub struct ChannelStream {
    tx: UnboundedSender<StreamEvent>,
}

// All implement EventStream trait
```

## Migration Steps

### Phase 1 - Create New Module Structure (PR 1)
```bash
mkdir -p src/data_export/{formats,pagination,streaming,export}
touch src/data_export/mod.rs
touch src/data_export/query.rs
touch src/data_export/cache.rs
touch src/data_export/types.rs
# ... etc
```

### Phase 2 - Extract Query Builder (PR 2)
1. Analyze both export_service.rs and streaming_service.rs query building
2. Create unified `DataQuery` struct
3. Implement `build_sql()` for both use cases
4. Add query tests

### Phase 3 - Extract Pagination (PR 3)
1. Identify cursor vs offset pagination usage
2. Create `Paginator` trait
3. Implement CursorPagination and OffsetPagination
4. Test cursor encoding/decoding

### Phase 4 - Consolidate Formats (PR 4)
1. Create FormatRegistry
2. Move JSON, CSV, Parquet to formats/
3. Implement Formatter trait for each
4. Update export_service to use FormatRegistry

### Phase 5 - Consolidate Streaming (PR 5)
1. Create EventStream trait
2. Move WebSocket, SSE, Channel implementations
3. Unify sender interface
4. Update streaming_service to use unified API

### Phase 6 - Consolidate Exports (PR 6)
1. Create unified export_service.rs using DataQuery
2. Support streaming output (don't collect all in memory)
3. Integrate cache layer
4. Add progress tracking

### Phase 7 - Final Cleanup (PR 7)
1. Delete old modules
2. Update all imports across codebase
3. Update tests
4. Add documentation

## Code Examples

### Before (Duplicate Logic)
```rust
// export_service.rs
fn build_query(filters: &ExportFilters) -> String {
    let mut q = "SELECT * FROM events WHERE tenant_id = ? ".to_string();
    if let Some(date_range) = &filters.date_range {
        q.push_str(&format!("AND timestamp BETWEEN {} AND {}", date_range.start, date_range.end));
    }
    // ... more duplication
    q
}

// streaming_service.rs
fn build_query(filters: &StreamFilters) -> String {
    let mut q = "SELECT * FROM events WHERE tenant_id = ? ".to_string();
    if let Some(date_range) = &filters.date_range {
        q.push_str(&format!("AND timestamp BETWEEN {} AND {}", date_range.start, date_range.end));
    }
    // ... same duplication
    q
}
```

### After (Unified)
```rust
// data_export/query.rs
impl DataQuery {
    pub fn build_sql(&self) -> (String, Vec<SqlValue>) {
        let mut sql = "SELECT * FROM events WHERE tenant_id = ?".to_string();
        let mut params = vec![self.filters.tenant_id];
        
        if let Some(range) = &self.filters.date_range {
            sql.push_str(" AND timestamp BETWEEN ? AND ?");
            params.push(range.start);
            params.push(range.end);
        }
        // ... unified logic
        (sql, params)
    }
}

// Used by both modules
let query = DataQuery { ... };
let (sql, params) = query.build_sql();
```

## Expected Outcomes

✅ Single query builder (DRY)  
✅ Consistent pagination API  
✅ Unified format support  
✅ Clearer module responsibilities  
✅ Easier to add new formats/transports  
✅ Better testability  
✅ Shared caching layer  

## Performance Considerations

- **Memory**: Streaming prevents memory bloat by not collecting all rows
- **Pagination**: Cursor-based pagination more efficient than offset for large datasets
- **Caching**: Unified cache layer can serve both export and streaming queries
- **Format**: Parquet + Arrow for columnar performance

## Testing Strategy

```rust
#[test]
fn query_builder_handles_all_filters() {
    let query = DataQuery {
        filters: QueryFilters {
            date_range: Some((start, end)),
            event_types: Some(vec!["created".into()]),
            ..
        },
        ..
    };
    
    let (sql, params) = query.build_sql();
    assert!(sql.contains("timestamp BETWEEN"));
    assert!(sql.contains("event_type IN"));
}

#[test]
fn cursor_pagination_roundtrips() {
    let paginator = CursorPagination { ... };
    let cursor = paginator.encode_cursor(&row);
    let decoded = paginator.decode_cursor(&cursor).unwrap();
    assert_eq!(cursor, paginator.encode_cursor(&decoded));
}
```

## Checklist

- [ ] Design and approve unified structure
- [ ] Create src/data_export/ directory
- [ ] Extract DataQuery (deduplicate query builders)
- [ ] Extract Paginator trait (deduplicate pagination)
- [ ] Extract Formatter trait (consolidate formats)
- [ ] Extract EventStream trait (consolidate streaming)
- [ ] Update export_service to use new APIs
- [ ] Update streaming_service to use new APIs
- [ ] Add comprehensive tests
- [ ] Update documentation
- [ ] Delete old modules
- [ ] Performance benchmarks (verify no regression)

## Migration Dependencies

This work should follow/coordinate with:
- #1454 (Observability consolidation) - For telemetry
- #1451 (Tenancy reconciliation) - For tenant filtering
