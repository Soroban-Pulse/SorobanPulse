# Audit Trail for Compliance (Issue #946)

This document describes SorobanPulse's comprehensive audit trail system for regulatory compliance.

## Overview

SorobanPulse maintains an immutable, tamper-evident audit trail covering every sensitive operation. The trail is designed to satisfy requirements from SOC 2 Type II, GDPR (Article 30 records of processing), and financial industry regulations.

## Architecture

### Core Components

| Component | File | Purpose |
|---|---|---|
| Base audit logging | `src/audit_logging.rs` | Low-level log insertion and querying |
| Compliance extensions | `src/audit_trail.rs` | Chain hashing, signing, export, health reports |
| Compliance reports | `src/compliance_report.rs` | Deletion and erasure verification reports |
| DB schema | `migrations/20260626000002_audit_logs.sql` | Base `audit_logs` table |
| Compliance columns | `migrations/20260830000001_audit_trail_compliance.sql` | Chain hash, signing, retention class |

### Database Schema Extensions

The compliance migration adds to `audit_logs`:

| Column | Type | Purpose |
|---|---|---|
| `log_hash` | TEXT | SHA-256 hash of this record's immutable fields |
| `chain_hash` | TEXT | Hash chaining this record to its predecessor |
| `signed_at` | TIMESTAMPTZ | When this record was cryptographically acknowledged |
| `compliance_tags` | TEXT[] | Tags such as `gdpr`, `soc2`, `erasure` |
| `retention_class` | TEXT | `transient`, `standard`, `regulatory`, `permanent` |
| `archived_at` | TIMESTAMPTZ | When this record was archived to cold storage |

## Immutability and Tamper Detection

### Log Hashing

Every audit entry receives a `log_hash` computed from its immutable fields:

```rust
use soroban_pulse::audit_trail::{compute_log_hash, verify_log_hash, ComplianceAuditEntry};

// Compute hash when creating an entry
let mut entry = ComplianceAuditEntry { /* ... */ };
entry.log_hash = Some(compute_log_hash(&entry));

// Verify integrity later
if !verify_log_hash(&entry) {
    eprintln!("ALERT: Audit log entry {} may have been tampered with", entry.id);
}
```

### Chain Hashing

Each record's `chain_hash` incorporates the previous record's chain hash, forming a Merkle-like chain. Modifying any entry invalidates all subsequent chain hashes, making bulk tampering detectable.

```rust
use soroban_pulse::audit_trail::compute_chain_hash;

let chain_hash = compute_chain_hash(prev_chain_hash.as_deref(), &log_hash);
```

### Database-Level Immutability

Apply a policy to prevent direct updates:

```sql
-- Prevent updates to existing audit log entries
CREATE POLICY audit_logs_no_update ON audit_logs
    FOR UPDATE USING (false);

-- Only allow deletion of expired records
CREATE POLICY audit_logs_delete_expired ON audit_logs
    FOR DELETE USING (expires_at < NOW());
```

## Retention Policies

| Class | Duration | Use Case |
|---|---|---|
| `transient` | 30 days | Development and testing events |
| `standard` | 1 year | Routine operational logs |
| `regulatory` | 7 years | GDPR, financial, legal compliance |
| `permanent` | Never expires | Security incidents, key events |

Assign a retention class when creating entries:

```rust
use soroban_pulse::audit_trail::RetentionClass;

let class = RetentionClass::Regulatory;
println!("Retention: {:?} days", class.retention_days());
```

## Compliance Reporting

### Health Report

Generate a health report on the audit trail:

```rust
use soroban_pulse::audit_trail::generate_audit_trail_health_report;
use chrono::Utc;

let report = generate_audit_trail_health_report(
    &pool,
    Utc::now() - chrono::Duration::days(30),
    Utc::now(),
).await?;

println!("Total entries: {}", report.total_entries);
println!("Signed entries: {}", report.signed_entries);
println!("Coverage: {:.1}%", report.compliance_coverage_pct);
```

### Deletion Audit Report

```rust
use soroban_pulse::compliance_report::generate_deletion_audit_report;

let report = generate_deletion_audit_report(&pool, from, to).await?;
println!("Total deletions: {}", report.total_delete_events);
println!("Successful: {}", report.successful_deletions);
```

## Audit Log Export

Export audit logs for an external SIEM or auditor:

