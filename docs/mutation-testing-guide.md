# Mutation Testing Guide

> **Issue #922** — Comprehensive mutation testing for SorobanPulse.
>
> This guide tells you everything you need to know: what mutation testing is,
> how to run it locally, how to read the results, and how CI enforces quality gates.

---

## Table of Contents

1. [What is mutation testing?](#what-is-mutation-testing)
2. [Why it matters for this project](#why-it-matters)
3. [Quick start](#quick-start)
4. [Running locally](#running-locally)
5. [Interpreting results](#interpreting-results)
6. [Understanding the score](#understanding-the-score)
7. [Survivor analysis — fixing test gaps](#survivor-analysis)
8. [Common mutation patterns in this codebase](#common-patterns)
9. [CI integration](#ci-integration)
10. [Updating the baseline](#updating-the-baseline)
11. [Configuration reference](#configuration-reference)

---

## What is mutation testing?

Mutation testing answers a precise question: **"Can our tests detect introduced bugs?"**

The tool (`cargo-mutants`) makes small, targeted changes to the source code — called
*mutations* — and then runs the full test suite against each mutated version.

| Outcome | Meaning |
|---|---|
| **Killed** ✅ | The test suite caught the bug. Good. |
| **Survived** ❌ | The test suite did not catch the bug. Test gap. |
| **Unviable** ℹ️ | The mutation doesn't compile. Not counted. |

### Simple example

```rust
// Original
fn is_valid_limit(limit: i64) -> bool {
    limit > 0 && limit <= 10_000
}
```

`cargo-mutants` might generate:

```rust
// Mutation 1 — operator change
fn is_valid_limit(limit: i64) -> bool {
    limit >= 0 && limit <= 10_000   // 0 would now be accepted — wrong
}

// Mutation 2 — boundary change
fn is_valid_limit(limit: i64) -> bool {
    limit > 0 && limit <= 10_001   // 10_001 would now be accepted — wrong
}
```

If your tests only call `is_valid_limit(50)` and `is_valid_limit(-1)`, both mutations
survive. Adding `is_valid_limit(0)` and `is_valid_limit(10_001)` kills them.

---

## Why it matters

Standard code coverage tells you which lines were executed. It does **not** tell you
whether the tests would catch a bug on those lines. You can have 100 % line coverage
and still have mutations survive.

For SorobanPulse, the critical paths are:

- Pagination boundary logic (`handlers.rs`, `models.rs`)
- Event deduplication (`dedup.rs`, `event_dedup_replicas.rs`)
- Authentication and admin key checks (`middleware/auth.rs`)
- Indexer advisory lock logic (`advisory_lock.rs`)
- Webhook HMAC verification (`webhook_verification.rs`)
- Rate limiter counters and enforcement (`rate_limiter.rs`)

Mutations surviving in these modules are high-severity test gaps.

---

## Quick start

```bash
# 1. Install the tool (one-time)
make -f Makefile.mutations mutants-install

# 2. Run the full suite
make -f Makefile.mutations mutants

# 3. See the score table
make -f Makefile.mutations mutants-score

# 4. Generate the survivor report
make -f Makefile.mutations mutants-report
# → writes docs/mutation-report.md
```

> **Time estimate:** the full suite takes 30–90 minutes depending on hardware.
> Use `mutants-quick` for a fast sanity check during development.

---

## Running locally

### Full suite

```bash
make -f Makefile.mutations mutants
```

Outputs to `target/mutants/`. Opens `target/mutants/index.html` in a browser for
the HTML report.

### Quick check (during development)

```bash
make -f Makefile.mutations mutants-quick
```

Runs with a shorter timeout and capped parallelism. Good for checking a single
module's score before opening a PR.

### Target a specific file

```bash
cargo mutants --file src/handlers.rs --timeout 120
```

### Run on changed files only (PR mode)

```bash
make -f Makefile.mutations mutants-diff
```

Uses `git diff HEAD~1` to find changed `.rs` files and mutates only those.

### See per-mutation output

```bash
make -f Makefile.mutations mutants-verbose
```

Runs single-threaded with full test output per mutation. Slow but useful for
understanding exactly which test killed (or failed to kill) each mutation.

### Profile slow modules

```bash
make -f Makefile.mutations mutants-profile
```

Prints wall-clock time per module so you can identify which modules slow down the
mutation run and consider trimming their test dependencies.

---

## Interpreting results

### Terminal output

```
RESULT  175 killed, 18 survived, 6 unviable
```

### Score table (from `mutants-score`)

```
Module                             Killed  Survived   Score
------------------------------------------------------------------
✅ advisory_lock                        8         0   100.0%
✅ dedup                               12         1    92.3%
🟡 handlers                            89        14    86.4%
🟡 rate_limiter                        11         3    78.6%
❌ webhook_verification                  7         5    58.3%
------------------------------------------------------------------
🟡 OVERALL                            175        18    90.7%

Target: 80% | Minimum: 70%
```

- ✅ ≥ 80 % — meets target
- 🟡 70–79 % — passes CI gate, room for improvement
- ❌ < 70 % — fails CI gate on PRs

### HTML report

Open `target/mutants/index.html` for:
- Line-by-line view of which mutations survived
- Colour-coded by module score
- Clickable source links showing the exact change made

---

## Understanding the score

```
Score = Killed ÷ (Killed + Survived) × 100
```

Unviable mutations are excluded from the denominator — they can't compile so there
is nothing to catch.

### Thresholds

| Level | Score | Effect |
|---|---|---|
| Target | ≥ 80 % | Green badge in CI |
| Minimum | ≥ 70 % | PR gate passes |
| Below minimum | < 70 % | PR gate **fails** |

The thresholds are set in:
- `mutants.toml` (documentation)
- `.github/workflows/mutation-testing.yml` (`MUTATION_SCORE_MINIMUM` / `MUTATION_SCORE_TARGET`)

---

## Survivor analysis

### Step 1 — read the report

```bash
make -f Makefile.mutations mutants-report
cat docs/mutation-report.md
```

Each entry looks like:

```
### webhook_verification (5 survivors)

- `src/webhook_verification.rs:47` — replace > with >=
- `src/webhook_verification.rs:89` — replace && with ||
```

### Step 2 — understand what survived

Open the file at the reported line. The mutation type tells you what was changed:

| Mutation type | Example | Fix |
|---|---|---|
| Operator change | `>` → `>=` | Add boundary test for exact value |
| Boolean inversion | `&&` → `\|\|` | Test both conditions independently |
| Return value | `Ok(x)` → `Err(...)` | Test the happy path explicitly |
| Constant change | `true` → `false` | Assert the constant's effect |

### Step 3 — write the killing test

For `replace > with >=` on a validation function:

```rust
// Before: only tested happy path
#[test]
fn valid_signature() { assert!(verify(sig, body, key)); }

// After: also test exact boundary
#[test]
fn zero_length_body_rejected() {
    assert!(verify(sig, b"", key).is_err());
}
```

### Step 4 — re-run and verify

```bash
cargo mutants --file src/webhook_verification.rs
```

Confirm the new survivors are now killed.

---

## Common patterns in this codebase

### 1. Pagination boundary mutations (handlers.rs, models.rs)

```rust
// Typical survivor
if limit > 10_000 { ... }
// Mutation: limit >= 10_000
```

**Fix:** test exactly `limit = 10_000` (valid) and `limit = 10_001` (invalid).

### 2. Advisory lock result mutations (advisory_lock.rs)

```rust
// Typical survivor
match pg_try_advisory_lock(...) {
    true => ...,
    false => ...,
}
```

**Fix:** mock the DB to return both outcomes and assert the indexer state changes.

### 3. Authentication short-circuit (middleware/auth.rs)

```rust
// Typical survivor
if api_key.is_none() { return Ok(next.run(req).await); }
```

**Fix:** test requests with and without `API_KEY` set and assert the 401 is returned.

### 4. Rate limiter counter increments (rate_limiter.rs)

These often survive because the test checks eventual 429 behaviour, not the exact
counter values. Use `assert_eq!(counter.load(), expected)` checks.

### 5. HMAC comparison mutations (webhook_verification.rs)

```rust
// Typical survivor
hmac_a == hmac_b  → hmac_a != hmac_b
```

**Fix:** test with a deliberately wrong signature and confirm the function returns
`false` / an error.

---

## CI integration

### When mutation tests run

| Trigger | Scope |
|---|---|
| PR to `main` or `develop` | Full suite on changed modules |
| Push to `main` or `develop` | Full suite |
| Weekly schedule (Monday 02:00 UTC) | Full suite |
| Manual workflow dispatch | Full suite (configurable cap) |

### What CI does

1. Installs `cargo-mutants` and runs the full suite.
2. Extracts the score from `target/mutants/mutants.json`.
3. Prints a per-module score table and survivor analysis to the job log.
4. Posts a summary comment on the PR with score, killed/survived counts, and verdict.
5. **Fails the PR gate** if score < 70 %.
6. On pushes to `main`, commits the updated `mutation-score.json` trend file.

### Viewing results

- **PR comment** — score table and verdict appear automatically.
- **Artifacts** — `mutation-testing-report-<run_id>` contains `target/mutants/` and
  `mutation-score.json`. Download from the Actions run page.
- **Score history** — `mutation-score.json` in the repo root tracks the score per
  commit on `main`.

---

## Updating the baseline

After intentionally lowering the score (e.g. adding a new untestable module) or
raising it (after improving tests), commit the new baseline:

```bash
# Run the full suite
make -f Makefile.mutations mutants

# Inspect the new score
make -f Makefile.mutations mutants-score

# Save the snapshot
# (mutants-score already writes mutation-score.json)

# Commit
git add mutation-score.json
git commit -m "chore: update mutation score baseline to X.X%"
```

If you need to change the CI thresholds, edit the `env` section of
`.github/workflows/mutation-testing.yml`:

```yaml
env:
  MUTATION_SCORE_MINIMUM: "70"   # PR gate — fail if below
  MUTATION_SCORE_TARGET:  "80"   # Target — yellow badge if below
```

---

## Configuration reference

All configuration lives in `mutants.toml`. Key settings:

| Setting | Default | Purpose |
|---|---|---|
| `mutation.paths` | `["src/"]` | Directories to mutate |
| `mutation.exclude` | see file | Files to skip |
| `test.test_timeout` | `300` | Seconds before killing a test run |
| `test.jobs` | `4` | Parallel jobs |
| `test.command` | `cargo test` | Test runner |
| `output.report_dir` | `target/mutants` | Where to write reports |
| `output.json_report` | `true` | Needed for `mutants-score` |
| `output.html_report` | `true` | Human-readable browsing |

### Environment variable overrides

```bash
CARGO_MUTANTS_JOBS=8     cargo mutants   # more parallelism
CARGO_MUTANTS_TIMEOUT=60 cargo mutants   # shorter timeout per mutation
```

### Excluding a module from mutation

Add to `mutants.toml`:

```toml
[skip]
skip = ["my_module"]
```

Only do this for modules where mutations are structurally untestable (e.g. OS-level
I/O, metrics counters). Document the reason in the config file.

---

## See also

- `Makefile.mutations` — all available `make` targets with descriptions
- `mutation-score.json` — historical score trend (committed to `main`)
- `docs/mutation-report.md` — latest survivor report (generated, not committed)
- [cargo-mutants documentation](https://mutants.live)
- [Mutation testing — Wikipedia](https://en.wikipedia.org/wiki/Mutation_testing)
