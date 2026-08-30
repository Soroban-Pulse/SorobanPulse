//! Automated data quality checks and reporting.
//!
//! Provides a small rule engine for evaluating completeness, consistency,
//! and anomaly-style checks against batches of records, plus metrics
//! tracking and human-readable report generation.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A single record under evaluation, represented as a flat field map.
/// Kept generic (rather than tied to `models::Event`) so the engine can be
/// reused for any tabular data source.
pub type Record = HashMap<String, Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

/// Outcome of running a single rule against a single record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleViolation {
    pub rule_name: String,
    pub severity: Severity,
    pub message: String,
    pub record_index: usize,
}

/// A single data quality rule. Implementations return violations for a
/// record, or an empty vec if the record passes.
pub trait QualityRule: Send + Sync {
    fn name(&self) -> &str;
    fn severity(&self) -> Severity;
    fn evaluate(&self, record: &Record) -> Option<String>;
}

/// Completeness check: a required field must be present and non-empty.
pub struct CompletenessRule {
    pub field: String,
    pub severity: Severity,
}

impl QualityRule for CompletenessRule {
    fn name(&self) -> &str {
        "completeness"
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn evaluate(&self, record: &Record) -> Option<String> {
        match record.get(&self.field) {
            Some(Some(v)) if !v.trim().is_empty() => None,
            _ => Some(format!("field '{}' is missing or empty", self.field)),
        }
    }
}

/// Consistency check: if `if_field` is present, `then_field` must also be
/// present (e.g. `successful=true` implies `ledger_sequence` is set).
pub struct ConsistencyRule {
    pub if_field: String,
    pub then_field: String,
    pub severity: Severity,
}

impl QualityRule for ConsistencyRule {
    fn name(&self) -> &str {
        "consistency"
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn evaluate(&self, record: &Record) -> Option<String> {
        let if_present = matches!(record.get(&self.if_field), Some(Some(v)) if !v.is_empty());
        if !if_present {
            return None;
        }
        let then_present = matches!(record.get(&self.then_field), Some(Some(v)) if !v.is_empty());
        if then_present {
            None
        } else {
            Some(format!(
                "field '{}' set but dependent field '{}' is missing",
                self.if_field, self.then_field
            ))
        }
    }
}

/// Range check for numeric fields, used as a lightweight anomaly detector
/// (values far outside expected bounds are flagged).
pub struct NumericRangeRule {
    pub field: String,
    pub min: f64,
    pub max: f64,
    pub severity: Severity,
}

impl QualityRule for NumericRangeRule {
    fn name(&self) -> &str {
        "numeric_range_anomaly"
    }

    fn severity(&self) -> Severity {
        self.severity
    }

    fn evaluate(&self, record: &Record) -> Option<String> {
        let raw = match record.get(&self.field) {
            Some(Some(v)) => v,
            _ => return None,
        };
        match raw.parse::<f64>() {
            Ok(n) if n < self.min || n > self.max => Some(format!(
                "field '{}' value {} outside expected range [{}, {}]",
                self.field, n, self.min, self.max
            )),
            Ok(_) => None,
            Err(_) => Some(format!("field '{}' is not numeric: '{}'", self.field, raw)),
        }
    }
}

/// Statistical anomaly detector: flags values further than `z_threshold`
/// standard deviations from the batch mean for a numeric field.
pub struct ZScoreAnomalyRule {
    pub field: String,
    pub z_threshold: f64,
    pub severity: Severity,
}

impl ZScoreAnomalyRule {
    fn values(field: &str, records: &[Record]) -> Vec<f64> {
        records
            .iter()
            .filter_map(|r| r.get(field).and_then(|v| v.as_ref()).and_then(|v| v.parse::<f64>().ok()))
            .collect()
    }

