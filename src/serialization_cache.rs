//! Cached JSON serialization for hot entities (Issue #687, extended in #959).
//!
//! Serializing the same event repeatedly - once per subscriber, once per replay,
//! once per export - burns CPU producing bytes that are identical every time.
//! This module keeps those bytes and hands them back.
//!
//! The hard part of any cache is not the hit; it is knowing when the hit is
//! wrong. Three mechanisms cover that here:
//!
//! * **TTL and capacity**, from moka, bound how long a stale entry can survive
//!   and how much memory the cache can take.
//! * **Explicit invalidation** removes one known-changed entry immediately.
//! * **Versioning** invalidates a whole entity type in constant time by changing
//!   the key prefix, without walking the cache. The orphaned entries are then
//!   unreachable and age out on their own.
//!
//! Versioning is what makes bulk invalidation cheap enough to do on every schema
//! change or redeploy. Walking a ten-thousand-entry cache to drop half of it
//! would cost more than the serializations it saves.

use moka::future::Cache;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tracing::debug;

use dashmap::DashMap;

/// Entries retained before capacity eviction begins.
pub const DEFAULT_MAX_CAPACITY: u64 = 10_000;

/// How long a serialized payload may be served before it is re-serialized.
pub const DEFAULT_TTL_SECS: u64 = 300;

/// Entity type used by [`SerializedEventCache::get_or_serialize`], which takes a
/// pre-built key rather than an entity type and id.
pub const DEFAULT_ENTITY_TYPE: &str = "event";

/// A point-in-time copy of the cache's counters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SerializationMetrics {
    pub total_serializations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub total_serialization_time_us: u64,
    pub evictions: u64,
    pub invalidations: u64,
    pub prewarmed_entries: u64,
    /// Bytes handed back from cache: the serialization work actually avoided.
    pub bytes_served_from_cache: u64,
    /// Bytes produced by real serialization passes.
    pub bytes_serialized: u64,
    /// Live entry count at the time of the snapshot.
    pub entry_count: u64,
    /// Global cache version; bumped by a full invalidation.
    pub version: u64,
}

impl SerializationMetrics {
    /// Fraction of lookups served from cache, 0.0 when nothing has been asked for.
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 {
            0.0
        } else {
            self.cache_hits as f64 / total as f64
        }
    }

    /// Mean time of a real serialization pass, in microseconds.
    pub fn avg_serialization_time_us(&self) -> f64 {
        if self.total_serializations == 0 {
            0.0
        } else {
            self.total_serialization_time_us as f64 / self.total_serializations as f64
        }
    }

    /// Microseconds of serialization the cache avoided, estimated by charging
    /// each hit the mean cost of a miss.
    ///
    /// An estimate rather than a measurement - the exact cost of a
    /// serialization that never happened is not knowable - but it is the figure
    /// that answers "is this cache worth its memory", which hit rate alone does
    /// not: a 99% hit rate on payloads that take a microsecond to build saves
    /// nothing worth having.
    pub fn estimated_time_saved_us(&self) -> f64 {
        self.avg_serialization_time_us() * self.cache_hits as f64
    }
}

/// Reason an entry left the cache, for metrics and logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidationStrategy {
    /// One entry, by entity type and id.
    Key,
    /// Every entry of one entity type, by version bump.
    EntityType,
    /// The whole cache.
    All,
}

impl InvalidationStrategy {
    fn label(self) -> &'static str {
        match self {
            InvalidationStrategy::Key => "key",
            InvalidationStrategy::EntityType => "entity_type",
            InvalidationStrategy::All => "all",
        }
    }
}

#[derive(Debug, Default)]
struct Counters {
    total_serializations: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    total_serialization_time_us: AtomicU64,
    evictions: AtomicU64,
    invalidations: AtomicU64,
    prewarmed_entries: AtomicU64,
    bytes_served_from_cache: AtomicU64,
    bytes_serialized: AtomicU64,
}

