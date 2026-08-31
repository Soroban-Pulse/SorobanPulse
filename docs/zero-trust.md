# Zero-trust security model (Issue #940)

`src/zero_trust.rs` provides the building blocks: HMAC-SHA256 request
signing/verification, `ApiKeySet` key rotation (see
`docs/key-rotation.md`), a policy-based `PolicyEvaluator` with pluggable
`PolicyRule`s producing `Allow`/`Deny`/`Challenge` decisions, and an
in-memory `AccessLogger` for audit trail. All of it existed already; none
of it had a single caller anywhere in the codebase before this change.

## What this change actually wires in

`IpDenyListRule` / `IpAllowListRule` (both extended with real CIDR
support as part of this change — see `docs/ip-access-control.md`) are now
live-enforced via `src/middleware/ip_access.rs`, registered in
`routes.rs`'s middleware stack. This is a real instance of "extend
zero_trust for \[an\] endpoint\[s\]" — every route behind that middleware
layer now actually evaluates a zero-trust policy rule before proceeding,
where before this framework was entirely inert.

## What's still a building block, not a live feature

The remaining checklist items for this issue are not implemented:

- **Device fingerprinting** — nothing here collects or compares device
  signals (TLS JA3/JA4, canvas/font fingerprinting, etc.).
- **Risk scoring for requests** — `AccessDecision` is binary/ternary
  (Allow/Deny/Challenge) with no numeric risk score feeding it.
- **Adaptive authentication** — `MutationChallengeRule` exists (challenges
  every `POST`/`PUT`/`PATCH`/`DELETE` with MFA unconditionally) but that's
  a static rule, not adaptive to observed risk.
- **Anomaly detection for access patterns** — `AccessLogger` records
  entries but nothing analyzes them for anomalies; it's a passive log.
- **Continuous verification** — access decisions are evaluated once per
  request at the middleware layer; nothing re-verifies mid-session.

Each of these is a substantial feature in its own right (anomaly
detection and risk scoring in particular need a real model/heuristic, not
a plausible-looking stub) and wasn't attempted here. What exists now is a
genuinely more complete zero-trust *foundation* than before this
change — real request signing, real key rotation, a real extensible
policy engine with a first live rule set (IP-based) actually gating
traffic — rather than 900+ lines of well-tested but entirely unreachable
code.
