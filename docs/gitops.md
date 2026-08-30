# GitOps for Infrastructure Management (Issue #910)

This document describes how to operate SorobanPulse deployments through
GitOps: the desired state of each environment lives in Git, and a
reconciling controller (ArgoCD) continuously makes the cluster match it,
rather than operators running `kubectl apply` / `helm upgrade` by hand.

This is a **documentation and example-manifest deliverable**. It does not
install ArgoCD into any cluster, and there is no ArgoCD/Flux control plane
running against this repository today. Treat everything below as the runbook
to follow when a maintainer decides to actually bootstrap GitOps for a real
cluster.

## Why GitOps

SorobanPulse already has two ways to deploy: raw manifests in `k8s/` and the
Helm chart in `helm/soroban-pulse/`. Both assume a human (or a CI job with
cluster credentials) runs `kubectl` / `helm` against the target cluster.
That means:

- The cluster's actual state and the repository's declared state can drift
  silently — nothing notices if someone runs an imperative `kubectl edit`.
- Deploy credentials must be handed to CI (or a human's laptop), widening the
  blast radius of a compromised pipeline or workstation.
- There's no single place to answer "what was deployed, when, and why" other
  than shell history and CI logs.

GitOps addresses this by making Git the single source of truth for deployed
state, and by running the reconciler *inside* the cluster (pulling changes)
instead of pushing changes to the cluster from outside it. The Application
manifests under [`gitops/argocd/`](../gitops/argocd/) are what that reconciler
watches.

## 1. Set up ArgoCD

ArgoCD is the recommended controller here — it has a more mature UI/CLI/RBAC
story than Flux for a small number of applications with distinct
staging/production environments, which matches this project's shape.

Installing ArgoCD is a **one-time cluster bootstrap step performed by a
cluster administrator**. It is not something this repository's CI does
automatically, and no workflow in `.github/workflows/` installs or manages
ArgoCD itself — CI's involvement is limited to validating manifests before
they reach `main` (see [Implement PR-based deployment
workflow](#7-implement-pr-based-deployment-workflow) below).

### Install

```bash
kubectl create namespace argocd
kubectl apply -n argocd -f https://raw.githubusercontent.com/argoproj/argo-cd/stable/manifests/install.yaml
```

Wait for the ArgoCD pods to become ready:

```bash
kubectl -n argocd wait --for=condition=available --timeout=300s deployment --all
```

### Initial admin access

ArgoCD generates a random initial password for the `admin` user, stored in a
Secret:

```bash
kubectl -n argocd get secret argocd-initial-admin-secret \
  -o jsonpath='{.data.password}' | base64 -d
```

Change this password (or switch to SSO) immediately after first login —
treat the generated password as a bootstrap credential, not a long-lived one.

By default the `argocd-server` Service is `ClusterIP`. For initial access,
port-forward rather than exposing it publicly:

```bash
kubectl -n argocd port-forward svc/argocd-server 8080:443
# UI: https://localhost:8080  (user: admin)
```

A production install should front `argocd-server` with the same ingress
pattern already used for the app itself (see `k8s/ingress.yaml` /
`helm/soroban-pulse/templates/ingress.yaml`) rather than leaving it on
port-forward.

### CLI login

```bash
# macOS: brew install argocd
# Linux: see https://argo-cd.readthedocs.io/en/stable/cli_installation/

argocd login localhost:8080 --username admin
```

Once logged in, `argocd app ...` commands (used throughout this document)
target this ArgoCD instance.

## 2. Create application manifests repository

There are two common repository layouts for GitOps. Pick one deliberately —
they have a real tradeoff, not just a style preference.

### Option A: separate "config repo" (recommended at scale)

Application source code (`SorobanPulse`, this repo) stays separate from a
second repository — e.g. `SorobanPulse-deploy` — that holds only Helm
values overlays and/or rendered manifests per environment. A release process
(CI, or a human) bumps an image tag / chart version in the config repo after
the app repo's CI publishes an artifact.

**Why**: it separates *"the app changed"* commits from *"the deployed state
changed"* commits. Without this separation, every commit to `main` in an
app repo that ArgoCD watches directly can trigger a sync, and ArgoCD's own
commentary (health status, sync annotations written back, in some setups)
can create feedback loops with app-repo CI. A separate repo also lets you
apply different access control to "who can change what's running in
production" versus "who can merge application code."

**Cost**: two repositories to keep in sync, an extra promotion step, and
more moving parts for a project this size.

### Option B: in-repo `gitops/` directory (what this repository uses)

The [`gitops/`](../gitops/) directory added alongside this document keeps
the ArgoCD Application/AppProject manifests in the same repository as the
application and the Helm chart they deploy.

**Why this is reasonable here**: SorobanPulse is a single-service project
with two environments and a small maintainer group. The feedback-loop risk
above is real but small — ArgoCD watches `helm/soroban-pulse` for chart/value
changes, not the application source under `src/`, so a typical `cargo`
commit doesn't touch anything ArgoCD reconciles, and a values-file commit is
already something a human deliberately made.

**Tradeoff being accepted**: as the project grows (more services, more
environments, more frequent chart changes), revisit Option A. The signal to
switch is Application syncs firing on unrelated commits, or wanting
different merge permissions for "app code" versus "what's deployed."

## 3. Application manifests

Example manifests live under [`gitops/argocd/`](../gitops/argocd/):

| File | Purpose |
|---|---|
| [`project.yaml`](../gitops/argocd/project.yaml) | `AppProject` scoping allowed source repos, destination namespaces, and resource kinds |
| [`application-production.yaml`](../gitops/argocd/application-production.yaml) | `Application` deploying `helm/soroban-pulse` (branch `main`) into `soroban-pulse-production` |
| [`application-staging.yaml`](../gitops/argocd/application-staging.yaml) | `Application` deploying the same chart into `soroban-pulse-staging` |

Apply the project before the applications:

```bash
kubectl apply -n argocd -f gitops/argocd/project.yaml
kubectl apply -n argocd -f gitops/argocd/application-production.yaml
kubectl apply -n argocd -f gitops/argocd/application-staging.yaml
```

Both `Application` manifests point `source.repoURL` at
`https://github.com/Soroban-Pulse/SorobanPulse.git` as a placeholder — set it
to the actual clone URL of the repository as configured in your ArgoCD
instance, and make sure it matches an entry in `project.yaml`'s
`sourceRepos` (ArgoCD rejects an Application whose source repo isn't
whitelisted by its project).

**Prerequisite gap — staging values file does not exist yet.**
`application-staging.yaml` references `helm/soroban-pulse/values-staging.yaml`
via `spec.source.helm.valueFiles`. As of this writing,
`helm/soroban-pulse/values.yaml` only defines one set of defaults (shaped for
production: `env.ENVIRONMENT: "production"`, `replicaCount: 2`), and no
`values-staging.yaml` exists in the chart directory. **This Application will
fail to sync until that file is created.** The comment in
`application-staging.yaml` lists the minimum keys it should override
(`ENVIRONMENT`, lower `replicaCount`/`autoscaling` bounds, a staging
`existingSecret` name). Creating that file is out of scope for this
documentation change — it belongs to whoever first wires up a real ArgoCD
instance against this repo, since it also requires deciding on the staging
secret backend (see [Secrets management](#6-secrets-management-integration)).

Production does not have this gap: it works against
`helm/soroban-pulse/values.yaml` as committed, with an optional
`values-production.yaml` overlay called out in a comment for when
production-specific overrides (e.g. `existingSecret`) are needed beyond the
chart defaults.

## 4. Automatic sync on git push

Both example `Application` manifests set:

```yaml
syncPolicy:
  automated:
    prune: true
```

`spec.syncPolicy.automated` is what makes ArgoCD deploy new commits without a
human running `argocd app sync`. By default ArgoCD discovers new commits by
**polling** the source repository (every 3 minutes by default, cluster-wide,
configurable via the `timeout.reconciliation` setting in the
`argocd-cm` ConfigMap) — so "automatic sync on git push" happens within one
polling interval with zero extra setup.

For near-instant sync instead of waiting on the poll interval, configure a
repository webhook that calls ArgoCD's webhook endpoint on push:

```
<argocd-server>/api/webhook
```

This repo does not currently configure such a webhook (there is no ArgoCD
instance to point it at). Setting one up means, at a high level: add a
webhook in the repository's settings pointing at
`https://<your-argocd-server>/api/webhook`, using the payload URL and secret
ArgoCD's own webhook documentation specifies for the git host in use (GitHub,
GitLab, etc.) — ArgoCD validates the request against
`webhook.github.secret` (or the equivalent for other hosts) configured in
`argocd-secret`. Treat this as a follow-up once a real ArgoCD instance
exists; polling alone is a reasonable starting point.

## 5. Manual sync with drift detection

"Drift" is any difference between the manifests ArgoCD would render from Git
and what's actually running in the cluster — whether from a manual
`kubectl edit`, a different tool writing to the same namespace, or a
partially-applied change. `selfHeal` in `syncPolicy.automated` controls what
ArgoCD does when it detects drift:

**Detect only** (`selfHeal: false`) — used in
[`application-staging.yaml`](../gitops/argocd/application-staging.yaml):

```yaml
syncPolicy:
  automated:
    prune: true
    selfHeal: false
```

ArgoCD marks the Application `OutOfSync` and shows what changed, but leaves
the drifted resource alone until someone runs a manual sync. Useful in
staging, where an engineer might deliberately `kubectl edit` a Deployment to
test something without ArgoCD immediately reverting it.

**Detect and auto-heal** (`selfHeal: true`) — used in
[`application-production.yaml`](../gitops/argocd/application-production.yaml):

```yaml
syncPolicy:
  automated:
    prune: true
    selfHeal: true
```

ArgoCD reverts any drift back to what's declared in Git, typically within
seconds of detecting it. This is the standard choice for production: it
guarantees the cluster matches the last reviewed commit, and it means a
manual "hotfix" applied directly to the cluster gets silently undone unless
it's also committed — which is the intended forcing function (fix it in Git,
not by hand).

Regardless of `selfHeal`, both of the following work at any time:

```bash
# Show the diff between Git state and live cluster state
argocd app diff soroban-pulse-production

# Manually trigger a sync (e.g. after a values change, or to force-heal
# even if selfHeal is false)
argocd app sync soroban-pulse-staging
```

## 6. Secrets management integration

`k8s/secret.yaml` and `helm/soroban-pulse/templates/secret.yaml` currently
render a plain `Secret` populated from `stringData` — either hardcoded
placeholder values (`k8s/secret.yaml`, meant as a template for manual
`kubectl apply`, not real credentials) or from `values.yaml`'s `secrets:`
block (the Helm chart). Both are documented in
[`helm/soroban-pulse/README.md`](../helm/soroban-pulse/README.md) as
development/staging-only, because a Kubernetes `Secret` is base64-encoded,
not encrypted, and `values.yaml` or a raw `Secret` manifest sitting in Git
makes those credentials readable to anyone with read access to the repo.

That's incompatible with GitOps as described in this document: if
`gitops/argocd/application-production.yaml` syncs `helm/soroban-pulse`
against a `values.yaml` (or override file) that contains
`secrets.databaseUrl`, that connection string is in Git history, in
ArgoCD's synced-manifest cache, and in every clone of the repo, permanently.

**Recommended: External Secrets Operator (ESO)**, documented here in depth
because the chart already supports the mechanism it relies on
(`existingSecret`) without any template changes:

1. Install ESO into the cluster (separately from the ArgoCD bootstrap in
   [Set up ArgoCD](#1-set-up-argocd), same one-time nature):
   ```bash
   helm repo add external-secrets https://charts.external-secrets.io
   helm install external-secrets external-secrets/external-secrets \
     -n external-secrets --create-namespace
   ```
2. Configure a `ClusterSecretStore` pointing at the real secret backend
   (AWS Secrets Manager, GCP Secret Manager, Vault, etc.) — this holds the
   actual credentials and is itself never committed to Git.
3. Commit an `ExternalSecret` manifest (safe to store in `gitops/`, since it
   contains no secret values — only *references* to keys in the external
   store) that tells ESO which remote keys to materialize into which
   Kubernetes `Secret`:
   ```yaml
   apiVersion: external-secrets.io/v1beta1
   kind: ExternalSecret
   metadata:
     name: soroban-pulse-production-credentials
     namespace: soroban-pulse-production
   spec:
     refreshInterval: 1h
     secretStoreRef:
       name: aws-secretsmanager
       kind: ClusterSecretStore
     target:
       name: soroban-pulse-production-credentials
     data:
       - secretKey: DATABASE_URL
         remoteRef:
           key: soroban-pulse/production/database-url
       - secretKey: API_KEY
         remoteRef:
           key: soroban-pulse/production/api-key
   ```
4. Point the chart at the resulting Secret instead of letting it manage its
   own — this is exactly the `existingSecret` mechanism already documented
   in `helm/soroban-pulse/README.md`:
   ```yaml
   # values-production.yaml
   existingSecret: "soroban-pulse-production-credentials"
   ```
5. ArgoCD then syncs the `ExternalSecret` object (a reference) via the
   normal GitOps flow; ESO reconciles it against the real secret store
   independently, on its own `refreshInterval`, outside of ArgoCD's sync
   cycle.

This keeps every file ArgoCD syncs free of real credentials, while still
letting the deployed Secret rotate when the underlying value in the cloud
secret store changes.

**See also**: Sealed Secrets (`kubeseal`-encrypted `SealedSecret` objects
that are safe to commit and are decrypted in-cluster by a controller) is a
lighter-weight alternative when there's no existing cloud secret manager to
integrate with — it's already mentioned as an option in
`helm/soroban-pulse/README.md`.

## 7. Implement PR-based deployment workflow

Intended flow once a real ArgoCD instance is wired up:

1. A PR changes a Helm values file (e.g. a future `values-staging.yaml`,
   `values-production.yaml`) or a manifest under `gitops/argocd/`.
2. CI validates the change before merge.
3. On merge to `main`, ArgoCD's poller (or webhook, see [Automatic
   sync](#4-automatic-sync-on-git-push)) picks up the new commit and syncs
   the affected `Application` automatically (or flags it `OutOfSync` for a
   manual `argocd app sync`, per [Manual sync with drift
   detection](#5-manual-sync-with-drift-detection), if `selfHeal: false`).

Step 2 already exists for the Helm chart itself: `.github/workflows/ci.yml`
has a `helm-lint` job that runs on every PR touching this repository:

```yaml
helm-lint:
  name: Helm Chart Lint
  steps:
    - uses: azure/setup-helm@v4
      with:
        version: "3.14.0"
    - run: helm lint helm/soroban-pulse
    - run: helm template soroban-pulse helm/soroban-pulse | grep -v "DATABASE_URL\|API_KEY\|SMTP_PASSWORD"
    - run: helm template soroban-pulse helm/soroban-pulse --set existingSecret=my-secret
```

(`helm lint` + `helm template` against both the default and `existingSecret`
code paths — see `.github/workflows/ci.yml` for the exact job.)

**Gap**: this job validates the chart itself, not the contents of
`gitops/argocd/*.yaml`, and not any future `values-staging.yaml` /
`values-production.yaml` overlay files. There is currently no CI step that:

- Lints the `Application`/`AppProject` manifests under `gitops/argocd/`
  (e.g. `kubeconform` or `kubectl apply --dry-run=client` against the ArgoCD
  CRD schemas).
- Runs `helm template helm/soroban-pulse -f helm/soroban-pulse/values-staging.yaml`
  once that file exists, to catch a values file that doesn't actually render.

This repository does not add such a workflow as part of this change — no
`.github/workflows/` file is modified or added here, since neither the
`gitops/` manifests nor the missing values overlays exist as validated CI
inputs yet. If one is added later, the minimal version would extend the
existing `helm-lint` job (or add a sibling job) with steps roughly like:

```yaml
gitops-validate:
  name: GitOps Manifest Validation
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v4
    - uses: azure/setup-helm@v4
      with:
        version: "3.14.0"
    - name: Render staging values (once values-staging.yaml exists)
      run: helm template soroban-pulse helm/soroban-pulse -f helm/soroban-pulse/values-staging.yaml
    - name: Validate ArgoCD manifests are well-formed YAML
      run: |
        for f in gitops/argocd/*.yaml; do
          python3 -c "import yaml,sys; list(yaml.safe_load_all(open('$f')))"
        done
```

## 8. Audit logging for all changes

Two layers of audit trail already exist, or fall out naturally, once GitOps
is in place — no new logging infrastructure is required for the basics:

**Git history on `gitops/`** is the primary audit log for *intended* state
changes: every commit touching `gitops/argocd/*.yaml` or a Helm values
overlay records who changed what target state, when, and (via the PR
description/review) why. This is stronger than shell history or CI logs
because it's tied to code review — a change to production's sync policy or
destination namespace goes through the same PR process as any other change
to this repository.

**ArgoCD's sync history** is the audit log for *applied* state changes —
what ArgoCD actually did in the cluster, including syncs triggered by
`selfHeal` reverting manual drift:

```bash
argocd app history soroban-pulse-production
```

This shows each sync's revision (git commit SHA), deploy time, and whether
it succeeded, independent of Git history — useful for correlating "when did
this actually roll out" against "when was it merged," and for catching
self-heal reverts that weren't triggered by a normal merge.

**Forwarding ArgoCD events to existing observability**: this repository
already has logging and tracing pipelines documented in
[`docs/log-aggregation.md`](log-aggregation.md) (structured JSON logs to
ELK/Datadog/CloudWatch) and [`docs/tracing.md`](tracing.md) (OpenTelemetry).
ArgoCD supports a
[Notifications](https://argo-cd.readthedocs.io/en/stable/operator-manual/notifications/)
subsystem that can emit sync/health-status events to webhooks, Slack, or
other sinks on trigger conditions (e.g. `on-sync-failed`, `on-deployed`).
Wiring that into this project's existing stack — e.g. a webhook receiver
that reshapes ArgoCD notification payloads into the same JSON log format
described in `docs/log-aggregation.md` — is not implemented as part of this
documentation change; it's called out here as the integration point for
whoever picks up running a real ArgoCD instance against this repo.

## Summary of what exists vs. what's still needed

| Item | Status |
|---|---|
| ArgoCD install/access/CLI runbook | Documented above (§1) — not run against any cluster |
| Config-repo vs in-repo layout decision | Documented (§2) — in-repo `gitops/` chosen for this project's current size |
| `AppProject` + production/staging `Application` manifests | Created: `gitops/argocd/` |
| Automated sync on push | Configured in the example manifests (`syncPolicy.automated`); webhook setup not performed (no live ArgoCD instance) |
| Manual sync + drift detection | Documented (§5), demonstrated via `selfHeal: true` (production) vs `false` (staging) in the example manifests |
| Secrets integration | Documented in depth (ESO), Sealed Secrets noted as alternative — not installed |
| PR-based deployment workflow | Existing `helm-lint` CI job covers chart validation today; `gitops/` manifest validation and values-overlay linting are a documented gap, not yet implemented |
| Audit logging | Git history + `argocd app history` need no new work; forwarding to `docs/log-aggregation.md`/`docs/tracing.md` is a documented follow-up |
| `values-staging.yaml` / `values-production.yaml` | **Do not exist yet** — flagged as a prerequisite in §3, referenced but not fabricated by the example manifests |
