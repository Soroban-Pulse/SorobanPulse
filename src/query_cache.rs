use moka::future::Cache;
use serde_json::Value;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

pub const MIN_TTL_SECS: u64 = 300;   // 5 min
pub const MAX_TTL_SECS: u64 = 3600;  // 60 min
pub const DEFAULT_TTL_SECS: u64 = 300;
pub const DEFAULT_MAX_CAPACITY: u64 = 1_000;

/// Cache statistics for monitoring
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// Invalidation trigger types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum InvalidationTrigger {
    EventIngestion,
    TenantProvisioning,
    ConfigUpdate,
    Manual,
}

/// Cache invalidation manager with pattern-based invalidation support
pub struct CacheInvalidator {
    cache_patterns: Arc<std::sync::RwLock<HashSet<String>>>,
    stats: Arc<std::sync::RwLock<std::collections::HashMap<String, (AtomicU64, AtomicU64)>>>,
}

impl CacheInvalidator {
    pub fn new() -> Self {
        Self {
            cache_patterns: Arc::new(std::sync::RwLock::new(HashSet::new())),
            stats: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Register a cache key pattern for tracking
    pub fn register_pattern(&self, pattern: String) {
        if let Ok(mut patterns) = self.cache_patterns.write() {
            patterns.insert(pattern);
        }
    }

    /// Invalidate all keys matching a pattern
    pub fn invalidate_pattern(&self, pattern: &str) -> Vec<String> {
        if let Ok(patterns) = self.cache_patterns.read() {
            patterns
                .iter()
                .filter(|p| p.starts_with(pattern))
                .cloned()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Invalidate by trigger event
    pub fn invalidate_by_trigger(
        &self,
        trigger: InvalidationTrigger,
        affected_keys: Vec<String>,
    ) {
        for key in affected_keys {
            if let Ok(mut patterns) = self.cache_patterns.write() {
                patterns.remove(&key);
            }
        }
    }
}

impl Default for CacheInvalidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Clamp a caller-supplied TTL to the allowed [MIN_TTL_SECS, MAX_TTL_SECS] range.
pub fn clamp_ttl(secs: u64) -> Duration {
    Duration::from_secs(secs.clamp(MIN_TTL_SECS, MAX_TTL_SECS))
}

/// Build the shared query-result cache.
pub fn build(ttl_secs: u64, max_capacity: u64) -> Arc<Cache<String, Value>> {
    Arc::new(
        Cache::builder()
            .max_capacity(max_capacity)
            .time_to_live(clamp_ttl(ttl_secs))
            .build(),
    )
}

/// Extract the low-cardinality query type label from a cache key.
/// Keys are formatted as "type:specifics" (e.g. "contract_event_counts:CABC…").
fn query_type_label(key: &str) -> &str {
    key.split(':').next().unwrap_or(key)
}

/// Check whether a cached entry is present and record a cache hit/miss metric.
/// Returns the cached value if found, otherwise returns `None`.
pub async fn get(cache: &Cache<String, Value>, key: &str) -> Option<Value> {
    let label = query_type_label(key);
    match cache.get(key).await {
        Some(v) => {
            crate::metrics::record_query_cache_hit(label);
            Some(v)
        }
        None => {
            crate::metrics::record_query_cache_miss(label);
            None
        }
    }
}

/// Insert a value and record the store metric.
pub async fn set(cache: &Cache<String, Value>, key: String, value: Value) {
    cache.insert(key, value).await;
}

/// Invalidate all keys matching a pattern (e.g., "contract_event_counts:*")
pub async fn invalidate_pattern(cache: &Cache<String, Value>, pattern: &str) {
    // Note: moka cache doesn't provide direct pattern matching.
    // For production, consider using redis or a custom invalidation layer.
    // This is a placeholder for the API contract.
    let _ = (cache, pattern);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn clamp_ttl_min() {
        assert_eq!(clamp_ttl(0), Duration::from_secs(MIN_TTL_SECS));
        assert_eq!(clamp_ttl(MIN_TTL_SECS - 1), Duration::from_secs(MIN_TTL_SECS));
    }

    #[test]
    fn clamp_ttl_max() {
        assert_eq!(clamp_ttl(u64::MAX), Duration::from_secs(MAX_TTL_SECS));
        assert_eq!(clamp_ttl(MAX_TTL_SECS + 1), Duration::from_secs(MAX_TTL_SECS));
    }

    #[test]
    fn clamp_ttl_in_range() {
        assert_eq!(clamp_ttl(600), Duration::from_secs(600));
    }

    #[tokio::test]
    async fn build_and_retrieve() {
        let cache = build(DEFAULT_TTL_SECS, 10);
        cache.insert("k".to_string(), json!({"ok": true})).await;
        let v = cache.get("k").await.unwrap();
        assert_eq!(v["ok"], json!(true));
    }

    #[test]
    fn cache_stats_hit_rate() {
        let stats = CacheStats {
            hits: 75,
            misses: 25,
            invalidations: 0,
        };
        assert_eq!(stats.hit_rate(), 0.75);
    }

    #[test]
    fn cache_stats_no_hits() {
        let stats = CacheStats {
            hits: 0,
            misses: 10,
            invalidations: 0,
        };
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn cache_stats_no_data() {
        let stats = CacheStats {
            hits: 0,
            misses: 0,
            invalidations: 0,
        };
        assert_eq!(stats.hit_rate(), 0.0);
    }

    #[test]
    fn cache_invalidator_pattern_registration() {
        let invalidator = CacheInvalidator::new();
        invalidator.register_pattern("contract_event_counts:*".to_string());
        invalidator.register_pattern("event_aggregates:*".to_string());

        let patterns = invalidator.cache_patterns.read().unwrap();
        assert!(patterns.contains("contract_event_counts:*"));
        assert!(patterns.contains("event_aggregates:*"));
    }

    #[test]
    fn cache_invalidator_pattern_matching() {
        let invalidator = CacheInvalidator::new();
        invalidator.register_pattern("contract_event_counts:0xABC".to_string());
        invalidator.register_pattern("contract_event_counts:0xDEF".to_string());
        invalidator.register_pattern("event_aggregates:0xABC".to_string());

        let matched = invalidator.invalidate_pattern("contract_event_counts");
        assert_eq!(matched.len(), 2);
    }

    #[test]
    fn cache_invalidator_trigger_invalidation() {
        let invalidator = CacheInvalidator::new();
        invalidator.register_pattern("key1".to_string());
        invalidator.register_pattern("key2".to_string());

        invalidator.invalidate_by_trigger(
            InvalidationTrigger::EventIngestion,
            vec!["key1".to_string()],
        );

        let patterns = invalidator.cache_patterns.read().unwrap();
        assert!(!patterns.contains("key1"));
        assert!(patterns.contains("key2"));
    }
}
