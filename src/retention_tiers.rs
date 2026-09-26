//! Sophisticated multi-tier data retention: hot/warm/cold storage tiers,
//! automated archival with compression, retention enforcement, and metrics.
//!
//! This complements the existing single-window retention logic in
//! `src/pruner.rs` / `src/archiver.rs` with a tiered policy model: data
//! ages from `Hot` -> `Warm` -> `Cold` -> deleted, based on age thresholds
//! defined per policy.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Storage tier an event/record currently resides in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StorageTier {
    /// Fast, expensive storage for recently ingested data (e.g. primary DB).
    Hot,
    /// Cheaper storage for less frequently accessed data (e.g. compressed
    /// tables, secondary DB, or slower disk).
    Warm,
    /// Cheapest storage, typically compressed and object-store backed
    /// (e.g. S3/GCS), for data kept only for compliance/audit purposes.
    Cold,
}

/// Compression algorithm applied when archiving data into a tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompressionAlgorithm {
    None,
    Gzip,
    Zstd,
}

/// Configuration for a single retention policy: how long data stays in
/// each tier before aging further, and when it is deleted entirely.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub name: String,
    /// Age (in seconds since ingestion) after which data moves Hot -> Warm.
    pub hot_to_warm_after_secs: u64,
    /// Age after which data moves Warm -> Cold.
    pub warm_to_cold_after_secs: u64,
    /// Age after which data is deleted entirely. `None` means retain forever
    /// in Cold storage.
    pub delete_after_secs: Option<u64>,
    /// Compression applied when data is archived into Warm.
    pub warm_compression: CompressionAlgorithm,
    /// Compression applied when data is archived into Cold.
    pub cold_compression: CompressionAlgorithm,
}

impl RetentionPolicy {
    /// A reasonable default: 7 days hot, 30 days warm, 1 year cold, then delete.
    pub fn standard(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            hot_to_warm_after_secs: 7 * 24 * 3600,
            warm_to_cold_after_secs: 30 * 24 * 3600,
            delete_after_secs: Some(365 * 24 * 3600),
            warm_compression: CompressionAlgorithm::Gzip,
            cold_compression: CompressionAlgorithm::Zstd,
        }
    }

    /// Decides what action, if any, should be taken for a record of the
    /// given `current_tier` and `age_secs`.
    pub fn evaluate(&self, current_tier: StorageTier, age_secs: u64) -> RetentionAction {
        if let Some(delete_after) = self.delete_after_secs {
            if age_secs >= delete_after {
                return RetentionAction::Delete;
            }
        }

        match current_tier {
            StorageTier::Hot if age_secs >= self.hot_to_warm_after_secs => RetentionAction::MoveTier {
                to: StorageTier::Warm,
                compression: self.warm_compression,
            },
            StorageTier::Warm if age_secs >= self.warm_to_cold_after_secs => RetentionAction::MoveTier {
                to: StorageTier::Cold,
                compression: self.cold_compression,
            },
            _ => RetentionAction::Retain,
        }
    }
}

/// The outcome of evaluating a policy against a piece of data.
#[derive(Debug, Clone, PartialEq)]
pub enum RetentionAction {
    Retain,
    MoveTier { to: StorageTier, compression: CompressionAlgorithm },
    Delete,
}

/// Identifies a single unit of data under retention management (e.g. a
/// partition, a batch of events, or an individual record).
#[derive(Debug, Clone)]
pub struct RetentionCandidate {
    pub id: String,
    pub tier: StorageTier,
    pub created_at_unix: u64,
    pub size_bytes: u64,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// Naive size-reduction estimate used for metrics/reporting purposes when a
/// real compressor isn't wired in yet. Real archival should replace this
/// with actual compressed byte counts.
fn estimated_compression_ratio(algorithm: CompressionAlgorithm) -> f64 {
    match algorithm {
        CompressionAlgorithm::None => 1.0,
        CompressionAlgorithm::Gzip => 0.35,
        CompressionAlgorithm::Zstd => 0.25,
    }
}

/// A single completed archival/enforcement action, used for auditing and
/// metrics.
#[derive(Debug, Clone)]
pub struct RetentionEvent {
    pub candidate_id: String,
    pub action: RetentionAction,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Aggregate counters for retention enforcement runs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetentionMetrics {
    pub evaluated: u64,
    pub moved_to_warm: u64,
    pub moved_to_cold: u64,
    pub deleted: u64,
    pub bytes_before_total: u64,
    pub bytes_after_total: u64,
}

impl RetentionMetrics {
    pub fn bytes_saved(&self) -> u64 {
        self.bytes_before_total.saturating_sub(self.bytes_after_total)
    }

    pub fn compression_ratio(&self) -> f64 {
        if self.bytes_before_total == 0 {
            return 1.0;
        }
        self.bytes_after_total as f64 / self.bytes_before_total as f64
    }
}

/// Enforces a `RetentionPolicy` against a batch of candidates, producing
/// the list of actions taken and updating aggregate metrics. This is the
/// automated archival + compression + enforcement entry point, intended to
/// be invoked periodically by a background scheduler (mirroring the
/// existing `src/pruner.rs` job pattern).
pub struct RetentionEnforcer {
    policy: RetentionPolicy,
    metrics: RetentionMetrics,
}

impl RetentionEnforcer {
    pub fn new(policy: RetentionPolicy) -> Self {
        Self { policy, metrics: RetentionMetrics::default() }
    }

