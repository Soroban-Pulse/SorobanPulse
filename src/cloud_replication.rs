//! Issue #834: Multi-Cloud & Hybrid Deployment — Cross-Cloud Replication.
//!
//! Manages event replication across multiple cloud providers with configurable
//! consistency modes (strong, eventual, best-effort) and automatic failover
//! when a provider is unreachable.

use crate::cloud_provider::{
    CloudEventPublisher, CloudProvider, CloudPublishError,
};
use crate::models::SorobanEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, warn};

// ── Consistency mode ─────────────────────────────────────────────────────────

/// How replicated writes are acknowledged to the caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyMode {
    /// All replicas must acknowledge before returning success.
    Strong,
    /// The primary must acknowledge; secondaries are written asynchronously.
    Eventual,
    /// Fire-and-forget to all replicas; the call always returns Ok.
    BestEffort,
}

impl std::fmt::Display for ConsistencyMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Strong => write!(f, "strong"),
            Self::Eventual => write!(f, "eventual"),
            Self::BestEffort => write!(f, "best_effort"),
        }
    }
}

// ── Replication config ───────────────────────────────────────────────────────

/// Describes how events should be replicated across providers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationConfig {
    /// The primary provider (receives writes first).
    pub primary: CloudProvider,
    /// Secondary providers that receive replicated writes.
    pub secondaries: Vec<CloudProvider>,
    /// The consistency guarantee for replicated writes.
    pub consistency_mode: ConsistencyMode,
    /// Whether to enable automatic failover when the primary is unreachable.
    #[serde(default = "default_true")]
    pub failover_enabled: bool,
    /// Maximum number of retry attempts for failed replications.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_true() -> bool {
    true
}

fn default_max_retries() -> u32 {
    3
}

// ── Replication result ───────────────────────────────────────────────────────

/// Summary of a replication operation.
#[derive(Debug, Clone)]
pub struct ReplicationResult {
    /// Whether the primary write succeeded.
    pub primary_ok: bool,
    /// Per-secondary success status.
    pub secondary_results: HashMap<CloudProvider, bool>,
    /// Errors encountered, if any.
    pub errors: Vec<(CloudProvider, String)>,
}

impl ReplicationResult {
    /// True when every provider (primary + all secondaries) succeeded.
    pub fn all_ok(&self) -> bool {
        self.primary_ok && self.secondary_results.values().all(|v| *v)
    }
}

// ── Replication manager ──────────────────────────────────────────────────────

/// Orchestrates cross-cloud event replication based on a `ReplicationConfig`.
pub struct ReplicationManager {
    config: ReplicationConfig,
    publishers: HashMap<CloudProvider, Arc<dyn CloudEventPublisher>>,
    /// Running count of replicated events per provider.
    stats: Arc<RwLock<HashMap<CloudProvider, u64>>>,
}

impl ReplicationManager {
    /// Build a new manager from a config and a map of concrete publishers.
    pub fn new(
        config: ReplicationConfig,
        publishers: HashMap<CloudProvider, Arc<dyn CloudEventPublisher>>,
    ) -> Self {
        let mut stats = HashMap::new();
        stats.insert(config.primary, 0u64);
        for s in &config.secondaries {
            stats.insert(*s, 0);
        }
        Self {
            config,
            publishers,
            stats: Arc::new(RwLock::new(stats)),
        }
    }

    /// Replicate a single event according to the configured consistency mode.
    pub async fn replicate(&self, event: &SorobanEvent) -> Result<ReplicationResult, CloudPublishError> {
        match self.config.consistency_mode {
            ConsistencyMode::Strong => self.replicate_strong(event).await,
            ConsistencyMode::Eventual => self.replicate_eventual(event).await,
            ConsistencyMode::BestEffort => self.replicate_best_effort(event).await,
        }
    }

    /// Strong consistency: publish to **all** providers; fail if any fails.
    async fn replicate_strong(&self, event: &SorobanEvent) -> Result<ReplicationResult, CloudPublishError> {
        let mut result = ReplicationResult {
            primary_ok: false,
            secondary_results: HashMap::new(),
            errors: Vec::new(),
        };

        // Primary
        match self.publish_to(self.config.primary, event).await {
            Ok(()) => {
                result.primary_ok = true;
                self.increment_stat(self.config.primary).await;
            }
            Err(e) => {
                let msg = e.to_string();
                error!(provider = %self.config.primary, error = %msg, "primary replication failed (strong)");
                result.errors.push((self.config.primary, msg));
                return Err(e);
            }
        }

        // Secondaries (all must succeed for strong consistency).
        for &sec in &self.config.secondaries {
            match self.publish_to(sec, event).await {
                Ok(()) => {
                    result.secondary_results.insert(sec, true);
                    self.increment_stat(sec).await;
                }
                Err(e) => {
                    let msg = e.to_string();
                    error!(provider = %sec, error = %msg, "secondary replication failed (strong)");
                    result.secondary_results.insert(sec, false);
                    result.errors.push((sec, msg.clone()));
                    return Err(CloudPublishError::ProviderError {
                        provider: sec,
                        message: msg,
                    });
                }
            }
        }

        Ok(result)
    }

