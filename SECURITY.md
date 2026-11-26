# Security Policy

## Supported Versions

| Version | Supported |
|---------|-----------|
| 1.x     | Yes       |
| < 1.0   | No        |

## Reporting a Vulnerability

Do not open a public GitHub issue for security vulnerabilities.

### Private Reporting via GitHub Security Advisories

1. Go to the [Security tab](https://github.com/Soroban-Pulse/SorobanPulse/security)
   of this repository, or open
   <https://github.com/Soroban-Pulse/SorobanPulse/security/advisories/new> directly.
2. Click "Report a vulnerability".
3. Include: description, affected component/version, reproduction steps, severity estimate.

GitHub keeps the report private and notifies maintainers immediately.

### Alternative Contact

Email security@sorobanpulse.dev with subject [SECURITY]. PGP key available on request.

## Response Timeline

| Milestone                   | Target SLA                 |
|-----------------------------|----------------------------|
| Acknowledgement             | 2 business days            |
| Initial triage and severity | 5 business days            |
| Fix or mitigation           | 30 days (critical: 7 days) |
| Public disclosure           | Coordinated with reporter  |

## Disclosure Policy

We practise coordinated disclosure:

1. The report is triaged privately in a GitHub Security Advisory; the reporter
   is invited to collaborate on the advisory.
2. A fix is developed in a private fork and released for all supported
   versions.
3. The advisory is published (with a CVE requested through GitHub where
   appropriate) once the fix is available, or **90 days** after the initial
   report, whichever comes first. We may agree on a different timeline with
   the reporter, for example if a vulnerability is being actively exploited.
4. Release notes and the [CHANGELOG](CHANGELOG.md) reference the advisory.

We follow responsible disclosure and will credit reporters in release notes and
the advisory unless anonymity is requested. We will not take legal action
against researchers who act in good faith and disclose responsibly.

## Out-of-Scope

- Third-party dependency advisories already tracked by Dependabot / cargo audit
- DoS attacks requiring significant attacker resources with no amplification
- Social-engineering attacks targeting maintainers
- Already-public issues

## Threat Model

The threat model and security architecture document lives at
[docs/security/threat-model.md](docs/security/threat-model.md). It contains the
data-flow diagram, the asset and trust-boundary inventory, a STRIDE analysis
per boundary (with existing mitigations and gaps linked to tracking issues),
and the [security review guide](docs/security/threat-model.md#security-review-guide)
that every PR adding endpoints, integrations or scripting must follow.

## Admin API Authentication (Fail-Closed)

Admin routes under `/v1/admin/*` are **fail-closed**. If no admin credential is
configured — neither the `ADMIN_API_KEY` environment variable nor a
database-backed API key carrying the `admin` scope — every admin request is
rejected with `403 Forbidden` and a startup warning explains how to enable
admin access.

- Admin access requires a valid `ADMIN_API_KEY` or a DB key with `admin` scope.
- The development-only override (`ALLOW_UNAUTHENTICATED_ADMIN=true` /
  `ADMIN_API_DISABLED=false`) is refused when `ENVIRONMENT=production`.
- Deployers should supply `ADMIN_API_KEY` from a secret manager and, where
  possible, expose admin routes on an internal-only listener.

See [docs/security/threat-model.md](docs/security/threat-model.md) for the
trust boundaries around the admin API and the related tracking issues.

## Security Best Practices for Deployers

See docs/secret-management.md, docs/deployment.md, and
docs/owasp_security_headers.md for production hardening guidance.