```rust
use soroban_pulse::audit_trail::{export_audit_logs, record_export, AuditSearchParams};

let params = AuditSearchParams {
    from: Some(period_start),
    to: Some(period_end),
    limit: 10_000,
    ..AuditSearchParams::new()
};

let rows = export_audit_logs(&pool, &params).await?;
record_export(&pool, "compliance-officer", "json", rows.len() as i64, None).await?;
```

## Client IP Attribution

Audit records store the originating client IP. That value is **never** read directly from request headers by handlers. Instead, every handler that writes an audit record (including the anonymize/delete handler) uses the central `ClientIp` extractor, which combines the TCP peer address from axum's `ConnectInfo` with the proxy logic below.

### How the client IP is resolved

1. The TCP peer address is taken from `ConnectInfo<SocketAddr>`.
2. If the peer is **not** in `TRUSTED_PROXIES`, any `X-Forwarded-For` or `Forwarded` header is ignored and the peer address is used. This prevents clients from spoofing their IP in audit trails, per-IP rate limits, SSE per-IP caps, and IP access control.
3. If the peer **is** a trusted proxy, the forwarding chain is parsed and the **right-most untrusted hop** is selected — i.e. the last address that is not itself a trusted proxy. Left-most values are attacker-controlled and are never trusted.

### Configuration

`TRUSTED_PROXIES` is a comma-separated list of CIDRs (IPv4 and/or IPv6). Leave it empty when the service is exposed directly to clients.

```bash
# No proxies in front of the service (default): headers are ignored
TRUSTED_PROXIES=""

# Single reverse proxy on the loopback interface
TRUSTED_PROXIES="127.0.0.1/32,::1/128"

# Private network range used by an internal load balancer
TRUSTED_PROXIES="10.0.0.0/8,172.16.0.0/12,192.168.0.0/16"
```

Only list addresses that you control and that always append to the forwarding chain. Never add `0.0.0.0/0` or `::/0`.

#### nginx ingress

nginx ingress appends the peer address to `X-Forwarded-For`. Trust the ingress controller pod/network range:

```bash
TRUSTED_PROXIES="10.244.0.0/16"
```

```nginx
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Real-IP $remote_addr;
```

#### Cloudflare

Cloudflare edge IPs must be trusted, and the origin should only be reachable through Cloudflare (use `authenticated origin pulls` or firewall rules). Keep the list in sync with <https://www.cloudflare.com/ips/>:

```bash
TRUSTED_PROXIES="173.245.48.0/20,103.21.244.0/22,103.22.200.0/22,103.31.4.0/22,141.101.64.0/18,108.162.192.0/18,190.93.240.0/20,188.114.96.0/20,197.234.240.0/22,198.41.128.0/17,162.158.0.0/15,104.16.0.0/13,104.24.0.0/14,172.64.0.0/13,131.0.72.0/22,2400:cb00::/32,2606:4700::/32,2803:f800::/32,2405:b500::/32,2405:8100::/32,2a06:98c0::/29,2c0f:f248::/32"
```

#### AWS Application Load Balancer

ALB appends the client address to `X-Forwarded-For` and sets `X-Forwarded-Proto`. Trust the ALB subnet CIDRs:

```bash
TRUSTED_PROXIES="10.0.1.0/24,10.0.2.0/24"
```

Ensure the security group only allows traffic from the ALB so clients cannot reach the service directly and bypass the trusted-proxy check.

### Tests

`src/client_ip.rs` contains tests covering:

- Spoofed `X-Forwarded-For` from an untrusted peer is ignored (peer address is used).
- Forwarded chain from a trusted proxy resolves to the right-most untrusted hop.
- Malformed or empty headers fall back to the peer address.

Run them with:

```bash
cargo test client_ip
```

## API Access

### Query Audit Logs
=======

```
GET /v1/admin/audit-logs
  ?event_type=DELETE
  &severity=CRITICAL
  &from_date=2026-01-01T00:00:00Z
  &to_date=2026-12-31T23:59:59Z
  &limit=100
  &offset=0
```

All audit log endpoints require the `ADMIN_API_KEY`.

## Verification Tests

The `src/audit_trail.rs` module includes unit tests covering:

- Retention class day calculations
- Deterministic log hash computation
- Chain hash differentiation
- Hash verification — valid entry
- Hash verification — tampered entry detection
- Hash verification — missing hash detection

Run them with:

```bash
cargo test audit_trail
```

## See Also

- [Audit Logging](audit_logging.md) — base audit logging module
- [GDPR Compliance](gdpr-compliance.md) — data subject rights
- [SOC 2 Compliance](soc2-compliance.md) — control framework
- [Data Retention Policy](data-retention.md) — retention periods
