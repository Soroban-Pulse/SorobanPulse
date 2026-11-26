# Issues #1460 & #1461 - Cargo Workspace + Dependency Upgrades

## Issue #1460 - Convert to Cargo Workspace

### Problem

cli/ and fuzz/ are separate crates with own Cargo.lock files, not workspace members. This causes:
- Duplicate builds in CI
- Version skew (different versions of shared dependencies)
- Separate cargo deny/audit runs
- Difficult to manage monorepo as one system

### Current State

```
SorobanPulse/
├── Cargo.toml (no [workspace])
├── Cargo.lock
├── src/
├── cli/
│   ├── Cargo.toml (standalone)
│   ├── Cargo.lock (independent)
│   └── src/
└── fuzz/
    ├── Cargo.toml
    └── src/
```

### Proposed Structure

```
SorobanPulse/
├── Cargo.toml (root workspace)
├── Cargo.lock (single, unified)
├── Cargo.toml (server crate)
├── src/ (server code)
├── cli/Cargo.toml
├── cli/src/
├── fuzz/Cargo.toml (optional, can stay independent)
└── docs/cargo-workspace-plan.md
```

### Implementation Steps

#### Phase 1 - Create Workspace Structure (PR 1)
1. Add [workspace] to root Cargo.toml:
```toml
[workspace]
members = [".", "cli"]
# fuzz stays independent (optional)

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Young850"]

[workspace.dependencies]
tokio = { version = "1.40", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
axum = "0.8"
sqlx = { version = "0.8", features = ["runtime-tokio-native-tls", "postgres"] }
reqwest = "0.12"
http = "1.0"
tower = "0.4"
tower-http = "0.6"
# ... other shared crates
```

2. Update cli/Cargo.toml to use workspace:
```toml
[package]
name = "soroban-pulse-cli"
version.workspace = true
edition.workspace = true
authors.workspace = true

[dependencies]
soroban-pulse = { path = ".." }
tokio = { workspace = true }
clap = "4.5"
```

3. Remove duplicate Cargo.lock from cli/

#### Phase 2 - Merge Dependencies (PR 2)
1. Update all workspace members to use [workspace.dependencies]
2. Remove [workspace] sections from cli/
3. Verify single Cargo.lock is generated
4. Test: `cargo build --workspace`

#### Phase 3 - CI Updates (PR 3)
1. Update CI to use single cache key (not per-crate)
2. Update cargo deny to run once
3. Update cargo audit to run once
4. Speed up: `cargo build -p soroban-pulse` only

#### Optional Phase 4 - Further Split (PR 4)
If docs/cargo-workspace-plan.md suggests:
```
├── crates/
│   ├── core/           (domain + DB)
│   ├── server/         (axum handlers)
│   ├── cli/            (CLI)
│   └── fuzz/           (fuzzing)
```

This requires:
- Moving src/ → crates/core/src/
- Moving core types to crates/core
- Moving handlers to crates/server
- Updating workspace members

---

## Issue #1461 - Upgrade Outdated Core Dependencies

### Problem

Several core dependencies are 1+ major versions behind, causing:
- Incompatibility with axum 0.8 (uses http 1.x)
- Version conflicts in middleware
- Advisory exposure (unmaintained dotenv)
- Missed security fixes

### Current Outdated Versions

| Crate | Current | Latest | Gap | Impact |
|-------|---------|--------|-----|--------|
| dotenv | 0.15 | unmaintained | RUSTSEC-2021-0141 | **SECURITY: unmaintained** |
| dotenvy | (not used in CLI) | 0.15 | — | CLI already uses it correctly |
| reqwest | 0.11 | 0.12 | 1 major | http 1.x incompatibility |
| tower-http | 0.5 | 0.6 | 1 major | Pairs with axum 0.8 |
| sqlx | 0.7 | 0.8 | 1 major | #1347 needs query! macros |
| base64 | 0.21 | 0.22 | 1 minor | Deprecation warnings |
| config | 0.13 | 0.14 | 1 minor | Various fixes |
| opentelemetry | 0.21 | 0.23 | 2 minor | Performance improvements |

### Upgrade Strategy

#### Phase 1 - Security First (PR 1)
```bash
# Replace dotenv everywhere
# src/main.rs - Already calls dotenvy::dotenv() ✅
# src/bin/seed.rs - Change: dotenv::dotenv() → dotenvy::dotenv()
```

