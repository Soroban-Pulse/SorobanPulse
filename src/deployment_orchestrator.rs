//! Issue #834: Multi-Cloud & Hybrid Deployment — Deployment Orchestration.
//!
//! Manages deployment lifecycle across cloud providers: planning, provisioning,
//! health verification, and teardown.  Includes a simple cost-estimation model
//! and status tracking for each deployment target.

use crate::cloud_provider::{CloudProvider, CloudProviderConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tracing::{info, warn};

// ── Deployment target ────────────────────────────────────────────────────────

/// A single deployment target (e.g. "us-east-1 on AWS").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentTarget {
    /// Human-readable name for this target (e.g. "prod-east").
    pub name: String,
    /// Which cloud provider hosts this target.
    pub provider: CloudProvider,
    /// Cloud region (e.g. "us-east-1", "europe-west1", "eastus").
    pub region: String,
    /// Provider-specific configuration overrides.
    pub config: CloudProviderConfig,
}

// ── Deployment status ────────────────────────────────────────────────────────

/// Tracks the state of a deployment on a specific target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentStatus {
    /// Not yet started.
    Pending,
    /// Resources are being provisioned.
    Provisioning,
    /// Target is live and healthy.
    Active,
    /// Target is being drained before teardown.
    Draining,
    /// Target has been torn down.
    Terminated,
    /// Deployment encountered an unrecoverable error.
    Failed(String),
}

impl fmt::Display for DeploymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Provisioning => write!(f, "provisioning"),
            Self::Active => write!(f, "active"),
            Self::Draining => write!(f, "draining"),
            Self::Terminated => write!(f, "terminated"),
            Self::Failed(msg) => write!(f, "failed: {msg}"),
        }
    }
}

// ── Cost estimation ──────────────────────────────────────────────────────────

/// Very simple cost model: each provider has a flat per-hour base cost and a
/// per-event cost.  This is intentionally simplistic — a real implementation
/// would query provider pricing APIs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostEstimate {
    /// Provider being estimated.
    pub provider: CloudProvider,
    /// Region.
    pub region: String,
    /// Estimated hourly cost (USD).
    pub hourly_cost_usd: f64,
    /// Estimated per-event cost (USD).
    pub per_event_cost_usd: f64,
    /// Monthly estimate assuming 30 days and the given events/hour.
    pub monthly_estimate_usd: f64,
}

/// Return a rough cost estimate for a provider + region + throughput.
pub fn estimate_cost(
    provider: CloudProvider,
    region: &str,
    events_per_hour: u64,
) -> CostEstimate {
    // Flat hourly base costs (illustrative).
    let (hourly_base, per_event) = match provider {
        CloudProvider::Aws => (0.50, 0.000_004),       // Kinesis-like
        CloudProvider::Gcp => (0.40, 0.000_005),       // Pub/Sub-like
        CloudProvider::Azure => (0.45, 0.000_004_5),   // Event Hubs-like
        CloudProvider::OnPremise => (0.10, 0.000_001), // Kafka-like
    };

    let hourly_cost = hourly_base + per_event * events_per_hour as f64;
    let monthly = hourly_cost * 24.0 * 30.0;

    CostEstimate {
        provider,
        region: region.to_string(),
        hourly_cost_usd: hourly_cost,
        per_event_cost_usd: per_event,
        monthly_estimate_usd: monthly,
    }
}

// ── Resource info ────────────────────────────────────────────────────────────

/// Captures resource usage for a deployment target.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// Approximate CPU utilisation percentage (0–100).
    pub cpu_percent: f64,
    /// Approximate memory utilisation percentage (0–100).
    pub memory_percent: f64,
    /// Number of events processed in the current reporting window.
    pub events_processed: u64,
    /// Provider-specific resource identifiers.
    pub resource_ids: Vec<String>,
}

// ── Deployment orchestrator ──────────────────────────────────────────────────

/// Manages deployments across multiple cloud targets, tracking status and
/// providing lifecycle operations (provision, activate, drain, terminate).
pub struct DeploymentOrchestrator {
    /// All known deployment targets, keyed by target name.
    targets: HashMap<String, DeploymentTarget>,
    /// Current status of each target.
    statuses: HashMap<String, DeploymentStatus>,
    /// Resource usage snapshots.
    resources: HashMap<String, ResourceUsage>,
}

impl DeploymentOrchestrator {
    /// Create an empty orchestrator.
    pub fn new() -> Self {
        Self {
            targets: HashMap::new(),
            statuses: HashMap::new(),
            resources: HashMap::new(),
        }
    }

    /// Register a deployment target.  Starts in `Pending` state.
    pub fn add_target(&mut self, target: DeploymentTarget) {
        let name = target.name.clone();
        info!(target = %name, provider = %target.provider, region = %target.region, "deployment target added");
        self.statuses.insert(name.clone(), DeploymentStatus::Pending);
        self.resources.insert(name.clone(), ResourceUsage::default());
        self.targets.insert(name, target);
    }

    /// Transition a target from `Pending` to `Provisioning`.
    pub fn start_provisioning(&mut self, name: &str) -> Result<(), String> {
        self.transition(name, &[DeploymentStatus::Pending], DeploymentStatus::Provisioning)
    }

    /// Transition a target from `Provisioning` to `Active`.
    pub fn activate(&mut self, name: &str) -> Result<(), String> {
        self.transition(name, &[DeploymentStatus::Provisioning], DeploymentStatus::Active)
    }

