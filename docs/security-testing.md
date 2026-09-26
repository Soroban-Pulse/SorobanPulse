# Security Testing

SorobanPulse treats security as a first-class engineering concern. This document covers the full automated security testing suite: what it tests, how to run it locally, how it integrates with CI, and how to extend it.

---

## Overview

The security testing strategy is built on several complementary layers:

| Layer | Tool | Purpose |
|-------|------|---------|
| Unit/middleware tests | Rust + Axum test utilities | Verify auth, headers, and crypto invariants |
| OWASP Top 10 coverage | `tests/security/owasp_tests.rs` | Systematic coverage of the top web vulnerabilities |
| Injection tests | `tests/security/owasp_tests.rs` | SQL, NoSQL, XSS, shell injection pattern validation |
| Auth bypass tests | `tests/security/auth_bypass_tests.rs` | 30+ auth bypass scenarios |
| Crypto verification | `tests/security/crypto_tests.rs` | HMAC, key hashing, rotation, constant-time comparison |
| Regression tests | `tests/security/regression_tests.rs` | Locked-in invariants to prevent known issues from recurring |
| Dependency scanning | `cargo-audit` + `cargo-deny` | Known CVEs in dependencies, license and policy checks |
| Secrets detection | `scripts/check_secrets.sh` | Hardcoded credentials in source files |
| Unsafe code audit | `cargo-geiger` | Tracks `unsafe` code usage per crate |

The philosophy is **shift-left**: security problems are caught at the earliest possible stage, before code ever reaches production.

---

## Test Suite Structure

```
tests/
├── security.rs                  # Entry point — declares the security module
└── security/
    ├── mod.rs                   # Sub-module declarations
    ├── owasp_tests.rs           # OWASP Top 10 + SQL injection
    ├── auth_bypass_tests.rs     # Authentication/authorization bypass
    ├── crypto_tests.rs          # Cryptographic strength verification
    └── regression_tests.rs      # Security regression tests (REG-NNN)

scripts/
└── check_secrets.sh             # Secrets/credential detection script

.github/workflows/
└── security.yml                 # Dedicated security CI workflow
```

---

## OWASP Top 10 Coverage

Each OWASP risk category maps to specific test functions in `tests/security/owasp_tests.rs`.

| OWASP ID | Risk Category | Test Coverage |
|----------|---------------|---------------|
| A01 | Broken Access Control | Admin endpoint requires correct key; wrong key → 403 not 200; no-key → 401; `PolicyEvaluator` default-denies |
| A02 | Cryptographic Failures | HMAC-SHA256 is 64 hex chars; different secrets produce different sigs; API keys are hashed (SHA-256) before storage |
| A03 | Injection | SQL injection strings, NoSQL patterns, XSS payloads, path traversal, null bytes, URL-encoded injection — all fail contract ID and tx hash validators |
| A04 | Insecure Design | Error responses have exactly one JSON field (`error`); no key hints in 401 bodies |
| A05 | Security Misconfiguration | All 7 OWASP headers present; strict CSP on API routes; HSTS ≥ 1 year with `includeSubDomains`; Permissions-Policy disables camera/mic/geo/payment |
| A06 | Vulnerable Components | Covered by `cargo-audit` and `cargo-deny` (see CI workflow) |
| A07 | Authentication Failures | Missing header → 401; malformed Bearer → 401; empty key → 401; valid health endpoints bypass auth; both headers present → Bearer wins |
| A08 | Software/Data Integrity | HMAC verify roundtrip; stale timestamp → verify fails; tampered signature → fails; wrong path → fails |
| A09 | Logging Failures | `AccessLogger` captures both Allow and Deny decisions; filters by key hash; respects capacity limits |
| A10 | SSRF | Webhook URL validator accepts only `http://` and `https://`; rejects `file://`, `gopher://`, `ldap://`, `ftp://`, etc. |

---

## SQL Injection Testing

SorobanPulse uses SQLx with **parameterized queries** throughout, which prevents SQL injection at the database layer. The security tests verify the defensive validation layer above that.

