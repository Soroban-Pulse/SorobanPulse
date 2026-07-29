//! Integration tests for Issue #834: Multi-Cloud & Hybrid Deployment.
//!
//! All tests use mock publishers — no real cloud credentials are required.

use soroban_pulse::cloud_provider::{
    CloudEventPublisher, CloudProvider, CloudProviderConfig, CloudProviderRegistry,
    CloudPublishError, MockCloudPublisher,
};
use soroban_pulse::cloud_replication::{
    ConsistencyMode, ReplicationConfig, ReplicationManagerBuilder,
};
use soroban_pulse::deployment_orchestrator::{
    estimate_cost, DeploymentOrchestrator, DeploymentStatus, DeploymentTarget, ResourceUsage,
};
use soroban_pulse::models::SorobanEvent;

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
// ── Helper ───────────────────────────────────────────────────────────────────

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

// ═══════════════════════════════════════════════════════════════════════════
// CloudProvider enum tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cloud_provider_display_all_variants() {
    assert_eq!(CloudProvider::Aws.to_string(), "aws");
    assert_eq!(CloudProvider::Gcp.to_string(), "gcp");
    assert_eq!(CloudProvider::Azure.to_string(), "azure");
    assert_eq!(CloudProvider::OnPremise.to_string(), "on-premise");
}

#[test]
fn cloud_provider_serde_roundtrip() {
    for provider in [
        CloudProvider::Aws,
        CloudProvider::Gcp,
        CloudProvider::Azure,
        CloudProvider::OnPremise,
    ] {
        let json = serde_json::to_string(&provider).unwrap();
        let back: CloudProvider = serde_json::from_str(&json).unwrap();
        assert_eq!(back, provider, "roundtrip failed for {provider}");
    }
}

#[test]
fn cloud_provider_json_values() {
    assert_eq!(serde_json::to_string(&CloudProvider::Aws).unwrap(), r#""aws""#);
    assert_eq!(serde_json::to_string(&CloudProvider::Gcp).unwrap(), r#""gcp""#);
    assert_eq!(serde_json::to_string(&CloudProvider::Azure).unwrap(), r#""azure""#);
    assert_eq!(
        serde_json::to_string(&CloudProvider::OnPremise).unwrap(),
        r#""on_premise""#
    );
}

#[test]
fn cloud_provider_deserialize_from_string() {
    let p: CloudProvider = serde_json::from_str(r#""aws""#).unwrap();
    assert_eq!(p, CloudProvider::Aws);
}

// ═══════════════════════════════════════════════════════════════════════════
// CloudPublishError tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cloud_publish_error_display() {
    let err = CloudPublishError::Serialization("bad json".into());
    assert!(err.to_string().contains("bad json"));

    let err = CloudPublishError::ProviderError {
        provider: CloudProvider::Aws,
        message: "throttled".into(),
    };
    assert!(err.to_string().contains("aws"));
    assert!(err.to_string().contains("throttled"));

    let err = CloudPublishError::ProviderUnhealthy(CloudProvider::Gcp);
    assert!(err.to_string().contains("gcp"));

    let err = CloudPublishError::AllProvidersFailed(vec![
        (CloudProvider::Aws, "down".into()),
        (CloudProvider::Gcp, "timeout".into()),
    ]);
    let msg = err.to_string();
    assert!(msg.contains("aws"));
    assert!(msg.contains("gcp"));

    let err = CloudPublishError::Configuration("missing key".into());
    assert!(err.to_string().contains("missing key"));
}

// ═══════════════════════════════════════════════════════════════════════════
// MockCloudPublisher tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn mock_publisher_records_events() {
    let mock = MockCloudPublisher::new(CloudProvider::Aws);
    let event = make_event();
    mock.publish(&event).await.unwrap();
    mock.publish(&event).await.unwrap();
    assert_eq!(mock.published_count(), 2);
}

#[tokio::test]
async fn mock_publisher_returns_correct_provider() {
    let mock = MockCloudPublisher::new(CloudProvider::Azure);
    assert_eq!(mock.provider(), CloudProvider::Azure);
}

#[tokio::test]
async fn mock_publisher_fails_when_configured() {
    let mock = MockCloudPublisher::new(CloudProvider::Gcp);
    mock.set_fail("quota exceeded");
    let err = mock.publish(&make_event()).await.unwrap_err();
    assert!(err.to_string().contains("quota exceeded"));
}

#[tokio::test]
async fn mock_publisher_clear_fail_resumes() {
    let mock = MockCloudPublisher::new(CloudProvider::Aws);
    mock.set_fail("temporary");
    assert!(mock.publish(&make_event()).await.is_err());

    mock.clear_fail();
    assert!(mock.publish(&make_event()).await.is_ok());
    assert_eq!(mock.published_count(), 1);
}

#[tokio::test]
async fn mock_publisher_health_check() {
    let mock = MockCloudPublisher::new(CloudProvider::Aws);
    assert!(mock.health_check().await);

    mock.set_healthy(false);
    assert!(!mock.health_check().await);

    mock.set_healthy(true);
    assert!(mock.health_check().await);
}

// ═══════════════════════════════════════════════════════════════════════════
// CloudProviderRegistry tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn registry_publishes_to_primary() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    let registry = CloudProviderRegistry::new(vec![
        primary.clone() as Arc<dyn CloudEventPublisher>,
        secondary.clone() as Arc<dyn CloudEventPublisher>,
    ]);

    registry.publish(&make_event()).await.unwrap();

    assert_eq!(primary.published_count(), 1);
    assert_eq!(secondary.published_count(), 0);
}

