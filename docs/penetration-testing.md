# Penetration testing and security assessment

This document covers pen-testing procedures, common vulnerability scenarios
for this codebase specifically, remediation guidance, and how to report a
finding.

## Existing automated security testing

Before any manual pen test, know what's already continuously checked in CI:

- **`cargo-deny`** (`.github/workflows/ci.yml`, `deny.toml`) — dependency
  vulnerability advisories (RustSec), yanked-crate detection, license
  compliance, and banned-dependency checks, on every push.
- **Trivy** (`.github/workflows/docker-publish.yml`) — container image
  vulnerability scanning on published Docker images, results uploaded as a
  SARIF report.

A pen test should assume these already caught known-CVE dependency and
base-image issues, and focus on application-level logic instead.

## Pen testing checklist

### Authentication & authorization

- [ ] Can an unauthenticated request reach any `/v1/admin/*` endpoint?
- [ ] Does an expired/revoked admin session token still work? (session TTL,
      revocation-list check)
- [ ] Can a valid token for tenant A read or mutate tenant B's data?
      (`tenant_id` isolation — see `src/models.rs`'s `Event.tenant_id`)
- [ ] Are webhook signing secrets (`webhook_verification.rs`) validated on
      every inbound webhook, with constant-time comparison?

### Injection

- [ ] SQL injection via any user-controlled filter param (contract ID,
      topic filters, full-text search query) — confirm all queries use
      `sqlx` bind parameters, never string interpolation.
- [ ] Confirm Lua sandbox (`admin/lua/preview`) can't escape to the host
      filesystem/network — check `LuaPreviewResponse` handling for
      resource limits (execution timeout, memory cap).
- [ ] JSON injection via `event_data` into downstream integrations (Slack
      Block Kit, Discord embeds, Teams Adaptive Cards) — confirm
      user-controlled event data is always placed in a JSON *value*, never
      concatenated into the JSON *structure* itself.

### Rate limiting & abuse

- [ ] Can `notification_rate_limit.rs`'s limits be bypassed via IP
      spoofing headers, or by rotating API keys/tenants faster than the
      window?
- [ ] Does an SSE client that never disconnects exhaust
      `sse_ring_buffer.rs` resources for other tenants?

### Secrets handling

- [ ] Are webhook URLs, bot tokens, and OAuth client secrets
      (`SlackOAuthConfig`, Discord `bot_token`, Teams `webhook_url`) logged
      anywhere at `info`/`debug` level? (`tracing` calls in `slack.rs` /
      `discord.rs` / `teams.rs` should log only IDs/status, never
      credentials — verify this holds as new fields are logged.)
- [ ] Are secrets present in `event_data` echoed back into third-party
      integrations verbatim, e.g. a leaked private key pasted into
      transaction memo data?

### Integration-specific (Slack / Discord / Teams)

- [ ] Webhook URLs are bearer credentials — anyone with the URL can post
      as the configured bot. Confirm they're stored encrypted at rest and
      never returned in a GET/list API response.
- [ ] OAuth `redirect_uri` (Slack) is an exact allowlist match, not a
      prefix match, to prevent authorization-code interception via an
      open redirect.

## Common vulnerability scenarios

| Scenario | Where to look | Typical remediation |
|---|---|---|
| Tenant data leakage | Any query missing a `tenant_id` filter | Add `WHERE tenant_id = $1` and a regression test asserting cross-tenant 403/empty result |
| Webhook replay | `webhook_verification.rs` | Enforce a timestamp window + nonce/idempotency key, not signature-only |
| SSRF via user-supplied webhook URL | Channel config validation (`notification_channel.rs` and per-integration configs) | Resolve and reject URLs pointing at RFC 1918 / link-local / cloud metadata IPs before first use |
| Stored XSS via event data rendered in a dashboard/email | `notification_formatter.rs`, email templates | HTML-escape on render, never trust `event_data` as pre-sanitized |
| Dependency CVE | `cargo-deny` catches most; check `cargo tree` for anything pulled in transitively and ignored via `deny.toml`'s `ignore` list | Bump the dependency; if blocked, document the `ignore` entry's justification and expiry |

## Remediation procedures

1. **Triage severity** using CVSS-like impact (data exposure scope ×
   exploitability). Cross-tenant data leakage and auth bypass are always
   Critical/High regardless of CVSS score.
2. **Patch on a branch off `main`**, never force-push a fix directly —
   security fixes still go through the same CI/review gates so the fix
   itself doesn't introduce a regression.
3. **Add a regression test** reproducing the vulnerable behavior failing,
   then passing after the fix — a finding without a regression test can
   silently reappear in a later refactor.
4. **Backport** if the affected code exists on a still-supported release
   branch.

## Vulnerability reporting process

- Report privately — do not open a public GitHub issue for an unpatched
  vulnerability. Use GitHub's private vulnerability reporting
  (Security → Report a vulnerability) on this repository, or email the
  address in `SECURITY.md`.
- Include: affected version/commit, reproduction steps, impact assessment,
  and (if available) a suggested fix.
- Expect acknowledgment within 3 business days and a fix timeline based on
  severity (Critical: target 7 days; High: 30 days; Medium/Low: next
  regular release).

## Security patch procedures

- Patches for actively-exploited or Critical findings ship as an
  out-of-band point release; everything else rides the normal release
  train (see `CHANGELOG.md` conventions).
- Every security patch gets a `CHANGELOG.md` entry describing user-facing
  impact and required action (e.g. "rotate your webhook URLs"), without
  detailing the exploit technique until a fix has been available for a
  reasonable disclosure window.
- Coordinate with `.github/workflows/docker-publish.yml` to ensure a
  patched image is rebuilt and the Trivy scan is clean before the release
  is announced.
