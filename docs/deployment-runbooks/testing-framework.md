# Testing the Deployment Runbooks

Deployment runbooks rot quietly: a heading gets renamed during an edit, a
code fence is left unclosed by a copy-paste, and nobody notices until someone
follows the doc during an actual outage. `scripts/check-deployment-runbooks.sh`
is a lightweight, fast, dependency-free check that catches the structural
version of that problem in CI or locally, before it ships.

## What it checks

For every file in `docs/deployment-runbooks/*.md` **except** `README.md`,
`template.md`, and this file itself (i.e. every platform runbook — currently
`aws.md`, `gcp.md`, `azure.md`, `self-hosted.md`):

1. **Required section headings are present.** The script reads
   [`template.md`](template.md), extracts every line starting with `## `
   (currently `Prerequisites`, `Architecture`, `Deployment Steps`,
   `Verification`, `Rollback`, `Troubleshooting`), and greps each runbook for
   that exact heading text. If a runbook is missing one, the script prints
   `FAIL: <file> is missing required section heading: <heading>`.

   Because the list of required headings is derived from `template.md`
   itself rather than hardcoded in the script, adding or renaming a section
   in the template automatically updates what gets enforced — you don't have
   to edit the script when you add a section.

2. **Every fenced code block is closed.** It counts the lines in each file
   that start with ` ``` ` and fails if the count is odd — an odd count means
   a ` ```bash ` (or any other fence) was opened but never closed, which
   breaks Markdown rendering for everything after it in the file.

The script exits non-zero and prints one `FAIL:` line per problem if any
runbook fails either check, and exits `0` with a summary if all runbooks
pass.

## How to run it

```bash
./scripts/check-deployment-runbooks.sh
```

No arguments, no dependencies beyond `bash` and coreutils (`grep`,
`basename`) — it does not require Docker, cloud CLIs, or network access, so
it's safe to run in any CI job or pre-commit hook. Consider wiring it into
CI alongside `make lint`/`cargo clippy` so a broken runbook fails the same
way a broken build does.

## Limitations

This is a **static structure check**, not a functional test of the runbooks.
It deliberately does **not**:

- Verify that any command in a runbook is syntactically valid for its CLI
  (e.g. that an `aws`/`gcloud`/`az` flag actually exists).
- Verify that following the runbook end-to-end actually produces a working
  deployment.
- Touch any real cloud account, VM, database, or load balancer.
- Check prose quality, broken cross-links to other docs, or whether the
  numbered steps are in a sensible order.

Real infrastructure verification — actually provisioning the VPC/EC2/RDS/ALB
(or GCP/Azure/self-hosted equivalents) described in a runbook and confirming
the **Verification** section's commands succeed against it — requires a real
cloud account, costs real money, and takes real wall-clock time to spin up
and tear down. That's out of scope for a static, offline check like this
one. If and when this project wants that level of assurance, the natural
next step is a **scheduled or manually-triggered smoke-test job** (e.g. a
nightly or pre-release GitHub Actions workflow) that runs a real, disposable
copy of one runbook's steps against a sandbox/test cloud account and then
tears it down — not something `check-deployment-runbooks.sh` should attempt
on every commit.

Until such a job exists, treat a passing `check-deployment-runbooks.sh` run
as "the runbook is structurally complete and well-formed," not as "the
runbook has been proven to work."
