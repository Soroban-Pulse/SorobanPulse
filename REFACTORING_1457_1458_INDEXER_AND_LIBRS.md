# Issues #1457 & #1458 - Indexer Refactoring + Public API Reduction

## Issue #1457 - Split src/indexer.rs (2,088 lines)

### Problem
Indexer mixes 5 distinct concerns:
- Advisory lock leader election (416–476)
- RPC polling with exponential backoff (515+)
- Event normalization (cryptographic validation)
- Large INSERT INTO events (23 columns, 1274)
- Savepoint handling for rollback (1336)
- Hash-chain validation
- Tests including 3-second promotion test (2021)

### Proposed Structure

```
src/indexer/
├── mod.rs                        -- Coordinator, main loop
├── leader_election.rs            -- Advisory lock, failover
├── rpc_poller.rs                 -- Polling with backoff
├── event_normalizer.rs           -- Crypto validation, normalization
├── persistence.rs                -- Database writes, savepoints
├── hash_validator.rs             -- Hash-chain verification
└── tests.rs                       -- Integration tests
```

### Module Breakdown

| Module | Lines | Responsibility |
|--------|-------|-----------------|
| leader_election.rs | ~150 | Advisory locks, failover, heartbeat |
| rpc_poller.rs | ~400 | Poll endpoints, backoff, retry strategy |
| event_normalizer.rs | ~300 | Crypto validation, timestamp normalization |
| persistence.rs | ~500 | INSERT, savepoints, rollback, indexes |
| hash_validator.rs | ~200 | Hash-chain verification, proof validation |
| mod.rs | ~200 | Coordinator, heartbeat, error handling |
| tests.rs | ~350 | Integration tests, mock RPC |

### Implementation Order

1. Create src/indexer/ directory
2. Move leader_election logic → leader_election.rs
3. Move rpc_poller logic → rpc_poller.rs
4. Move event_normalizer logic → event_normalizer.rs
5. Move persistence logic → persistence.rs
6. Move hash_validator logic → hash_validator.rs
7. Update mod.rs coordinator
8. Move tests → tests.rs

### Expected Outcome

**Before:** indexer.rs (2,088 lines)  
**After:** 7 focused modules (~300 lines each) + mod.rs (~200 lines)

---

## Issue #1458 - Shrink src/lib.rs Public API (166 pub mods)

### Problem
src/lib.rs declares 166 modules, all pub. The crate is primarily a service binary, but everything is exposed as a library API, which:
- Makes clippy::pedantic docs lints noisier
- Prevents dead-code detection (rustc never warns about unused pub items)
- Invites tight coupling from tests and binaries
- Inflates binary size with unused symbols

### Current lib.rs

```rust
pub mod api_key_service;
pub mod audit_log_service;
pub mod batch_parser;
pub mod cache_layer;
pub mod certificate_pdf;
pub mod config;
pub mod database;
pub mod db_pool;
pub mod error;
pub mod event_service;
pub mod export_service;
pub mod geojson_validation;
pub mod graphql_server;
pub mod handlers;
pub mod indexer;
pub mod jwt_auth;
pub mod main;  // ← Should not be pub!
pub mod metrics;
pub mod middleware;
pub mod migrations;
pub mod ml_pipeline;
pub mod mrv_oracle;
pub mod notification_pipeline;
pub mod observability;
pub mod pagination;
pub mod partition_manager;
pub mod query_builder;
pub mod rate_limiter;
pub mod rpc_client;
pub mod saas_platform;
pub mod schema;
pub mod scheduler;
pub mod search;
pub mod server;
pub mod statistics_management;
pub mod stream_manager;
pub mod subscription_service;
pub mod telemetry;
pub mod tenant_isolation;
pub mod types;
pub mod user_service;
pub mod validation;
pub mod webhook_service;
// ... 130+ more
```

### Proposed Strategy

