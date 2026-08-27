# Video Tutorials and Demonstrations

This page is the canonical index for the SorobanPulse video series. The recordings are designed to complement the written guides: each episode follows a complete workflow, shows the expected output, and links to the source commands so viewers can reproduce it locally.

> Video URLs are intentionally kept in one index so they can be published or replaced without changing the technical documentation. Maintainers should replace each `TBD` value with the approved recording URL after publication.

## Series overview

| Episode | Audience | Demonstration | Video |
|---|---|---|---|
| 1. From zero to events | New users | Start the service, run migrations, verify health, and query indexed events | TBD |
| 2. Run an indexer locally | Developers | Configure a Soroban RPC, start PostgreSQL and the indexer, and inspect lag metrics | TBD |
| 3. API and SSE walkthrough | Integrators | Authenticate, paginate events, filter by contract, and reconnect to an SSE stream | TBD |
| 4. Production operations | Operators | Deploy with Helm, configure secrets and ingress, inspect readiness, and perform a safe rollout | TBD |
| 5. Troubleshooting clinic | Contributors and operators | Diagnose database pool exhaustion, indexer lag, and failed readiness checks | TBD |

## Episode recording scripts

### 1. From zero to events

Show the repository prerequisites, copy the environment template, start the development stack, and wait for `/healthz/ready` to report success. Run the documented migration and seed or event-ingestion command, then use `curl` to request a paginated event response. Explain the difference between liveness and readiness and point viewers to [onboarding](onboarding.md), [deployment](deployment.md), and [troubleshooting](troubleshooting.md).

### 2. Run an indexer locally

Explain the relationship between the Soroban RPC, the indexer worker, PostgreSQL, and the API. Configure `STELLAR_RPC_URL`, `DATABASE_URL`, and the indexing start position. Demonstrate a clean startup, inspect structured logs, and use the metrics endpoint to identify ledger progress and lag. Finish by showing how a second replica is prevented from indexing concurrently.

### 3. API and SSE walkthrough

Demonstrate API authentication without displaying secrets on screen. Request events with pagination, filter by contract ID, retrieve a transaction, and open an SSE stream. Disconnect the client intentionally, show the reconnect behavior and `Last-Event-ID` flow, and explain rate limits and the NDJSON export path. Link viewers to the API examples in the [README](../README.md).

### 4. Production operations

Render the Helm chart with a production values file, explain external secret management, and deploy to a non-production namespace. Verify the Service, readiness probe, HPA, and PodDisruptionBudget. Perform a rolling image update, watch `kubectl rollout status`, and demonstrate rollback. Never place real credentials, tokens, or customer data in the recording.

### 5. Troubleshooting clinic

Reproduce a controlled database-pool exhaustion warning and an indexer-lag warning using safe local limits. Use the relevant [runbooks](runbooks/operator-runbook.md), inspect logs and metrics, apply the documented mitigation, and verify recovery. Close with escalation criteria and the information maintainers need in a bug report.

## Recording and publication checklist

Before publishing an episode, confirm that all commands work against the current supported version, terminal output contains no credentials or personal data, captions and a transcript are available, and every referenced URL is stable. Record the repository commit or release version in the video description. Review the episode after each breaking API, deployment, or configuration change.

Maintainers should upload the recording to the project’s approved video channel, add the URL to the table above, attach the transcript when available, and update the relevant written guide if the demonstration reveals a missing step. Video issues and corrections should reference the episode number so stale content can be retired deliberately.
