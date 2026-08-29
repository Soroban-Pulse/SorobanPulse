# Query Result Caching & Cache Invalidation - Issue #889

## Overview

SorobanPulse implements intelligent caching for frequently accessed queries with smart invalidation strategies. The cache system includes hit/miss tracking, pattern-based invalidation, and tenant-aware cache management.

## Architecture

### Cache Components

- **Query Result Cache**: In-memory cache for full query result sets (Moka)
- **Cache Invalidator**: Pattern-based invalidation manager
- **Cache Statistics**: Hit/miss tracking and performance metrics
- **Cache Configuration**: Per-tenant cache configuration and TTL management

## Enabling Caching

```rust
use soroban_pulse::query_cache;

// Initialize cache at startup
let cache = query_cache::build(300, 10_000);  // 5-min TTL, 10k entries max
```

## Cache Configuration

### Environment Variables

```bash
# Cache sizing
QUERY_CACHE_TTL_SECS=300          # Time-to-live for cached entries
QUERY_CACHE_MAX_ENTRIES=10000     # Maximum number of cached entries

# Cache behavior
QUERY_CACHE_AUTO_WARMUP=false     # Pre-populate cache on startup
CACHE_INVALIDATION_STRATEGY=event-based  # event-based or ttl-based

# Per-tenant configuration (via API)
```

### TTL Constraints

```rust
pub const MIN_TTL_SECS: u64 = 300;   // 5 minutes minimum
pub const MAX_TTL_SECS: u64 = 3600;  // 1 hour maximum
pub const DEFAULT_TTL_SECS: u64 = 300;
```

## Cache Keys

### Key Format

Keys follow a pattern for type-based invalidation:

```
<query_type>:<specifics>
```

### Examples

```
contract_event_counts:0xABC123
event_aggregates:transfer:ledger_1000_2000
transaction_stats:2024-08-27
user_events:user123:contract456
```

## Usage

### Storing in Cache

```rust
use serde_json::json;
use soroban_pulse::query_cache;

let key = "contract_event_counts:0xABC123".to_string();
let result = json!({
    "count": 1000,
    "updated_at": "2024-08-27T10:00:00Z"
});

query_cache::set(&cache, key, result).await;
```

### Retrieving from Cache

```rust
let key = "contract_event_counts:0xABC123";
if let Some(cached_result) = query_cache::get(&cache, key).await {
    // Use cached result
    return cached_result;
} else {
    // Cache miss - execute query
    let fresh_result = execute_expensive_query().await;
    query_cache::set(&cache, key.to_string(), fresh_result).await;
}
```

## Cache Invalidation

### Invalidation Triggers

The cache supports multiple invalidation strategies:

```rust
use soroban_pulse::query_cache::InvalidationTrigger;

enum InvalidationTrigger {
    EventIngestion,         // New events arrive
    TenantProvisioning,     // New tenant created
    ConfigUpdate,           // Settings changed
    Manual,                 // Explicit invalidation
}
```

### Pattern-Based Invalidation

```rust
let invalidator = query_cache::CacheInvalidator::new();

// Register patterns
invalidator.register_pattern("contract_event_counts:*".to_string());
invalidator.register_pattern("event_aggregates:*".to_string());

// Invalidate matching patterns
let matched = invalidator.invalidate_pattern("contract_event_counts");
println!("Invalidated {} entries", matched.len());
```

### Event-Based Invalidation

```rust
// When new events are ingested:
invalidator.invalidate_by_trigger(
    InvalidationTrigger::EventIngestion,
    vec![
        "contract_event_counts:0xABC123".to_string(),
        "event_aggregates:transfer:*".to_string(),
    ],
);
```

## Cache Statistics

### Hit Rate Calculation

```rust
use soroban_pulse::query_cache::CacheStats;

let stats = CacheStats {
    hits: 750,
    misses: 250,
    invalidations: 10,
};

println!("Hit rate: {:.1}%", stats.hit_rate() * 100.0);  // 75.0%
```

### Monitoring Metrics

```
soroban_pulse_query_cache_hits{query_type="contract_event_counts"}
soroban_pulse_query_cache_misses{query_type="contract_event_counts"}
soroban_pulse_query_cache_evictions{query_type="contract_event_counts"}
soroban_pulse_query_cache_invalidations{trigger="event_ingestion"}
```

## Database Schema

### Cache Statistics Table

