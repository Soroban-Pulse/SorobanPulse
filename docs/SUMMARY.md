# Summary

[Introduction](../README.md)

---

# Getting Started

- [Quickstart Tutorial](tutorial-quickstart.md)
- [Token Alert Bot Tutorial](tutorial-token-alert-bot.md)
- [Architecture Overview](architecture.md)
- [Environment Variable Reference](configuration.md)
- [Deployment Guide](deployment.md)
- [Deployment Platforms](deployment-platforms.md)

---

# API Reference

- [Generated API Reference](api-reference.md)
- [API Usage](api-usage.md)
- [API Versioning](api-versioning.md)
  - [Migrating unversioned routes to /v1/](migration-unversioned-to-v1.md)
- [API SLA](api-sla.md)
- [GraphQL API](graphql_api.md)
- [OpenAPI Spec](codegen.md)
- [Postman Collection](postman.md)
- [SDK Integration Guide](sdk-integration-guide.md)

---

# Notifications

- [Notification Channels](notification-channels.md)
- [Notification Features](notification-features.md)
- [Webhook Signing](webhook_signing.md)
- [Webhook Verification](webhook-verification.md)
- [Webhook Endpoint Rate Limits](webhook-endpoint-rate-limits.md)
- [Email Notifications](email-notifications.md)
- [Email Quick Start](email-quick-start.md)
- [Notification Rate Limiting](notification-rate-limiting.md)
- [Notification Content Filter](notification-content-filter.md)
- [Notification Deduplication](notification-deduplication.md)
- [Notification Delivery Receipts](notification-delivery-receipts.md)

---

# Event Streaming

- [Event Feeds](event-feeds.md)
- [SSE Reconnection](sse-reconnection.md)
- [SSE Reverse Proxy Configuration](sse_reverse_proxy_configuration.md)
- [Subscription Best Practices](subscription-best-practices.md)
- [Contract Event Schemas](contract-event-schemas.md)
- [Contract Event Simulation](contract-event-simulation.md)
- [Event Deduplication](event-deduplication.md)
- [Temporal Queries](temporal-queries.md)

---

# Integrations

- [AWS SQS Integration](sqs-integration.md)
- [Azure Event Hubs Integration](event-hubs-integration.md)
- [Lua Event Transformation](lua-transformation.md)
- [Streaming Responses](STREAMING_RESPONSES.md)

---

# Database & Schema

- [Database Schema](schema.md)
- [Data Retention Policy](data-retention.md)
- [Encryption](encryption.md)

---

# Security

- [Threat Model & Security Architecture](security/threat-model.md)

---

# Observability

- [Logging](logging.md)
- [Log Aggregation](log-aggregation.md)
- [Rate Limiting](rate_limiting.md)
- [Performance Tuning](performance-tuning.md)
- [Performance Regression Testing](performance-regression-testing.md)
- [Capacity Planning](capacity-planning.md)
- [Replica Monitoring](replica-monitoring.md)
- [Statistics Auto Analysis](statistics-auto-analysis.md)
- [Kubernetes Probes](kubernetes-probes.md)
- [OWASP Security Headers](owasp_security_headers.md)

---

# Testing & Quality

- [API Compatibility Testing](api-compatibility-testing.md)
- [Contract Testing](contract-testing.md)
- [Property-Based Testing](property-based-testing.md)
- [Mutation Testing](mutation-testing.md)
- [Backup Verification](backup-verification.md)

---

# Multi-Tenancy & Advanced

- [Multi-Tenancy](multi-tenancy.md)
- [Multi-Deployment Architecture](multi-deployment-architecture.md)
- [SaaS Platform (Experimental)](SAAS_PLATFORM.md)
- [ML Integration (Experimental)](ML_INTEGRATION.md)
- [Feature Flag Rollback](feature-flag-rollback.md)

---

# Infrastructure

- [Terraform](terraform.md)
- [CI Workflows Overview](development/ci.md)

---

# Architecture Decisions