#[tokio::test]
async fn registry_failover_to_secondary() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    primary.set_fail("aws outage");

    let registry = CloudProviderRegistry::new(vec![
        primary.clone() as Arc<dyn CloudEventPublisher>,
        secondary.clone() as Arc<dyn CloudEventPublisher>,
    ]);

    registry.publish(&make_event()).await.unwrap();

    assert_eq!(primary.published_count(), 0);
    assert_eq!(secondary.published_count(), 1);
}

#[tokio::test]
async fn registry_failover_to_tertiary() {
    let p1 = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let p2 = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    let p3 = Arc::new(MockCloudPublisher::new(CloudProvider::Azure));

    p1.set_fail("aws down");
    p2.set_fail("gcp down");

    let registry = CloudProviderRegistry::new(vec![
        p1.clone() as Arc<dyn CloudEventPublisher>,
        p2.clone() as Arc<dyn CloudEventPublisher>,
        p3.clone() as Arc<dyn CloudEventPublisher>,
    ]);

    registry.publish(&make_event()).await.unwrap();

    assert_eq!(p1.published_count(), 0);
    assert_eq!(p2.published_count(), 0);
    assert_eq!(p3.published_count(), 1);
}

#[tokio::test]
async fn registry_all_fail_returns_error() {
    let p1 = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let p2 = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    p1.set_fail("aws down");
    p2.set_fail("gcp down");

    let registry = CloudProviderRegistry::new(vec![
        p1 as Arc<dyn CloudEventPublisher>,
        p2 as Arc<dyn CloudEventPublisher>,
    ]);

    let err = registry.publish(&make_event()).await.unwrap_err();
    match err {
        CloudPublishError::AllProvidersFailed(failures) => {
            assert_eq!(failures.len(), 2);
        }
        other => panic!("expected AllProvidersFailed, got: {other}"),
    }
}

#[tokio::test]
async fn registry_marks_provider_unhealthy_after_max_failures() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    primary.set_fail("always fail");

    let registry = CloudProviderRegistry::new(vec![
        primary.clone() as Arc<dyn CloudEventPublisher>,
        secondary.clone() as Arc<dyn CloudEventPublisher>,
    ])
    .with_max_failures(2);

    // First failure: primary fails, secondary succeeds.
    registry.publish(&make_event()).await.unwrap();
    // Second failure: primary fails again, reaches threshold.
    registry.publish(&make_event()).await.unwrap();
    // Third call: primary is now unhealthy and skipped entirely.
    registry.publish(&make_event()).await.unwrap();

    // Secondary should have received all 3 events.
    assert_eq!(secondary.published_count(), 3);

    // Verify health snapshot.
    let snapshot = registry.health_snapshot().await;
    let aws_health = snapshot.get(&CloudProvider::Aws).unwrap();
    assert!(!aws_health.healthy);
}

#[tokio::test]
async fn registry_manual_recovery() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    primary.set_fail("fail");

    let registry = CloudProviderRegistry::new(vec![
        primary.clone() as Arc<dyn CloudEventPublisher>,
        secondary.clone() as Arc<dyn CloudEventPublisher>,
    ])
    .with_max_failures(1);

    // Trigger unhealthy marking.
    registry.publish(&make_event()).await.unwrap();

    let snapshot = registry.health_snapshot().await;
    assert!(!snapshot[&CloudProvider::Aws].healthy);

    // Manually recover and fix the publisher.
    primary.clear_fail();
    registry.mark_healthy(CloudProvider::Aws).await;

    let snapshot = registry.health_snapshot().await;
    assert!(snapshot[&CloudProvider::Aws].healthy);

    // Now primary should be tried again and succeed.
    registry.publish(&make_event()).await.unwrap();
    assert_eq!(primary.published_count(), 1);
}