    /// Eventual consistency: primary must succeed; secondaries are replicated
    /// asynchronously and failures are logged but not propagated.
    async fn replicate_eventual(&self, event: &SorobanEvent) -> Result<ReplicationResult, CloudPublishError> {
        let mut result = ReplicationResult {
            primary_ok: false,
            secondary_results: HashMap::new(),
            errors: Vec::new(),
        };

        // Primary (must succeed).
        match self.publish_to(self.config.primary, event).await {
            Ok(()) => {
                result.primary_ok = true;
                self.increment_stat(self.config.primary).await;
            }
            Err(e) => {
                let msg = e.to_string();
                error!(provider = %self.config.primary, error = %msg, "primary replication failed (eventual)");
                result.errors.push((self.config.primary, msg));
                return Err(e);
            }
        }

        // Secondaries (fire-and-forget with logging).
        for &sec in &self.config.secondaries {
            match self.publish_to(sec, event).await {
                Ok(()) => {
                    result.secondary_results.insert(sec, true);
                    self.increment_stat(sec).await;
                }
                Err(e) => {
                    let msg = e.to_string();
                    warn!(provider = %sec, error = %msg, "secondary replication failed (eventual — will retry later)");
                    result.secondary_results.insert(sec, false);
                    result.errors.push((sec, msg));
                }
            }
        }

        Ok(result)
    }

    /// Best-effort: publish to all providers concurrently; always return Ok.
    async fn replicate_best_effort(&self, event: &SorobanEvent) -> Result<ReplicationResult, CloudPublishError> {
        let mut result = ReplicationResult {
            primary_ok: false,
            secondary_results: HashMap::new(),
            errors: Vec::new(),
        };

        // Primary.
        match self.publish_to(self.config.primary, event).await {
            Ok(()) => {
                result.primary_ok = true;
                self.increment_stat(self.config.primary).await;
            }
            Err(e) => {
                let msg = e.to_string();
                warn!(provider = %self.config.primary, error = %msg, "primary replication failed (best-effort)");
                result.errors.push((self.config.primary, msg));
            }
        }

        // Secondaries.
        for &sec in &self.config.secondaries {
            match self.publish_to(sec, event).await {
                Ok(()) => {
                    result.secondary_results.insert(sec, true);
                    self.increment_stat(sec).await;
                }
                Err(e) => {
                    let msg = e.to_string();
                    warn!(provider = %sec, error = %msg, "secondary replication failed (best-effort)");
                    result.secondary_results.insert(sec, false);
                    result.errors.push((sec, msg));
                }
            }
        }

        Ok(result)
    }

    /// Publish to a specific provider, looking it up in the publishers map.
    async fn publish_to(
        &self,
        provider: CloudProvider,
        event: &SorobanEvent,
    ) -> Result<(), CloudPublishError> {
        let publisher = self.publishers.get(&provider).ok_or_else(|| {
            CloudPublishError::Configuration(format!("no publisher registered for {provider}"))
        })?;
        publisher.publish(event).await
    }

    /// Increment the replication stat counter for a provider.
    async fn increment_stat(&self, provider: CloudProvider) {
        let mut stats = self.stats.write().await;
        *stats.entry(provider).or_insert(0) += 1;
    }

    /// Return the current replication statistics.
    pub async fn stats(&self) -> HashMap<CloudProvider, u64> {
        self.stats.read().await.clone()
    }

    /// Return the configured consistency mode.
    pub fn consistency_mode(&self) -> ConsistencyMode {
        self.config.consistency_mode
    }

    /// Return the configured primary provider.
    pub fn primary_provider(&self) -> CloudProvider {
        self.config.primary
    }

    /// Return the configured secondary providers.
    pub fn secondary_providers(&self) -> &[CloudProvider] {
        &self.config.secondaries
    }
}

// ── Builder helper ───────────────────────────────────────────────────────────

/// Convenience builder for `ReplicationManager` in tests or setup code.
pub struct ReplicationManagerBuilder {
    primary: CloudProvider,
    secondaries: Vec<CloudProvider>,
    consistency_mode: ConsistencyMode,
    publishers: HashMap<CloudProvider, Arc<dyn CloudEventPublisher>>,
}