impl Counters {
    fn reset(&self) {
        self.total_serializations.store(0, Ordering::Relaxed);
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.total_serialization_time_us.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.invalidations.store(0, Ordering::Relaxed);
        self.prewarmed_entries.store(0, Ordering::Relaxed);
        self.bytes_served_from_cache.store(0, Ordering::Relaxed);
        self.bytes_serialized.store(0, Ordering::Relaxed);
    }
}

pub struct SerializedEventCache {
    cache: Arc<Cache<String, Vec<u8>>>,
    counters: Arc<Counters>,
    /// Bumped by [`SerializedEventCache::invalidate_all`]; part of every key.
    global_version: Arc<AtomicU64>,
    /// Per-entity-type versions, bumped by
    /// [`SerializedEventCache::invalidate_entity_type`].
    entity_versions: Arc<DashMap<String, u64>>,
}

impl SerializedEventCache {
    pub fn new(max_capacity: u64, ttl_secs: u64) -> Self {
        let counters = Arc::new(Counters::default());
        let eviction_counters = Arc::clone(&counters);

        let cache = Arc::new(
            Cache::builder()
                .max_capacity(max_capacity)
                .time_to_live(std::time::Duration::from_secs(ttl_secs))
                .eviction_listener(move |key: Arc<String>, _value, _cause| {
                    eviction_counters.evictions.fetch_add(1, Ordering::Relaxed);
                    crate::metrics::record_serialization_cache_eviction(&entity_type_of(&key));
                })
                .build(),
        );

        debug!(
            max_capacity,
            ttl_secs, "Initialized serialization cache (versioned invalidation)"
        );

        Self {
            cache,
            counters,
            global_version: Arc::new(AtomicU64::new(0)),
            entity_versions: Arc::new(DashMap::new()),
        }
    }