    /// Runs the z-score check across a whole batch at once, since it needs
    /// batch-level mean/stddev rather than a single record in isolation.
    pub fn evaluate_batch(&self, records: &[Record]) -> Vec<RuleViolation> {
        let values = Self::values(&self.field, records);
        if values.len() < 2 {
            return Vec::new();
        }
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let stddev = variance.sqrt();
        if stddev == 0.0 {
            return Vec::new();
        }

        let mut violations = Vec::new();
        for (idx, record) in records.iter().enumerate() {
            if let Some(Some(raw)) = record.get(&self.field) {
                if let Ok(v) = raw.parse::<f64>() {
                    let z = (v - mean) / stddev;
                    if z.abs() > self.z_threshold {
                        violations.push(RuleViolation {
                            rule_name: "zscore_anomaly".to_string(),
                            severity: self.severity,
                            message: format!(
                                "field '{}' value {} is {:.2} std devs from mean {:.2}",
                                self.field, v, z, mean
                            ),
                            record_index: idx,
                        });
                    }
                }
            }
        }
        violations
    }
}

/// Aggregate counters tracked across quality check runs, exposed for
/// metrics scraping (e.g. via `src/metrics.rs`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub records_checked: u64,
    pub violations_by_severity: HashMap<String, u64>,
    pub runs: u64,
}

impl QualityMetrics {
    fn record_run(&mut self, records_checked: u64, violations: &[RuleViolation]) {
        self.runs += 1;
        self.records_checked += records_checked;
        for v in violations {
            let key = format!("{:?}", v.severity);
            *self.violations_by_severity.entry(key).or_insert(0) += 1;
        }
    }

    pub fn pass_rate(&self) -> f64 {
        if self.records_checked == 0 {
            return 1.0;
        }
        let total_violations: u64 = self.violations_by_severity.values().sum();
        1.0 - (total_violations as f64 / self.records_checked as f64).min(1.0)
    }
}

/// A data quality report summarizing a single evaluation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub generated_at_unix: u64,
    pub records_evaluated: usize,
    pub violations: Vec<RuleViolation>,
    pub pass_rate: f64,
}

impl QualityReport {
    /// Renders the report as a compact human-readable summary.
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "Data Quality Report ({} records evaluated, {:.2}% pass rate)\n",
            self.records_evaluated,
            self.pass_rate * 100.0
        );
        if self.violations.is_empty() {
            out.push_str("No violations found.\n");
            return out;
        }
        for v in &self.violations {
            out.push_str(&format!(
                "- [{:?}] record #{}: {} ({})\n",
                v.severity, v.record_index, v.message, v.rule_name
            ));
        }
        out
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Rule engine that owns a set of per-record rules plus batch-level
/// anomaly detectors, tracks running metrics, and generates reports.
pub struct QualityRuleEngine {
    rules: Vec<Box<dyn QualityRule>>,
    anomaly_rules: Vec<ZScoreAnomalyRule>,
    metrics: QualityMetrics,
}

impl QualityRuleEngine {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            anomaly_rules: Vec::new(),
            metrics: QualityMetrics::default(),
        }
    }

    pub fn add_rule(&mut self, rule: Box<dyn QualityRule>) -> &mut Self {
        self.rules.push(rule);
        self
    }

    pub fn add_anomaly_rule(&mut self, rule: ZScoreAnomalyRule) -> &mut Self {
        self.anomaly_rules.push(rule);
        self
    }

    pub fn metrics(&self) -> &QualityMetrics {
        &self.metrics
    }

    /// Evaluates all registered rules against a batch of records and
    /// produces a `QualityReport`, updating internal metrics as a side
    /// effect.
    pub fn run(&mut self, records: &[Record]) -> QualityReport {
        let mut violations = Vec::new();

        for (idx, record) in records.iter().enumerate() {
            for rule in &self.rules {
                if let Some(message) = rule.evaluate(record) {
                    violations.push(RuleViolation {
                        rule_name: rule.name().to_string(),
                        severity: rule.severity(),
                        message,
                        record_index: idx,
                    });
                }
            }
        }

        for anomaly_rule in &self.anomaly_rules {
            violations.extend(anomaly_rule.evaluate_batch(records));
        }

        self.metrics.record_run(records.len() as u64, &violations);

        let pass_rate = if records.is_empty() {
            1.0
        } else {
            1.0 - (violations.len() as f64 / records.len() as f64).min(1.0)
        };

        QualityReport {
            generated_at_unix: now_unix(),
            records_evaluated: records.len(),
            violations,
            pass_rate,
        }
    }
}