```sql
CREATE TABLE query_cache_stats (
    id UUID PRIMARY KEY,
    cache_key TEXT NOT NULL UNIQUE,
    query_type TEXT NOT NULL,
    tenant_id TEXT DEFAULT 'default',
    hit_count BIGINT DEFAULT 0,
    miss_count BIGINT DEFAULT 0,
    last_hit TIMESTAMPTZ,
    last_miss TIMESTAMPTZ,
    entry_size_bytes BIGINT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Cache Invalidation Log

```sql
CREATE TABLE cache_invalidation_log (
    id UUID PRIMARY KEY,
    trigger_event TEXT NOT NULL,
    affected_keys TEXT[] NOT NULL,
    tenant_id TEXT DEFAULT 'default',
    reason TEXT,
    triggered_by TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Cache Configuration

```sql
CREATE TABLE cache_config (
    id TEXT PRIMARY KEY,
    tenant_id TEXT NOT NULL UNIQUE,
    ttl_seconds INT DEFAULT 300,
    max_size_mb INT DEFAULT 100,
    enabled BOOLEAN DEFAULT TRUE,
    auto_warmup BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

## Cache Warming

### Startup Cache Warming

```rust
#[tokio::main]
async fn main() {
    let cache = query_cache::build(300, 10_000);
    
    // Warm up cache with frequently accessed queries
    warmup_cache(&cache).await;
}

async fn warmup_cache(cache: &Cache<String, Value>) {
    let hot_queries = vec![
        "contract_event_counts:0xABC123",
        "event_aggregates:transfer:recent",
        "transaction_stats:today",
    ];
    
    for query in hot_queries {
        if let Ok(result) = execute_query(query).await {
            query_cache::set(cache, query.to_string(), result).await;
        }
    }
}
```

## Performance Optimization

### Best Practices

1. **Key Design**: Use consistent, hierarchical key patterns
2. **TTL Tuning**: Match TTL to query freshness requirements
3. **Size Management**: Monitor cache memory usage
4. **Invalidation Strategy**: Use event-based over TTL-based when possible

### Cache Hit Rate Targets

- **High-value queries**: Target 80%+ hit rate
- **Moderate queries**: Target 60%+ hit rate
- **Low-value queries**: Consider disabling cache

### Memory Usage

```bash
# Monitor per-tenant cache memory
SELECT tenant_id, SUM(entry_size_bytes) as total_bytes
FROM query_cache_stats
GROUP BY tenant_id;
```

## Tenant-Aware Caching

### Per-Tenant Configuration

```sql
INSERT INTO cache_config (id, tenant_id, ttl_seconds, max_size_mb)
VALUES ('acme-corp', 'acme-corp', 600, 200);
```

### Isolated Cache Per Tenant

Cache keys should include tenant_id:

```rust
let key = format!("contract_event_counts:{}:0xABC123", tenant_id);
```

This ensures cross-tenant cache poisoning is impossible.

## API Endpoints

### Clear Cache Pattern

```http
POST /api/admin/cache/invalidate
Authorization: Bearer <admin-token>
Content-Type: application/json

{
  "pattern": "contract_event_counts:*",
  "reason": "Data quality issue"
}
```

### Inspect Cache Stats

```http
GET /api/admin/cache/stats?query_type=contract_event_counts
Authorization: Bearer <admin-token>
```

Response:

```json
{
  "cache_key": "contract_event_counts:0xABC123",
  "query_type": "contract_event_counts",
  "hit_count": 1250,
  "miss_count": 50,
  "hit_rate": 0.9615,
  "entry_size_bytes": 1024,
  "last_hit": "2024-08-27T12:30:45Z"
}
```

## Testing

### Unit Tests

```bash
cargo test query_cache::tests
```

Tests cover:
- Cache hit/miss rates
- Pattern matching
- TTL enforcement
- Cache statistics accuracy

### Integration Tests

```bash
cargo test --test '*cache*'
```

Scenarios:
- Cache invalidation timing
- Multi-tenant isolation
- Memory pressure handling
- Concurrent access patterns

## Monitoring

### Key Metrics to Track

```
hit_rate = hits / (hits + misses)
avg_entry_size = total_bytes / entry_count
memory_usage = total_bytes / 1024 / 1024  // MB
invalidation_frequency = invalidations / time_period
```

### Alerts

Configure alerts for:
- Hit rate < 60% (cache thrashing)
- Memory usage > 80% capacity
- Frequent invalidations (> 100/min)
- Stale entries (age > 2x TTL due to memory pressure)

## Troubleshooting

### Low Hit Rate

**Symptom**: `hit_rate < 50%`

**Causes**:
- TTL too short - increase `QUERY_CACHE_TTL_SECS`
- Cache too small - increase `QUERY_CACHE_MAX_ENTRIES`
- Poor key patterns - review cache key design
- Frequent invalidations - review invalidation logic

**Fix**: Increase TTL, monitor invalidations, optimize key patterns

### High Memory Usage

**Symptom**: Cache uses > 1GB memory

**Causes**:
- Too many unique cache keys
- Large result sets
- Too high `QUERY_CACHE_MAX_ENTRIES`

**Fix**: Reduce max entries, implement result compression, increase TTL to reduce key churn

### Cache Inconsistency

**Symptom**: Stale data in cache after updates

**Causes**:
- Incomplete invalidation patterns
- Missing invalidation triggers
- Clock skew affecting TTL

**Fix**: Review invalidation patterns, ensure all modification paths trigger invalidation

## Performance Benchmarks

### Query Latency Improvement

With 75% hit rate:

```
Cached query:    5ms  (cache lookup + deserialization)
Uncached query: 200ms (database query execution)
Average:        55ms  (0.75 * 5 + 0.25 * 200)
```

### Memory vs Hit Rate Tradeoff

```
Max Entries    Memory    Hit Rate
1,000          10MB      60%
10,000         100MB     75%
100,000        1GB       85%
```

## References

- [Moka Cache Documentation](https://github.com/moka-rs/moka)
- [Cache Invalidation Strategies](https://en.wikipedia.org/wiki/Cache_invalidation)
- [Pattern Matching Algorithms](https://en.wikipedia.org/wiki/Pattern_matching)
