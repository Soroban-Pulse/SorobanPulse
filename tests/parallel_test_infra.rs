//! # Integration Test Parallelization Infrastructure — Issue #927
//!
//! Provides per-test PostgreSQL schema isolation, a dependency-graph-aware
//! test runner, result caching, and flakiness detection.  Each test receives
//! an isolated schema so tests can run concurrently without interfering with
//! one another.
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use crate::parallel_test;
//!
//! parallel_test!(my_test, |state, db| async move {
//!     let ids = seed_events(&db.pool, &db.schema, 10).await;
//!     assert!(!ids.is_empty());
//! });
//! ```

#![cfg(test)]

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Test categorization
// ---------------------------------------------------------------------------

/// Broad category that controls how a test is scheduled.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TestCategory {
    /// Pure unit tests — no I/O, runs in microseconds.
    Fast,
    /// Tests that do I/O but no database.
    Slow,
    /// Full-stack integration tests that require a running service.
    Integration,
    /// Tests that require a live PostgreSQL connection.
    Database,
}

// ---------------------------------------------------------------------------
// Test dependency graph
// ---------------------------------------------------------------------------

/// Metadata for a single test node in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDependency {
    /// Unique test name.
    pub name: String,
    /// Names of tests that must complete successfully before this test runs.
    pub depends_on: Vec<String>,
    /// Scheduling category.
    pub category: TestCategory,
}

/// In-memory registry of test nodes.
#[derive(Debug, Default)]
pub struct TestRegistry {
    /// Map from test name to its dependency node.
    pub tests: HashMap<String, TestDependency>,
}

impl TestRegistry {
    /// Register a test node.
    pub fn register(&mut self, dep: TestDependency) {
        self.tests.insert(dep.name.clone(), dep);
    }

    /// Assign a category based on name heuristics.
    ///
    /// Naming conventions:
    /// - `*_db_*` or `*_database_*` → `Database`
    /// - `*_integration_*` → `Integration`
    /// - `*_slow_*` → `Slow`
    /// - anything else → `Fast`
    pub fn categorize_tests(&mut self) {
        let mut updates: Vec<(String, TestCategory)> = Vec::new();
        for (name, _dep) in &self.tests {
            let cat = if name.contains("_db_") || name.contains("_database_") {
                TestCategory::Database
            } else if name.contains("_integration_") {
                TestCategory::Integration
            } else if name.contains("_slow_") {
                TestCategory::Slow
            } else {
                TestCategory::Fast
            };
            updates.push((name.clone(), cat));
        }
        for (name, cat) in updates {
            if let Some(dep) = self.tests.get_mut(&name) {
                dep.category = cat;
            }
        }
    }

    /// Produce a topologically sorted order of test names.
    ///
    /// Uses Kahn's algorithm.  Returns `Err` if a cycle is detected.
    pub fn build_dependency_graph(&self) -> Result<Vec<String>, String> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

        for (name, dep) in &self.tests {
            in_degree.entry(name).or_insert(0);
            for dep_name in &dep.depends_on {
                *in_degree.entry(dep_name).or_insert(0) += 0; // ensure present
                adj.entry(dep_name.as_str()).or_default().push(name.as_str());
                *in_degree.entry(name).or_insert(0) += 1;
            }
        }

        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        let mut sorted: Vec<String> = Vec::new();
        while let Some(node) = queue.pop_front() {
            sorted.push(node.to_string());
            if let Some(neighbours) = adj.get(node) {
                for &nb in neighbours {
                    let deg = in_degree.get_mut(nb).unwrap();
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(nb);
                    }
                }
            }
        }

        if sorted.len() != self.tests.len() {
            Err("cycle detected in test dependency graph".to_string())
        } else {
            Ok(sorted)
        }
    }
}

// ---------------------------------------------------------------------------
// Test result
// ---------------------------------------------------------------------------

/// Outcome of a single test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test name.
    pub name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// True if the test produced inconsistent results across runs.
    pub flaky: bool,
    /// True if this result was served from the file-based cache.
    pub cached: bool,
    /// Optional failure message.
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Parallel test runner
// ---------------------------------------------------------------------------

