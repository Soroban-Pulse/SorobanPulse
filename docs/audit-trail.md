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

## API Access

### Query Audit Logs

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