impl Default for QualityRuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[(&str, Option<&str>)]) -> Record {
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(|s| s.to_string())))
            .collect()
    }

    #[test]
    fn completeness_rule_flags_missing_field() {
        let rule = CompletenessRule { field: "contract_id".into(), severity: Severity::Critical };
        let r = record(&[("contract_id", None)]);
        assert!(rule.evaluate(&r).is_some());

        let r2 = record(&[("contract_id", Some("C123"))]);
        assert!(rule.evaluate(&r2).is_none());
    }

    #[test]
    fn consistency_rule_flags_dependent_missing_field() {
        let rule = ConsistencyRule {
            if_field: "successful".into(),
            then_field: "ledger_sequence".into(),
            severity: Severity::Warning,
        };
        let r = record(&[("successful", Some("true")), ("ledger_sequence", None)]);
        assert!(rule.evaluate(&r).is_some());

        let r2 = record(&[("successful", Some("true")), ("ledger_sequence", Some("10"))]);
        assert!(rule.evaluate(&r2).is_none());
    }

    #[test]
    fn numeric_range_rule_flags_out_of_bounds() {
        let rule = NumericRangeRule { field: "size".into(), min: 0.0, max: 1000.0, severity: Severity::Warning };
        let r = record(&[("size", Some("5000"))]);
        assert!(rule.evaluate(&r).is_some());
    }

    #[test]
    fn zscore_rule_flags_outlier_in_batch() {
        let rule = ZScoreAnomalyRule { field: "size".into(), z_threshold: 2.0, severity: Severity::Warning };
        let records: Vec<Record> = vec![
            record(&[("size", Some("10"))]),
            record(&[("size", Some("11"))]),
            record(&[("size", Some("9"))]),
            record(&[("size", Some("10000"))]),
        ];
        let violations = rule.evaluate_batch(&records);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].record_index, 3);
    }

    #[test]
    fn engine_generates_report_and_tracks_metrics() {
        let mut engine = QualityRuleEngine::new();
        engine.add_rule(Box::new(CompletenessRule { field: "contract_id".into(), severity: Severity::Critical }));

        let records = vec![
            record(&[("contract_id", Some("C1"))]),
            record(&[("contract_id", None)]),
        ];
        let report = engine.run(&records);
        assert_eq!(report.records_evaluated, 2);
        assert_eq!(report.violations.len(), 1);
        assert!(report.pass_rate < 1.0);
        assert_eq!(engine.metrics().runs, 1);
        assert_eq!(engine.metrics().records_checked, 2);
    }

    #[test]
    fn report_text_rendering_includes_violations() {
        let mut engine = QualityRuleEngine::new();
        engine.add_rule(Box::new(CompletenessRule { field: "x".into(), severity: Severity::Critical }));
        let report = engine.run(&[record(&[("x", None)])]);
        let text = report.to_text();
        assert!(text.contains("completeness"));
    }

    #[test]
    fn clean_batch_has_full_pass_rate() {
        let mut engine = QualityRuleEngine::new();
        engine.add_rule(Box::new(CompletenessRule { field: "x".into(), severity: Severity::Info }));
        let report = engine.run(&[record(&[("x", Some("ok"))])]);
        assert_eq!(report.pass_rate, 1.0);
        assert!(report.violations.is_empty());
    }
}