    /// A cache with the module defaults.
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_MAX_CAPACITY, DEFAULT_TTL_SECS)
    }

    // ── Keys and versioning ──────────────────────────────────────────────────

    fn entity_version(&self, entity_type: &str) -> u64 {
        self.entity_versions
            .get(entity_type)
            .map_or(0, |entry| *entry.value())
    }

    /// Build the physical cache key for a logical entity.
    ///
    /// Both versions are baked in, so bumping either makes every key built
    /// before the bump unreachable without touching a single entry.
    pub fn versioned_key(&self, entity_type: &str, id: &str) -> String {
        format!(
            "v{}:{}:e{}:{}",
            self.global_version.load(Ordering::Acquire),
            entity_type,
            self.entity_version(entity_type),
            id
        )
    }

    /// Current global version.
    pub fn version(&self) -> u64 {
        self.global_version.load(Ordering::Acquire)
    }

    /// Current version of one entity type.
    pub fn entity_type_version(&self, entity_type: &str) -> u64 {
        self.entity_version(entity_type)
    }

    // ── Lookup ───────────────────────────────────────────────────────────────

    /// Serialize `value` unless an identical payload is already cached.
    ///
    /// `key` is used verbatim, for callers that build their own keys. Prefer
    /// [`SerializedEventCache::get_or_serialize_entity`], which participates in
    /// versioned invalidation.
    pub async fn get_or_serialize<F>(
        &self,
        key: &str,
        value: &Value,
        fallback: F,
    ) -> Result<Vec<u8>, serde_json::Error>
    where
        F: FnOnce(&Value) -> Result<Vec<u8>, serde_json::Error>,
    {
        self.get_or_serialize_with_type(DEFAULT_ENTITY_TYPE, key, value, fallback)
            .await
    }

    /// Versioned variant: the key is derived from `entity_type` and `id`, so a
    /// version bump for that entity type takes this entry with it.
    pub async fn get_or_serialize_entity<F>(
        &self,
        entity_type: &str,
        id: &str,
        value: &Value,
        fallback: F,
    ) -> Result<Vec<u8>, serde_json::Error>
    where
        F: FnOnce(&Value) -> Result<Vec<u8>, serde_json::Error>,
    {
        let key = self.versioned_key(entity_type, id);
        self.get_or_serialize_with_type(entity_type, &key, value, fallback)
            .await
    }

    async fn get_or_serialize_with_type<F>(
        &self,
        entity_type: &str,
        key: &str,
        value: &Value,
        fallback: F,
    ) -> Result<Vec<u8>, serde_json::Error>
    where
        F: FnOnce(&Value) -> Result<Vec<u8>, serde_json::Error>,
    {
        if let Some(cached) = self.cache.get(key).await {
            let len = cached.len() as u64;
            self.counters.cache_hits.fetch_add(1, Ordering::Relaxed);
            self.counters
                .bytes_served_from_cache
                .fetch_add(len, Ordering::Relaxed);
            crate::metrics::record_serialization_cache_hit(entity_type);
            crate::metrics::record_serialization_cache_bytes_saved(entity_type, len);
            return Ok(cached);
        }

        let start = Instant::now();
        let serialized = fallback(value)?;
        let duration_us = start.elapsed().as_micros() as u64;
        let len = serialized.len() as u64;

        self.cache.insert(key.to_string(), serialized.clone()).await;

        self.counters.cache_misses.fetch_add(1, Ordering::Relaxed);
        self.counters
            .total_serializations
            .fetch_add(1, Ordering::Relaxed);
        self.counters
            .total_serialization_time_us
            .fetch_add(duration_us, Ordering::Relaxed);
        self.counters
            .bytes_serialized
            .fetch_add(len, Ordering::Relaxed);

        crate::metrics::record_serialization_time(entity_type, duration_us);
        crate::metrics::record_serialization_cache_miss(entity_type);
        crate::metrics::update_serialization_cache_entry_count(self.cache.entry_count());

        Ok(serialized)
    }

    /// Read a cached payload without serializing on a miss.
    pub async fn peek(&self, entity_type: &str, id: &str) -> Option<Vec<u8>> {
        self.cache.get(&self.versioned_key(entity_type, id)).await
    }

    // ── Pre-warming ──────────────────────────────────────────────────────────

    /// Serialize and cache a batch of entities up front.
    ///
    /// Worth doing after a restart or a version bump, where the alternative is
    /// that the first request for each hot entity pays full serialization cost.
    /// Entries already present are left alone, so a pre-warm pass is safe to run
    /// against a warm cache and cannot evict something newer than itself.
    ///
    /// Returns the number of entries actually inserted.
    pub async fn prewarm<'a, I>(&self, entity_type: &str, entries: I) -> u64
    where
        I: IntoIterator<Item = (&'a str, &'a Value)>,
    {
        let mut inserted: u64 = 0;

        for (id, value) in entries {
            let key = self.versioned_key(entity_type, id);
            if self.cache.get(&key).await.is_some() {
                continue;
            }

            let start = Instant::now();
            let Ok(serialized) = serde_json::to_vec(value) else {
                // A value that cannot be serialized is not a pre-warm failure
                // worth aborting the batch for; it will surface on the real
                // request path with a caller to return the error to.
                debug!(entity_type, id, "Skipping unserializable prewarm entry");
                continue;
            };
            let duration_us = start.elapsed().as_micros() as u64;
            let len = serialized.len() as u64;

            self.cache.insert(key, serialized).await;

            inserted += 1;
            self.counters
                .total_serializations
                .fetch_add(1, Ordering::Relaxed);
            self.counters
                .total_serialization_time_us
                .fetch_add(duration_us, Ordering::Relaxed);
            self.counters
                .bytes_serialized
                .fetch_add(len, Ordering::Relaxed);
        }

        self.counters
            .prewarmed_entries
            .fetch_add(inserted, Ordering::Relaxed);
        crate::metrics::record_serialization_cache_prewarm(entity_type, inserted);
        crate::metrics::update_serialization_cache_entry_count(self.cache.entry_count());

        debug!(entity_type, inserted, "Serialization cache prewarmed");
        inserted
    }

    // ── Invalidation ─────────────────────────────────────────────────────────

    /// Drop one entry. Use when a single entity is known to have changed.
    pub async fn invalidate(&self, entity_type: &str, id: &str) {
        self.cache.invalidate(&self.versioned_key(entity_type, id)).await;
        self.counters.invalidations.fetch_add(1, Ordering::Relaxed);
        crate::metrics::record_serialization_cache_invalidation(
            entity_type,
            InvalidationStrategy::Key.label(),
        );
    }

    /// Drop every entry of one entity type, in constant time.
    ///
    /// The version bump changes the key prefix rather than walking the cache, so
    /// this costs the same whether the type has ten entries or ten thousand. The
    /// orphaned entries are unreachable and age out through TTL and capacity
    /// eviction; they are not leaked, only deferred.
    pub async fn invalidate_entity_type(&self, entity_type: &str) -> u64 {
        let next = self
            .entity_versions
            .entry(entity_type.to_string())
            .and_modify(|v| *v += 1)
            .or_insert(1);
        let version = *next;
        drop(next);

        self.counters.invalidations.fetch_add(1, Ordering::Relaxed);
        crate::metrics::record_serialization_cache_invalidation(
            entity_type,
            InvalidationStrategy::EntityType.label(),
        );
        crate::metrics::update_serialization_cache_version(entity_type, version);

        debug!(entity_type, version, "Serialization cache entity invalidated");
        version
    }

    /// Drop everything, and bump the global version so any key built from a
    /// stale snapshot cannot resolve either.
    pub async fn invalidate_all(&self) -> u64 {
        let version = self.global_version.fetch_add(1, Ordering::AcqRel) + 1;
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;

        self.counters.invalidations.fetch_add(1, Ordering::Relaxed);
        crate::metrics::record_serialization_cache_invalidation(
            "all",
            InvalidationStrategy::All.label(),
        );
        crate::metrics::update_serialization_cache_version("all", version);
        crate::metrics::update_serialization_cache_entry_count(self.cache.entry_count());

        debug!(version, "Serialization cache fully invalidated");
        version
    }

    // ── Statistics ───────────────────────────────────────────────────────────

    /// Snapshot the counters.
    ///
    /// Pending moka maintenance is run first, so eviction counts and the entry
    /// count reflect what has actually happened rather than lagging behind it.
    pub async fn get_metrics(&self) -> SerializationMetrics {
        self.cache.run_pending_tasks().await;

        let metrics = SerializationMetrics {
            total_serializations: self.counters.total_serializations.load(Ordering::Relaxed),
            cache_hits: self.counters.cache_hits.load(Ordering::Relaxed),
            cache_misses: self.counters.cache_misses.load(Ordering::Relaxed),
            total_serialization_time_us: self
                .counters
                .total_serialization_time_us
                .load(Ordering::Relaxed),
            evictions: self.counters.evictions.load(Ordering::Relaxed),
            invalidations: self.counters.invalidations.load(Ordering::Relaxed),
            prewarmed_entries: self.counters.prewarmed_entries.load(Ordering::Relaxed),
            bytes_served_from_cache: self
                .counters
                .bytes_served_from_cache
                .load(Ordering::Relaxed),
            bytes_serialized: self.counters.bytes_serialized.load(Ordering::Relaxed),
            entry_count: self.cache.entry_count(),
            version: self.global_version.load(Ordering::Acquire),
        };

        crate::metrics::update_serialization_cache_hit_rate(
            DEFAULT_ENTITY_TYPE,
            metrics.hit_rate(),
        );
        crate::metrics::update_serialization_cache_entry_count(metrics.entry_count);

        metrics
    }

    /// Live entry count without running maintenance. Approximate by design.
    pub fn entry_count(&self) -> u64 {
        self.cache.entry_count()
    }

    /// Empty the cache and reset every counter.
    ///
    /// Distinct from [`SerializedEventCache::invalidate_all`], which keeps the
    /// counters so the effect of the invalidation stays visible in metrics.
    pub async fn clear(&self) {
        self.cache.invalidate_all();
        self.cache.run_pending_tasks().await;
        self.counters.reset();
        crate::metrics::update_serialization_cache_entry_count(self.cache.entry_count());
    }
}

