# Summary

<!-- Canonical site map for the documentation site (Issue #1170).
     docs/site/sidebars.js builds every sidebar from this file, and
     `npm run check-nav` fails when a doc under docs/ is missing here.
     Each top-level `##` heading becomes a navbar section, `###` a category,
     and a nested list a category whose page is its parent link.

     Merged duplicates (client-side redirects in docusaurus.config.js):
       /docs/property-testing     → /docs/guides/property-based-testing
       /docs/api-contract-testing → /docs/guides/contract-testing

     All pages live in topical subdirectories and use kebab-case file names
     (Issue #1485). -->

## Getting Started

- [Developer Onboarding](../guides/onboarding.md)
- [Development Environment Setup](../guides/development-setup.md)
- [Tutorial: Token Transfer Alert Bot](../guides/tutorials/token-alert-bot.md)
- [Frequently Asked Questions](../guides/faq.md)
- [Video Tutorials and Demonstrations](../guides/video-tutorials.md)
- [Feature Maturity](../reference/feature-maturity.md)

## Guides

### Events and Indexing

- [SorobanPulse Architecture](../reference/architecture.md)
- [System Design](../reference/system-design.md)
- [Contract Event Schemas](../reference/contract-event-schemas.md)
- [Event Normalization](../guides/event-normalization.md)
- [Event Deduplication](../guides/event-deduplication.md)
- [Event Deduplication Across Replicas](../guides/event-deduplication-replicas.md)
- [Event Aggregation](../guides/event-aggregation.md)
- [Event Tagging](../guides/event-tagging.md)
- [Batch Event Operations](../guides/batch-operations.md)
- [Bulk Event Export with Compression](../guides/bulk-export.md)
- [Historical backfill from the Stellar ledger data lake (Galexie)](../guides/backfill.md)
- [Scheduled Event Replay](../guides/scheduled-replay.md)
- [Webhook Dead-Letter Replay](../guides/dead-letter-replay.md)
- [Background Job Framework](../guides/background-jobs.md)
- [Advisory Lock Behavior & Leader Election Race Fix](../guides/advisory-lock-behavior.md)
- [Latency-Scored RPC Endpoint Selection](../guides/rpc-endpoint-selection.md)
- [Stellar Asset Contract Detection](../guides/sac-detection.md)
- [Event Filtering DSL](../guides/filter-dsl.md)
- [Temporal Event Queries](../guides/temporal-queries.md)
- [Time Series Analysis](../guides/time-series-analysis.md)
- [Derived Metrics DSL](../guides/derived-metrics-dsl.md)
- [Lua Event Transformation](../guides/lua-transformation.md)
- [Contract Event Simulation](../guides/contract-event-simulation.md)
- [Soroban Event Simulator](../guides/event-simulator.md)
- [Cross-Chain Event Correlation](../guides/cross-chain-events.md)
- [Data Quality Monitoring](../guides/data-quality-monitoring.md)
- [Data Warehouse Export](../guides/data-warehouse.md)
- [Event Explorer & Event Detail — Design Specification](../guides/event-explorer-design.md)

### Streaming (SSE)

- [SSE Client Reconnection](../guides/sse-reconnection.md)
- [SSE Connection Timeout Configuration for Reverse Proxies](../guides/sse-reverse-proxy-configuration.md)
- [Streaming Response Optimization](../guides/streaming-optimization.md)
- [Query Response Streaming](../guides/streaming-responses.md)
- [Query Result Streaming](../guides/query-streaming.md)
- [Real-Time Event Stream Statistics](../guides/stream-statistics.md)
- [Event feeds](../guides/event-feeds.md)

### Webhooks and Subscriptions

- [Webhook Signature Verification](../guides/webhook-verification.md)
- [Webhook Signing](../guides/webhook-signing.md)
- [Webhook Endpoint Circuit Breaker](../guides/webhook-circuit-breaker.md)
- [Webhook endpoint rate limits](../guides/webhook-endpoint-rate-limits.md)
- [Webhook Request/Response Logging](../guides/webhook-logging.md)
- [Subscription Best Practices](../guides/subscription-best-practices.md)
- [Subscription Configuration Validator](../guides/subscription-validator.md)
- [Webhook Retry Policies](../guides/retry-policies.md)
- [Priority-Based Webhook Delivery](../guides/priority-queueing.md)
- [Customer Delivery History API](../guides/delivery-history-api.md)
- [Notification Delivery Receipts](../guides/delivery-receipts.md)

### Notifications

- [Notification Delivery Architecture](../guides/notification-architecture.md)
- [Notification Channels – Advanced Features](../guides/notification-channels.md)
- [Notification Channel Interface](../guides/notification-channel-interface.md)
- [Notification System — Advanced Features](../guides/notification-features.md)
- [Per-Channel Notification Rate Limiting](../guides/notification-rate-limiting.md)
- [Notification Batching](../guides/notification-batching.md)
- [Notification Deduplication](../guides/notification-deduplication.md)
- [Content-Based Notification Filtering](../guides/notification-content-filter.md)
- [Email Notifications](../guides/email-notifications.md)
- [Email Notifications - Quick Start](../guides/email-quick-start.md)
- [Incident Correlation Engine](../guides/incident-correlation.md)

### Dashboard and Tools

- [Web dashboard](../guides/dashboard.md)
- [Web Operational Dashboard](../guides/web-dashboard.md)
- [App Builder](../guides/app-builder.md)
- [spulse CLI Usage](../guides/cli-usage.md)
- [Soroban Pulse VSCode Extension](../guides/vscode-extension.md)
- [Log Analysis Tool](../guides/log-analysis-tool.md)
- [GET /v1/meta](../reference/meta-capabilities.md)

### Advanced

- [Multi-Tenant Access Isolation](../guides/multi-tenancy.md)
- [SaaS Platform & Multi-Tenant Hosting (Experimental)](../guides/saas-platform.md)
- [AI/ML Integration & Intelligence Features (Experimental)](../guides/ml-integration.md)
- [ML Model Serving](../guides/ml-model-serving.md)
- [Quantum-Ready Cryptography (Planned)](../guides/quantum-cryptography.md)
- [Edge Computing & CDN Integration Guide](../guides/edge-computing-guide.md)

### Migration Guides

- [Migration Guides](../guides/migration-guides/README.md)
  - [Migrating from Stellar Horizon](../guides/migration-guides/from-stellar-horizon.md)
  - [Migrating from other event indexers](../guides/migration-guides/from-other-indexers.md)
  - [Data migration procedures](../guides/migration-guides/data-migration-procedures.md)
  - [Subscription mapping](../guides/migration-guides/subscription-mapping.md)
  - [Rollback procedures](../guides/migration-guides/rollback-procedures.md)
  - [Migration validation](../guides/migration-guides/validation.md)

## API

- [API Guide](../reference/api-guide.md)
- [API Usage Guide](../guides/api-usage.md)
- [API Cookbook](../guides/api-cookbook.md)
- [API Versioning Policy](../reference/api-versioning.md)
- [API Deprecation Policy and Timeline](../reference/api-deprecation.md)
- [API Changelog Process](../reference/api-changelog.md)
- [API Response Time SLA](../reference/api-sla.md)
- [Local API Sandbox](../guides/api-sandbox.md)
- [Postman Collection Generator](../guides/postman.md)

### Querying

- [Cursor (Keyset) Pagination](../guides/cursor-pagination.md)
- [Relay Cursor Pagination](../guides/relay-pagination.md)
- [HTTP Caching](../guides/http-caching.md)
- [HTTP Response Compression](../guides/compression-optimization.md)
- [Idempotency-Key Support](../guides/idempotency-keys.md)

### Auth, Limits and Errors

- [Scoped API Keys](../guides/scoped-api-keys.md)
- [Rate Limiting Per API Key](../guides/rate-limiting.md)
- [Advanced per-API-key rate limiting](../guides/rate-limiting-advanced.md)
- [Error Handling Documentation](../guides/error-handling.md)
- [RFC 9457 Problem Details](../reference/problem-json-errors.md)
- [Correlation IDs](../guides/correlation-ids.md)

### GraphQL

- [GraphQL API Documentation (Experimental)](../guides/graphql-api.md)
- [GraphQL Subscriptions for Real-Time Events (Experimental)](../guides/graphql-subscriptions.md)
- [GraphQL Safety Limits](../guides/graphql-safety.md)

### OpenAPI and SDKs

- [OpenAPI 3.1 Upgrade](../reference/openapi-31-upgrade.md)
- [OpenAPI Client Library Code Generation](../guides/openapi-codegen.md)
- [Subscription Code Generator](../guides/codegen.md)
- [SDK Integration Guide](../guides/sdk-integration-guide.md)
- [SDK Development Guide](../guides/sdk-development.md)
- [Client Libraries](../reference/client-libraries.md)
- [JavaScript / TypeScript SDK](../guides/js-sdk.md)
- [Python SDK](../guides/python-sdk.md)

## Operations

### Configuration

- [Configuration Management](../operations/configuration-management.md)
- [Cargo Feature Matrix](../reference/features.md)
- [Feature Flags](../operations/feature-flags.md)
- [Feature Flag Rollback Automation](../operations/feature-flag-rollback.md)
- [Secret Management and Secrets Scanning](../operations/secret-management.md)
- [API key rotation](../operations/key-rotation.md)

### Deployment

- [Deployment Guide](../operations/deployment.md)
- [Deployment Guide: Common Platforms](../operations/deployment-platforms.md)
- [Kubernetes Probes](../operations/kubernetes-probes.md)
- [Helm Migration Job](../operations/helm-migration-job.md)
- [Terraform Infrastructure Guide](../operations/terraform.md)
- [Infrastructure as Code Testing](../operations/iac-testing.md)
- [GitOps for Infrastructure Management](../operations/gitops.md)
- [Multi-Region Deployment](../operations/multi-region.md)
- [Multi-Deployment Architecture Guide](../operations/multi-deployment-architecture.md)
- [Service Mesh Integration Guide](../operations/service-mesh-guide.md)
- [HTTP/3 Support](../operations/http3-support.md)
- [Deployment Runbooks](../operations/deployment-runbooks/README.md)
  - [AWS Deployment Runbook (EC2 + RDS + ALB)](../operations/deployment-runbooks/aws.md)
  - [GCP Deployment Runbook (Compute Engine + Cloud SQL + Load Balancing)](../operations/deployment-runbooks/gcp.md)
  - [Azure Deployment Runbook (VM + Azure Database for PostgreSQL + Application Gateway)](../operations/deployment-runbooks/azure.md)
  - [Self-Hosted Deployment Runbook](../operations/deployment-runbooks/self-hosted.md)
  - [Runbook Template](../operations/deployment-runbooks/template.md)
  - [Testing the Deployment Runbooks](../operations/deployment-runbooks/testing-framework.md)

### Observability

- [Real-time Alerting Configuration](../operations/alerting.md)
- [Distributed Tracing Configuration](../operations/tracing.md)
- [Structured Logging Convention](../operations/logging.md)
- [Log Aggregation Setup](../operations/log-aggregation.md)
- [Metrics Counter Design](../operations/metrics-design.md)
- [SLI / SLO Dashboard](../operations/sli-slo.md)
- [Prometheus Remote Write Integration](../operations/prometheus-remote-write.md)
- [Replica Sync Monitoring](../operations/replica-monitoring.md)

### Database

- [Connection Pool Configuration & Optimization](../operations/connection-pool.md)
- [Table Partitioning Strategy](../operations/table-partitioning.md)
- [Index Usage Analysis and Recommendations](../operations/index-analysis.md)
- [Index Maintenance Guide](../operations/index-maintenance.md)
- [Query Result Caching & Cache Invalidation](../operations/query-caching.md)
- [PostgreSQL Query Plan Caching](../operations/query-plan-caching.md)
- [Query Plan Caching and Tuning](../operations/query-plan-tuning.md)
- [Query Profiler](../operations/query-profiler.md)
- [Database Statistics Auto-Analysis Configuration](../operations/statistics-auto-analysis.md)
- [Database Configuration Tuning](../operations/database-configuration-tuning.md)
- [Backup Verification](../operations/backup-verification.md)
- [Data Retention Policy](../operations/data-retention.md)
- [Serialization Caching](../operations/serialization-caching.md)
- [JSON Serialization Optimization](../operations/serialization-optimization.md)
- [Bloom Filter Deduplication Memory Management](../operations/dedup-memory-management.md)

### Security and Compliance

- [SorobanPulse Threat Model](../operations/security/threat-model.md)
- [Security Headers](../operations/security-headers.md)
- [OWASP API Security Headers](../operations/owasp-security-headers.md)
- [IP allow/deny list enforcement](../operations/ip-access-control.md)
- [Event Encryption at Rest](../operations/event-encryption.md)
- [Zero-trust security model](../operations/zero-trust.md)
- [Container Image Scanning and Signing](../operations/image-security.md)
- [Penetration testing and security assessment](../operations/penetration-testing.md)
- [SOC 2 Compliance Checklist](../operations/soc2-compliance.md)
- [GDPR Data Handling Compliance](../operations/gdpr-compliance.md)
- [Audit Logging for Sensitive Operations](../operations/audit-logging.md)

### Reliability and Scale

- [Capacity Planning Automation](../operations/capacity-planning.md)
- [Performance Tuning Guide](../operations/performance-tuning.md)
- [Load Testing Guide](../operations/load-testing-guide.md)
- [Load Testing Runbook](../operations/load-testing-runbook.md)
- [Disaster Recovery Runbook](../operations/disaster-recovery.md)
- [Graceful Degradation](../operations/graceful-degradation.md)

### Runbooks

- [Operator Runbook](../operations/runbooks/operator-runbook.md)
- [Indexer Lag Runbook](../operations/runbooks/indexer-lag.md)
- [Database Connection Pool Exhaustion Runbook](../operations/runbooks/db-pool-exhaustion.md)
- [RPC Errors Runbook](../operations/runbooks/rpc-errors.md)
- [Webhook Failures Runbook](../operations/runbooks/webhook-failures.md)
- [Notification Delivery Troubleshooting](../operations/runbooks/notifications.md)
- [SSE Connections Runbook](../operations/runbooks/sse-connections.md)
- [Feature Flag Rollback Troubleshooting](../operations/runbooks/feature-flag-rollback.md)
- [Runbook: Query Plan Cache](../operations/runbooks/query-plan-cache.md)
- [Runbook: Schema Consolidation](../operations/runbooks/schema-consolidation.md)
- [Runbook: RPC Retention Gap](../operations/runbooks/rpc-retention-gap.md)
- [Emergency Runbooks](../operations/emergency-runbooks/README.md)
  - [Runbook: Security Breach Response](../operations/emergency-runbooks/security-breach-response.md)
  - [Runbook: Database Corruption Recovery](../operations/emergency-runbooks/database-corruption-recovery.md)
  - [Runbook: Complete Data Loss Recovery](../operations/emergency-runbooks/data-loss-recovery.md)
  - [Runbook: Widespread Service Failure Recovery](../operations/emergency-runbooks/service-failure-recovery.md)

### Troubleshooting

- [Troubleshooting Decision Tree](../operations/troubleshooting-guide.md)
- [Troubleshooting and Debugging Guide](../operations/troubleshooting.md)

## Integrations

### Chat and Incident Tools

- [Slack integration](../integrations/slack-integration.md)
- [Discord integration](../integrations/discord-integration.md)
- [Microsoft Teams integration](../integrations/teams-integration.md)
- [PagerDuty Integration](../integrations/pagerduty-integration.md)
- [Platform Integration Guide](../integrations/integration-guide.md)

### Event Streaming

- [Kafka Event Publishing](../integrations/kafka-event-publishing.md)
- [AWS SQS integration](../integrations/sqs-integration.md)
- [Azure Event Hubs integration](../integrations/event-hubs-integration.md)
- [AWS EventBridge Integration](../integrations/eventbridge-integration.md)

## Contributing

- [Module Map](../development/module-map.md)
- [Archive: Implementation Reports](../archive/README.md)

### Architecture Decisions

- [Architecture Decision Records](../development/adr/README.md)
  - [ADR Template](../development/adr/0000-template.md)
  - [0001 — Use a numbered Markdown ADR system](../development/adr/0001-adr-system.md)
  - [0002 — Single-writer indexing with monitored read replicas](../development/adr/0002-multi-replica-indexing.md)
  - [0003 — Layered retry, backoff, and circuit breaking for webhook delivery](../development/adr/0003-webhook-retry-strategy.md)
  - [0004 — Optional gzip compression for stored event data](../development/adr/0004-event-compression.md)
  - [ADR review guidelines](../development/adr/review-guidelines.md)

### Testing

- [E2E Testing](../development/e2e-testing.md)
- [Property-Based Testing Guide](../development/property-based-testing.md)
- [API Contract Testing with Pact](../development/contract-testing.md)
- [API Compatibility Testing](../development/api-compatibility-testing.md)
- [API Compliance Testing](../development/api-compliance-testing.md)
- [Mutation Testing Guide](../development/mutation-testing.md)
- [Chaos Engineering Tests](../development/chaos-testing.md)
- [Security Testing](../development/security-testing.md)
- [Performance Regression Testing](../development/performance-regression-testing.md)
- [Integration Test Parallelization](../development/test-parallelization.md)

### Design

- [SorobanPulse design system](../development/design/design-system.md)
- [Data-Visualisation Guidelines](../development/design/dataviz.md)
- [SorobanPulse Design Tokens](../development/design-tokens.md)
- [SorobanPulse Diagram Style Guide](../development/diagram-style-guide.md)
- [Repository assets](../assets/README.md)
- [SorobanPulse brand assets](../assets/brand/README.md)

### Engineering Plans

- [Aggregation Merge Plan](../development/aggregation-merge-plan.md)
- [Cargo Workspace Plan](../development/cargo-workspace-plan.md)
- [Connection Pool Consolidation](../development/connection-pool-consolidation.md)
- [Feature Gate Plan](../development/feature-gate-plan.md)
- [Handler Split Plan](../development/handler-split-plan.md)
- [Module Pruning Audit](../development/module-pruning-audit.md)
- [Router Consolidation](../development/router-consolidation.md)
- [Schema Audit — SorobanPulse](../development/schema-audit.md)
- [Typed Error Migration](../development/typed-error-migration.md)
- [Unified Query Builder Pattern](../development/query-builder-pattern.md)

### Design Specs / RFCs

- [Design Specs / RFCs](../rfcs/README.md)
  - [Webhook delivery improvements: spec](../rfcs/webhook-delivery-improvements/spec.md)
  - [Webhook delivery improvements: executive summary](../rfcs/webhook-delivery-improvements/executive-summary.md)
  - [Webhook delivery improvements: quick start](../rfcs/webhook-delivery-improvements/quick-start.md)
  - [Webhook delivery improvements: testing and validation](../rfcs/webhook-delivery-improvements/testing-and-validation.md)

### Pull Request Notes

- [Chideraisiguzor 1137 1140](../pr/chideraisiguzor-1137-1140.md)
- [Misrasamuelisiguzor Oss 1121 1124](../pr/misrasamuelisiguzor-oss-1121-1124.md)
- [Samuelisi 1133 1136](../pr/samuelisi-1133-1136.md)
- [Spotkorner Dot 1125 1128](../pr/spotkorner-dot-1125-1128.md)
- [Techisigu 1129 1132](../pr/techisigu-1129-1132.md)

## Reference

- [Configuration Reference](../configuration.md)
- [Operational Metrics Reference](../metrics-reference.md)
- [Database Schema](../schema.md)
- [Stellar & Soroban Glossary](../glossary.md)
