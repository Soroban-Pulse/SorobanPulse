# SorobanPulse Threat Model

**Status:** Living document  
**Reviewed by:** _(requires two maintainer approvals before merging — see acceptance criteria)_  
**Last updated:** 2026-09-28

---

## 1. Scope

This document covers the SorobanPulse backend service and all components it directly controls:

| Component | Description |
|---|---|
| REST API (Axum) | Public HTTP endpoints, admin endpoints, SSE streams |
| Indexer | Background Tokio task polling Stellar Soroban RPC |
| PostgreSQL | Primary data store + advisory lock |
| Outbound Integrations | Webhooks, Discord, Slack, Telegram, PagerDuty, email (SMTP), SMS, GitHub |
| Admin API | Pause/resume indexer, replay, anonymize, schema/ABI management |
| Multi-tenant layer | Per-tenant RLS, API key scoping, data isolation |
| Lua scripting engine | User-supplied transform/filter scripts |
| Email subsystem | Tracking pixel, click redirect, unsubscribe, bounce endpoints |

Out of scope for this document: Stellar network / Soroban RPC infrastructure, infrastructure managed by cloud providers (VMs, managed Postgres, load balancers), CI/CD pipeline, DNS/TLS termination.

---

## 2. Data Flow Diagram

```
                  ┌──────────────────────────────────────────────────┐
                  │                TRUST BOUNDARY                    │
                  │           (Public Internet / Clients)            │
                  └──────────────┬───────────────────────────────────┘
                                 │ HTTPS
                  ┌──────────────▼────────────────┐
                  │         Load Balancer / TLS    │
                  │       (nginx / Caddy / ALB)    │
                  └──────────────┬────────────────┘
                                 │ HTTP (internal)
       ┌─────────────────────────▼──────────────────────────────┐
       │                    SorobanPulse API                     │
       │  ┌──────────────┐  ┌─────────────┐  ┌───────────────┐  │
       │  │  REST / SSE  │  │  Admin API  │  │  Metrics /    │  │
       │  │  /v1/events  │  │ /v1/admin/* │  │  /metrics     │  │
       │  └──────┬───────┘  └──────┬──────┘  └───────────────┘  │
       │         │                 │                              │
       │  ┌──────▼─────────────────▼──────────────────────────┐  │
       │  │               AppState / Handlers                  │  │
       │  │  rate_limiter · auth middleware · audit log        │  │
       │  └──────────────────────┬────────────────────────────┘  │
       │                         │                               │
       │  ┌──────────────────────▼───────────────────────────┐  │
       │  │             Indexer (background task)             │  │
       │  │   advisory lock · polling · dedup · checkpoint    │  │
       │  └──────────────────────┬───────────────────────────┘  │
       └─────────────────────────┼──────────────────────────────┘
                    TRUST BOUNDARY (backend services)
            ┌────────────────────┼──────────────────────┐
            │                    │                       │
  ┌─────────▼──────┐  ┌──────────▼──────┐  ┌───────────▼──────────┐
  │  PostgreSQL DB  │  │  Stellar RPC    │  │  Outbound Integrations│
  │  (primary +     │  │  (Soroban       │  │  webhooks, Discord,   │
  │   read replica) │  │   testnet/main) │  │  Slack, email, SMS,   │
  └─────────────────┘  └─────────────────┘  │  PagerDuty, GitHub   │
                                             └──────────────────────┘
```

**Trust boundaries:**

| # | Boundary | Notes |
|---|---|---|
| TB-1 | Public Internet → Load Balancer | TLS termination; rate limiting |
| TB-2 | Load Balancer → API | Internal HTTP only; LB enforces IP allowlisting for admin |
| TB-3 | API → PostgreSQL | Parameterised queries; RLS; advisory lock |
| TB-4 | API → Stellar RPC | Untrusted external service; response data validated before insertion |
| TB-5 | API → Outbound Integrations | User-controlled URLs; must go through SSRF guard |
| TB-6 | Internet → Email sub-endpoints | Unauthenticated by design; HMAC tokens + rate limits required |
| TB-7 | Admin API → Internal state | Separate `ADMIN_API_KEY`; audit logging |
| TB-8 | Multi-tenant → DB | Row-Level Security per tenant; key scoping |
| TB-9 | Lua sandbox → Host | Script isolation; resource limits |

---

## 3. STRIDE Analysis