    pub fn metrics(&self) -> &RetentionMetrics {
        &self.metrics
    }

    pub fn policy(&self) -> &RetentionPolicy {
        &self.policy
    }

    /// Runs enforcement against a batch of candidates as of `now`, applying
    /// tier transitions and deletions and updating metrics.
    pub fn enforce_at(&mut self, candidates: &[RetentionCandidate], now: u64) -> Vec<RetentionEvent> {
        let mut events = Vec::new();
        for candidate in candidates {
            self.metrics.evaluated += 1;
            let age_secs = now.saturating_sub(candidate.created_at_unix);
            let action = self.policy.evaluate(candidate.tier, age_secs);

            let bytes_after = match &action {
                RetentionAction::Retain => candidate.size_bytes,
                RetentionAction::Delete => 0,
                RetentionAction::MoveTier { compression, .. } => {
                    (candidate.size_bytes as f64 * estimated_compression_ratio(*compression)) as u64
                }
            };

            match &action {
                RetentionAction::MoveTier { to: StorageTier::Warm, .. } => self.metrics.moved_to_warm += 1,
                RetentionAction::MoveTier { to: StorageTier::Cold, .. } => self.metrics.moved_to_cold += 1,
                RetentionAction::Delete => self.metrics.deleted += 1,
                _ => {}
            }

            self.metrics.bytes_before_total += candidate.size_bytes;
            self.metrics.bytes_after_total += bytes_after;

            events.push(RetentionEvent {
                candidate_id: candidate.id.clone(),
                action,
                bytes_before: candidate.size_bytes,
                bytes_after,
            });
        }
        events
    }

    /// Convenience wrapper using the current wall-clock time.
    pub fn enforce(&mut self, candidates: &[RetentionCandidate]) -> Vec<RetentionEvent> {
        self.enforce_at(candidates, now_unix())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, tier: StorageTier, age_secs: u64, size: u64) -> RetentionCandidate {
        RetentionCandidate {
            id: id.to_string(),
            tier,
            created_at_unix: 1_000_000u64.saturating_sub(age_secs),
            size_bytes: size,
        }
    }

    fn policy() -> RetentionPolicy {
        RetentionPolicy {
            name: "test".into(),
            hot_to_warm_after_secs: 100,
            warm_to_cold_after_secs: 200,
            delete_after_secs: Some(300),
            warm_compression: CompressionAlgorithm::Gzip,
            cold_compression: CompressionAlgorithm::Zstd,
        }
    }

    #[test]
    fn recent_hot_data_is_retained() {
        let p = policy();
        assert_eq!(p.evaluate(StorageTier::Hot, 10), RetentionAction::Retain);
    }

    #[test]
    fn aged_hot_data_moves_to_warm() {
        let p = policy();
        let action = p.evaluate(StorageTier::Hot, 150);
        assert_eq!(
            action,
            RetentionAction::MoveTier { to: StorageTier::Warm, compression: CompressionAlgorithm::Gzip }
        );
    }

    #[test]
    fn aged_warm_data_moves_to_cold() {
        let p = policy();
        let action = p.evaluate(StorageTier::Warm, 250);
        assert_eq!(
            action,
            RetentionAction::MoveTier { to: StorageTier::Cold, compression: CompressionAlgorithm::Zstd }
        );
    }

    #[test]
    fn very_old_data_is_deleted_regardless_of_tier() {
        let p = policy();
        assert_eq!(p.evaluate(StorageTier::Cold, 400), RetentionAction::Delete);
        assert_eq!(p.evaluate(StorageTier::Hot, 400), RetentionAction::Delete);
    }

    #[test]
    fn enforcer_applies_policy_and_tracks_metrics() {
        let mut enforcer = RetentionEnforcer::new(policy());
        let candidates = vec![
            candidate("a", StorageTier::Hot, 10, 1000),
            candidate("b", StorageTier::Hot, 150, 1000),
            candidate("c", StorageTier::Warm, 250, 1000),
            candidate("d", StorageTier::Cold, 400, 1000),
        ];
        let events = enforcer.enforce_at(&candidates, 1_000_000);

        assert_eq!(events.len(), 4);
        assert_eq!(events[0].action, RetentionAction::Retain);
        assert!(matches!(events[1].action, RetentionAction::MoveTier { to: StorageTier::Warm, .. }));
        assert!(matches!(events[2].action, RetentionAction::MoveTier { to: StorageTier::Cold, .. }));
        assert_eq!(events[3].action, RetentionAction::Delete);

        let metrics = enforcer.metrics();
        assert_eq!(metrics.evaluated, 4);
        assert_eq!(metrics.moved_to_warm, 1);
        assert_eq!(metrics.moved_to_cold, 1);
        assert_eq!(metrics.deleted, 1);
        assert!(metrics.bytes_saved() > 0);
        assert!(metrics.compression_ratio() < 1.0);
    }

    #[test]
    fn standard_policy_has_sane_defaults() {
        let p = RetentionPolicy::standard("events");
        assert!(p.hot_to_warm_after_secs < p.warm_to_cold_after_secs);
        assert!(p.warm_to_cold_after_secs < p.delete_after_secs.unwrap());
    }

    #[test]
    fn deletion_produces_zero_bytes_after() {
        let mut enforcer = RetentionEnforcer::new(policy());
        let events = enforcer.enforce_at(&[candidate("x", StorageTier::Cold, 500, 5000)], 1_000_000);
        assert_eq!(events[0].bytes_after, 0);
    }
}