/// Recover the entity type from a versioned key, for eviction accounting.
///
/// Keys are `v{global}:{entity_type}:e{entity}:{id}`. A key that does not
/// parse is attributed to `unknown` rather than dropped, so eviction counts
/// stay complete even for hand-built keys.
fn entity_type_of(key: &str) -> String {
    key.split(':')
        .nth(1)
        .filter(|segment| !segment.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

pub fn optimize_serialization(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}

pub fn optimize_serialization_pretty(value: &Value) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

pub fn serialize_compact(value: &Value) -> Result<Vec<u8>, serde_json::Error> {
    let mut buffer = Vec::with_capacity(1024);
    let mut ser = serde_json::Serializer::new(&mut buffer);
    value.serialize(&mut ser)?;
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(id: u64) -> Value {
        json!({
            "type": "contract_event",
            "contract_id": "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABC",
            "seq": id,
            "data": {"key": "value"}
        })
    }

    // ── Basic caching ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn serialization_cache_basic() {
        let cache = SerializedEventCache::new(100, 300);
        let value = event(1);
        let key = "event:12345";

        let first = cache
            .get_or_serialize(key, &value, serde_json::to_vec)
            .await
            .expect("serializes");
        let second = cache
            .get_or_serialize(key, &value, serde_json::to_vec)
            .await
            .expect("serves from cache");

        assert_eq!(first, second);

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 1);
        assert_eq!(metrics.total_serializations, 1);
    }

    #[tokio::test]
    async fn a_hit_does_not_call_the_fallback() {
        let cache = SerializedEventCache::with_defaults();
        let value = event(1);

        cache
            .get_or_serialize_entity("event", "1", &value, serde_json::to_vec)
            .await
            .unwrap();

        let hit = cache
            .get_or_serialize_entity("event", "1", &value, |_| {
                panic!("fallback ran on what should have been a hit")
            })
            .await
            .unwrap();

        assert_eq!(hit, serde_json::to_vec(&value).unwrap());
    }

    #[tokio::test]
    async fn distinct_ids_do_not_collide() {
        let cache = SerializedEventCache::with_defaults();

        let a = cache
            .get_or_serialize_entity("event", "1", &event(1), serde_json::to_vec)
            .await
            .unwrap();
        let b = cache
            .get_or_serialize_entity("event", "2", &event(2), serde_json::to_vec)
            .await
            .unwrap();

        assert_ne!(a, b);
        assert_eq!(cache.get_metrics().await.cache_misses, 2);
    }

    #[tokio::test]
    async fn the_same_id_under_different_entity_types_does_not_collide() {
        let cache = SerializedEventCache::with_defaults();

        cache
            .get_or_serialize_entity("event", "1", &event(1), serde_json::to_vec)
            .await
            .unwrap();
        cache
            .get_or_serialize_entity("receipt", "1", &event(99), serde_json::to_vec)
            .await
            .unwrap();

        assert_eq!(cache.get_metrics().await.cache_misses, 2);
        assert_eq!(
            cache.peek("receipt", "1").await,
            Some(serde_json::to_vec(&event(99)).unwrap())
        );
    }

    #[tokio::test]
    async fn a_fallback_error_is_propagated_and_nothing_is_cached() {
        let cache = SerializedEventCache::with_defaults();

        let result = cache
            .get_or_serialize_entity("event", "1", &event(1), |_| {
                serde_json::from_str::<Vec<u8>>("not json")
            })
            .await;

        assert!(result.is_err());
        assert!(cache.peek("event", "1").await.is_none());
    }

    // ── Versioning ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn keys_carry_the_global_and_entity_versions() {
        let cache = SerializedEventCache::with_defaults();
        assert_eq!(cache.versioned_key("event", "7"), "v0:event:e0:7");

        cache.invalidate_entity_type("event").await;
        assert_eq!(cache.versioned_key("event", "7"), "v0:event:e1:7");

        cache.invalidate_all().await;
        assert_eq!(cache.versioned_key("event", "7"), "v1:event:e1:7");
    }

    #[tokio::test]
    async fn bumping_an_entity_version_leaves_other_types_alone() {
        let cache = SerializedEventCache::with_defaults();

        cache
            .get_or_serialize_entity("event", "1", &event(1), serde_json::to_vec)
            .await
            .unwrap();
        cache
            .get_or_serialize_entity("receipt", "1", &event(2), serde_json::to_vec)
            .await
            .unwrap();

        cache.invalidate_entity_type("event").await;

        assert!(cache.peek("event", "1").await.is_none());
        assert!(cache.peek("receipt", "1").await.is_some());
    }

    #[tokio::test]
    async fn version_accessors_track_the_bumps() {
        let cache = SerializedEventCache::with_defaults();
        assert_eq!(cache.version(), 0);
        assert_eq!(cache.entity_type_version("event"), 0);

        assert_eq!(cache.invalidate_entity_type("event").await, 1);
        assert_eq!(cache.invalidate_entity_type("event").await, 2);
        assert_eq!(cache.entity_type_version("event"), 2);
        assert_eq!(cache.entity_type_version("receipt"), 0);

        assert_eq!(cache.invalidate_all().await, 1);
        assert_eq!(cache.version(), 1);
    }

    // ── Invalidation ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn invalidating_one_key_leaves_its_neighbours() {
        let cache = SerializedEventCache::with_defaults();

        cache
            .get_or_serialize_entity("event", "1", &event(1), serde_json::to_vec)
            .await
            .unwrap();
        cache
            .get_or_serialize_entity("event", "2", &event(2), serde_json::to_vec)
            .await
            .unwrap();

        cache.invalidate("event", "1").await;

        assert!(cache.peek("event", "1").await.is_none());
        assert!(cache.peek("event", "2").await.is_some());
        assert_eq!(cache.get_metrics().await.invalidations, 1);
    }

    #[tokio::test]
    async fn invalidate_all_empties_the_cache_but_keeps_the_counters() {
        let cache = SerializedEventCache::with_defaults();
        cache
            .get_or_serialize_entity("event", "1", &event(1), serde_json::to_vec)
            .await
            .unwrap();

        cache.invalidate_all().await;

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics.entry_count, 0);
        // The miss that populated the cache is still on the record.
        assert_eq!(metrics.cache_misses, 1);
        assert!(metrics.invalidations >= 1);
    }

    #[tokio::test]
    async fn clear_resets_the_counters_too() {
        let cache = SerializedEventCache::with_defaults();
        cache
            .get_or_serialize_entity("event", "1", &event(1), serde_json::to_vec)
            .await
            .unwrap();

        cache.clear().await;

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics, SerializationMetrics::default());
    }

    // ── Pre-warming ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn prewarm_populates_the_cache_so_the_first_request_hits() {
        let cache = SerializedEventCache::with_defaults();
        let one = event(1);
        let two = event(2);

        let inserted = cache
            .prewarm("event", vec![("1", &one), ("2", &two)])
            .await;
        assert_eq!(inserted, 2);

        cache
            .get_or_serialize_entity("event", "1", &one, |_| {
                panic!("fallback ran on a prewarmed entry")
            })
            .await
            .unwrap();

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics.prewarmed_entries, 2);
        assert_eq!(metrics.cache_hits, 1);
        assert_eq!(metrics.cache_misses, 0);
    }

    #[tokio::test]
    async fn prewarm_skips_entries_that_are_already_cached() {
        let cache = SerializedEventCache::with_defaults();
        let one = event(1);

        cache
            .get_or_serialize_entity("event", "1", &one, serde_json::to_vec)
            .await
            .unwrap();

        assert_eq!(cache.prewarm("event", vec![("1", &one)]).await, 0);
    }

    #[tokio::test]
    async fn prewarming_after_a_version_bump_repopulates_the_new_keys() {
        let cache = SerializedEventCache::with_defaults();
        let one = event(1);

        cache.prewarm("event", vec![("1", &one)]).await;
        cache.invalidate_entity_type("event").await;
        assert!(cache.peek("event", "1").await.is_none());

        assert_eq!(cache.prewarm("event", vec![("1", &one)]).await, 1);
        assert!(cache.peek("event", "1").await.is_some());
    }

    // ── Statistics ───────────────────────────────────────────────────────────

    #[test]
    fn hit_rate_is_zero_before_any_lookup() {
        assert!((SerializationMetrics::default().hit_rate() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hit_rate_and_averages_are_computed_from_the_counters() {
        let metrics = SerializationMetrics {
            cache_hits: 75,
            cache_misses: 25,
            total_serializations: 25,
            total_serialization_time_us: 2_500,
            ..SerializationMetrics::default()
        };

        assert!((metrics.hit_rate() - 0.75).abs() < f64::EPSILON);
        assert!((metrics.avg_serialization_time_us() - 100.0).abs() < f64::EPSILON);
        assert!((metrics.estimated_time_saved_us() - 7_500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn averages_are_zero_rather_than_nan_with_no_serializations() {
        let metrics = SerializationMetrics::default();
        assert!((metrics.avg_serialization_time_us() - 0.0).abs() < f64::EPSILON);
        assert!((metrics.estimated_time_saved_us() - 0.0).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn byte_counters_separate_work_done_from_work_avoided() {
        let cache = SerializedEventCache::with_defaults();
        let value = event(1);
        let size = serde_json::to_vec(&value).unwrap().len() as u64;

        cache
            .get_or_serialize_entity("event", "1", &value, serde_json::to_vec)
            .await
            .unwrap();
        cache
            .get_or_serialize_entity("event", "1", &value, serde_json::to_vec)
            .await
            .unwrap();

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics.bytes_serialized, size);
        assert_eq!(metrics.bytes_served_from_cache, size);
    }

    #[tokio::test]
    async fn entry_count_reflects_what_is_held() {
        let cache = SerializedEventCache::with_defaults();

        for i in 0..5_u64 {
            cache
                .get_or_serialize_entity("event", &i.to_string(), &event(i), serde_json::to_vec)
                .await
                .unwrap();
        }

        assert_eq!(cache.get_metrics().await.entry_count, 5);
    }

    // ── Eviction ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn capacity_eviction_is_counted() {
        let cache = SerializedEventCache::new(5, 300);

        for i in 0..40_u64 {
            cache
                .get_or_serialize_entity("event", &i.to_string(), &event(i), serde_json::to_vec)
                .await
                .unwrap();
        }

        let metrics = cache.get_metrics().await;
        assert!(
            metrics.entry_count <= 5,
            "capacity was not enforced: {} entries",
            metrics.entry_count
        );
        assert!(
            metrics.evictions > 0,
            "entries left the cache without being counted"
        );
    }

    #[test]
    fn entity_type_is_recovered_from_a_versioned_key() {
        assert_eq!(entity_type_of("v0:event:e0:1"), "event");
        assert_eq!(entity_type_of("v12:receipt:e3:abc"), "receipt");
    }

    #[test]
    fn an_unparseable_key_is_attributed_rather_than_dropped() {
        assert_eq!(entity_type_of("legacy-key"), "unknown");
        assert_eq!(entity_type_of("v0::e0:1"), "unknown");
    }

    #[test]
    fn invalidation_strategy_labels_are_stable() {
        assert_eq!(InvalidationStrategy::Key.label(), "key");
        assert_eq!(InvalidationStrategy::EntityType.label(), "entity_type");
        assert_eq!(InvalidationStrategy::All.label(), "all");
    }

    // ── Serialization helpers ────────────────────────────────────────────────

    #[test]
    fn optimize_serialization_basic() {
        let result = optimize_serialization(&event(1));
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn serialize_compact_efficiency() {
        let compact = serialize_compact(&event(1)).unwrap();
        let pretty = optimize_serialization_pretty(&event(1)).unwrap();

        assert!(!compact.is_empty());
        assert!(
            compact.len() < pretty.len(),
            "compact form should be smaller than the pretty form"
        );
    }

    #[test]
    fn all_three_helpers_agree_on_content() {
        let value = event(1);
        let compact = serialize_compact(&value).unwrap();
        let optimized = optimize_serialization(&value).unwrap();

        assert_eq!(compact, optimized);
        assert_eq!(
            serde_json::from_slice::<Value>(&compact).unwrap(),
            serde_json::from_str::<Value>(&optimize_serialization_pretty(&value).unwrap()).unwrap()
        );
    }
}
