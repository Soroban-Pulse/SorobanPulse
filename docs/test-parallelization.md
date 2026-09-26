# Integration Test Parallelization

## Overview

SorobanPulse uses per-test PostgreSQL schema isolation so that integration
tests can run concurrently without interfering with each other.  The
infrastructure (Issue #927) lives in `tests/parallel_test_infra.rs` and
provides:

- `DbIsolation` — a per-test schema provisioned in the test database.
- `seed_events` — insert synthetic events into an isolated schema.
- `ParallelTestRunner` — dependency-graph-aware scheduler with caching and
  flakiness detection.
- `parallel_test!` — a macro that wires a test function to its own schema.

---

## Architecture

```
 TestRegistry
      │  (topological sort)
      ▼
 TestDependency graph
      │  (Kahn's algorithm)
      ▼
 Execution order
      │  (tokio::spawn + Semaphore)
      ▼
 ParallelTestRunner ──────────────────────────────────────────────┐
      │                                                            │
      │  per-test                                                  │ aggregate
      ▼                                                            ▼
 DbIsolation (CREATE SCHEMA test_<uuid>)               TestResult[]
      │                                                            │
      │  migrations                                                │ cache_result / load_cached
      ▼                                                            ▼
 seed_events()                                         .test-cache/<name>.test-result.json
      │
      ▼
 test body (async closure)
      │
      ▼
 DbIsolation::Drop (DROP SCHEMA … CASCADE)
```

---

## Database Isolation Strategy

Each test gets its own PostgreSQL schema.  This approach gives full isolation
without the overhead of spawning separate databases:

1. `DbIsolation::new()` runs `CREATE SCHEMA IF NOT EXISTS "test_<uuid>"`.
2. The test schema is set as the `search_path` for all queries on that pool.
3. `DROP` is scheduled on `DbIsolation::Drop` via `tokio::spawn`.

Because schemas share the same PostgreSQL process, tests still run at
database speed without needing Docker-in-Docker or separate DB instances.

### Schema naming

Schemas are named `test_<uuid_simple>` — for example `test_018e2c7a1234abcd`.
The UUID ensures no two parallel tests collide even if the test name is the
same across test binaries.

---

## AppState Fixture Factory

`test_app_state(&db)` constructs a minimal `AppState` backed by the isolated
test pool.  It does not start background workers, connect to the RPC endpoint,
or load real configuration — only the fields needed by handlers under test are
populated.

```rust
let db = DbIsolation::new().await;
let state = test_app_state(&db).await;
// state.pool is the isolated pool
// state.sse_connections, health_state, etc. are stubs
```

---

## Test Categorization Rules

The `TestRegistry::categorize_tests()` method assigns categories based on
the test function name:

| Name pattern | Category |
|--------------|----------|
| `*_db_*` or `*_database_*` | `Database` |
| `*_integration_*` | `Integration` |
| `*_slow_*` | `Slow` |
| anything else | `Fast` |

Categories influence scheduling:
- `Fast` tests run first, with maximum concurrency.
- `Database` tests run with limited concurrency (default: 4) to avoid
  exhausting the connection pool.
- `Integration` tests run sequentially by default.

---

## Dependency Graph Approach

`TestRegistry` holds a `HashMap<String, TestDependency>`.  Each
`TestDependency` names the tests that must complete before it can start.

`build_dependency_graph()` runs Kahn's algorithm to produce a
topologically sorted list.  If a cycle is detected it returns `Err`.

```rust
let mut registry = TestRegistry::default();
registry.register(TestDependency {
    name: "seed_contracts".into(),
    depends_on: vec![],
    category: TestCategory::Database,
});
registry.register(TestDependency {
    name: "test_events_by_contract".into(),
    depends_on: vec!["seed_contracts".into()],
    category: TestCategory::Database,
});
let order = registry.build_dependency_graph()?;
// order = ["seed_contracts", "test_events_by_contract"]
```

---

## Parallelization Strategy

`ParallelTestRunner::run_parallel(tests, max_concurrency)` uses a
`tokio::sync::Semaphore` to cap the number of concurrent tests.  Each test
is spawned as a `tokio::task`.

```
tests = ["a", "b", "c", "d", "e"]
max_concurrency = 3

  time →
  ┌───────────────────────────┐
a │████████                   │
b │█████████████              │
c │███████                    │
d │         ████████          │  (starts when a finishes)
e │              █████        │  (starts when c finishes)
  └───────────────────────────┘
```

Dependent tests always follow their predecessors in the execution order
produced by the topological sort; the scheduler doesn't need to check
dependency status at runtime.

---

## Result Aggregation and Reporting

`aggregate_results(&results)` returns a `HashMap<String, serde_json::Value>`:

```json
{
  "total":    42,
  "passed":   40,
  "failed":    2,
  "flaky":     1,
  "cached":    5,
  "avg_duration_ms": 34
}
```

Print a human-readable report:

```rust
let summary = runner.aggregate_results(&results);
println!("{}", serde_json::to_string_pretty(&summary).unwrap());
```

---

## Flakiness Detection Algorithm

`detect_flaky(name, run_fn)` runs the test closure `flakiness_runs` times
(default: 3) and compares outcomes.  If any run produces a different result
than the last, `flaky = true` is set on the returned `TestResult`.

```
runs: [pass, pass, fail]  →  flaky = true,  passed = false
runs: [pass, pass, pass]  →  flaky = false, passed = true
runs: [fail, fail, fail]  →  flaky = false, passed = false
```

Configure the run count:

```rust
let mut runner = ParallelTestRunner::new(".test-cache");
runner.flakiness_runs = 5;
```

---

## Caching Mechanism

`cache_result` serialises a `TestResult` to
`.test-cache/<test_name>.test-result.json`.

`load_cached` reads the file if it exists and is less than **24 hours** old.
If the file is stale or absent it returns `None` and the test runs normally.

Cache files are gitignored via `.test-cache/` in `.gitignore`.

To bust the cache for all tests:

```bash
rm -rf .test-cache/
```

To bust a single test:

```bash
rm .test-cache/my_test.test-result.json
```

---

## Usage Guide

### Using the `parallel_test!` macro

```rust
// tests/my_integration_tests.rs
#[allow(unused)]
mod parallel_test_infra;  // or use the shared file

parallel_test!(test_retrieve_events, |db| async move {
    let ids = seed_events(&db.pool, &db.schema_name, 10).await;
    assert_eq!(ids.len(), 10);
    // call handler, assert response …
});
```

### Manual setup without the macro

```rust
#[tokio::test]
async fn my_test() {
    let db = DbIsolation::new().await;
    let ids = seed_events(&db.pool, &db.schema_name, 5).await;
    // … test logic …
    // db dropped here → schema cleaned up asynchronously
}
```

### Running only database tests

```bash
cargo test --test parallel_test_infra -- --test-threads=4
```

---

## CI Integration

Add to `.github/workflows/test.yml` (or your CI config):

```yaml
test-parallel:
  runs-on: ubuntu-latest
  services:
    postgres:
      image: postgres:16
      env:
        POSTGRES_PASSWORD: postgres
      options: >-
        --health-cmd pg_isready
        --health-interval 10s
        --health-timeout 5s
        --health-retries 5

  steps:
    - uses: actions/checkout@v4
    - uses: dtolnay/rust-toolchain@stable
    - name: Run parallel integration tests
      env:
        DATABASE_URL: postgres://postgres:postgres@localhost/soroban_pulse_test
      run: |
        cargo test --test parallel_test_infra -- --test-threads=8
```

The `--test-threads=8` flag instructs the Rust test harness to run up to 8
test functions concurrently.  Each function gets its own schema, so there is
no shared state.

---

## Adding New Parallel Tests

1. Create or locate your test file under `tests/`.
2. Use `parallel_test!` for simple cases or `DbIsolation::new()` directly
   for fine-grained control.
3. Name your test to match the categorisation heuristics
   (`_db_`, `_integration_`, etc.) if you want automatic scheduling.
4. Register it in `TestRegistry` if you need dependency ordering.

---

## Troubleshooting

**Schema is not dropped after a test failure**

`DbIsolation::Drop` schedules cleanup via `tokio::spawn`.  If the test
process terminates immediately after a panic, the schema may linger.  Run:

```sql
SELECT nspname FROM pg_namespace WHERE nspname LIKE 'test_%';
DROP SCHEMA "test_<uuid>" CASCADE;
```

Or use the helper script:

```bash
psql "$DATABASE_URL" -c "
  DO \$\$ DECLARE r record;
  BEGIN
    FOR r IN SELECT nspname FROM pg_namespace WHERE nspname LIKE 'test_%' LOOP
      EXECUTE 'DROP SCHEMA IF EXISTS \"' || r.nspname || '\" CASCADE';
    END LOOP;
  END \$\$;"
```

**Tests fail with "connection pool exhausted"**

Reduce `--test-threads` or lower `max_concurrency` in `ParallelTestRunner`.

**Cache returns stale results**

Delete `.test-cache/` or set the TTL to 0 by modifying `load_cached`.