**Rule: Only pub if used outside src/**

1. **Core modules (stays pub):**
   - types (used by external consumers)
   - error (used by handlers)
   - config (used by binary main)

2. **Internal modules (becomes pub(crate)):**
   - indexer (only used by binary main)
   - handlers (only used by binary server)
   - middleware (only used by binary server)
   - rpc_client (only used by indexer)
   - database (only used by services)
   - migrations (only used by binary)
   - All service modules (api_key_service, etc.) - only used internally

3. **Action: Only keep 5-10 pub, rest pub(crate)**

### Implementation

```rust
// src/lib.rs - After refactoring

// === Public API (5-10 exports) ===
pub mod error;        // Used by external error handlers
pub mod types;        // Used by external consumers
pub mod config;       // Used by binary initialization

// === Internal modules (pub(crate)) ===
pub(crate) mod indexer;
pub(crate) mod handlers;
pub(crate) mod middleware;
pub(crate) mod database;
pub(crate) mod rpc_client;
pub(crate) mod services {
    pub mod api_key_service;
    pub mod audit_log_service;
    pub mod event_service;
    // ... all services inside
}
pub(crate) mod cache_layer;
pub(crate) mod observability;
// ... remaining internal modules
```

### Benefits

| Aspect | Before | After |
|--------|--------|-------|
| Public items | 166 | 10 |
| Dead-code detection | Disabled | Enabled |
| Clippy noise | High | Low |
| Binary size | Bloated | Trimmed |
| External coupling | Easy | Hard |
| Test clarity | Unclear imports | Clear internal access |

### Implementation Steps

1. **Audit Dependencies:**
   - Grep for external imports from each module
   - Identify which modules are actually used outside src/

2. **Mark Internal Modules:**
   - Change `pub mod` → `pub(crate) mod` for internal-only
   - Keep only 5-10 pub for actual public API

3. **Update Tests:**
   - Internal tests use `crate::` prefix
   - External tests import via public API only

4. **Measure Impact:**
   - Binary size reduction (expected 2-5%)
   - Compilation time (may improve dead-code detection)
   - Clippy warnings (expected 50+ fewer)

### Verification

```bash
# After changes:
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo check --lib --release
ls -lh target/release/soroban_pulse  # Check binary size
```

### Checklist

- [ ] Audit all 166 modules for external usage
- [ ] Create pub(crate) PR with majority of modules
- [ ] Keep only essential pub items
- [ ] Update all internal imports to use pub(crate)
- [ ] Update tests to use internal paths
- [ ] Verify zero clippy warnings
- [ ] Check binary size reduction
- [ ] Update documentation

## Coordination Between Issues

### #1457 (Indexer Split) → #1458 (lib.rs Cleanup)
After indexer refactoring:
- indexer/ becomes pub(crate) since only binary uses it
- Cleaner module hierarchy improves visibility

### #1455 (Config Split) → #1458 (lib.rs Cleanup)
After config refactoring:
- Only config/ stays pub (for initialization)
- Internal domain configs become pub(crate)
- Reduces public surface further

### #1456 (Handlers Split) → #1458 (lib.rs Cleanup)
After handlers refactoring:
- handlers/ becomes pub(crate) organization
- Domain handlers are internal implementation
- Only router/registration is exposed

## Recommended Order

1. **Phase 1**: #1455 (config split) - Foundation for other refactors
2. **Phase 2**: #1457 (indexer split) - Critical path for performance
3. **Phase 3**: #1456 (handlers split) - Improves maintainability
4. **Phase 4**: #1458 (lib.rs cleanup) - Final polish, reduces public API

## Final lib.rs (After All Refactors)

```rust
// === Public API ===
pub mod error;
pub mod types;

// === Re-export critical config ===
pub use config::AppConfig;

// === Everything else is internal ===
pub(crate) mod config;
pub(crate) mod handlers;
pub(crate) mod indexer;
pub(crate) mod database;
pub(crate) mod services;
// ... 155+ more modules as pub(crate)

// Binary main stays in binary crate, not library
```

Expected final count: **2-3 pub items** vs current 166 (98% reduction!)

## Expected Outcomes

✅ rustc can warn about unused pub items  
✅ Dead-code detection actually works  
✅ Clearer separation of public API vs internal implementation  
✅ Smaller binary (symbols not exported)  
✅ Fewer clippy warnings  
✅ Better encapsulation  
✅ Tests import clearly (pub vs internal)  