#[tokio::test]
async fn registry_health_check_updates_state() {
    let p1 = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let p2 = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    p2.set_healthy(false);

    let registry = CloudProviderRegistry::new(vec![
        p1.clone() as Arc<dyn CloudEventPublisher>,
        p2.clone() as Arc<dyn CloudEventPublisher>,
    ])
    .with_max_failures(1);

    registry.check_health().await;

    let snapshot = registry.health_snapshot().await;
    assert!(snapshot[&CloudProvider::Aws].healthy);
    assert!(!snapshot[&CloudProvider::Gcp].healthy);
}

#[test]
fn registry_publisher_count() {
    let p1 = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let p2 = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    let registry = CloudProviderRegistry::new(vec![
        p1 as Arc<dyn CloudEventPublisher>,
        p2 as Arc<dyn CloudEventPublisher>,
    ]);

    assert_eq!(registry.publisher_count(), 2);
}

// ═══════════════════════════════════════════════════════════════════════════
// ConsistencyMode tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn consistency_mode_display() {
    assert_eq!(ConsistencyMode::Strong.to_string(), "strong");
    assert_eq!(ConsistencyMode::Eventual.to_string(), "eventual");
    assert_eq!(ConsistencyMode::BestEffort.to_string(), "best_effort");
}

#[test]
fn consistency_mode_serde_roundtrip() {
    for mode in [
        ConsistencyMode::Strong,
        ConsistencyMode::Eventual,
        ConsistencyMode::BestEffort,
    ] {
        let json = serde_json::to_string(&mode).unwrap();
        let back: ConsistencyMode = serde_json::from_str(&json).unwrap();
        assert_eq!(back, mode);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ReplicationManager tests
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn replication_strong_all_succeed() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .consistency_mode(ConsistencyMode::Strong)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Gcp, secondary.clone())
        .build();

    let result = manager.replicate(&make_event()).await.unwrap();
    assert!(result.all_ok());
    assert_eq!(primary.published_count(), 1);
    assert_eq!(secondary.published_count(), 1);
}

#[tokio::test]
async fn replication_strong_primary_fails() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    primary.set_fail("aws outage");

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .consistency_mode(ConsistencyMode::Strong)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Gcp, secondary.clone())
        .build();

    let err = manager.replicate(&make_event()).await.unwrap_err();
    assert!(err.to_string().contains("aws"));
    // Secondary should not have been written to since primary failed.
    assert_eq!(secondary.published_count(), 0);
}

#[tokio::test]
async fn replication_strong_secondary_fails() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    secondary.set_fail("gcp timeout");

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .consistency_mode(ConsistencyMode::Strong)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Gcp, secondary.clone())
        .build();

    let err = manager.replicate(&make_event()).await.unwrap_err();
    assert!(err.to_string().contains("gcp"));
    // Primary was written to before the secondary failure.
    assert_eq!(primary.published_count(), 1);
}

#[tokio::test]
async fn replication_eventual_secondary_failure_is_ok() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Azure));
    secondary.set_fail("azure unreachable");

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Azure)
        .consistency_mode(ConsistencyMode::Eventual)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Azure, secondary.clone())
        .build();

    let result = manager.replicate(&make_event()).await.unwrap();
    assert!(result.primary_ok);
    assert_eq!(result.secondary_results[&CloudProvider::Azure], false);
    assert_eq!(result.errors.len(), 1);
}

#[tokio::test]
async fn replication_eventual_primary_failure_is_err() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    primary.set_fail("aws down");

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .consistency_mode(ConsistencyMode::Eventual)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .build();

    let err = manager.replicate(&make_event()).await.unwrap_err();
    assert!(err.to_string().contains("aws"));
}

#[tokio::test]
async fn replication_best_effort_all_fail_returns_ok() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    primary.set_fail("aws fail");
    secondary.set_fail("gcp fail");

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .consistency_mode(ConsistencyMode::BestEffort)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Gcp, secondary.clone())
        .build();

    let result = manager.replicate(&make_event()).await.unwrap();
    assert!(!result.primary_ok);
    assert!(!result.all_ok());
    assert_eq!(result.errors.len(), 2);
}

