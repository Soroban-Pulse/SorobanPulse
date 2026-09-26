# Infrastructure as Code Testing (Issue #906)

Testing strategy for SorobanPulse's Terraform infrastructure (see [terraform.md](terraform.md) for the module reference and directory layout). This document covers what's already enforced in CI today and the additional layers of IaC testing the checklist asks for.

## Current State: `terraform-validate` CI Job

Every PR runs the `terraform-validate` job defined in [`.github/workflows/ci.yml`](../.github/workflows/ci.yml):

| Step | Tool | Checks |
|---|---|---|
| Format check | `terraform fmt -check -recursive -diff` | Consistent formatting across all `.tf` files |
| Init | `terraform init -backend=false` | Modules resolve and providers are available (no real backend/state touched) |
| Validate | `terraform validate` | Syntax and internal consistency (types, required arguments, references) |
| Lint | [TFLint](https://github.com/terraform-linters/tflint) (`tflint --init && tflint --recursive`) | AWS provider best practices, deprecated syntax, unused declarations |

This catches syntax errors, formatting drift, and common misconfigurations, but it does **not**: validate against real cloud state, estimate cost, check policy/compliance rules, or run module-level unit tests. Those are the remaining checklist items below.

## Terraform Cloud Organization

Not yet set up. Terraform Cloud (or Terraform Enterprise) would centralize state management, provide a policy-as-code gate (Sentinel) at plan time, and give a UI for run history — currently state is managed via the S3 + DynamoDB backend described in [terraform.md § Bootstrapping Remote State](terraform.md#bootstrapping-remote-state).

To adopt it: create an org, migrate the existing S3 state with `terraform init -migrate-state` after adding a `cloud {}` block to `providers.tf`, and connect the VCS workflow so `terraform plan` runs automatically on PRs against the org's workspace instead of (or in addition to) the local backend.

## Module Validation Tests

Not yet implemented. Two viable approaches for this codebase's module structure (`vpc`, `rds`, `alb`, `ecs`, `monitoring`, `backup`):

- **Native `terraform test`** (built into Terraform ≥ 1.6, which this repo already requires) — write `.tftest.hcl` files per module, e.g. `terraform/modules/rds/tests/main.tftest.hcl`, asserting on plan/apply output without needing a separate Go toolchain:
  ```hcl
  run "rds_enforces_encryption" {
    command = plan
    assert {
      condition     = aws_db_instance.main.storage_encrypted == true
      error_message = "RDS instances must be encrypted at rest"
    }
  }
  ```
- **Terratest** (Go) — heavier but supports real `apply`-then-assert-then-`destroy` integration tests against a live sandbox account; better suited to validating cross-module wiring (e.g., that the `alb` security group actually permits the `app` module's port).

Start with native `terraform test` for per-module assertions (fast, no extra dependency) and reserve Terratest for the multi-region composite module once it exists (see the `modules/soroban-pulse` gap noted in [multi-region.md](multi-region.md#terraform-layout)).

## Terraform-Compliance Rules

Not yet implemented. [`terraform-compliance`](https://terraform-compliance.com/) runs BDD-style policy checks against a `terraform plan` JSON export, independent of TFLint's syntax-level linting. Example policy relevant to this codebase's existing security posture (documented in [terraform.md § Security Notes](terraform.md#security-notes)):

```gherkin
Feature: RDS instances must not be publicly accessible

Scenario: Ensure publicly_accessible is false
  Given I have aws_db_instance defined
  Then it must contain publicly_accessible
  And its value must be false
```

Add as a CI step after `terraform plan -out=tfplan && terraform show -json tfplan > plan.json`, then `terraform-compliance -p plan.json -f compliance/`.

## Cost Estimation

Not yet implemented. [Infracost](https://www.infracost.io/) integrates as a PR-comment bot showing the monthly cost delta of a plan before merge — useful given this repo's multi-region design (three regions × ASG + RDS, see [multi-region.md](multi-region.md)) makes cost regressions easy to introduce accidentally. Typical CI step:

```yaml
- uses: infracost/actions/setup@v3
  with:
    api-key: ${{ secrets.INFRACOST_API_KEY }}
- run: infracost breakdown --path=terraform --format=json --out-file=infracost.json
- uses: infracost/actions/comment@v3
  with:
    path: infracost.json
```

## Drift Detection

Not yet implemented. Drift occurs when real infrastructure diverges from Terraform state (manual console changes, out-of-band fixes during an incident). Recommended approach:

1. A scheduled workflow (e.g., daily) runs `terraform plan -detailed-exitcode` against each environment's state.
2. Exit code `2` (changes detected) posts an alert to the same channel used for [alerting.md](alerting.md) notifications — treat drift as an incident signal, not routine noise, since it often means someone bypassed IaC during an emergency.
3. **Automated remediation** should be scoped narrowly and opt-in per resource (e.g., re-applying tag or security-group drift automatically is reasonable; auto-applying drift on `aws_db_instance` or anything that could trigger a replacement/destroy is not — require manual review for stateful resources).

## Testing Locally

```bash
cd terraform

# Format + validate (matches CI)
terraform fmt -check -recursive -diff
terraform init -backend=false
terraform validate

# Lint (matches CI)
tflint --init && tflint --recursive

# Native module tests (once written)
terraform test

# Plan-based checks (once adopted)
terraform plan -out=tfplan
terraform show -json tfplan > plan.json
terraform-compliance -p plan.json -f compliance/
infracost breakdown --path=. --format=table
```

## Related Documentation

- [terraform.md](terraform.md) — module reference, bootstrapping, and troubleshooting
- [multi-region.md](multi-region.md) — multi-region Terraform layout this testing strategy must also cover
- [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) — current `terraform-validate` job
