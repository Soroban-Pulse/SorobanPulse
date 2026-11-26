# Issue #1455 - Split src/config/mod.rs into Focused Modules

## Problem Statement

All application configuration (3,197 lines) + validation (config_validation.rs 864 lines) in one file. Configuration domains are intermixed:
- Server settings (host, port, TLS)
- Database (connection, pooling, credentials)
- Indexer (RPC endpoints, polling intervals, leader election)
- RPC client (timeout, retry, gas limits)
- Authentication (JWT, keys, CORS)
- SSE streaming (reconnect, buffer size)
- WebSocket (backpressure, timeout)
- Notifications (retry, rate limits, SLA)
- SaaS (quotas, billing, API keys)
- ML/analytics (model paths, batch sizes)
- Observability (metrics, tracing, SLO)

## Proposed Structure

```
src/config/
├── mod.rs                        -- AppConfig struct, load(), merged validation
├── server.rs                     -- ServerConfig (host, port, TLS, timeout)
├── database.rs                   -- DatabaseConfig (pool, credentials, migrations)
├── indexer.rs                    -- IndexerConfig (RPC, polling, leader election)
├── rpc.rs                        -- RpcClientConfig (endpoints, timeout, retry)
├── auth.rs                       -- AuthConfig (JWT, keys, CORS, session)
├── streaming.rs                  -- StreamingConfig (SSE, WebSocket settings)
├── notifications.rs              -- NotificationConfig (channels, retry, SLA)
├── saas.rs                       -- SaasConfig (quotas, billing, API keys)
├── ml.rs                         -- MlConfig (model paths, inference settings)
├── observability.rs              -- ObservabilityConfig (metrics, tracing, SLO)
└── validation/
    ├── mod.rs                    -- Validation trait impl
    ├── server.rs                 -- ServerConfig validation
    ├── database.rs               -- DatabaseConfig validation
    ├── indexer.rs                -- IndexerConfig validation
    └── ... (one per domain)
```

## Migration Strategy

### Phase 1 - Create Module Structure (PR 1)
1. Create src/config/ directory
2. Create empty domain modules (server.rs, database.rs, etc.)
3. Create src/config/validation/ directory
4. Keep AppConfig in mod.rs temporarily

### Phase 2 - Extract Server Config (PR 2)
1. Move ServerConfig struct → server.rs
2. Move ServerConfig impl → server.rs
3. Move ServerConfig validation → validation/server.rs
4. Update AppConfig to use nested struct

### Phase 3 - Extract Database Config (PR 3)
1. Move DatabaseConfig → database.rs
2. Move validation → validation/database.rs
3. Update imports in app startup

### Phase 4 - Extract Indexer Config (PR 4)
1. Move IndexerConfig → indexer.rs
2. Move validation → validation/indexer.rs

### Phase 5 - Extract RPC, Auth, Streaming (PR 5)
1. Move RpcClientConfig → rpc.rs
2. Move AuthConfig → auth.rs
3. Move StreamingConfig → streaming.rs
4. Update validations

### Phase 6 - Extract Notifications, SaaS, ML, Observability (PR 6)
1. Move NotificationConfig → notifications.rs
2. Move SaasConfig → saas.rs
3. Move MlConfig → ml.rs
4. Move ObservabilityConfig → observability.rs

### Phase 7 - Consolidate Validation (PR 7)
1. Move all validation logic → validation/mod.rs
2. Implement validation trait for each domain
3. Remove config_validation.rs (consolidate into validation/)

### Phase 8 - Final Cleanup (PR 8)
1. Update all imports across codebase
2. Add documentation to each module
3. Add module-level docs
4. Verify no dead code

## Expected Outcomes

✅ Single Responsibility Principle per module  
✅ Easier to find and modify specific config  
✅ Clear dependencies between domains  
✅ Better testability (mock individual configs)  
✅ Reduced cognitive load per file  
✅ Clearer validation organization  
✅ Easier onboarding for new developers  

## Checklist

- [ ] Design module structure (get team approval)
- [ ] Create src/config/ directory
- [ ] Create Phase 1 PR (structure + empty modules)
- [ ] Create Phase 2-6 PRs (domain extraction)
- [ ] Create Phase 7 PR (validation consolidation)
- [ ] Create Phase 8 PR (cleanup + docs)
- [ ] Update architecture.md
- [ ] Add module documentation
- [ ] Verify no unused config fields
- [ ] Performance test (config loading time)