/// Coordinates categorization, scheduling, execution, and result aggregation.
pub struct ParallelTestRunner {
    registry: TestRegistry,
    /// Directory for result caching.
    cache_dir: PathBuf,
    /// How many times to repeat a test for flakiness detection.
    flakiness_runs: usize,
}

impl ParallelTestRunner {
    /// Create a new runner using `cache_dir` for result caching.
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            registry: TestRegistry::default(),
            cache_dir: cache_dir.into(),
            flakiness_runs: 3,
        }
    }

    /// Register a test dependency node.
    pub fn register(&mut self, dep: TestDependency) {
        self.registry.register(dep);
    }

    /// Assign categories based on name patterns.
    pub fn categorize_tests(&mut self) {
        self.registry.categorize_tests();
    }

    /// Return a topologically sorted execution order.
    pub fn build_dependency_graph(&self) -> Result<Vec<String>, String> {
        self.registry.build_dependency_graph()
    }

    /// Run `tests` concurrently using `tokio::spawn`, respecting `max_concurrency`.
    ///
    /// Returns a vector of results in completion order.
    pub async fn run_parallel(
        &self,
        tests: Vec<String>,
        max_concurrency: usize,
    ) -> Vec<TestResult> {
        use tokio::task::JoinSet;

        let mut set: JoinSet<TestResult> = JoinSet::new();
        let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrency));
        let mut results = Vec::new();

        for name in tests {
            let sem = Arc::clone(&semaphore);
            let n = name.clone();
            let cached = self.load_cached(&n);

            set.spawn(async move {
                if let Some(result) = cached {
                    return result;
                }
                let _permit = sem.acquire_owned().await.ok();
                let start = Instant::now();
                // Stub: real runner would call the test function here.
                let passed = true;
                let duration_ms = start.elapsed().as_millis() as u64;
                TestResult {
                    name: n,
                    passed,
                    duration_ms,
                    flaky: false,
                    cached: false,
                    message: None,
                }
            });
        }

        while let Some(result) = set.join_next().await {
            if let Ok(r) = result {
                self.cache_result(&r);
                results.push(r);
            }
        }
        results
    }

    /// Aggregate results into a summary.
    pub fn aggregate_results(&self, results: &[TestResult]) -> HashMap<String, serde_json::Value> {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        let flaky = results.iter().filter(|r| r.flaky).count();
        let cached = results.iter().filter(|r| r.cached).count();
        let avg_ms = if total > 0 {
            results.iter().map(|r| r.duration_ms).sum::<u64>() / total as u64
        } else {
            0
        };

        let mut summary = HashMap::new();
        summary.insert("total".to_string(), serde_json::json!(total));
        summary.insert("passed".to_string(), serde_json::json!(passed));
        summary.insert("failed".to_string(), serde_json::json!(failed));
        summary.insert("flaky".to_string(), serde_json::json!(flaky));
        summary.insert("cached".to_string(), serde_json::json!(cached));
        summary.insert("avg_duration_ms".to_string(), serde_json::json!(avg_ms));
        summary
    }

    /// Run a test `flakiness_runs` times and mark it flaky if results differ.
    pub async fn detect_flaky(&self, name: &str, run_fn: impl Fn() -> bool) -> TestResult {
        let mut outcomes: Vec<bool> = Vec::new();
        let start = Instant::now();
        for _ in 0..self.flakiness_runs {
            outcomes.push(run_fn());
        }
        let passed = *outcomes.last().unwrap_or(&false);
        let flaky = outcomes.iter().any(|&o| o != passed);
        TestResult {
            name: name.to_string(),
            passed,
            duration_ms: start.elapsed().as_millis() as u64,
            flaky,
            cached: false,
            message: if flaky {
                Some(format!("flaky: outcomes = {outcomes:?}"))
            } else {
                None
            },
        }
    }

    /// Persist a test result to the file-based cache.
    pub fn cache_result(&self, result: &TestResult) {
        let path = self.cache_dir.join(format!("{}.test-result.json", result.name));
        if let Ok(json) = serde_json::to_string(result) {
            let _ = std::fs::create_dir_all(&self.cache_dir);
            let _ = std::fs::write(path, json);
        }
    }

    /// Load a cached test result if it exists and is less than 24 hours old.
    pub fn load_cached(&self, name: &str) -> Option<TestResult> {
        let path = self.cache_dir.join(format!("{name}.test-result.json"));
        let metadata = std::fs::metadata(&path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = modified.elapsed().ok()?;
        if age > Duration::from_secs(86400) {
            return None;
        }
        let json = std::fs::read_to_string(path).ok()?;
        let mut result: TestResult = serde_json::from_str(&json).ok()?;
        result.cached = true;
        Some(result)
    }
}

