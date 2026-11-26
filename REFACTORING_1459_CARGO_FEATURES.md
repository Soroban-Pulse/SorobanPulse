# Issue #1459 - Gate Optional Subsystems Behind Cargo Features

## Problem Statement

Default build compiles everything (SaaS, ML, SMS, push, geospatial, Lua, Parquet) even when unused. This causes:
- Long clean builds for contributors and CI
- Large release binary (goal: < 80 MB, currently larger due to optional code)
- Unused network-facing code paths in every deployment
- Unnecessary dependency footprint

## Current State

**Existing Features (Partial Coverage):**
- kafka, sqs, pubsub, dashboard, istio already have feature gates
- BUT saas_platform.rs, anomaly_detection.rs, push_notification.rs, sms.rs, lua_transform.rs, parquet_export.rs are compiled unconditionally

**Unconditional Dependencies (Always Compiled):**
- aws-sdk-* (S3, Kinesis)
- redis
- lettre (email with templates)
- twiml/twilio (SMS)
- apns, fcm (push notifications)
- polars (Parquet/Arrow)
- mlua (Lua scripting)
- geo, geohash (geospatial)
- various ML/anomaly libs

## Proposed Cargo.toml Structure

```toml
[package]
name = "soroban-pulse"
version = "0.1.0"

[features]
default = ["core"]
core = ["tokio", "axum", "sqlx", "serde"]  # Always included

# Optional subsystems
saas = ["billing", "rate-limit"]
billing = ["stripe", "invoicing"]
ml = ["tch-rs", "polars"]
anomaly = ["ml", "statistical"]
push = ["apns", "fcm"]
sms = ["twilio", "twiml"]
export = ["parquet", "arrow"]
geospatial = ["geo", "geohash"]
lua = ["mlua"]
aws = ["aws-sdk-s3", "aws-sdk-kinesis"]
email = ["lettre", "askama"]

# Message queues (existing)
kafka = ["rdkafka"]
sqs = ["aws-sdk-sqs"]
pubsub = ["google-cloud-pubsub"]

# Operational features
dashboard = ["plotters", "warp"]
istio = ["x509-parser"]
observability = ["opentelemetry", "jaeger"]
metrics = ["prometheus"]

# Convenience meta-features
integrations = ["saas", "ml", "push", "sms", "export"]
aws-stack = ["aws", "sqs", "kafka"]
all = ["integrations", "aws-stack", "dashboard", "istio"]
```

## Implementation Steps

### Phase 1 - Create Feature Structure (PR 1)
1. Add [features] section to Cargo.toml as above
2. Reorganize [dependencies] with feature markers
3. Create feature_gates.rs module with feature-conditional pub use
4. Document build profiles for different deployments

### Phase 2 - Gate SaaS Platform (PR 2)
1. Move saas_platform.rs code into #[cfg(feature = "saas")] blocks
2. Create stub implementations for non-saas builds
3. Update saas_platform module initialization in lib.rs
4. Test builds with `cargo build --no-default-features --features core`

### Phase 3 - Gate ML/Anomaly Detection (PR 3)
1. Move anomaly_detection.rs into #[cfg(feature = "anomaly")] blocks
2. Create fallback anomaly handler (always returns Ok)
3. Update config to skip ML initialization when feature disabled
4. Test: `cargo build --no-default-features --features core`

### Phase 4 - Gate Push/SMS/Email (PR 4)
1. Gate push_notification.rs behind #[cfg(feature = "push")]
2. Gate sms.rs behind #[cfg(feature = "sms")]
3. Gate email sender behind #[cfg(feature = "email")]
4. Create stub implementations for no-op sends

### Phase 5 - Gate Export Formats (PR 5)
1. Gate parquet_export.rs behind #[cfg(feature = "export")]
2. Gate geospatial.rs behind #[cfg(feature = "geospatial")]
3. Gate lua_transform.rs behind #[cfg(feature = "lua")]
4. Create fallback export handlers

### Phase 6 - Conditional Initialization (PR 6)
1. Update main.rs to conditionally initialize subsystems
2. Update config loading to skip disabled features
3. Update routes/handlers to skip feature-gated endpoints
4. Test all combinations:
   - `cargo build --no-default-features --features core` (minimal)
   - `cargo build --features core,kafka` (messaging only)
   - `cargo build --all-features` (full)

### Phase 7 - CI/Build Testing (PR 7)
1. Add CI matrix for different feature combinations
2. Add docker builds for each profile
3. Update docker-compose with minimal profile
4. Document deployment profiles

## Expected Builds

| Profile | Features | Size | Build Time | Compile Targets |
|---------|----------|------|------------|-----------------|
| minimal | core | ~20 MB | ~30s | fast, offline |
| messaging | core, kafka, sqs | ~35 MB | ~45s | event-driven |
| analytics | core, ml, export | ~40 MB | ~60s | analysis-focused |
| full | all | ~80+ MB | ~120s | complete system |

## Code Structure Example

```rust
// src/lib.rs
pub mod core;

#[cfg(feature = "saas")]
pub mod saas_platform;

#[cfg(feature = "ml")]
pub mod anomaly_detection;

#[cfg(feature = "push")]
pub mod push_notification;

#[cfg(feature = "sms")]
pub mod sms;

#[cfg(feature = "email")]
pub mod email_sender;

#[cfg(feature = "export")]
pub mod parquet_export;

#[cfg(feature = "geospatial")]
pub mod geospatial;

#[cfg(feature = "lua")]
pub mod lua_transform;
```

```rust
// src/feature_gates.rs - Conditional re-exports
#[cfg(feature = "saas")]
pub use crate::saas_platform::{SaasManager, BillingService};

#[cfg(not(feature = "saas"))]
pub struct SaasManager;
#[cfg(not(feature = "saas"))]
pub struct BillingService;
```

## Checklist

- [ ] Design feature hierarchy
- [ ] Create Phase 1 PR (Cargo.toml structure)
- [ ] Create Phase 2-5 PRs (feature gating)
- [ ] Create Phase 6 PR (initialization)
- [ ] Create Phase 7 PR (CI testing)
- [ ] Update docker-compose
- [ ] Document deployment profiles
- [ ] Test all build combinations
- [ ] Measure binary size reduction
- [ ] Measure compile time reduction

## Benefits

✅ Reduces binary size by 30-50% for minimal deployments  
✅ Compile time reduced for fast iteration  
✅ Attack surface reduced (no unused code paths)  
✅ Dependency management simpler  
✅ Clear deployment profiles for different use cases  
✅ Better resource utilization in CI  

## Related Issues

Coordinates with:
- #1460 (workspace refactoring) - Feature gates work better in workspace
- #1461 (dependency upgrades) - Easier to upgrade when features separate
- #1462 (error handling) - Core error types remain lean with feature gates
