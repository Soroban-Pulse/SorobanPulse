# Secret Management and Secrets Scanning (Issue #943)

This document covers SorobanPulse's approach to managing secrets and preventing them from being committed to the repository.

## What Counts as a Secret

| Type | Examples |
|---|---|
| Database credentials | `DATABASE_URL`, PostgreSQL passwords |
| API keys | `API_KEY`, `ADMIN_API_KEY`, third-party API keys |
| Webhook secrets | HMAC signing keys |
| Cloud credentials | AWS access keys, GCP service account keys |
| TLS private keys | `*.pem`, `*.key` files |
| SMTP credentials | Email provider passwords or tokens |
| Encryption keys | `ENCRYPTION_KEY`, `KEK_*` variables |

## Secret Storage

### Development

Copy `.env.example` to `.env` and fill in real values. The `.env` file is gitignored.

```bash
cp .env.example .env
# Edit .env with your real credentials
```

**Never commit `.env` files.** The `.gitignore` explicitly excludes `.env` and `.env.*` (except the `*.example` templates).

### Production

Use a secrets manager rather than environment variables in CI/CD:

| Platform | Recommended Tool |
|---|---|
| AWS | AWS Secrets Manager or SSM Parameter Store |
| GCP | Secret Manager |
| Kubernetes | Kubernetes Secrets (external-secrets-operator recommended) |
| GitHub Actions | GitHub Encrypted Secrets |

## Secret Rotation Procedure

When a secret is exposed or rotated:

1. **Revoke immediately** — invalidate the exposed secret at the provider.
2. **Generate a new secret** — use a cryptographically secure generator.
3. **Update the secrets manager** — deploy the new secret to all environments.
4. **Rotate zero-downtime** — use `ADMIN_API_KEY_SECONDARY` for API key rotation without downtime.
5. **Audit log the rotation** — create an `AUTH_CHANGE` audit entry with `AuditSeverity::Critical`.
6. **Verify** — confirm the old secret no longer works and the new one does.
7. **Notify** — alert the security team via the incident response procedure.

## Pre-Commit Secrets Scanning

The `.pre-commit-config.yaml` configures two scanning tools:

### detect-secrets

Scans for high-entropy strings and common secret patterns.

```bash
# Install
pip install detect-secrets pre-commit

# Install hooks
pre-commit install

# Scan the whole repo manually
detect-secrets scan --baseline .secrets.baseline
detect-secrets audit .secrets.baseline
```

### Gitleaks

Scans for secrets matching a large rule set. Configuration is in `.gitleaks.toml`.

```bash
# Install gitleaks
brew install gitleaks  # macOS
# or: https://github.com/gitleaks/gitleaks/releases

# Scan the working tree
gitleaks detect --source . --config .gitleaks.toml

# Scan git history
gitleaks detect --source . --config .gitleaks.toml --log-opts="HEAD~50..HEAD"
```

## CI Pipeline Scanning

The `.github/workflows/secrets-scan.yml` workflow runs on every push and pull request:

- **Gitleaks** scans for committed secrets
- **detect-secrets** validates the baseline has not been bypassed
- Results are uploaded as workflow artifacts
- Failures block merge

## Shell Script Scanning

The `scripts/secrets-scan.sh` script wraps gitleaks for local use and provides a human-readable report:

```bash
./scripts/secrets-scan.sh            # Scan working tree
./scripts/secrets-scan.sh --history  # Scan last 50 commits
```

## Allowlisting False Positives

If a pattern is a false positive, add it to `.gitleaks.toml` under `[[rules]]` with `allowlist`:

```toml
[[rules]]
id = "example-false-positive"
# ... rule definition ...
[rules.allowlist]
regexes = ["soroban-testnet\.stellar\.org"]
paths = ["README.md"]
```

For detect-secrets, audit and mark as false positive:
```bash
detect-secrets audit .secrets.baseline
# Mark each flagged item as a false positive when prompted
```

## Incident Response: Secret Exposed in Git History

If a secret is found in git history:

1. **Revoke the secret immediately** — assume it is compromised.
2. **Do not use `git filter-branch` or `git rebase` on shared branches** — it rewrites history and causes conflicts for all collaborators.
3. **Contact your Git hosting provider** — GitHub, GitLab, and Bitbucket have processes to remove sensitive data from history.
4. **Log the incident** — create a `CRITICAL` severity audit log entry.
5. **Notify affected parties** — if the secret granted access to external services, notify those service owners.

## Related Configuration Files

| File | Purpose |
|---|---|
| `.gitleaks.toml` | Gitleaks rule configuration |
| `.pre-commit-config.yaml` | Pre-commit hook definitions |
| `.github/workflows/secrets-scan.yml` | CI secrets scanning workflow |
| `scripts/secrets-scan.sh` | Local scanning helper script |
| `.gitignore` | Excludes `.env` and credential files |

## See Also

- [SOC 2 Compliance](soc2-compliance.md)
- [Audit Trail](audit-trail.md)
- [Encryption](encryption.md)
- [Deployment Security](deployment.md)
