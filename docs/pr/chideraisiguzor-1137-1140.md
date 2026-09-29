## Summary

First slice of the batch-2 CI/CD items #1137-#1140: a `cargo-hack` job that checks every cargo feature on its own, a CI workflow for the standalone `spulse` CLI crate, `helm lint` + chart-testing for the Helm chart, and `kubeconform` validation of `k8s/` together with fixes for the errors it found. Each issue is only partly implemented; the lists below give the exact scope.

## Related Issue

Closes #1137, #1138, #1139, #1140 (see the trailing block).

## Important: `main` does not compile

Found while working on #1137, not changed here:
- `src/handlers.rs:15465` contains a pasted diff fragment (`++ b/src/metrics.rs` followed by `update_fd_count` and an unterminated `update_disk_read_bytes`, both of which already exist in `src/metrics.rs`), so the crate fails at parse time.
- With that fragment removed locally, the crate still fails with hundreds of errors: commit `b43e010` replaced the root `Cargo.toml` with an actix-web / async-graphql / diesel manifest, while the code uses axum, utoipa, moka, dashmap, lettre, flate2, stellar-xdr, tower-http and others that are no longer declared.

So the existing `clippy`, `test` and `coverage` jobs are red on `main`, and the new `features` job below will be too until the manifest is restored. The job is still correct for when it is.

## Changes

### #1137 Lint and test all cargo feature combinations with `cargo-hack`

The issue lists `graphql`, `lua`, `encryption`, `kinesis`, `archive`, `otel` and `zipkin`; the current `Cargo.toml` has `classical-crypto` (default), `quantum-crypto`, `hybrid-crypto`, `istio`, `linkerd`, `full-observability`, `kafka`, `sqs` and `pubsub`.

Done:
- New `features` job in `ci.yml`: `cargo hack check --each-feature --no-dev-deps`, which runs 12 checks: `--all-features`, `--no-default-features` and each feature on its own (listed locally with `--print-command-list`)
- Postgres 16 service + `sqlx migrate run` before the check, because there is no sqlx offline cache and `query!` macros are checked against the live schema at compile time
- `cmake` for `rdkafka`'s `cmake-build` (`kafka` feature); `cargo-hack` and `sqlx-cli` installed with `taiki-e/install-action`

Not done in this PR:
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all-features` matrix entry
- `--feature-powerset --depth 2` nightly run
- Not verified by running: see "`main` does not compile" above

### #1138 CI job for the `spulse` CLI crate

What existed: `cli/` is its own crate (own `Cargo.lock`) outside any root workspace, so no workflow built it.

Done:
- `.github/workflows/cli.yml`, triggered on `cli/**` (and the workflow file) for pushes to `main` and PRs: `cargo test --locked` and `cargo build --release --locked` in `cli/`, cached with `Swatinem/rust-cache` `workspaces: cli`
- Ran both commands locally on `main`'s `cli/`: 6 tests pass, release build succeeds, lock file is in sync

Not done in this PR:
- `cargo fmt --check` and `cargo clippy -- -D warnings`: the CLI is not rustfmt-clean and has 7 clippy warnings on `main`, so these steps would be red immediately; they need a cleanup first
- Folding the job into the main CI once the workspace refactor lands

### #1139 Lint and validate the Helm chart with chart-testing and kind

Done:
- `ct.yaml` (`chart-dirs: [helm]`, `target-branch: main`; `validate-maintainers: false` because `Chart.yaml` has no maintainers; `check-version-increment: false` for now, which maintainers may want to turn on to force chart version bumps)
- `.github/workflows/helm.yml` on `helm/**` / `ct.yaml` changes: `helm lint --strict` and `ct lint` (via `helm/chart-testing-action`, which brings yamllint and yamale)
- Both pass locally (helm v3.16.4, ct v3.12.0); the rendered chart also validates with `kubeconform -strict` for Kubernetes 1.31 (7/7 resources)

Not done in this PR:
- `ct install` on a `kind` cluster with a Postgres dependency and a `/healthz/ready` smoke test
- `kubeconform` of rendered templates against 1.28-1.31 in CI
- `values.schema.json`

### #1140 Validate raw Kubernetes manifests in `k8s/`

What existed: nothing validated `k8s/`. `kubeconform -strict` found:
- `k8s/linkerd-server.yaml` was not valid YAML: the `ServerAuthorization` `spec.client` mixed a mapping (`meshTLS:`) with list items (`- meshTLS:`), so `kubectl apply` of the file fails
- `HTTPRoute` used `timeouts` under `policy.linkerd.io/v1beta1`, where that field does not exist
- the Istio `VirtualService` used `websocketUpgrade`, which is no longer part of the API

Done:
- `ServerAuthorization` split into one `meshTLS` rule listing the three service accounts (ingress, prometheus, soroban-pulse) and a separate `soroban-pulse-authz-unauthenticated` rule (`client` is a single object)
- `HTTPRoute` moved to `policy.linkerd.io/v1beta3`, the first version with `timeouts`
- `websocketUpgrade` removed (Istio upgrades WebSocket connections automatically); `timeout: 0s` kept
- `.github/workflows/k8s-manifests.yml` on `k8s/**`: `kubeconform -strict` for Kubernetes 1.28 and 1.31, with Istio/Linkerd CRDs validated against the datreeio CRDs-catalog. Skipped kinds, with the reason in the workflow: `TrafficSplit` (SMI publishes no schema) and `HTTPRoute` (the catalog schema checks `format: duration` as ISO-8601 and rejects valid Kubernetes durations like `5s`; the rest of the resource passed validation)
- Result: 39/39 validatable resources pass on both versions; on `main` the same command reports 1 invalid and 1 unparseable resource

Not done in this PR:
- `kube-linter` / `polaris` checks (limits, probes, non-root, read-only rootfs) and fixing what they report
- Deciding whether `k8s/` should be generated from `helm template`, and documenting it
- Note: the `soroban-pulse-authz-unauthenticated` rule (kept from the original intent "allow unauthenticated traffic for health checks") admits any client to the server, which makes the meshTLS rule moot; worth narrowing with `networks` in a follow-up

## Testing

- [ ] `cargo test` passes (N/A for the root crate: `main` does not compile, see above; `cli/`: 6 passed)
- [ ] `cargo clippy` reports no warnings (N/A: no Rust changes)
- [x] Manually tested locally

Verification: all four workflows pass `actionlint` 1.7.7; the commands in `cli.yml`, `helm.yml` and `k8s-manifests.yml` were run locally and pass; pinned action versions checked to exist.

## Notes

No application code, API, schema or deployment behaviour changes besides the three manifest fixes in `k8s/`.

Closes #1137
Closes #1138
Closes #1139
Closes #1140
