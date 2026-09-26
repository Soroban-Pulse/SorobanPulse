# SOC 2 Compliance Checklist (Issue #944)

This document describes SorobanPulse's SOC 2 Type II compliance controls across all five Trust Service Criteria (TSC).

## Trust Service Criteria Coverage

### CC1 — Control Environment

| Control | Status | Evidence |
|---|---|---|
| Written security policies | ✅ | This document and linked docs |
| Roles and responsibilities defined | ✅ | `ADMIN_API_KEY` separation, RBAC in `src/middleware/auth.rs` |
| Background checks for privileged users | 📋 Operator responsibility | HR process documentation |
| Security awareness training | 📋 Operator responsibility | Training records |
| Code of conduct | 📋 Operator responsibility | Internal policy document |

### CC2 — Communication and Information

| Control | Status | Evidence |
|---|---|---|
| Incident response procedures | ✅ | `docs/runbooks/operator-runbook.md` |
| Change management communication | ✅ | `CHANGELOG.md`, PR process in `CONTRIBUTING.md` |
| Monitoring and alerting | ✅ | `docs/alerts.yml`, Grafana dashboards |
| Audit logging of all admin actions | ✅ | `src/audit_logging.rs`, `src/audit_trail.rs` |

### CC3 — Risk Assessment

| Control | Status | Evidence |
|---|---|---|
| Risk register maintained | 📋 Operator responsibility | Internal risk register |
| Vulnerability management | ✅ | `cargo audit`, `deny.toml`, Dependabot |
| Penetration testing schedule | 📋 Operator responsibility | Annual pentest |
| OWASP security headers | ✅ | `src/middleware/security_headers.rs`, `docs/owasp_security_headers.md` |

### CC4 — Monitoring Activities

| Control | Status | Evidence |
|---|---|---|
| Continuous monitoring | ✅ | Prometheus + Grafana (`docs/grafana-dashboard.json`) |
| SLO tracking | ✅ | `src/slo_tracker.rs`, `docs/sli-slo.md` |
| Anomaly detection | ✅ | `src/anomaly_detection.rs` |
| Audit log review procedures | ✅ | `docs/audit_logging.md`, `docs/audit-trail.md` |
| Log retention ≥ 1 year | ✅ | `RetentionClass::Standard` = 365 days |

### CC5 — Control Activities

| Control | Status | Evidence |
|---|---|---|
| Least privilege access | ✅ | `API_KEY` vs `ADMIN_API_KEY` separation |
| Multi-factor authentication | 📋 Operator responsibility | Identity provider configuration |
| Secrets management | ✅ | `docs/secret-management.md`, `.gitleaks.toml` |
| Encryption at rest | ✅ | `src/encryption.rs`, `docs/encryption.md` |
| Encryption in transit | ✅ | TLS required; `docs/deployment.md` |
| Zero-trust networking | ✅ | `src/zero_trust.rs` |

### CC6 — Logical and Physical Access

| Control | Status | Evidence |
|---|---|---|
| Authentication required for API | ✅ | `src/middleware/auth.rs` |
| Admin endpoint separation | ✅ | `/v1/admin/*` requires `ADMIN_API_KEY` |
| Rate limiting | ✅ | `src/rate_limiter.rs` |
| Session / API key management | ✅ | Key hashing in `src/audit_logging.rs` |
| Access reviews | 📋 Operator responsibility | Quarterly access review |
| Privileged access monitoring | ✅ | All admin calls in `audit_logs` |

### CC7 — System Operations

| Control | Status | Evidence |
|---|---|---|
| Incident response runbook | ✅ | `docs/runbooks/operator-runbook.md` |
| Backup procedures | ✅ | `scripts/backup.sh`, `docs/disaster-recovery.md` |
| Backup verification | ✅ | `src/backup_verification.rs`, `.github/workflows/backup-verify.yml` |
| Disaster recovery plan | ✅ | `docs/disaster-recovery.md` |
| Change management | ✅ | Git-based, PRs required, CI enforced |
| Patch management | ✅ | Dependabot + `deny.toml` |

### CC8 — Change Management