### How parameterized queries protect us

All database queries use SQLx bind parameters:

```rust
// SAFE — parameter bound, never interpolated as SQL
sqlx::query_as::<_, Event>("SELECT * FROM events WHERE contract_id = $1")
    .bind(contract_id)
    .fetch_all(&pool)
    .await
```

The SQL string is fixed at compile time. User input is always passed as a typed binding, so it can never alter the query structure.

### What the tests verify

The injection tests operate at the **input validation layer** — they ensure that the validators used before data reaches the database correctly reject malicious inputs:

```
tests/security/owasp_tests.rs::a03_sql_injection_patterns_fail_contract_id_validation
tests/security/owasp_tests.rs::a03_nosql_injection_patterns_fail_contract_id_validation
tests/security/owasp_tests.rs::a03_path_traversal_fails_contract_id_validation
tests/security/owasp_tests.rs::a03_xss_payloads_fail_contract_id_validation
tests/security/owasp_tests.rs::a03_null_byte_injection_fails_contract_id_validation
```

The contract ID validator enforces the Stellar format:

```rust
fn is_valid_contract_id(id: &str) -> bool {
    id.len() == 56
        && id.starts_with('C')
        && id.chars().all(|c| c.is_ascii_alphanumeric())
}
```

The tx hash validator enforces hex format:

```rust
fn is_valid_tx_hash(hash: &str) -> bool {
    hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit())
}
```

These reject all forms of injection: SQL, NoSQL, template, shell, XSS, path traversal, null bytes, and URL-encoded variants.

---

## Authentication & Authorization Tests

`tests/security/auth_bypass_tests.rs` covers 30+ bypass scenarios across three test categories.

### Open API behaviour

When `API_KEY` is unset, all requests pass through. This is tested explicitly:

```rust
#[tokio::test]
async fn no_keys_configured_all_requests_pass() { ... }
```

### Two-layer auth model

SorobanPulse has a two-layer auth model:

1. **Global auth gate** (`auth_middleware`) — guards all non-public endpoints
2. **Admin auth gate** (`admin_auth_middleware`) — additionally guards `/v1/admin/*`

Key invariants tested:

| Scenario | Expected status |
|----------|----------------|
| No key, auth configured | 401 |
| Wrong key, auth configured | 401 |
| Valid regular key | 200 |
| Valid admin key at regular gate | 200 (admin keys are also valid regular keys) |
| No key at admin endpoint | 401 |
| Regular key at admin endpoint | 403 (authenticated but lacks privilege) |
| Admin key at admin endpoint | 200 |
| No admin keys configured | admin layer is no-op (backward compat) |

### Test helper pattern

```rust
fn auth_app(api_keys: Vec<String>) -> Router {
    let state = Arc::new(AuthState {
        api_keys,
        admin_api_keys: vec![],
        tenant_map: Arc::new(HashMap::new()),
        multi_tenant: false,
    });
    Router::new()
        .route("/test", get(|| async { "OK" }))
        .route_layer(axum::middleware::from_fn_with_state(state, auth_middleware))
}

// Usage
let resp = auth_app(vec!["my-key".into()])
    .oneshot(Request::get("/test").header("X-Api-Key", "my-key").body(Body::empty()).unwrap())
    .await.unwrap();
assert_eq!(resp.status(), StatusCode::OK);
```

### Edge cases covered