#[tokio::test]
async fn replication_stats_tracking() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .consistency_mode(ConsistencyMode::Strong)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Gcp, secondary.clone())
        .build();

    for _ in 0..5 {
        manager.replicate(&make_event()).await.unwrap();
    }

    let stats = manager.stats().await;
    assert_eq!(stats[&CloudProvider::Aws], 5);
    assert_eq!(stats[&CloudProvider::Gcp], 5);
}

#[tokio::test]
async fn replication_manager_accessors() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let secondary = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .consistency_mode(ConsistencyMode::Strong)
        .register_publisher(CloudProvider::Aws, primary)
        .register_publisher(CloudProvider::Gcp, secondary)
        .build();

    assert_eq!(manager.primary_provider(), CloudProvider::Aws);
    assert_eq!(manager.secondary_providers(), &[CloudProvider::Gcp]);
    assert_eq!(manager.consistency_mode(), ConsistencyMode::Strong);
}

#[tokio::test]
async fn replication_multiple_secondaries() {
    let primary = Arc::new(MockCloudPublisher::new(CloudProvider::Aws));
    let sec1 = Arc::new(MockCloudPublisher::new(CloudProvider::Gcp));
    let sec2 = Arc::new(MockCloudPublisher::new(CloudProvider::Azure));

    let manager = ReplicationManagerBuilder::new(CloudProvider::Aws)
        .add_secondary(CloudProvider::Gcp)
        .add_secondary(CloudProvider::Azure)
        .consistency_mode(ConsistencyMode::Strong)
        .register_publisher(CloudProvider::Aws, primary.clone())
        .register_publisher(CloudProvider::Gcp, sec1.clone())
        .register_publisher(CloudProvider::Azure, sec2.clone())
        .build();

    manager.replicate(&make_event()).await.unwrap();

    assert_eq!(primary.published_count(), 1);
    assert_eq!(sec1.published_count(), 1);
    assert_eq!(sec2.published_count(), 1);
}

// ═══════════════════════════════════════════════════════════════════════════
// DeploymentOrchestrator tests
// ═══════════════════════════════════════════════════════════════════════════

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
fn orchestrator_full_lifecycle() {
    let mut orch = DeploymentOrchestrator::new();
    orch.add_target(sample_target("prod-east", CloudProvider::Aws));

    assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Pending));
    orch.start_provisioning("prod-east").unwrap();
    assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Provisioning));
    orch.activate("prod-east").unwrap();
    assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Active));
    orch.drain("prod-east").unwrap();
    assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Draining));
    orch.terminate("prod-east").unwrap();
    assert_eq!(orch.status("prod-east"), Some(&DeploymentStatus::Terminated));
}

#[test]
fn orchestrator_invalid_transition() {
    let mut orch = DeploymentOrchestrator::new();
    orch.add_target(sample_target("t1", CloudProvider::Gcp));

    let err = orch.activate("t1").unwrap_err();
    assert!(err.contains("cannot transition"));
}

#[test]
fn orchestrator_unknown_target() {
    let mut orch = DeploymentOrchestrator::new();
    let err = orch.start_provisioning("nonexistent").unwrap_err();
    assert!(err.contains("unknown target"));
}

#[test]
fn orchestrator_mark_failed() {
    let mut orch = DeploymentOrchestrator::new();
    orch.add_target(sample_target("t1", CloudProvider::Azure));
    orch.start_provisioning("t1").unwrap();
    orch.mark_failed("t1", "out of quota").unwrap();

    match orch.status("t1") {
        Some(DeploymentStatus::Failed(msg)) => assert_eq!(msg, "out of quota"),
        other => panic!("expected Failed, got: {other:?}"),
    }
}

#[test]
fn orchestrator_active_count() {
    let mut orch = DeploymentOrchestrator::new();
    orch.add_target(sample_target("t1", CloudProvider::Aws));
    orch.add_target(sample_target("t2", CloudProvider::Gcp));
    orch.add_target(sample_target("t3", CloudProvider::Azure));

    assert_eq!(orch.active_count(), 0);

    orch.start_provisioning("t1").unwrap();
    orch.activate("t1").unwrap();

    orch.start_provisioning("t2").unwrap();
    orch.activate("t2").unwrap();

    assert_eq!(orch.active_count(), 2);
    assert_eq!(orch.target_count(), 3);
}

#[test]
fn orchestrator_resource_tracking() {
    let mut orch = DeploymentOrchestrator::new();
    orch.add_target(sample_target("t1", CloudProvider::Aws));

    orch.update_resources(
        "t1",
        ResourceUsage {
            cpu_percent: 45.0,
            memory_percent: 60.0,
            events_processed: 10_000,
            resource_ids: vec!["arn:aws:kinesis:us-east-1:123:stream/events".into()],
        },
    )
    .unwrap();

    let usage = orch.resource_usage("t1").unwrap();
    assert!((usage.cpu_percent - 45.0).abs() < f64::EPSILON);
    assert_eq!(usage.events_processed, 10_000);
    assert_eq!(usage.resource_ids.len(), 1);
}