- [ADR Index](adr/README.md)
- [ADR-0001: Web Framework](adr/0001-web-framework.md)
- [ADR-0002: Canonical Frontend](adr/0002-canonical-frontend.md)
- [ADR-0003: SDK Packaging](adr/0003-sdk-packaging.md)
- [ADR-0004: Tenancy Model](adr/0004-tenancy-model.md)
- [ADR-0005: Scripting Sandbox](adr/0005-scripting-sandbox.md)
- [ADR-0006: Event Storage and Partitioning](adr/0006-event-storage-partitioning.md)
- [ADR-0007: Telemetry Stack](adr/0007-telemetry-stack.md)
- [ADR-0008: Feature-Flag Strategy](adr/0008-feature-flag-strategy.md)
- [ADR-0009: Aggregation Merge](adr/0009-aggregation-merge.md)
- [ADR-0010: Cargo Workspace Layout](adr/0010-cargo-workspace.md)
- [ADR-0011: Connection Pool Consolidation](adr/0011-connection-pool-consolidation.md)

---

# Runbooks

- [Operator Runbook](runbooks/operator-runbook.md)
- [DB Pool Exhaustion](runbooks/db-pool-exhaustion.md)
- [Indexer Lag](runbooks/indexer-lag.md)
- [RPC Errors](runbooks/rpc-errors.md)
- [SSE Connections](runbooks/sse-connections.md)
- [Webhook Failures](runbooks/webhook-failures.md)
- [Notifications](runbooks/notifications.md)
- [Feature Flag Rollback](runbooks/feature-flag-rollback.md)

---

# Troubleshooting

- [Troubleshooting Guide](troubleshooting.md)

---

# Project

- [Feature Maturity](feature-maturity.md)

# Reference

