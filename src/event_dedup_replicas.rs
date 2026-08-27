/// Issue #880: Event deduplication across replicas
/// Ensures exactly-once event processing using distributed hash verification and advisory locks

use sqlx::{PgPool, postgres::PgAdvisoryLock};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use chrono::{DateTime, Utc};

/// Configuration for replica-aware deduplication
#[derive(Debug, Clone)]
pub struct ReplicaDedupConfig {
    /// Deduplication window in seconds
    pub dedup_window_secs: u64,
    /// Initial bloom filter capacity
    pub bloom_capacity: usize,
    /// False positive rate for bloom filter
    pub bloom_fp_rate: f64,
    /// Whether to use cross-replica sync
    pub enable_cross_replica_sync: bool,
}

impl Default for ReplicaDedupConfig {
    fn default() -> Self {
        Self {
            dedup_window_secs: 3600,
            bloom_capacity: 10_000,
            bloom_fp_rate: 0.01,
            enable_cross_replica_sync: true,
        }
    }
}

/// Distributed hash for cross-replica verification
#[derive(Debug, Clone)]
pub struct DistributedHash {
    pub fingerprint: String,
    pub replica_id: String,
    pub created_at: DateTime<Utc>,
}

/// Cross-replica deduplication state
#[derive(Debug, Clone)]
pub struct ReplicaDedupState {
    /// Replica identifier
    pub replica_id: String,
    /// Local dedup fingerprints with timestamps
    pub local_hashes: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// Configuration
    pub config: ReplicaDedupConfig,
}

impl ReplicaDedupState {
    /// Create a new replica dedup state
    pub fn new(replica_id: String, config: ReplicaDedupConfig) -> Self {
        Self {
            replica_id,
            local_hashes: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// Check if an event fingerprint is a duplicate across replicas
    pub async fn is_duplicate(
        &self,
        pool: &PgPool,
        fingerprint: &str,
    ) -> Result<bool, sqlx::Error> {
        // Check local cache first
        let local_hashes = self.local_hashes.read().await;
        if let Some(created_at) = local_hashes.get(fingerprint) {
            let age = (Utc::now() - *created_at).num_seconds();
            if age < self.config.dedup_window_secs as i64 {
                return Ok(true);
            }
        }
        drop(local_hashes);

        // Check database with cross-replica verification
        self.check_cross_replica_duplicate(pool, fingerprint).await
    }

    /// Check if fingerprint exists in any replica using advisory lock
    async fn check_cross_replica_duplicate(
        &self,
        pool: &PgPool,
        fingerprint: &str,
    ) -> Result<bool, sqlx::Error> {
        // Use advisory lock to prevent race conditions between replicas
        // Lock ID is derived from fingerprint hash
        let lock_id = self.derive_lock_id(fingerprint);

        // Acquire advisory lock (advisory locks are per-session in PostgreSQL)
        let mut tx = pool.begin().await?;

        // Acquire the advisory lock - this will be held until transaction commits
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(lock_id as i64)
            .execute(&mut *tx)
            .await?;

        // Check if fingerprint exists within the dedup window
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM events
             WHERE fingerprint = $1
               AND created_at >= NOW() - ($2 * INTERVAL '1 second')",
        )
        .bind(fingerprint)
        .bind(self.config.dedup_window_secs as i64)
        .fetch_one(&mut *tx)
        .await?;

        if count > 0 {
            tx.commit().await?;
            return Ok(true);
        }

        // Also check for this fingerprint in the distributed hash verification table
        let replica_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_dedup_replicas
             WHERE fingerprint = $1
               AND created_at >= NOW() - ($2 * INTERVAL '1 second')",
        )
        .bind(fingerprint)
        .bind(self.config.dedup_window_secs as i64)
        .fetch_one(&mut *tx)
        .await?;

        if replica_count > 0 {
            tx.commit().await?;
            return Ok(true);
        }

        tx.commit().await?;
        Ok(false)
    }