#[test]
fn orchestrator_resource_unknown_target() {
    let mut orch = DeploymentOrchestrator::new();
    let err = orch
        .update_resources("nope", ResourceUsage::default())
        .unwrap_err();
    assert!(err.contains("unknown target"));
}

#[test]
fn orchestrator_cost_estimates() {
    let mut orch = DeploymentOrchestrator::new();
    orch.add_target(sample_target("t1", CloudProvider::Aws));
    orch.add_target(sample_target("t2", CloudProvider::Gcp));

    let estimates = orch.cost_estimates(100_000);
    assert_eq!(estimates.len(), 2);
    for est in &estimates {
        assert!(est.hourly_cost_usd > 0.0);
        assert!(est.monthly_estimate_usd > est.hourly_cost_usd);
    }
}

#[test]
fn deployment_status_display() {
    assert_eq!(DeploymentStatus::Pending.to_string(), "pending");
    assert_eq!(DeploymentStatus::Provisioning.to_string(), "provisioning");
    assert_eq!(DeploymentStatus::Active.to_string(), "active");
    assert_eq!(DeploymentStatus::Draining.to_string(), "draining");
    assert_eq!(DeploymentStatus::Terminated.to_string(), "terminated");
    assert_eq!(
        DeploymentStatus::Failed("oops".into()).to_string(),
        "failed: oops",
    );
}

#[test]
fn deployment_status_serde_roundtrip() {
    for status in [
        DeploymentStatus::Pending,
        DeploymentStatus::Active,
        DeploymentStatus::Terminated,
        DeploymentStatus::Failed("err".into()),
    ] {
        let json = serde_json::to_string(&status).unwrap();
        let back: DeploymentStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, status);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Cost estimation tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cost_estimate_aws() {
    let est = estimate_cost(CloudProvider::Aws, "us-east-1", 10_000);
    assert_eq!(est.provider, CloudProvider::Aws);
    assert_eq!(est.region, "us-east-1");
    assert!(est.hourly_cost_usd > 0.0);
    assert!(est.per_event_cost_usd > 0.0);
    assert!(est.monthly_estimate_usd > 0.0);
}

#[test]
fn cost_estimate_on_premise_cheapest() {
    let aws = estimate_cost(CloudProvider::Aws, "us-east-1", 100_000);
    let onprem = estimate_cost(CloudProvider::OnPremise, "local", 100_000);
    assert!(
        onprem.hourly_cost_usd < aws.hourly_cost_usd,
        "on-premise should be cheaper than AWS"
    );
}

#[test]
fn cost_estimate_monthly_is_hourly_times_720() {
    let est = estimate_cost(CloudProvider::Gcp, "us-central1", 50_000);
    let expected_monthly = est.hourly_cost_usd * 24.0 * 30.0;
    assert!(
        (est.monthly_estimate_usd - expected_monthly).abs() < 0.001,
        "monthly should be hourly * 720"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// CloudProviderConfig tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn cloud_provider_config_serde() {
    let config = CloudProviderConfig {
        provider: CloudProvider::Aws,
        region: "us-west-2".into(),
        settings: HashMap::from([
            ("stream_name".into(), "events".into()),
            ("batch_size".into(), "100".into()),
        ]),
    };

    let json = serde_json::to_string(&config).unwrap();
    let back: CloudProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.provider, CloudProvider::Aws);
    assert_eq!(back.region, "us-west-2");
    assert_eq!(back.settings.len(), 2);
}

// ═══════════════════════════════════════════════════════════════════════════
// ReplicationConfig tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn replication_config_serde() {
    let config = ReplicationConfig {
        primary: CloudProvider::Aws,
        secondaries: vec![CloudProvider::Gcp, CloudProvider::Azure],
        consistency_mode: ConsistencyMode::Eventual,
        failover_enabled: true,
        max_retries: 5,
    };

    let json = serde_json::to_string(&config).unwrap();
    let back: ReplicationConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.primary, CloudProvider::Aws);
    assert_eq!(back.secondaries.len(), 2);
    assert_eq!(back.consistency_mode, ConsistencyMode::Eventual);
    assert!(back.failover_enabled);
    assert_eq!(back.max_retries, 5);
}