| Control | Status | Evidence |
|---|---|---|
| All changes via pull requests | ✅ | `CONTRIBUTING.md` |
| CI/CD gates | ✅ | `.github/workflows/ci.yml` |
| Database migration review | ✅ | `migrations/` directory, reviewed in PRs |
| Rollback procedures | ✅ | `.down.sql` for every migration |
| Approval workflow | 📋 Operator responsibility | Branch protection rules |

### CC9 — Risk Mitigation

| Control | Status | Evidence |
|---|---|---|
| Vendor risk assessment | 📋 Operator responsibility | Third-party risk register |
| DPA with processors | 📋 Operator responsibility | Contracts with AWS/GCP/SMTP |
| Business continuity plan | ✅ | `docs/disaster-recovery.md`, `docs/multi-deployment-architecture.md` |

## Availability Criteria (A Series)

| Control | Status | Evidence |
|---|---|---|
| Uptime SLOs defined | ✅ | `docs/sli-slo.md` |
| Health checks | ✅ | `/healthz/live`, `/healthz/ready` |
| Auto-scaling | ✅ | `k8s/hpa.yaml`, `terraform/` |
| Multi-region deployment | ✅ | `docs/multi-deployment-architecture.md` |
| Load testing | ✅ | `tests/load/`, `docs/load-testing-runbook.md` |

## Confidentiality Criteria (C Series)

| Control | Status | Evidence |
|---|---|---|
| Data classification policy | ✅ | `docs/data-retention.md` |
| Encryption at rest | ✅ | `src/encryption.rs` |
| Encryption in transit | ✅ | TLS documentation in `docs/deployment.md` |
| Data masking / anonymization | ✅ | `src/anonymization.rs`, `bin/mask_event_data.rs` |
| Secrets scanning | ✅ | `docs/secret-management.md`, `.gitleaks.toml` |

## Privacy Criteria (P Series)

| Control | Status | Evidence |
|---|---|---|
| Privacy policy | 📋 Operator responsibility | External privacy policy document |
| Consent management | ✅ | `src/gdpr.rs`, `migrations/20260830000002_gdpr_consent_tracking.sql` |
| Data subject rights | ✅ | `src/gdpr.rs`, `docs/gdpr-compliance.md` |
| Breach notification | ✅ | `src/gdpr.rs::record_breach()` |
| Data retention enforcement | ✅ | `src/pruner.rs`, `src/archiver.rs` |

## Audit Checklist

Use this checklist when preparing for a SOC 2 audit:

### Pre-Audit Preparation

- [ ] Collect and organise audit log exports for the audit period
- [ ] Run `generate_audit_trail_health_report()` and review coverage percentage
- [ ] Verify all migrations have been applied (`sqlx migrate info`)
- [ ] Confirm backup verification workflows passed for the audit period
- [ ] Review and resolve any overdue DSRs
- [ ] Ensure Grafana dashboards and Prometheus alerts are operational
- [ ] Verify `.gitleaks.toml` rules are current and CI scan is passing
- [ ] Confirm all dependencies pass `cargo audit` / `cargo deny check`

### Evidence Collection

- [ ] CI/CD pipeline run history (90+ days)
- [ ] Audit log exports (full audit period)
- [ ] Backup verification reports
- [ ] Access control review records
- [ ] Incident / security event log
- [ ] DSR completion records
- [ ] Vulnerability scan reports
- [ ] Penetration test report (if applicable)

### Incident Response Procedures

1. **Detect** — Prometheus alert or user report triggers an incident
2. **Classify** — Assign severity (P1–P4) per operator runbook
3. **Contain** — Follow runbook for affected component
4. **Eradicate** — Remove root cause
5. **Recover** — Restore service, verify health checks
6. **Post-mortem** — Document timeline, root cause, and corrective actions
7. **Audit** — Ensure all steps are reflected in audit logs

## Operator Responsibilities

Items marked `📋 Operator responsibility` above require the deploying organisation to:

1. Establish and document internal policies
2. Train personnel on relevant procedures
3. Maintain evidence records for auditors
4. Configure external systems (identity provider, cloud security)

## See Also

- [Audit Trail](audit-trail.md)
- [GDPR Compliance](gdpr-compliance.md)
- [Secret Management](secret-management.md)
- [Deployment Security](deployment.md)
- [OWASP Security Headers](owasp_security_headers.md)
- [Image Security](image-security.md)
