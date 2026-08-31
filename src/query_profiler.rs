//! Query profiling tool built on top of [`crate::query_optimizer`].
//!
//! Captures execution timing per query stage, analyzes the resulting
//! "execution plan" (a lightweight, self-reported stage breakdown rather
//! than a real database `EXPLAIN` plan), identifies bottleneck stages, and
//! produces actionable optimization recommendations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A single stage of query execution (e.g. "parse", "plan", "scan", "sort").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStage {
    pub name: String,
    pub duration: Duration,
    pub rows_examined: u64,
    pub rows_returned: u64,
}

/// The execution plan for a profiled query: an ordered list of stages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExecutionPlan {
    pub stages: Vec<PlanStage>,
}

impl ExecutionPlan {
    pub fn total_duration(&self) -> Duration {
        self.stages.iter().map(|s| s.duration).sum()
    }

    /// Selectivity of a stage: rows_returned / rows_examined. Low
    /// selectivity (close to 0) suggests an index could narrow the scan.
    pub fn selectivity(stage: &PlanStage) -> f64 {
        if stage.rows_examined == 0 {
            1.0
        } else {
            stage.rows_returned as f64 / stage.rows_examined as f64
        }
    }
}

/// Records wall-clock timings for named stages of a single query execution.
/// Call `stage()` to open a timed section and drop the returned guard (or
/// call `.finish()`) to close it.
pub struct QueryProfile {
    pub query: String,
    started_at: Instant,
    stages: Vec<PlanStage>,
}

impl QueryProfile {
    pub fn start(query: impl Into<String>) -> Self {
        Self {
            query: query.into(),
            started_at: Instant::now(),
            stages: Vec::new(),
        }
    }

    pub fn record_stage(&mut self, name: impl Into<String>, duration: Duration, rows_examined: u64, rows_returned: u64) {
        self.stages.push(PlanStage {
            name: name.into(),
            duration,
            rows_examined,
            rows_returned,
        });
    }

    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn into_plan(self) -> ExecutionPlan {
        ExecutionPlan { stages: self.stages }
    }
}

/// A single bottleneck finding produced by analyzing an `ExecutionPlan`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    pub stage: String,
    pub share_of_total: f64,
    pub reason: String,
}

/// An actionable recommendation derived from one or more bottlenecks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub stage: String,
    pub message: String,
}

/// Aggregate metrics tracked across many profiled query executions, keyed
/// by a caller-supplied query identifier (e.g. a normalized query string).
#[derive(Debug, Clone, Default)]
pub struct ProfilingMetrics {
    pub executions: HashMap<String, Vec<Duration>>,
}

impl ProfilingMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, query_id: impl Into<String>, duration: Duration) {
        self.executions.entry(query_id.into()).or_default().push(duration);
    }

    pub fn p50(&self, query_id: &str) -> Option<Duration> {
        self.percentile(query_id, 0.50)
    }

    pub fn p95(&self, query_id: &str) -> Option<Duration> {
        self.percentile(query_id, 0.95)
    }

    pub fn percentile(&self, query_id: &str, p: f64) -> Option<Duration> {
        let durations = self.executions.get(query_id)?;
        if durations.is_empty() {
            return None;
        }
        let mut sorted = durations.clone();
        sorted.sort();
        let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
        Some(sorted[idx])
    }

    /// Returns query ids sorted by total accumulated time, descending —
    /// the queries most worth optimizing first.
    pub fn slowest_by_total_time(&self) -> Vec<(String, Duration)> {
        let mut totals: Vec<(String, Duration)> = self
            .executions
            .iter()
            .map(|(id, durs)| (id.clone(), durs.iter().sum()))
            .collect();
        totals.sort_by(|a, b| b.1.cmp(&a.1));
        totals
    }
}

/// Analyzes execution plans to identify bottleneck stages and produce
/// optimization recommendations. Extends the query optimizer with a
/// profiling-focused view of query performance.
pub struct QueryProfiler {
    /// Stages consuming at least this fraction of total query time are
    /// flagged as bottlenecks.
    pub bottleneck_threshold: f64,
    /// Stages with selectivity below this ratio are flagged as candidates
    /// for indexing.
    pub low_selectivity_threshold: f64,
}

impl Default for QueryProfiler {
    fn default() -> Self {
        Self {
            bottleneck_threshold: 0.30,
            low_selectivity_threshold: 0.05,
        }
    }
}