    /// Register a fingerprint for cross-replica deduplication
    pub async fn register_fingerprint(
        &self,
        pool: &PgPool,
        fingerprint: &str,
    ) -> Result<(), sqlx::Error> {
        let lock_id = self.derive_lock_id(fingerprint);

        let mut tx = pool.begin().await?;

        // Acquire advisory lock
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(lock_id as i64)
            .execute(&mut *tx)
            .await?;

        // Register in event_dedup_replicas table
        sqlx::query(
            "INSERT INTO event_dedup_replicas (fingerprint, replica_id, created_at)
             VALUES ($1, $2, NOW())
             ON CONFLICT (fingerprint, replica_id) DO NOTHING",
        )
        .bind(fingerprint)
        .bind(&self.replica_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Update local cache
        let mut local_hashes = self.local_hashes.write().await;
        local_hashes.insert(fingerprint.to_string(), Utc::now());

        Ok(())
    }

    /// Synchronize dedup state during failover
    pub async fn sync_failover_state(
        &self,
        pool: &PgPool,
        source_replica_id: &str,
    ) -> Result<u64, sqlx::Error> {
        // Copy recent fingerprints from source replica to this replica
        let result = sqlx::query(
            "INSERT INTO event_dedup_replicas (fingerprint, replica_id, created_at)
             SELECT fingerprint, $2, created_at
             FROM event_dedup_replicas
             WHERE replica_id = $1
               AND created_at >= NOW() - ($3 * INTERVAL '1 second')
             ON CONFLICT (fingerprint, replica_id) DO NOTHING",
        )
        .bind(source_replica_id)
        .bind(&self.replica_id)
        .bind(self.config.dedup_window_secs as i64)
        .execute(pool)
        .await?;

        info!(
            "Synchronized {} fingerprints from replica {} to {}",
            result.rows_affected(),
            source_replica_id,
            self.replica_id
        );

        Ok(result.rows_affected())
    }

    /// Get deduplication statistics
    pub async fn get_dedup_stats(
        &self,
        pool: &PgPool,
    ) -> Result<DedupStatistics, sqlx::Error> {
        let total_in_window: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_dedup_replicas
             WHERE created_at >= NOW() - ($1 * INTERVAL '1 second')",
        )
        .bind(self.config.dedup_window_secs as i64)
        .fetch_one(pool)
        .await?;

        let by_replica: Vec<(String, i64)> = sqlx::query_as(
            "SELECT replica_id, COUNT(*) as count FROM event_dedup_replicas
             WHERE created_at >= NOW() - ($1 * INTERVAL '1 second')
             GROUP BY replica_id",
        )
        .bind(self.config.dedup_window_secs as i64)
        .fetch_all(pool)
        .await?;

        let local_hashes = self.local_hashes.read().await;

        Ok(DedupStatistics {
            total_in_window,
            replicas_contributing: by_replica.len() as i64,
            local_cache_entries: local_hashes.len() as i64,
            by_replica: by_replica.into_iter().collect(),
        })
    }

    /// Derive a stable lock ID from fingerprint
    fn derive_lock_id(&self, fingerprint: &str) -> u32 {
        let bytes = fingerprint.as_bytes();
        let mut hash: u32 = 5381;
        for byte in bytes {
            hash = hash.wrapping_mul(33).wrapping_add(*byte as u32);
        }
        hash
    }

    /// Clean up expired entries from dedup tables
    pub async fn cleanup_expired(
        &self,
        pool: &PgPool,
    ) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            "DELETE FROM event_dedup_replicas
             WHERE created_at < NOW() - ($1 * INTERVAL '1 second')",
        )
        .bind(self.config.dedup_window_secs as i64)
        .execute(pool)
        .await?;

        info!("Cleaned up {} expired dedup entries", result.rows_affected());
        Ok(result.rows_affected())
    }
}

/// Deduplication statistics
#[derive(Debug, Clone, serde::Serialize)]
pub struct DedupStatistics {
    pub total_in_window: i64,
    pub replicas_contributing: i64,
    pub local_cache_entries: i64,
    pub by_replica: HashMap<String, i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_id_is_deterministic() {
        let state = ReplicaDedupState::new(
            "replica-1".to_string(),
            ReplicaDedupConfig::default(),
        );

        let fp = "abc123";
        let id1 = state.derive_lock_id(fp);
        let id2 = state.derive_lock_id(fp);

        assert_eq!(id1, id2, "Lock ID should be deterministic");
    }

    #[test]
    fn lock_id_differs_for_different_fingerprints() {
        let state = ReplicaDedupState::new(
            "replica-1".to_string(),
            ReplicaDedupConfig::default(),
        );

        let id1 = state.derive_lock_id("abc123");
        let id2 = state.derive_lock_id("xyz789");

        assert_ne!(id1, id2, "Different fingerprints should have different lock IDs");
    }

    #[test]
    fn default_config_has_reasonable_values() {
        let config = ReplicaDedupConfig::default();
        assert_eq!(config.dedup_window_secs, 3600);
        assert!(config.bloom_capacity > 0);
        assert!(config.bloom_fp_rate > 0.0 && config.bloom_fp_rate < 1.0);
        assert!(config.enable_cross_replica_sync);
    }
}