### 3.1 REST API (`/v1/events`, `/v1/events/stream`, `/v1/events/tx/*`)

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| Unauthenticated bulk export of all events | **S** (Spoofing) / **I** (Info Disclosure) | `API_KEY` bearer/X-Api-Key gate when configured | API key is optional — operators may forget to set it |
| Exhausting DB connections via concurrent slow queries | **D** (Denial of Service) | `DB_MAX_CONNECTIONS`, rate limiting per IP | No per-query timeout enforced at the query layer (only `SLOW_QUERY_THRESHOLD_MS` for logging) |
| Injecting malicious `event_type` / filter values | **T** (Tampering) | SQLx parameterised queries; enum validation | Low residual risk; no gap |
| SSE stream holds connections open indefinitely | **D** | Keep-alive pings; client disconnect cleanup | No hard connection-time limit per client |
| Cursor/pagination token forgery | **E** (Elevation) | Opaque UUID cursors from DB | Cursors are not signed; a guessed cursor leaks existence |

### 3.2 Admin API (`/v1/admin/*`)

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| Regular API key used to reach admin endpoints | **E** | Separate `ADMIN_API_KEY` gate; 403 on regular key | When `ADMIN_API_KEY` is unset, falls back to `API_KEY` — documented but easy to misconfigure |
| Replay endpoint triggered to overwhelm DB | **D** | Admin key required | No rate limit on individual admin operations |
| Anonymize/GDPR endpoint forged to delete wrong tenant data | **T** | Admin key; audit log | Audit log is written after the fact; no out-of-band approval flow |
| Lua script injection via transform endpoint | **E** | Lua sandbox | Sandbox resource limits not exhaustively tested; see #1162 |

### 3.3 Indexer

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| Malicious / poisoned event data from Stellar RPC | **T** | `ON CONFLICT DO NOTHING`; XDR validation | XDR validation is partial; crafted payloads could hit edge cases |
| Split-brain with two active indexers | **D** / **T** | Advisory lock; `soroban_pulse_indexer_is_leader` metric | Lock retry is interval-based — brief dual-leader window exists at failover |
| RPC endpoint substituted with attacker-controlled server | **S** | `STELLAR_RPC_URL` is operator-configured | No certificate pinning |

### 3.4 Outbound Integrations (Webhooks, Discord, Slack, Telegram, email, SMS)

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| Webhook URL points to internal metadata endpoint (SSRF) | **S** / **I** | `validate_callback_url` + `is_ssrf_host` in `subscriptions.rs` | Only applied to subscription callbacks — Discord/Slack/generic integrations and webhook templates do **not** use the shared guard. See **#1159** |
| Webhook template injection (Handlebars/Tera code execution) | **T** | Template sandbox | Templates not validated for server-side injection at creation time |
| Forged bounces suppress legitimate recipients | **T** | None | Bounce endpoint unauthenticated; no provider-signature check. See **#1158** |
| Tracking pixel / unsubscribe token enumeration | **I** | None | Tokens are plain UUIDs with no HMAC or expiry. See **#1158** |
| Click redirect to arbitrary URL | **E** | Redirect only if token exists in DB | Destination URL recorded at send time — if the DB row is absent (token guessed) returns 404; does not currently block open redirect via DB poisoning |
| Email sender spoofed (no DMARC/SPF check on outbound) | **S** | Operator-configured SMTP | No enforcement that operator sets up DMARC; out of scope for this document |

### 3.5 Email Sub-Endpoints (Unauthenticated)

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| Token enumeration via `/unsubscribe?token=` brute-force | **I** | None | No HMAC, no expiry, no per-IP rate limit. See **#1158** |
| Forged `/notifications/email/bounce` payload adds suppressions | **T** | None | Provider signature not verified. See **#1158** |
| Log flooding via tracking pixel / click endpoint | **D** | None | No sampling on abuse logging. See **#1158** |
| Open redirect via `/click/:token` | **E** | Token must exist in DB | Destination URL not validated against SSRF guard at storage time |

### 3.6 Multi-tenancy

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| Tenant A reads Tenant B's events | **I** | RLS on `events` table; `tenant_id` on all queries | RLS policies require systematic audit to confirm coverage of new tables |
| Tenant API key scope bypass | **E** | Scoped keys enforced in middleware | Key scope validation not exercised by contract tests |

### 3.7 Lua Scripting Engine

| Threat | Category | Existing Mitigations | Gaps / Follow-up |
|---|---|---|---|
| CPU/memory exhaustion via infinite loop | **D** | Instruction count limit | No wall-clock deadline in addition to instruction count |
| File system / network access from script | **E** | Lua standard-lib stripped | `io` / `socket` modules need explicit disable audit |
| Sandbox escape via debug library | **E** | None confirmed | `debug` library should be explicitly removed |

---

## 4. High-Risk Gaps and Follow-up Issues

The following gaps are **unmitigated** or **insufficiently mitigated** at high risk. Each links to a filed GitHub issue.