impl QueryProfiler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Identify stages that dominate total query time.
    pub fn identify_bottlenecks(&self, plan: &ExecutionPlan) -> Vec<Bottleneck> {
        let total = plan.total_duration();
        if total.is_zero() {
            return Vec::new();
        }
        let total_secs = total.as_secs_f64();

        plan.stages
            .iter()
            .filter_map(|stage| {
                let share = stage.duration.as_secs_f64() / total_secs;
                if share >= self.bottleneck_threshold {
                    Some(Bottleneck {
                        stage: stage.name.clone(),
                        share_of_total: share,
                        reason: format!(
                            "stage '{}' accounts for {:.1}% of total query time",
                            stage.name,
                            share * 100.0
                        ),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// Produce optimization recommendations from a plan's stages and the
    /// bottlenecks identified within it.
    pub fn recommend(&self, plan: &ExecutionPlan) -> Vec<Recommendation> {
        let mut recs = Vec::new();

        for stage in &plan.stages {
            let selectivity = ExecutionPlan::selectivity(stage);
            if stage.rows_examined > 1000 && selectivity < self.low_selectivity_threshold {
                recs.push(Recommendation {
                    stage: stage.name.clone(),
                    message: format!(
                        "stage '{}' examined {} rows but returned only {} ({:.2}% selectivity) — consider adding an index",
                        stage.name, stage.rows_examined, stage.rows_returned, selectivity * 100.0
                    ),
                });
            }
        }

        for bottleneck in self.identify_bottlenecks(plan) {
            recs.push(Recommendation {
                stage: bottleneck.stage.clone(),
                message: format!(
                    "{} — consider caching, batching, or narrowing the query for this stage",
                    bottleneck.reason
                ),
            });
        }

        recs
    }

    /// Produce a human-readable text report summarizing a profiled query.
    pub fn report(&self, query: &str, plan: &ExecutionPlan) -> String {
        let mut out = String::new();
        out.push_str(&format!("Query profile: {query}\n"));
        out.push_str(&format!("Total duration: {:?}\n", plan.total_duration()));
        out.push_str("Stages:\n");
        for stage in &plan.stages {
            out.push_str(&format!(
                "  - {name}: {dur:?} (examined={examined}, returned={returned}, selectivity={sel:.2}%)\n",
                name = stage.name,
                dur = stage.duration,
                examined = stage.rows_examined,
                returned = stage.rows_returned,
                sel = ExecutionPlan::selectivity(stage) * 100.0
            ));
        }

        let bottlenecks = self.identify_bottlenecks(plan);
        if bottlenecks.is_empty() {
            out.push_str("No dominant bottleneck stage detected.\n");
        } else {
            out.push_str("Bottlenecks:\n");
            for b in &bottlenecks {
                out.push_str(&format!("  - {}\n", b.reason));
            }
        }

        let recs = self.recommend(plan);
        if !recs.is_empty() {
            out.push_str("Recommendations:\n");
            for r in &recs {
                out.push_str(&format!("  - [{}] {}\n", r.stage, r.message));
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_plan() -> ExecutionPlan {
        ExecutionPlan {
            stages: vec![
                PlanStage {
                    name: "parse".into(),
                    duration: Duration::from_millis(5),
                    rows_examined: 0,
                    rows_returned: 0,
                },
                PlanStage {
                    name: "scan".into(),
                    duration: Duration::from_millis(400),
                    rows_examined: 100_000,
                    rows_returned: 12,
                },
                PlanStage {
                    name: "sort".into(),
                    duration: Duration::from_millis(20),
                    rows_examined: 12,
                    rows_returned: 12,
                },
            ],
        }
    }

    #[test]
    fn total_duration_sums_stages() {
        let plan = sample_plan();
        assert_eq!(plan.total_duration(), Duration::from_millis(425));
    }

    #[test]
    fn selectivity_computed_correctly() {
        let plan = sample_plan();
        let scan = &plan.stages[1];
        assert!((ExecutionPlan::selectivity(scan) - 0.00012).abs() < 1e-6);
    }

    #[test]
    fn selectivity_handles_zero_examined() {
        let stage = PlanStage {
            name: "parse".into(),
            duration: Duration::from_millis(1),
            rows_examined: 0,
            rows_returned: 0,
        };
        assert_eq!(ExecutionPlan::selectivity(&stage), 1.0);
    }

    #[test]
    fn identifies_dominant_bottleneck() {
        let plan = sample_plan();
        let profiler = QueryProfiler::new();
        let bottlenecks = profiler.identify_bottlenecks(&plan);
        assert_eq!(bottlenecks.len(), 1);
        assert_eq!(bottlenecks[0].stage, "scan");
    }

    #[test]
    fn recommends_index_for_low_selectivity() {
        let plan = sample_plan();
        let profiler = QueryProfiler::new();
        let recs = profiler.recommend(&plan);
        assert!(recs.iter().any(|r| r.stage == "scan" && r.message.contains("index")));
    }

    #[test]
    fn empty_plan_has_no_bottlenecks() {
        let plan = ExecutionPlan::default();
        let profiler = QueryProfiler::new();
        assert!(profiler.identify_bottlenecks(&plan).is_empty());
    }

    #[test]
    fn report_includes_query_and_stages() {
        let plan = sample_plan();
        let profiler = QueryProfiler::new();
        let report = profiler.report("SELECT * FROM events", &plan);
        assert!(report.contains("SELECT * FROM events"));
        assert!(report.contains("scan"));
    }

    #[test]
    fn metrics_track_percentiles() {
        let mut metrics = ProfilingMetrics::new();
        for ms in [10, 20, 30, 40, 50] {
            metrics.record("q1", Duration::from_millis(ms));
        }
        assert_eq!(metrics.p50("q1"), Some(Duration::from_millis(30)));
        assert_eq!(metrics.p95("q1"), Some(Duration::from_millis(50)));
    }

    #[test]
    fn metrics_rank_slowest_queries() {
        let mut metrics = ProfilingMetrics::new();
        metrics.record("fast", Duration::from_millis(5));
        metrics.record("slow", Duration::from_millis(500));
        let ranked = metrics.slowest_by_total_time();
        assert_eq!(ranked[0].0, "slow");
    }

    #[test]
    fn profile_records_stages_and_elapsed() {
        let mut profile = QueryProfile::start("SELECT 1");
        profile.record_stage("exec", Duration::from_millis(1), 1, 1);
        let plan = profile.into_plan();
        assert_eq!(plan.stages.len(), 1);
    }
}