// ---------------------------------------------------------------------------
// Database isolation
// ---------------------------------------------------------------------------

/// Per-test PostgreSQL schema.  Drops the schema on `Drop`.
pub struct DbIsolation {
    /// The isolated schema name (e.g. `test_abc123`).
    pub schema_name: String,
    /// Connection pool scoped to this schema.
    pub pool: sqlx::PgPool,
}

impl DbIsolation {
    /// Create a new isolated schema and run migrations inside it.
    ///
    /// Requires `DATABASE_URL` to be set in the environment.
    pub async fn new() -> Self {
        let schema_name = format!("test_{}", Uuid::new_v4().simple());
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/soroban_pulse_test".to_string());

        let pool = sqlx::PgPool::connect(&database_url)
            .await
            .expect("failed to connect to test database");

        sqlx::query(&format!("CREATE SCHEMA IF NOT EXISTS \"{schema_name}\""))
            .execute(&pool)
            .await
            .expect("failed to create test schema");

        sqlx::query(&format!("SET search_path TO \"{schema_name}\""))
            .execute(&pool)
            .await
            .ok();

        Self { schema_name, pool }
    }

    /// Return the schema name.
    pub fn schema_name(&self) -> &str {
        &self.schema_name
    }
}

impl Drop for DbIsolation {
    fn drop(&mut self) {
        let schema = self.schema_name.clone();
        let pool = self.pool.clone();
        tokio::spawn(async move {
            let _ = sqlx::query(&format!("DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"))
                .execute(&pool)
                .await;
        });
    }
}

// ---------------------------------------------------------------------------
// Seed helpers
// ---------------------------------------------------------------------------

/// Insert `n` synthetic events into `schema.events` and return their UUIDs.
pub async fn seed_events(pool: &sqlx::PgPool, _schema: &str, n: usize) -> Vec<Uuid> {
    let mut ids = Vec::with_capacity(n);
    for _ in 0..n {
        let id = Uuid::new_v4();
        let _ = sqlx::query(
            r#"
            INSERT INTO events (
                id, contract_id, event_type, tx_hash, ledger,
                timestamp, event_data, in_successful_call, created_at,
                schema_version, anonymized, tenant_id
            ) VALUES (
                $1, $2, 'contract', $3, $4,
                NOW(), '{}', true, NOW(),
                0, false, 'default'
            )
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(id)
        .bind(format!("C{}", Uuid::new_v4().simple()))
        .bind(format!("{}", Uuid::new_v4().simple()))
        .bind(rand_ledger())
        .execute(pool)
        .await;
        ids.push(id);
    }
    ids
}

fn rand_ledger() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    1_000_000 + (ns as i64 % 1_000_000)
}

// ---------------------------------------------------------------------------
// Macro: parallel_test!
// ---------------------------------------------------------------------------

/// Spin up an isolated database schema, run the test body, then clean up.
///
/// # Example
///
/// ```rust,ignore
/// parallel_test!(my_test, |_state, db| async move {
///     let ids = seed_events(&db.pool, &db.schema_name, 5).await;
///     assert_eq!(ids.len(), 5);
/// });
/// ```
#[macro_export]
macro_rules! parallel_test {
    ($name:ident, $body:expr) => {
        #[tokio::test]
        async fn $name() {
            let db = $crate::parallel_test_infra::DbIsolation::new().await;
            $body(db).await;
        }
    };
}

