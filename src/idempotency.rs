//! Request deduplication via idempotency keys.
//!
//! Complements the content-fingerprint event dedup in `dedup.rs`, which
//! guards against duplicate on-chain events. This module guards against
//! duplicate *client-initiated write requests* (e.g. a webhook registration
//! POST retried after a network timeout): the caller supplies an
//! `Idempotency-Key` header, and a repeat request with the same key within
//! the expiration window is served the cached response instead of being
//! re-executed.
//!
//! Storage here is an in-process, mutex-guarded map so no new infrastructure
//! is required; see `docs/idempotency.md` for the distributed-deployment
//! notes (shared Postgres/Redis-backed store keyed the same way).

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Header clients set to make a write request idempotent.
pub const IDEMPOTENCY_KEY_HEADER: &str = "idempotency-key";

/// Default time-to-live for a cached idempotency record.
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Debug)]
pub struct IdempotencyRecord {
    pub status_code: u16,
    pub body: String,
    pub created_at: SystemTime,
    pub ttl: Duration,
}

impl IdempotencyRecord {
    fn is_expired(&self) -> bool {
        self.created_at
            .elapsed()
            .map(|elapsed| elapsed > self.ttl)
            .unwrap_or(false)
    }
}

/// Tracks idempotency keys and their cached responses.
///
/// This is the "distributed deduplication" seam: `IdempotencyStore` is a
/// trait so a Postgres- or Redis-backed implementation can be swapped in for
/// multi-instance deployments without changing call sites. `InMemoryStore`
/// is the default, single-instance implementation.
pub trait IdempotencyStore: Send + Sync {
    fn get(&self, key: &str) -> Option<IdempotencyRecord>;
    fn put(&self, key: &str, record: IdempotencyRecord);
    fn remove_expired(&self);
}

/// Simple in-memory idempotency key store, suitable for single-instance
/// deployments or as a local cache in front of a distributed store.
#[derive(Default)]
pub struct InMemoryStore {
    records: RwLock<HashMap<String, IdempotencyRecord>>,
    hits: Mutex<u64>,
    misses: Mutex<u64>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hit_count(&self) -> u64 {
        *self.hits.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn miss_count(&self) -> u64 {
        *self.misses.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl IdempotencyStore for InMemoryStore {
    fn get(&self, key: &str) -> Option<IdempotencyRecord> {
        let record = self
            .records
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned();

        match &record {
            Some(r) if !r.is_expired() => {
                *self.hits.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                record_dedup_metric("hit");
            }
            _ => {
                *self.misses.lock().unwrap_or_else(|e| e.into_inner()) += 1;
                record_dedup_metric("miss");
            }
        }

        record.filter(|r| !r.is_expired())
    }

    fn put(&self, key: &str, record: IdempotencyRecord) {
        self.records
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_string(), record);
        record_dedup_metric("stored");
    }

    fn remove_expired(&self) {
        let mut guard = self.records.write().unwrap_or_else(|e| e.into_inner());
        let before = guard.len();
        guard.retain(|_, record| !record.is_expired());
        let removed = before - guard.len();
        if removed > 0 {
            extern crate metrics as m;
            m::counter!("soroban_pulse_idempotency_keys_expired_total").increment(removed as u64);
        }
    }
}

fn record_dedup_metric(outcome: &str) {
    extern crate metrics as m;
    m::counter!("soroban_pulse_idempotency_requests_total", "outcome" => outcome.to_string())
        .increment(1);
}

/// Look up a cached response for `key`, or run `execute` and cache its
/// result under `key` for `ttl`. This is the primary integration point for
/// handlers: wrap any non-idempotent side-effecting operation so retried
/// requests with the same key are answered from cache instead of
/// re-executing the side effect.
pub fn dedup_or_execute<F>(
    store: &dyn IdempotencyStore,
    key: &str,
    ttl: Duration,
    execute: F,
) -> (u16, String, bool)
where
    F: FnOnce() -> (u16, String),
{
    if let Some(cached) = store.get(key) {
        return (cached.status_code, cached.body, true);
    }

    let (status_code, body) = execute();
    store.put(
        key,
        IdempotencyRecord {
            status_code,
            body: body.clone(),
            created_at: SystemTime::now(),
            ttl,
        },
    );
    (status_code, body, false)
}

/// Compute an idempotency cache key from the caller-supplied key plus the
/// route, so the same key value cannot be replayed against a different
/// endpoint to fetch an unrelated cached response.
pub fn scoped_key(route: &str, idempotency_key: &str) -> String {
    format!("{route}:{idempotency_key}")
}

/// Returns the current unix-epoch milliseconds, used by callers that want to
/// display/report cache age alongside `IdempotencyRecord::created_at`.
pub fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_call_executes_and_caches() {
        let store = InMemoryStore::new();
        let (status, body, replayed) =
            dedup_or_execute(&store, "key-1", DEFAULT_TTL, || (201, "created".to_string()));
        assert_eq!(status, 201);
        assert_eq!(body, "created");
        assert!(!replayed);
    }

    #[test]
    fn second_call_with_same_key_is_served_from_cache() {
        let store = InMemoryStore::new();
        dedup_or_execute(&store, "key-2", DEFAULT_TTL, || (201, "created".to_string()));

        let mut executed_again = false;
        let (status, body, replayed) = dedup_or_execute(&store, "key-2", DEFAULT_TTL, || {
            executed_again = true;
            (201, "should-not-happen".to_string())
        });

        assert!(!executed_again);
        assert!(replayed);
        assert_eq!(status, 201);
        assert_eq!(body, "created");
    }

    #[test]
    fn expired_key_is_re_executed() {
        let store = InMemoryStore::new();
        store.put(
            "key-3",
            IdempotencyRecord {
                status_code: 200,
                body: "old".to_string(),
                created_at: SystemTime::now() - Duration::from_secs(10),
                ttl: Duration::from_secs(1),
            },
        );

        let (status, body, replayed) =
            dedup_or_execute(&store, "key-3", DEFAULT_TTL, || (200, "fresh".to_string()));

        assert!(!replayed);
        assert_eq!(status, 200);
        assert_eq!(body, "fresh");
    }

    #[test]
    fn remove_expired_purges_stale_entries() {
        let store = InMemoryStore::new();
        store.put(
            "stale",
            IdempotencyRecord {
                status_code: 200,
                body: "x".to_string(),
                created_at: SystemTime::now() - Duration::from_secs(100),
                ttl: Duration::from_secs(1),
            },
        );
        store.put(
            "fresh",
            IdempotencyRecord {
                status_code: 200,
                body: "y".to_string(),
                created_at: SystemTime::now(),
                ttl: DEFAULT_TTL,
            },
        );

        store.remove_expired();

        assert!(store.get("stale").is_none());
        assert!(store.get("fresh").is_some());
    }

    #[test]
    fn scoped_key_differs_per_route() {
        let a = scoped_key("/webhooks", "abc");
        let b = scoped_key("/subscriptions", "abc");
        assert_ne!(a, b);
    }

    #[test]
    fn hit_and_miss_counters_track_usage() {
        let store = InMemoryStore::new();
        assert!(store.get("missing").is_none());
        assert_eq!(store.miss_count(), 1);

        dedup_or_execute(&store, "present", DEFAULT_TTL, || (200, "ok".to_string()));
        // dedup_or_execute's internal `get` call also records a miss on first use.
        dedup_or_execute(&store, "present", DEFAULT_TTL, || (200, "ok".to_string()));
        assert!(store.hit_count() >= 1);
    }
}