- Empty API key string → 401
- Key with null byte suffix (`"valid\x00extra"`) → 401 (doesn't match `"valid"`)
- Key with CRLF (`"key\r\nX-Injected: evil"`) → 401
- Very long key (100,000 chars) → 401 without panicking
- Key that is a prefix of valid key → 401
- Key that is a suffix of valid key → 401
- Unicode homoglyphs (Cyrillic 'а' vs ASCII 'a') → 401
- Precomposed vs decomposed Unicode forms → 401

---

## Cryptographic Verification

`tests/security/crypto_tests.rs` verifies the properties of every cryptographic primitive.

### HMAC-SHA256 Request Signing

The zero-trust layer (`src/zero_trust.rs`) signs requests using HMAC-SHA256. The canonical payload is:

```
METHOD\nPATH\nTIMESTAMP\nBODY
```

Properties verified by tests:

- Output is exactly 64 lowercase hex characters (SHA-256 = 32 bytes)
- Deterministic: same inputs always produce the same signature
- Different secrets → different signatures
- Different bodies → different signatures
- Different paths → different signatures (path included in canonical payload)
- Different methods → different signatures
- Stale timestamp → `verify()` rejects even if HMAC is valid
- Far-future timestamp → rejected
- Unparseable timestamp → rejected
- Tampered signature (all-zeros, all-f) → rejected

### Timestamp Freshness

The `verify()` function checks RFC3339 timestamp freshness with a 5-minute window (300 seconds). Tests exercise the boundary conditions:

```
now               → accepted
4 minutes ago     → accepted (within 5-minute window)
6 minutes ago     → rejected (beyond window)
10 minutes ago    → always rejected
1 hour in future  → rejected
"not-a-date"      → rejected (parse error)
```

### Webhook HMAC-SHA256

The webhook delivery system signs payloads using `X-Signature-256: sha256=<hex>`. Tests verify:

- Roundtrip: `sign_webhook()` followed by `verify_webhook_hmac()` succeeds
- Wrong secret → fails
- Tampered body → fails
- Missing `sha256=` prefix → fails
- Wrong algorithm prefix (`md5=`, `sha1=`) → fails
- Empty body → has a valid signature (edge case handled)

### API Key Hashing

API keys are hashed with SHA-256 before storage/comparison:

- SHA-256 of empty string matches the known digest (`e3b0c4...`)
- Different keys produce different hashes
- Hash never contains the plaintext key

### Key Rotation

`ApiKeySet` supports seamless rotation with a grace period:

```
State after new("key-1"):                  valid: key-1
State after rotate("key-2"):              valid: key-1, key-2
State after rotate("key-3"):              valid: key-2, key-3 (key-1 evicted)
```

### Constant-Time Comparison

`key_matches_any()` uses the `subtle` crate for constant-time comparison to prevent timing-based key enumeration. Tests verify:

- Different-length inputs don't panic
- Near-miss keys (1 char different) return false
- Empty candidate list returns false
- Unicode homoglyphs don't confuse comparison

---

## Dependency Vulnerability Scanning

### cargo-audit

[cargo-audit](https://crates.io/crates/cargo-audit) checks `Cargo.lock` against the [RustSec Advisory Database](https://rustsec.org).

**Run locally:**
```bash
make audit
# or directly:
cargo audit
```

**Interpret results:**

- `error[vulnerability]` — a known CVE. Upgrade the affected crate or add an exception in `audit.toml` with a justification.
- `warning[yanked]` — the pinned version has been yanked. Upgrade to a non-yanked version.
- `warning[unsound]` — the crate has a known unsoundness. Evaluate impact and upgrade or isolate.

**Adding an exception** (use sparingly and document the reason):

```toml
# audit.toml
[ignore]
RUSTSEC-2023-XXXX = { reason = "Not exploitable in our usage: we do not expose X to untrusted input" }
```

### cargo-deny

[cargo-deny](https://embarkstudios.github.io/cargo-deny/) enforces four policy domains, configured in `deny.toml`:

| Check | Purpose |
|-------|---------|
| `advisories` | Same CVE database as cargo-audit |
| `bans` | Prevent banned crates from being included |
| `licenses` | Only allow approved SPDX licenses |
| `sources` | Only allow crates from crates.io (no arbitrary git sources) |

**Run locally:**
```bash
make deny
# or individually:
cargo deny check advisories
cargo deny check bans
cargo deny check licenses
cargo deny check sources
```

**Allowed licenses** (from `deny.toml`):
MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Unicode, Zlib, CC0-1.0

**Updating deny.toml:**
If a new dependency introduces an advisory you've reviewed and accepted, or uses a license not in the allow list, update `deny.toml` with a documented justification.

---

## Secrets Detection

`scripts/check_secrets.sh` scans the entire codebase for 15 categories of hardcoded secrets:

| Pattern | Severity |
|---------|----------|
| Hardcoded password literals | FAIL |
| Hardcoded API key literals | FAIL |
| AWS Access Key ID (`AKIA...`) | FAIL |
| AWS Secret Access Key value | FAIL |
| Private key PEM block | FAIL |
| JWT secret literal | FAIL |
| GCP service account key | FAIL |
| GitHub token pattern | FAIL |
| Database URL with embedded credentials | WARN |
| Hardcoded Bearer token | WARN |
| Long hex value labeled as secret/key | WARN |
| Long base64 value labeled as secret/key | WARN |
| Slack webhook URL | WARN |
| High-entropy password in env assignment | WARN |
| `.env` file tracked by git | FAIL |

**Run locally:**
```bash
make check-secrets
# or directly:
bash scripts/check_secrets.sh
```

**Suppressing false positives:**
Add `# nocheck-secrets` as a trailing comment on the offending line:

```rust
const TEST_VECTOR: &str = "deadbeef0123456789abcdef"; // nocheck-secrets
```

**What this script does NOT cover:**
- Binary files
- Secrets committed in git history (use `git-secrets` or `truffleHog` for history scanning)
- Dynamic secret construction at runtime
- Secrets passed as environment variables (those are correct — use env vars!)

For full historical scanning, run `truffleHog` against the repository:
```bash
pip install trufflehog
trufflehog filesystem . --exclude-paths .gitignore
```

---

## Security Regression Tests

`tests/security/regression_tests.rs` contains tests that lock in specific security invariants. Each test is named `REG-NNN` and documents what vulnerability it prevents.

### Naming convention

```
REG-001 — Timing attack prevention
REG-002 — Header injection prevention
REG-003 — Path traversal / input validation
REG-004 — HMAC replay prevention
REG-005 — Admin privilege escalation prevention
REG-006 — API key enumeration resistance
REG-007 — Error information leakage prevention
REG-008 — Null byte injection prevention
REG-009 — Unicode normalisation attacks
REG-010 — Input format enforcement
REG-011 — Security header completeness
REG-012 — Access log audit trail
```

### Adding a new regression test

When you fix a security issue, add a regression test before closing the PR:

1. Identify the next available REG number.
2. Create a test function named `reg_NNN_short_description_of_the_fix`.
3. Add a doc comment explaining:
   - What vulnerability was fixed.
   - What the test checks.
   - How the system behaved before the fix.
4. Keep the test self-contained: no database, no network.

Example:

```rust
/// REG-013: Empty body must not bypass HMAC verification.
///
/// Prior to fixing issue #XXX, a request with an empty body and a valid
/// non-empty-body signature would sometimes pass verification due to an
/// off-by-one error in the payload construction.
#[test]
fn reg_013_empty_body_not_verified_with_non_empty_signature() {
    let ts = Utc::now().to_rfc3339();
    let sig = RequestSignature::sign("key", "POST", "/path", &ts, "non-empty");
    assert!(!RequestSignature::verify("key", "POST", "/path", &ts, "", &sig));
}
```

---

## CI Integration

The `security.yml` workflow runs automatically:

- On every push to `main`
- On every pull request
- Every Sunday at 02:00 UTC (scheduled vulnerability scan)
- Manually via `workflow_dispatch`

### Jobs

| Job | Blocking | Description |
|-----|----------|-------------|
| `cargo-audit` | Yes | RustSec advisory database scan |
| `cargo-deny` | Yes | License, ban, source policy enforcement |
| `security-tests` | Yes | Full security test suite |
| `secrets-scan` | Yes | Hardcoded credential detection |
| `unsafe-audit` | No (advisory) | cargo-geiger unsafe code report |
| `clippy-security` | No (advisory) | Security-relevant clippy lints |
| `sbom` | No | SBOM generation (weekly only) |

Blocking jobs must pass for a PR to be merged. Advisory jobs produce reports as CI artifacts for review.

### Integration with the main CI pipeline

The main `.github/workflows/ci.yml` already runs `cargo-deny`. The `security.yml` workflow provides deeper coverage:

- `cargo-audit` (in addition to `cargo-deny`'s advisories check)
- The full `tests/security` module
- Secrets scanning
- Unsafe code reporting

---

## Running Tests Locally

### All security tests

```bash
make security
```

This runs: `cargo-deny` + `cargo-audit` + security test suite + secrets scan.

### Just the test suite

```bash
# All security sub-modules
make security-tests
# or:
cargo test --test security -- --nocapture

# Specific sub-module
cargo test --test security owasp_tests -- --nocapture
cargo test --test security auth_bypass_tests -- --nocapture
cargo test --test security crypto_tests -- --nocapture
cargo test --test security regression_tests -- --nocapture

# Specific test function
cargo test --test security a01_admin_no_key_returns_401 -- --nocapture
```

### Dependency scanning

```bash
# cargo-deny (fast, policy-based)
make deny

# cargo-audit (slower, advisory database)
make audit
```

### Secrets detection

```bash
make check-secrets
```

### Unsafe code audit

```bash
make geiger
cat geiger-report.txt
```

---

## Adding New Security Tests

When adding tests, follow these guidelines:

**1. No database dependency.** Security tests must run without `DATABASE_URL`. Use Axum's in-memory routing and tower's `oneshot()`.

**2. Use the correct timestamp format.** `RequestSignature::sign()` and `verify()` take RFC3339 strings:

```rust
let ts = Utc::now().to_rfc3339();
let sig = RequestSignature::sign("secret", "GET", "/path", &ts, "body");
```

**3. Use `RequestContext::new()` for the 4-arg constructor:**

```rust
let ctx = RequestContext::new("10.0.0.1", "key-hash", "/v1/events", "GET");
```

**4. `AccessLogger::record()` takes owned values:**

```rust
logger.record(ctx, AccessDecision::Allow);  // ctx is moved, not borrowed
```

**5. Name tests with their OWASP ID or REG number for easy triage:**

```
a01_admin_no_key_returns_401       → OWASP A01
reg_005_a_regular_key_gets_403     → Regression REG-005
```

---

## Threat Model Summary

### What this test suite covers

- Authentication bypass via missing/malformed/wrong credentials
- Authorisation escalation (regular key accessing admin endpoints)
- SQL/NoSQL injection in URL parameters (contract ID, tx hash)
- XSS and script injection in URL parameters
- Path traversal in URL parameters
- Header injection (CRLF, null bytes)
- Timing attacks via constant-time comparison enforcement
- HMAC replay attacks via timestamp freshness validation
- Key enumeration via consistent 401 response codes
- Information leakage in error responses
- SSRF via webhook URL scheme validation
- Cryptographic weaknesses (algorithm strength, key length)
- Hardcoded secrets in source code
- Known CVEs in dependencies (cargo-audit)
- Unsafe code tracking (cargo-geiger)
- Security header misconfiguration (CSP, HSTS, X-Frame-Options, etc.)

### What this test suite does NOT cover

- Runtime SSRF via DNS rebinding
- Business logic vulnerabilities (access control on data scope)
- Race conditions in concurrent request handling
- Secrets stored in environment variables (infrastructure concern)
- Secrets committed in git history (use `truffleHog` for history scanning)
- Application-layer DoS (covered by load tests)
- TLS configuration (covered by infrastructure/deployment docs)
- Binary/compiled artefact supply chain integrity

For production deployments, complement this test suite with:
- `truffleHog` for git history scanning
- DAST tools (OWASP ZAP, Burp Suite) for running-application testing
- Penetration testing for high-value deployments
- `cargo-fuzz` for input validation fuzzing (see the `fuzz/` directory)