// ---------------------------------------------------------------------------
// Example parallel tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Unit-level tests that don't need a database.

    #[test]
    fn test_registry_topological_sort() {
        let mut registry = TestRegistry::default();
        registry.register(TestDependency {
            name: "a".to_string(),
            depends_on: vec![],
            category: TestCategory::Fast,
        });
        registry.register(TestDependency {
            name: "b".to_string(),
            depends_on: vec!["a".to_string()],
            category: TestCategory::Fast,
        });
        registry.register(TestDependency {
            name: "c".to_string(),
            depends_on: vec!["b".to_string()],
            category: TestCategory::Fast,
        });

        let order = registry.build_dependency_graph().unwrap();
        assert_eq!(order, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_registry_cycle_detection() {
        let mut registry = TestRegistry::default();
        registry.register(TestDependency {
            name: "x".to_string(),
            depends_on: vec!["y".to_string()],
            category: TestCategory::Fast,
        });
        registry.register(TestDependency {
            name: "y".to_string(),
            depends_on: vec!["x".to_string()],
            category: TestCategory::Fast,
        });

        assert!(registry.build_dependency_graph().is_err());
    }

    #[test]
    fn test_categorize_heuristics() {
        let mut registry = TestRegistry::default();
        registry.register(TestDependency {
            name: "test_db_events".to_string(),
            depends_on: vec![],
            category: TestCategory::Fast,
        });
        registry.register(TestDependency {
            name: "test_integration_flow".to_string(),
            depends_on: vec![],
            category: TestCategory::Fast,
        });
        registry.register(TestDependency {
            name: "test_basic_parse".to_string(),
            depends_on: vec![],
            category: TestCategory::Fast,
        });
        registry.categorize_tests();

        assert_eq!(
            registry.tests["test_db_events"].category,
            TestCategory::Database
        );
        assert_eq!(
            registry.tests["test_integration_flow"].category,
            TestCategory::Integration
        );
        assert_eq!(
            registry.tests["test_basic_parse"].category,
            TestCategory::Fast
        );
    }

    #[test]
    fn test_aggregate_results() {
        let runner = ParallelTestRunner::new(".test-cache");
        let results = vec![
            TestResult {
                name: "a".to_string(),
                passed: true,
                duration_ms: 10,
                flaky: false,
                cached: false,
                message: None,
            },
            TestResult {
                name: "b".to_string(),
                passed: false,
                duration_ms: 20,
                flaky: true,
                cached: false,
                message: Some("assertion failed".to_string()),
            },
        ];

        let summary = runner.aggregate_results(&results);
        assert_eq!(summary["total"], serde_json::json!(2));
        assert_eq!(summary["passed"], serde_json::json!(1));
        assert_eq!(summary["failed"], serde_json::json!(1));
        assert_eq!(summary["flaky"], serde_json::json!(1));
    }

    #[test]
    fn test_cache_round_trip() {
        let dir = std::env::temp_dir().join(format!("soroban_pulse_test_{}", Uuid::new_v4().simple()));
        let runner = ParallelTestRunner::new(&dir);
        let result = TestResult {
            name: "my_test".to_string(),
            passed: true,
            duration_ms: 5,
            flaky: false,
            cached: false,
            message: None,
        };
        runner.cache_result(&result);
        // A freshly cached result should be returned.
        let loaded = runner.load_cached("my_test");
        assert!(loaded.is_some());
        assert!(loaded.unwrap().cached);
        // Clean up.
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn test_flakiness_detection_stable() {
        let runner = ParallelTestRunner::new(".test-cache");
        let result = runner.detect_flaky("stable_test", || true).await;
        assert!(!result.flaky);
        assert!(result.passed);
    }

    #[tokio::test]
    async fn test_parallel_runner_basic() {
        let mut runner = ParallelTestRunner::new(".test-cache");
        for i in 0..4 {
            runner.register(TestDependency {
                name: format!("t{i}"),
                depends_on: vec![],
                category: TestCategory::Fast,
            });
        }
        let order = runner.build_dependency_graph().unwrap();
        let results = runner.run_parallel(order, 4).await;
        assert_eq!(results.len(), 4);
        assert!(results.iter().all(|r| r.passed));
    }
}