Dependencies: `dotenvy = "0.15"`

#### Phase 2 - HTTP Stack (PR 2)
```toml
# Update together (axum 0.8 requires these)
axum = "0.8"
http = "1.0"
hyper = "1.0"
tower = "0.4"
tower-http = "0.6"
```

Impact: Updates middleware, CORS handling, path params from :param → {param}

#### Phase 3 - Database & Config (PR 3)
```toml
sqlx = { version = "0.8", features = ["runtime-tokio-native-tls", "postgres"] }
config = "0.14"
base64 = "0.22"
```

Enables: query! macros for compile-time SQL validation

#### Phase 4 - HTTP Client (PR 4)
```toml
reqwest = "0.12"  # Uses http 1.x, hyper 1.x
```

Pairs with: tower-http 0.6, http 1.0

#### Phase 5 - Observability (PR 5)
```toml
opentelemetry = "0.23"
opentelemetry-jaeger = "0.22"
# Update entire OpenTelemetry ecosystem
```

#### Phase 6 - Testing & Validation (PR 6)
1. Run full test suite
2. Verify no breaking changes in routes
3. Check middleware behavior (CORS, auth)
4. Test database queries
5. Performance benchmark

### Version Conflict Resolution

**Before Upgrade:**
```
axum 0.7 → depends on http 0.2
tower-http 0.5 → depends on http 0.2
reqwest 0.11 → depends on hyper 0.14 (http 0.2)
```

**After Upgrade:**
```
axum 0.8 → depends on http 1.0
tower-http 0.6 → depends on http 1.0
reqwest 0.12 → depends on hyper 1.0 (http 1.0)
# All aligned!
```

### Breaking Changes to Address

1. **Route Parameters:** `:param` → `{param}` in Path extractors
2. **CORS Handling:** tower-http CORS API updated
3. **Middleware Ordering:** Layer trait changes
4. **Status Codes:** http::StatusCode moved namespace

### Cargo.toml After All Upgrades

```toml
[package]
name = "soroban-pulse"
version = "0.1.0"
edition = "2021"

[dependencies]
# Web framework (HTTP 1.x stack)
axum = "0.8"
http = "1.0"
hyper = "1.0"
tower = "0.4"
tower-http = "0.6"

# HTTP client
reqwest = "0.12"

# Database
sqlx = { version = "0.8", features = ["runtime-tokio-native-tls", "postgres", "macros"] }

# Config
config = "0.14"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
base64 = "0.22"

# Runtime
tokio = { version = "1.40", features = ["full"] }

# Environment
dotenvy = "0.15"

# Observability
opentelemetry = "0.23"
opentelemetry-jaeger = "0.22"
opentelemetry-prometheus = "0.15"

# (rest of deps...)
```

### Checklist

- [ ] Phase 1 PR: Security (dotenv → dotenvy)
- [ ] Phase 2 PR: HTTP Stack (axum 0.8, tower-http 0.6)
- [ ] Phase 3 PR: Database (sqlx 0.8, config 0.14)
- [ ] Phase 4 PR: HTTP Client (reqwest 0.12)
- [ ] Phase 5 PR: Observability (OpenTelemetry 0.23+)
- [ ] Phase 6 PR: Testing & validation
- [ ] All routes compile without errors
- [ ] All tests pass
- [ ] Performance benchmarks unchanged

### Setup Automated Updates

After upgrades, configure tooling to prevent drift:

```toml
# renovate.json or .dependabot
{
  "extends": ["config:base"],
  "grouping": {
    "httpStack": ["axum", "http", "hyper", "tower", "tower-http"],
    "database": ["sqlx", "tokio-postgres"],
    "observability": ["opentelemetry*"],
    "otel": ["opentelemetry"]
  },
  "minimumReleaseAge": "7 days",
  "schedule": ["after 10pm every weekday"]
}
```

## Coordination

#1460 (workspace) and #1461 (dependency upgrades) should be done together:
1. Create workspace first (simpler review)
2. Then upgrade dependencies (larger PR)
3. CI improvements after both are merged

## Benefits

✅ Single dependency resolution  
✅ No version skew between crates  
✅ Security fixes applied uniformly  
✅ Compile-time SQL validation (sqlx 0.8)  
✅ HTTP 1.x compatibility (axum 0.8)  
✅ Easier to add new workspace members  
✅ Single Cargo.lock file  
✅ Faster CI builds (single cache)  