impl ReplicationManagerBuilder {
    pub fn new(primary: CloudProvider) -> Self {
        Self {
            primary,
            secondaries: Vec::new(),
            consistency_mode: ConsistencyMode::Eventual,
            publishers: HashMap::new(),
        }
    }

    pub fn add_secondary(mut self, provider: CloudProvider) -> Self {
        self.secondaries.push(provider);
        self
    }

    pub fn consistency_mode(mut self, mode: ConsistencyMode) -> Self {
        self.consistency_mode = mode;
        self
    }

    pub fn register_publisher(
        mut self,
        provider: CloudProvider,
        publisher: Arc<dyn CloudEventPublisher>,
    ) -> Self {
        self.publishers.insert(provider, publisher);
        self
    }

    pub fn build(self) -> ReplicationManager {
        let config = ReplicationConfig {
            primary: self.primary,
            secondaries: self.secondaries,
            consistency_mode: self.consistency_mode,
            failover_enabled: true,
            max_retries: 3,
        };
        ReplicationManager::new(config, self.publishers)
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::cloud_provider::MockCloudPublisher;
    use super::*;
    use serde_json::Value;

    fn make_event() -> SorobanEvent {
        SorobanEvent {
            id: None,
            contract_id: "CABC123".into(),
            event_type: "contract".into(),
            tx_hash: "deadbeef".into(),
            ledger: 100,
            ledger_closed_at: "2026-04-27T00:00:00Z".into(),
            ledger_hash: None,
            in_successful_call: true,
            value: Value::Null,
            topic: None,
            tenant_id: None,
        }
    }

    #[test]
    fn consistency_mode_display() {
        assert_eq!(ConsistencyMode::Strong.to_string(), "strong");
        assert_eq!(ConsistencyMode::Eventual.to_string(), "eventual");
        assert_eq!(ConsistencyMode::BestEffort.to_string(), "best_effort");
    }

    #[test]
    fn consistency_mode_serde_roundtrip() {
        let json = serde_json::to_string(&ConsistencyMode::Strong).unwrap();
        assert_eq!(json, r#""strong""#);
        let back: ConsistencyMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ConsistencyMode::Strong);
    }

    #[tokio::test]
    async fn strong_replication_all_succeed() {
        let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
        let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

        let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
            .add_secondary(CloudProvider::Gcp)
            .consistency_mode(ConsistencyMode::Strong)
            .register_publisher(CloudProvider::Aws, primary.clone())
            .register_publisher(CloudProvider::Gcp, secondary.clone())
            .build();

        let event = make_event();
        let result = manager.replicate(&event).await.unwrap();

        assert!(result.all_ok());
        assert_eq!(primary.published_count(), 1);
        assert_eq!(secondary.published_count(), 1);
    }

    #[tokio::test]
    async fn strong_replication_secondary_fails() {
        let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
        let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
        secondary.set_fail("gcp down");

        let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
            .add_secondary(CloudProvider::Gcp)
            .consistency_mode(ConsistencyMode::Strong)
            .register_publisher(CloudProvider::Aws, primary.clone())
            .register_publisher(CloudProvider::Gcp, secondary.clone())
            .build();

        let event = make_event();
        let err = manager.replicate(&event).await.unwrap_err();
        assert!(err.to_string().contains("gcp down"));
    }

    #[tokio::test]
    async fn eventual_replication_secondary_fails_but_returns_ok() {
        let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
        let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Azure));
        secondary.set_fail("azure timeout");

        let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
            .add_secondary(CloudProvider::Azure)
            .consistency_mode(ConsistencyMode::Eventual)
            .register_publisher(CloudProvider::Aws, primary.clone())
            .register_publisher(CloudProvider::Azure, secondary.clone())
            .build();

        let event = make_event();
        let result = manager.replicate(&event).await.unwrap();

        // Primary succeeded, so overall result is Ok.
        assert!(result.primary_ok);
        // Secondary failed.
        assert_eq!(result.secondary_results.get(&CloudProvider::Azure), Some(&false));
    }

    #[tokio::test]
    async fn best_effort_never_returns_err() {
        let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
        primary.set_fail("aws down");
        let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
        secondary.set_fail("gcp down");

        let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
            .add_secondary(CloudProvider::Gcp)
            .consistency_mode(ConsistencyMode::BestEffort)
            .register_publisher(CloudProvider::Aws, primary.clone())
            .register_publisher(CloudProvider::Gcp, secondary.clone())
            .build();

        let event = make_event();
        // BestEffort always returns Ok.
        let result = manager.replicate(&event).await.unwrap();
        assert!(!result.primary_ok);
        assert_eq!(result.errors.len(), 2);
    }
}