    /// Transition a target from `Active` to `Draining`.
    pub fn drain(&mut self, name: &str) -> Result<(), String> {
        self.transition(name, &[DeploymentStatus::Active], DeploymentStatus::Draining)
    }

    /// Transition a target from `Draining` to `Terminated`.
    pub fn terminate(&mut self, name: &str) -> Result<(), String> {
        self.transition(
            name,
            &[DeploymentStatus::Draining, DeploymentStatus::Pending],
            DeploymentStatus::Terminated,
        )
    }

    /// Mark a target as failed from any state.
    pub fn mark_failed(&mut self, name: &str, reason: &str) -> Result<(), String> {
        if !self.statuses.contains_key(name) {
            return Err(format!("unknown target: {name}"));
        }
        warn!(target = %name, reason = %reason, "deployment target marked failed");
        self.statuses.insert(name.to_string(), DeploymentStatus::Failed(reason.to_string()));
        Ok(())
    }

    /// Return the current status for a target.
    pub fn status(&self, name: &str) -> Option<&DeploymentStatus> {
        self.statuses.get(name)
    }

    /// Return all targets and their statuses.
    pub fn all_statuses(&self) -> Vec<(&str, &DeploymentStatus)> {
        self.statuses.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Return the number of active targets.
    pub fn active_count(&self) -> usize {
        self.statuses.values().filter(|s| **s == DeploymentStatus::Active).count()
    }

    /// Return the total number of registered targets.
    pub fn target_count(&self) -> usize {
        self.targets.len()
    }

    /// Update the resource usage snapshot for a target.
    pub fn update_resources(&mut self, name: &str, usage: ResourceUsage) -> Result<(), String> {
        if !self.targets.contains_key(name) {
            return Err(format!("unknown target: {name}"));
        }
        self.resources.insert(name.to_string(), usage);
        Ok(())
    }

    /// Return the resource usage for a target.
    pub fn resource_usage(&self, name: &str) -> Option<&ResourceUsage> {
        self.resources.get(name)
    }

    /// Return cost estimates for all targets given an expected throughput.
    pub fn cost_estimates(&self, events_per_hour: u64) -> Vec<CostEstimate> {
        self.targets
            .values()
            .map(|t| estimate_cost(t.provider, &t.region, events_per_hour))
            .collect()
    }

    /// Internal: validate and apply a state transition.
    fn transition(
        &mut self,
        name: &str,
        valid_from: &[DeploymentStatus],
        to: DeploymentStatus,
    ) -> Result<(), String> {
        let current = self.statuses.get(name).ok_or_else(|| format!("unknown target: {name}"))?;

        // For the `Failed` variant, match on discriminant only.
        let allowed = valid_from.iter().any(|from| {
            std::mem::discriminant(current) == std::mem::discriminant(from)
        });
        if !allowed {
            return Err(format!(
                "cannot transition {name} from {current} to {to}",
            ));
        }

        info!(target = %name, from = %current, to = %to, "deployment state transition");
        self.statuses.insert(name.to_string(), to);
        Ok(())
    }
}

impl Default for DeploymentOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// ── Unit tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target(name: &str, provider: CloudProvider) -> DeploymentTarget {
        DeploymentTarget {
            name: name.into(),
            provider,
            region: "us-east-1".into(),
            config: CloudProviderConfig {
                provider,
                region: "us-east-1".into(),
                settings: HashMap::new(),
            },
        }
    }

    #[test]
    fn deployment_lifecycle() {
        let mut orch = DeploymentOrchestrator::new();
        orch.add_target(sample_target("prod-east", CloudProvider::Aws));

        assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Pending));

        orch.start_provisioning("prod-east").unwrap();
        assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Provisioning));

        orch.activate("prod-east").unwrap();
        assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Active));
        assert_eq!(orch.active_count(), 1);

        orch.drain("prod-east").unwrap();
        assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Draining));

        orch.terminate("prod-east").unwrap();
        assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Terminated));
    }

    #[test]
    fn invalid_transition_rejected() {
        let mut orch = DeploymentOrchestrator::new();
        orch.add_target(sample_target("t1", CloudProvider::Gcp));

        // Cannot activate from Pending (must provision first).
        let err = orch.activate("t1").unwrap_err();
        assert!(err.contains("cannot transition"));
    }

    #[test]
    fn mark_failed_from_any_state() {
        let mut orch = DeploymentOrchestrator::new();
        orch.add_target(sample_target("t1", CloudProvider::Azure));
        orch.start_provisioning("t1").unwrap();
        orch.mark_failed("t1", "out of quota").unwrap();
        assert!(matches!(orch.status("t1"), Some(DeploymentStatus::Failed(_))));
    }

    #[test]
    fn cost_estimate_sanity() {
        let est = estimate_cost(CloudProvider::Aws, "us-east-1", 10_000);
        assert!(est.hourly_cost_usd > 0.0);
        assert!(est.monthly_estimate_usd > est.hourly_cost_usd);
    }

    #[test]
    fn deployment_status_display() {
        assert_eq!(DeploymentStatus::Active.to_string(), "active");
        assert_eq!(
            DeploymentStatus::Failed("boom".into()).to_string(),
            "failed: boom",
        );
    }
}
