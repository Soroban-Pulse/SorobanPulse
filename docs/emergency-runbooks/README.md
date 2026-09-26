# Emergency Runbooks — Issue #998

> **Warning:** These runbooks describe procedures for critical production
> incidents. Execute steps carefully. Irreversible actions are marked with ⚠️.
> Always involve a second engineer for severity-1 incidents.

## Index

| Runbook | Scenario | Severity |
|---------|----------|----------|
| [database-corruption-recovery.md](database-corruption-recovery.md) | Database corruption or data inconsistency | SEV-1 |
| [data-loss-recovery.md](data-loss-recovery.md) | Complete or partial data loss | SEV-1 |
| [security-breach-response.md](security-breach-response.md) | Security incident, credential compromise | SEV-1 |
| [service-failure-recovery.md](service-failure-recovery.md) | Widespread service failure, multi-region outage | SEV-1/2 |

## How to Use These Runbooks

1. **Declare the incident** using the communication template in each runbook.
2. **Assign roles**: Incident Commander (IC), Comms Lead, Technical Lead.
3. **Follow steps in order** — do not skip steps unless explicitly permitted.
4. **Document every action** in the incident timeline (Slack channel, PagerDuty note, or shared doc).
5. **Run the testing procedure** (each runbook has one) quarterly in a staging environment.

## Escalation Path

| Severity | Response Time | Escalation |
|----------|-------------|-----------|
| SEV-1 | Immediate (< 15 min) | Page on-call IC + Engineering Manager |
| SEV-2 | < 30 min | Page on-call IC |
| SEV-3 | < 4 hours | Assign in next standup |

## Related Runbooks

- `docs/runbooks/db-pool-exhaustion.md`
- `docs/runbooks/indexer-lag.md`
- `docs/runbooks/webhook-failures.md`
- `docs/deployment-runbooks/` — cloud-specific deployment runbooks