| # | Gap | Risk Level | Issue |
|---|---|---|---|
| G-1 | SSRF not applied to Discord/Slack/generic integration URLs | HIGH | **#1159** |
| G-2 | Email tracking/unsubscribe/bounce tokens unsigned and unauthenticated | HIGH | **#1158** |
| G-3 | Dependency vulnerabilities in npm/pip/Go not checked on PRs | MEDIUM | **#1161** |
| G-4 | Lua sandbox: `debug` library not explicitly removed; no wall-clock deadline | MEDIUM | File follow-up issue |
| G-5 | Admin fallback to regular `API_KEY` when `ADMIN_API_KEY` unset | MEDIUM | File follow-up issue |
| G-6 | No rate limit on individual admin operations | LOW–MEDIUM | File follow-up issue |
| G-7 | Cursor tokens unsigned (existence oracle) | LOW | File follow-up issue |
| G-8 | RLS policy audit for newly added tables | MEDIUM | File follow-up issue |

---

## 5. Existing Controls Summary

| Control | Where | Notes |
|---|---|---|
| API key auth (Bearer / X-Api-Key) | Middleware | Optional; enabled by `API_KEY` env var |
| Admin API key (separate) | Middleware | `ADMIN_API_KEY`; 401/403 on wrong key |
| Rate limiting (per IP) | Middleware | `RATE_LIMIT_PER_MINUTE`; 429 on breach |
| Parameterised SQL queries | SQLx | All queries use `$N` bind params |
| Row-Level Security | PostgreSQL | Per-tenant isolation on `events` and related tables |
| SSRF guard (subscriptions) | `subscriptions.rs` | Private IP ranges, loopback, link-local blocked |
| Redirect-disabled HTTP client | `subscriptions.rs` | `build_delivery_client()` |
| HMAC webhook signatures | `webhook_signing.rs` | SHA-256 HMAC; `X-Webhook-Signature` header |
| Audit logging | `audit_logging.rs` | Admin + sensitive operations logged to `audit_logs` |
| Advisory lock (single-leader indexing) | `advisory_lock.rs` | Prevents duplicate indexing in multi-replica deploys |
| Cargo deny (advisories + licences) | CI (`ci.yml`) | Blocks Rust dependency with known CVEs |
| Cargo audit | CI (`security.yml`) | RustSec advisory database |
| Secrets scan | CI (`security.yml`, `secrets-scan.yml`) | Blocks committed credentials |
| Gitleaks | `.gitleaks.toml` | Local pre-commit hook + CI |
| Content-Security-Policy | Dashboard | Restricts script/style sources |
| TLS termination | Load Balancer | nginx / Caddy / AWS ALB (docs/deployment.md) |

---

## 6. Recommendations

1. **Centralise SSRF validation** (G-1): move `validate_callback_url` / `is_ssrf_host` / `build_delivery_client` into a new `src/net/outbound.rs` module. All user-supplied outbound URLs must call this before any HTTP request. Tracked in **#1159**.

2. **Sign email tokens** (G-2): replace bare UUID tokens with `token = HMAC-SHA256(key, base_payload) || expiry`. Validate in constant time at the handler level. Add per-IP rate limiting to the unauthenticated endpoints. Authenticate bounce webhooks with provider signatures. Tracked in **#1158**.

3. **Add dependency scanning to PR checks** (G-3): `actions/dependency-review-action` + `npm audit`, `pip-audit`, `govulncheck`. Tracked in **#1161**.

4. **Harden Lua sandbox** (G-4): explicitly `nil` out `debug`, `io`, `os`, `package`, `require` before executing any user script; add a wall-clock deadline in addition to the instruction count limit.

5. **Enforce `ADMIN_API_KEY`** (G-5): emit a startup `WARN` (or refuse to start in production) when `ADMIN_API_KEY` is unset, rather than silently falling back to `API_KEY`.

6. **Rate-limit admin operations** (G-6): add a secondary, stricter rate limit (e.g., 10 req/min) specifically on `/v1/admin/*` endpoints.

7. **Sign pagination cursors** (G-7): apply HMAC to cursor payloads to prevent existence-oracle attacks.

8. **RLS audit** (G-8): systematically verify that every table added since the initial `add_tenant_id` migration has `tenant_id` column + matching RLS policy.

---

## 7. Revision History

| Date | Author | Change |
|---|---|---|
| 2026-09-28 | Initial | First draft — covers all components as of v1.x |

---

## 8. References

- [Architecture overview](../architecture.md)
- [Security testing](../security-testing.md)
- [Zero-trust design](../zero-trust.md)
- [OWASP Top 10 coverage](../owasp_security_headers.md)
- [SSRF protection — issue #1159](https://github.com/Soroban-Pulse/SorobanPulse/issues/1159)
- [Email endpoint protection — issue #1158](https://github.com/Soroban-Pulse/SorobanPulse/issues/1158)
- [Dependency review — issue #1161](https://github.com/Soroban-Pulse/SorobanPulse/issues/1161)