- [Frequently Asked Questions (Issue #999)](FAQ.md)
- [Accessibility (WCAG 2.2 AA) Guide](accessibility-guide.md)
- [Advisory Lock Behavior & Leader Election Race Fix](advisory-lock-behavior.md)
- [Real-time Alerting Configuration (Issue #897)](alerting.md)
- [API Changelog Process](api-changelog.md)
- [API Compliance Testing](api-compliance-testing.md)
- [API Cookbook](api-cookbook.md)
- [API Deprecation Policy and Timeline](api-deprecation.md)
- [API Guide](api-guide.md)
- [Local API Sandbox](api-sandbox.md)
- [App Builder](app-builder.md)
- [Audit Trail for Compliance (Issue #946)](audit-trail.md)
- [Historical backfill from the Stellar ledger data lake (Galexie)](backfill.md)
- [Background Job Framework](background-jobs.md)
- [Batch Event Operations](batch-operations.md)
- [Bulk Event Export with Compression](bulk_export.md)
- [Chaos Engineering Tests (Issue #921)](chaos-testing.md)
- [`spulse` CLI Usage](cli-usage.md)
- [Client Libraries](client-libraries.md)
- [HTTP Response Compression](compression-optimization.md)
- [Configuration Management — Issue #997](configuration-management.md)
- [Connection Pool Configuration & Optimization - Issue #888](connection-pool.md)
- [Correlation IDs](correlation-ids.md)
- [Cross-Chain Event Correlation](cross-chain-events.md)
- [Cursor (Keyset) Pagination](cursor-pagination.md)
- [Web dashboard](dashboard.md)
- [Data Quality Monitoring](data-quality-monitoring.md)
- [Data Warehouse Export](data-warehouse.md)
- [Database Configuration Tuning (Issue #824)](database-configuration-tuning.md)
- [Webhook Dead-Letter Replay](dead-letter-replay.md)
- [Bloom Filter Deduplication Memory Management — Issue #996](dedup-memory-management.md)
- [Customer Delivery History API](delivery-history-api.md)
- [Notification Delivery Receipts](delivery-receipts.md)
- [Derived Metrics DSL](derived-metrics-dsl.md)
- [SorobanPulse Design Tokens](design-tokens.md)
- [Development Environment Setup](development-setup.md)
- [SorobanPulse Diagram Style Guide](diagram-style-guide.md)
- [Disaster Recovery Runbook (Issue #908)](disaster-recovery.md)
- [Discord integration](discord-integration.md)
- [E2E Testing](e2e-testing.md)
- [Edge Computing & CDN Integration Guide](edge-computing-guide.md)
- [Error Handling Documentation](error-handling.md)
- [Event Aggregation](event-aggregation.md)
- [Event Encryption at Rest - Issue #886](event-encryption.md)
- [Event Explorer & Event Detail — Design Specification](event-explorer-design.md)
- [Event Normalization](event-normalization.md)
- [Soroban Event Simulator](event-simulator.md)
- [Event Tagging](event-tagging.md)
- [Event Deduplication Across Replicas](event_deduplication_replicas.md)
- [AWS EventBridge Integration](eventbridge-integration.md)
- [Feature Flags](feature-flags.md)
- [Feature Gate Plan](feature-gate-plan.md)
- [Cargo Feature Matrix](features.md)
- [Event Filtering DSL](filter-dsl.md)
- [GDPR Data Handling Compliance (Issue #945)](gdpr-compliance.md)
- [GitOps for Infrastructure Management (Issue #910)](gitops.md)
- [Stellar & Soroban Glossary](glossary.md)
- [Graceful Degradation](graceful-degradation.md)
- [GraphQL Safety Limits](graphql-safety.md)
- [GraphQL Subscriptions for Real-Time Events](graphql_subscriptions.md)
- [Handler Split Plan](handler-split-plan.md)
- [Helm Migration Job](helm-migration-job.md)
- [HTTP Caching](http-caching.md)
- [HTTP/3 Support](http3-support.md)
- [Internationalization (i18n) Guide](i18n-guide.md)
- [Infrastructure as Code Testing (Issue #906)](iac-testing.md)
- [Idempotency-Key Support](idempotency-keys.md)
- [Idempotency Keys / Request Deduplication](idempotency.md)
- [Container Image Scanning and Signing (Issue #907)](image-security.md)
- [Incident Correlation Engine](incident-correlation.md)
- [Index Usage Analysis and Recommendations](index-analysis.md)
- [Index Maintenance Guide](index-maintenance.md)
- [IP allow/deny list enforcement (Issue #942)](ip-access-control.md)
- [JavaScript / TypeScript SDK](js-sdk.md)
- [Kafka Event Publishing (Issue #705)](kafka-event-publishing.md)
- [API key rotation (Issue #939)](key-rotation.md)
- [Load Testing Guide](load-testing-guide.md)
- [Load Testing Runbook](load-testing-runbook.md)
- [Log Analysis Tool](log-analysis-tool.md)
- [`GET /v1/meta`](meta-capabilities.md)
- [Metrics Counter Design (Issue #993)](metrics-design.md)
- [Operational Metrics Reference (Issue #918)](metrics-reference.md)
- [ML Model Serving](ml-model-serving.md)
- [Module Map](module-map.md)
- [Module Pruning Audit](module-pruning-audit.md)
- [Multi-Region Deployment (Issue #909)](multi-region.md)
- [Mutation Testing Guide](mutation-testing-guide.md)
- [Notification Delivery Architecture (Issue #994)](notification-architecture.md)
- [Notification Batching](notification-batching.md)
- [Notification Channel Interface](notification-channel-interface.md)
- [Unified Error Metrics and Profiling Implementation Guide](observability-unified-metrics.md)
- [Developer Onboarding (Issue #826)](onboarding.md)
- [OpenAPI 3.1 Upgrade](openapi-31-upgrade.md)
- [OpenAPI Client Library Code Generation](openapi-codegen.md)
- [PagerDuty Integration](pagerduty-integration.md)
- [Penetration testing and security assessment](penetration-testing.md)
- [Connection Pool Optimization — Issue #995](pool-optimization.md)
- [Priority-Based Webhook Delivery](priority-queueing.md)
- [RFC 9457 Problem Details](problem-json-errors.md)
- [Prometheus Remote Write Integration](prometheus-remote-write.md)
- [Python SDK](python-sdk.md)
- [Quantum-Ready Cryptography](quantum-cryptography.md)
- [Unified Query Builder Pattern](query-builder-pattern.md)
- [Query Result Caching & Cache Invalidation - Issue #889](query-caching.md)
- [Query Plan Caching and Tuning](query-plan-tuning.md)
- [Query Profiler](query-profiler.md)
- [Query Result Streaming - Issue #960](query-streaming.md)
- [Advanced per-API-key rate limiting (Issue #941)](rate-limiting-advanced.md)
- [Relay Cursor Pagination](relay-pagination.md)
- [Webhook Retry Policies](retry-policies.md)
- [Router Consolidation](router-consolidation.md)
- [Latency-Scored RPC Endpoint Selection](rpc-endpoint-selection.md)
- [Stellar Asset Contract Detection](sac-detection.md)
- [Scheduled Event Replay](scheduled-replay.md)
- [Schema Audit — SorobanPulse (#804)](schema-audit.md)
- [SorobanPulse Database Schema Reference](schema-reference.md)
- [Scoped API Keys](scoped-api-keys.md)
- [SDK Development Guide](sdk-development.md)
- [Secret Management and Secrets Scanning (Issue #943)](secret-management.md)
- [Security Headers](security-headers.md)
- [Security Testing](security-testing.md)
- [Serialization Caching - Issue #959](serialization-caching.md)
- [Service Mesh Integration Guide](service-mesh-guide.md)
- [Slack integration](slack-integration.md)
- [SLI / SLO Dashboard (Issue #696)](sli-slo.md)
- [SOC 2 Compliance Checklist (Issue #944)](soc2-compliance.md)
- [Real-Time Event Stream Statistics](stream-statistics.md)
- [Streaming Response Optimization - Issue #958](streaming-optimization.md)
- [Subscription Configuration Validator](subscription-validator.md)
- [System Design](system-design.md)
- [Table Partitioning Strategy](table-partitioning.md)
- [Microsoft Teams integration](teams-integration.md)
- [Integration Test Parallelization](test-parallelization.md)
- [Time Series Analysis](time-series-analysis.md)
- [Distributed Tracing Configuration (Issue #895)](tracing.md)
- [Troubleshooting Decision Tree](troubleshooting-guide.md)
- [Typed Error Migration](typed-error-migration.md)
- [Video Tutorials and Demonstrations](video-tutorials.md)
- [Soroban Pulse VSCode Extension](vscode-extension.md)
- [Web Operational Dashboard](web-dashboard.md)
- [Webhook Request/Response Logging](webhook-logging.md)
- [Webhook Signing](webhook-signing.md)
- [Webhook Endpoint Circuit Breaker](webhook_circuit_breaker.md)
- [Zero-trust security model (Issue #940)](zero-trust.md)

- [Architecture Decision Records]()
  - [NNNN — Decision title](adr/0000-template.md)
  - [0001 — Use a numbered Markdown ADR system](adr/0001-adr-system.md)
  - [0002 — Single-writer indexing with monitored read replicas](adr/0002-multi-replica-indexing.md)
  - [0003 — Layered retry, backoff, and circuit breaking for webhook delivery](adr/0003-webhook-retry-strategy.md)
  - [0004 — Optional gzip compression for stored event data](adr/0004-event-compression.md)
  - [Architecture Decision Records](adr/README.md)
  - [ADR review guidelines](adr/review-guidelines.md)

- [Archive]()
  - [Webhook Delivery Improvements - Implementation Guide](archive/IMPLEMENTATION_GUIDE.md)
  - [Integration Implementation Status](archive/IMPLEMENTATION_STATUS.md)
  - [Implementation Summary: Database Optimization Issues #690-#693](archive/IMPLEMENTATION_SUMMARY_690-693.md)
  - [Implementation Summary: Issues #894-897](archive/IMPLEMENTATION_SUMMARY_894-897.md)
  - [Integrations Implementation Summary](archive/INTEGRATIONS_IMPLEMENTATION.md)
  - [docs/archive](archive/README.md)

- [Assets]()
  - [Repository assets](assets/README.md)
  - [SorobanPulse brand assets](assets/brand/README.md)

- [Deployment Runbooks]()
  - [Deployment Runbooks](deployment-runbooks/README.md)
  - [AWS Deployment Runbook (EC2 + RDS + ALB)](deployment-runbooks/aws.md)
  - [Azure Deployment Runbook (VM + Azure Database for PostgreSQL + Application Gateway)](deployment-runbooks/azure.md)
  - [GCP Deployment Runbook (Compute Engine + Cloud SQL + Load Balancing)](deployment-runbooks/gcp.md)
  - [Self-Hosted Deployment Runbook](deployment-runbooks/self-hosted.md)
  - [Runbook Template](deployment-runbooks/template.md)
  - [Testing the Deployment Runbooks](deployment-runbooks/testing-framework.md)

- [Design]()
  - [SorobanPulse Brand Guidelines](design/brand.md)
  - [Data-Visualisation Guidelines](design/dataviz.md)
  - [SorobanPulse design system](design/design-system.md)
  - [Event Explorer — Wireframes & Mockups](design/event-explorer/README.md)
  - [Dashboard State Patterns](design/patterns.md)

- [Emergency Runbooks]()
  - [Emergency Runbooks — Issue #998](emergency-runbooks/README.md)
  - [Runbook: Complete Data Loss Recovery](emergency-runbooks/data-loss-recovery.md)
  - [Runbook: Database Corruption Recovery](emergency-runbooks/database-corruption-recovery.md)
  - [Runbook: Security Breach Response](emergency-runbooks/security-breach-response.md)
  - [Runbook: Widespread Service Failure Recovery](emergency-runbooks/service-failure-recovery.md)

- [Guides]()
  - [Soroban Pulse Integration Guide](guides/integration-guide.md)

- [Migration Guides]()
  - [Migration Guides (Issue #1000)](migration-guides/README.md)
  - [Data migration procedures (Issue #1000)](migration-guides/data-migration-procedures.md)
  - [Migrating from other event indexers (Issue #1000)](migration-guides/from-other-indexers.md)
  - [Migrating from Stellar Horizon (Issue #1000)](migration-guides/from-stellar-horizon.md)
  - [Rollback procedures (Issue #1000)](migration-guides/rollback-procedures.md)
  - [Subscription mapping (Issue #1000)](migration-guides/subscription-mapping.md)
  - [Migration validation (Issue #1000)](migration-guides/validation.md)

- [Pr]()
  - [Chideraisiguzor 1137 1140](pr/chideraisiguzor-1137-1140.md)
  - [Misrasamuelisiguzor Oss 1121 1124](pr/misrasamuelisiguzor-oss-1121-1124.md)
  - [Samuelisi 1133 1136](pr/samuelisi-1133-1136.md)
  - [Spotkorner Dot 1125 1128](pr/spotkorner-dot-1125-1128.md)
  - [Techisigu 1129 1132](pr/techisigu-1129-1132.md)

- [Rfcs]()
  - [Design Specs / RFCs](rfcs/README.md)
  - [Webhook Delivery Reliability & Observability - Executive Summary](rfcs/webhook-delivery-improvements/executive-summary.md)
  - [Webhook Delivery Improvements - Quick Start Guide for Developers](rfcs/webhook-delivery-improvements/quick-start.md)
  - [Webhook Delivery Reliability & Observability Improvements](rfcs/webhook-delivery-improvements/spec.md)
  - [Webhook Delivery Improvements - Testing & Validation Strategy](rfcs/webhook-delivery-improvements/testing-and-validation.md)

- [Runbooks]()
  - [Production Profiling with pprof](runbooks/operator-profiling.md)
  - [Runbook: Query Plan Cache](runbooks/query-plan-cache.md)
  - [Runbook: RPC Retention Gap](runbooks/rpc-retention-gap.md)
  - [Runbook: Schema Consolidation](runbooks/schema-consolidation.md)

- [Security]()
  - [SorobanPulse Threat Model](security/threat-model.md)

- [Site]()
  - [docs/site/static/](site/static/README.md)

- [Tutorials]()
  - [Tutorial: Build a Token Transfer Alert Bot](tutorials/token-alert-bot.md)
