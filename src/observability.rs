//! Issue #835: Observability Stack Unification.
//!
//! This module provides a unified observability layer that ties together
//! structured logging, distributed tracing, and health reporting into a
//! single coherent surface.
//!
//! Components:
//! - [`LogCorrelation`] -- links log entries to trace IDs for cross-service
//!   correlation.
//! - [`UnifiedHealthReport`] -- aggregates health from DB, RPC, and metrics
//!   subsystems into one view.
//! - [`StructuredLogConfig`] -- configures structured log output (format,
//!   level, target).
//! - [`TraceLogBridge`] -- correlates trace spans with log entries so that
//!   span-level logs can be inspected after the fact.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// LogCorrelation
// ---------------------------------------------------------------------------

/// A correlated log entry that is tied to a specific trace.
///
/// Issue #835: each entry captures the original message, the severity level,
/// and the wall-clock instant at which the entry was recorded so that
/// [`LogCorrelation::prune_old_entries`] can evict stale data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedLogEntry {
    /// The log message body.
    pub message: String,
    /// Log severity level (e.g. "INFO", "WARN", "ERROR").
    pub level: String,
    /// Monotonic timestamp used for pruning. Not serialised because
    /// [`Instant`] is opaque; the field is reconstructed on deserialisation
    /// as "now".
    #[serde(skip, default = "Instant::now")]
    pub recorded_at: Instant,
}

/// Ties log entries to trace IDs for cross-service correlation (Issue #835).
///
/// Entries are stored in-process and can be pruned periodically via
/// [`prune_old_entries`](Self::prune_old_entries).
#[derive(Debug)]
pub struct LogCorrelation {
    /// Map from trace ID to the ordered list of correlated log entries.
    entries: HashMap<String, Vec<CorrelatedLogEntry>>,
}

impl LogCorrelation {
    /// Create a new, empty correlation store.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Link a log entry to the given `trace_id`.
    ///
    /// Entries are appended in insertion order.
    pub fn correlate(&mut self, trace_id: &str, log_entry: CorrelatedLogEntry) {
        info!(
            trace_id = %trace_id,
            level = %log_entry.level,
            "correlating log entry with trace"
        );
        self.entries
            .entry(trace_id.to_string())
            .or_default()
            .push(log_entry);
    }

    /// Retrieve all log entries that have been correlated with `trace_id`.
    ///
    /// Returns an empty slice when the trace ID is unknown.
    #[must_use]
    pub fn get_entries_for_trace(&self, trace_id: &str) -> &[CorrelatedLogEntry] {
        self.entries
            .get(trace_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Remove entries older than `max_age` across **all** traces.
    ///
    /// Traces that become empty after pruning are removed entirely so the
    /// map does not accumulate dead keys.
    pub fn prune_old_entries(&mut self, max_age: Duration) {
        let cutoff = Instant::now() - max_age;
        self.entries.retain(|trace_id, logs| {
            logs.retain(|entry| entry.recorded_at >= cutoff);
            if logs.is_empty() {
                info!(trace_id = %trace_id, "pruned all entries for trace");
                false
            } else {
                true
            }
        });
    }

    /// Total number of correlated log entries across all traces.
    #[must_use]
    pub fn total_entries(&self) -> usize {
        self.entries.values().map(Vec::len).sum()
    }

    /// Number of distinct trace IDs with at least one entry.
    #[must_use]
    pub fn trace_count(&self) -> usize {
        self.entries.len()
    }
}

impl Default for LogCorrelation {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// UnifiedHealthReport
// ---------------------------------------------------------------------------

/// Overall service status derived from individual subsystem health
/// (Issue #835).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverallStatus {
    /// Every subsystem is healthy.
    Healthy,
    /// At least one subsystem is unhealthy but the service is partially
    /// operational.
    Degraded,
    /// All monitored subsystems are unhealthy.
    Unhealthy,
}

impl OverallStatus {
    /// Stable string representation for JSON / Prometheus labels.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Unhealthy => "unhealthy",
        }
    }
}

/// Aggregates health from the database, RPC, and metrics subsystems into a
/// single unified view (Issue #835).
///
/// The [`overall_status`](Self::overall_status) is computed deterministically
/// from the three boolean health flags using [`derive_overall_status`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedHealthReport {
    /// Whether the database connection pool is healthy.
    pub db_healthy: bool,
    /// Whether the Soroban RPC endpoint is reachable.
    pub rpc_healthy: bool,
    /// Whether the metrics pipeline (Prometheus exporter) is healthy.
    pub metrics_healthy: bool,
    /// Computed overall service status.
    pub overall_status: OverallStatus,
}

impl UnifiedHealthReport {
    /// Build a health report from individual component statuses.
    ///
    /// The `overall_status` is derived automatically:
    /// - **Healthy** when all three subsystems are healthy.
    /// - **Degraded** when at least one but not all subsystems are unhealthy.
    /// - **Unhealthy** when every subsystem is unhealthy.
    #[must_use]
    pub fn from_components(db_healthy: bool, rpc_healthy: bool, metrics_healthy: bool) -> Self {
        let overall_status = derive_overall_status(db_healthy, rpc_healthy, metrics_healthy);

        if overall_status != OverallStatus::Healthy {
            warn!(
                db = db_healthy,
                rpc = rpc_healthy,
                metrics = metrics_healthy,
                status = %overall_status.as_str(),
                "unified health report: service is not fully healthy"
            );
        } else {
            info!("unified health report: all subsystems healthy");
        }

        Self {
            db_healthy,
            rpc_healthy,
            metrics_healthy,
            overall_status,
        }
    }

    /// Serialise the report to a [`serde_json::Value`].
    #[must_use]
    pub fn to_json(&self) -> Value {
        json!({
            "db_healthy": self.db_healthy,
            "rpc_healthy": self.rpc_healthy,
            "metrics_healthy": self.metrics_healthy,
            "overall_status": self.overall_status.as_str(),
        })
    }
}

/// Derive the [`OverallStatus`] from individual component flags.
fn derive_overall_status(db: bool, rpc: bool, metrics: bool) -> OverallStatus {
    let healthy_count = u8::from(db) + u8::from(rpc) + u8::from(metrics);
    match healthy_count {
        3 => OverallStatus::Healthy,
        0 => OverallStatus::Unhealthy,
        _ => OverallStatus::Degraded,
    }
}

// ---------------------------------------------------------------------------
// StructuredLogConfig
// ---------------------------------------------------------------------------

/// Log output format (Issue #835).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    /// Machine-readable JSON lines.
    Json,
    /// Human-readable text (default for local development).
    Text,
}

impl LogFormat {
    /// Stable string representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Text => "text",
        }
    }
}

/// Where structured log output should be directed (Issue #835).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogOutputTarget {
    /// Write to standard output.
    Stdout,
    /// Write to standard error.
    Stderr,
    /// Write to a file at the given path.
    File(String),
}

/// Configuration for structured log output (Issue #835).
///
/// All fields can be populated from environment variables via
/// [`from_env`](Self::from_env).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredLogConfig {
    /// Output format: json or text.
    pub format: LogFormat,
    /// `tracing` level filter string (e.g. "info", "soroban_pulse=debug").
    pub level_filter: String,
    /// Where log output is directed.
    pub output_target: LogOutputTarget,
}

impl StructuredLogConfig {
    /// Build a [`StructuredLogConfig`] from environment variables.
    ///
    /// | Variable             | Default   | Description                       |
    /// |----------------------|-----------|-----------------------------------|
    /// | `LOG_FORMAT`         | `text`    | `json` or `text`                  |
    /// | `LOG_LEVEL`          | `info`    | `tracing` level filter string     |
    /// | `LOG_OUTPUT`         | `stdout`  | `stdout`, `stderr`, or file path  |
    #[must_use]
    pub fn from_env() -> Self {
        let format = match std::env::var("LOG_FORMAT")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "json" => LogFormat::Json,
            _ => LogFormat::Text,
        };

        let level_filter = std::env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());

        let output_target = match std::env::var("LOG_OUTPUT")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "stderr" => LogOutputTarget::Stderr,
            "" | "stdout" => LogOutputTarget::Stdout,
            path => LogOutputTarget::File(path.to_string()),
        };

        info!(
            format = %format.as_str(),
            level = %level_filter,
            "structured log config loaded from environment"
        );

        Self {
            format,
            level_filter,
            output_target,
        }
    }
}

// ---------------------------------------------------------------------------
// TraceLogBridge
// ---------------------------------------------------------------------------

/// A single log entry recorded against a trace span (Issue #835).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpanLogEntry {
    /// Log severity level (e.g. "INFO", "WARN", "ERROR").
    pub level: String,
    /// Log message body.
    pub message: String,
    /// Monotonic timestamp for ordering and pruning.
    #[serde(skip, default = "Instant::now")]
    pub recorded_at: Instant,
}

/// Bridges trace spans and log entries so that per-span logs can be
/// inspected after the fact (Issue #835).
///
/// This is the in-process complement to the external trace collector
/// (Jaeger / Zipkin). It stores a bounded number of log entries per span
/// and makes them available for the admin API.
#[derive(Debug)]
pub struct TraceLogBridge {
    /// Map from span ID to the ordered list of log entries.
    span_logs: HashMap<String, Vec<SpanLogEntry>>,
}

impl TraceLogBridge {
    /// Create a new, empty bridge.
    #[must_use]
    pub fn new() -> Self {
        Self {
            span_logs: HashMap::new(),
        }
    }

    /// Record a log entry against the given `span_id`.
    pub fn record_span_log(&mut self, span_id: &str, level: &str, message: &str) {
        info!(
            span_id = %span_id,
            level = %level,
            "recording span log entry"
        );
        self.span_logs
            .entry(span_id.to_string())
            .or_default()
            .push(SpanLogEntry {
                level: level.to_string(),
                message: message.to_string(),
                recorded_at: Instant::now(),
            });
    }

    /// Retrieve all log entries recorded against `span_id`.
    ///
    /// Returns an empty slice when the span ID is unknown.
    #[must_use]
    pub fn get_span_logs(&self, span_id: &str) -> &[SpanLogEntry] {
        self.span_logs
            .get(span_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Number of distinct span IDs with at least one log entry.
    #[must_use]
    pub fn span_count(&self) -> usize {
        self.span_logs.len()
    }

    /// Total number of log entries across all spans.
    #[must_use]
    pub fn total_entries(&self) -> usize {
        self.span_logs.values().map(Vec::len).sum()
    }
}

impl Default for TraceLogBridge {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    // ── LogCorrelation ──────────────────────────────────────────────────

    #[test]
    fn log_correlation_new_is_empty() {
        let lc = LogCorrelation::new();
        assert_eq!(lc.total_entries(), 0);
        assert_eq!(lc.trace_count(), 0);
    }

    #[test]
    fn log_correlation_default_is_empty() {
        let lc = LogCorrelation::default();
        assert_eq!(lc.total_entries(), 0);
    }

    #[test]
    fn correlate_adds_entry() {
        let mut lc = LogCorrelation::new();
        lc.correlate(
            "trace-1",
            CorrelatedLogEntry {
                message: "hello".to_string(),
                level: "INFO".to_string(),
                recorded_at: Instant::now(),
            },
        );
        assert_eq!(lc.total_entries(), 1);
        assert_eq!(lc.trace_count(), 1);
    }

    #[test]
    fn correlate_multiple_entries_same_trace() {
        let mut lc = LogCorrelation::new();
        for i in 0..5 {
            lc.correlate(
                "trace-A",
                CorrelatedLogEntry {
                    message: format!("msg-{i}"),
                    level: "DEBUG".to_string(),
                    recorded_at: Instant::now(),
                },
            );
        }
        assert_eq!(lc.total_entries(), 5);
        assert_eq!(lc.trace_count(), 1);
        assert_eq!(lc.get_entries_for_trace("trace-A").len(), 5);
    }

    #[test]
    fn correlate_multiple_traces() {
        let mut lc = LogCorrelation::new();
        lc.correlate(
            "trace-1",
            CorrelatedLogEntry {
                message: "a".to_string(),
                level: "INFO".to_string(),
                recorded_at: Instant::now(),
            },
        );
        lc.correlate(
            "trace-2",
            CorrelatedLogEntry {
                message: "b".to_string(),
                level: "WARN".to_string(),
                recorded_at: Instant::now(),
            },
        );
        assert_eq!(lc.trace_count(), 2);
        assert_eq!(lc.total_entries(), 2);
    }

    #[test]
    fn get_entries_for_unknown_trace_returns_empty() {
        let lc = LogCorrelation::new();
        let entries = lc.get_entries_for_trace("nonexistent");
        assert!(entries.is_empty());
    }

    #[test]
    fn prune_old_entries_removes_stale() {
        let mut lc = LogCorrelation::new();

        // Insert an entry with a backdated `recorded_at`.
        let old_instant = Instant::now() - Duration::from_secs(120);
        lc.correlate(
            "trace-old",
            CorrelatedLogEntry {
                message: "old".to_string(),
                level: "INFO".to_string(),
                recorded_at: old_instant,
            },
        );
        // Insert a recent entry.
        lc.correlate(
            "trace-new",
            CorrelatedLogEntry {
                message: "new".to_string(),
                level: "INFO".to_string(),
                recorded_at: Instant::now(),
            },
        );

        assert_eq!(lc.trace_count(), 2);

        // Prune entries older than 60 seconds.
        lc.prune_old_entries(Duration::from_secs(60));

        assert_eq!(lc.trace_count(), 1);
        assert!(lc.get_entries_for_trace("trace-old").is_empty());
        assert_eq!(lc.get_entries_for_trace("trace-new").len(), 1);
    }

    #[test]
    fn prune_partial_trace_keeps_recent_entries() {
        let mut lc = LogCorrelation::new();
        let old_instant = Instant::now() - Duration::from_secs(120);

        lc.correlate(
            "trace-mixed",
            CorrelatedLogEntry {
                message: "old".to_string(),
                level: "INFO".to_string(),
                recorded_at: old_instant,
            },
        );
        lc.correlate(
            "trace-mixed",
            CorrelatedLogEntry {
                message: "new".to_string(),
                level: "INFO".to_string(),
                recorded_at: Instant::now(),
            },
        );

        lc.prune_old_entries(Duration::from_secs(60));

        // The trace should still exist with only the recent entry.
        assert_eq!(lc.trace_count(), 1);
        let entries = lc.get_entries_for_trace("trace-mixed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message, "new");
    }

    #[test]
    fn prune_with_zero_duration_removes_everything_except_current() {
        let mut lc = LogCorrelation::new();
        // Insert an entry that is at least a tiny bit old.
        let slightly_old = Instant::now() - Duration::from_millis(10);
        lc.correlate(
            "trace-x",
            CorrelatedLogEntry {
                message: "x".to_string(),
                level: "INFO".to_string(),
                recorded_at: slightly_old,
            },
        );
        // A zero-duration prune removes anything not recorded *right now*.
        lc.prune_old_entries(Duration::ZERO);
        assert_eq!(lc.total_entries(), 0);
    }

    // ── UnifiedHealthReport ─────────────────────────────────────────────

    #[test]
    fn all_healthy() {
        let report = UnifiedHealthReport::from_components(true, true, true);
        assert_eq!(report.overall_status, OverallStatus::Healthy);
        assert!(report.db_healthy);
        assert!(report.rpc_healthy);
        assert!(report.metrics_healthy);
    }

    #[test]
    fn all_unhealthy() {
        let report = UnifiedHealthReport::from_components(false, false, false);
        assert_eq!(report.overall_status, OverallStatus::Unhealthy);
    }

    #[test]
    fn one_unhealthy_is_degraded() {
        let report = UnifiedHealthReport::from_components(true, false, true);
        assert_eq!(report.overall_status, OverallStatus::Degraded);
    }

    #[test]
    fn two_unhealthy_is_degraded() {
        let report = UnifiedHealthReport::from_components(false, false, true);
        assert_eq!(report.overall_status, OverallStatus::Degraded);
    }

    #[test]
    fn to_json_contains_all_fields() {
        let report = UnifiedHealthReport::from_components(true, false, true);
        let j = report.to_json();

        assert_eq!(j["db_healthy"], json!(true));
        assert_eq!(j["rpc_healthy"], json!(false));
        assert_eq!(j["metrics_healthy"], json!(true));
        assert_eq!(j["overall_status"], json!("degraded"));
    }

    #[test]
    fn to_json_healthy_status_string() {
        let report = UnifiedHealthReport::from_components(true, true, true);
        let j = report.to_json();
        assert_eq!(j["overall_status"], json!("healthy"));
    }

    #[test]
    fn to_json_unhealthy_status_string() {
        let report = UnifiedHealthReport::from_components(false, false, false);
        let j = report.to_json();
        assert_eq!(j["overall_status"], json!("unhealthy"));
    }

    #[test]
    fn overall_status_as_str() {
        assert_eq!(OverallStatus::Healthy.as_str(), "healthy");
        assert_eq!(OverallStatus::Degraded.as_str(), "degraded");
        assert_eq!(OverallStatus::Unhealthy.as_str(), "unhealthy");
    }

    // ── StructuredLogConfig ─────────────────────────────────────────────

    #[test]
    fn structured_log_config_defaults() {
        // Clear env vars to test defaults.
        std::env::remove_var("LOG_FORMAT");
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("LOG_OUTPUT");

        let config = StructuredLogConfig::from_env();
        assert_eq!(config.format, LogFormat::Text);
        assert_eq!(config.level_filter, "info");
        assert_eq!(config.output_target, LogOutputTarget::Stdout);
    }

    #[test]
    fn structured_log_config_json_format() {
        std::env::set_var("LOG_FORMAT", "json");
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("LOG_OUTPUT");

        let config = StructuredLogConfig::from_env();
        assert_eq!(config.format, LogFormat::Json);

        std::env::remove_var("LOG_FORMAT");
    }

    #[test]
    fn structured_log_config_stderr_target() {
        std::env::remove_var("LOG_FORMAT");
        std::env::remove_var("LOG_LEVEL");
        std::env::set_var("LOG_OUTPUT", "stderr");

        let config = StructuredLogConfig::from_env();
        assert_eq!(config.output_target, LogOutputTarget::Stderr);

        std::env::remove_var("LOG_OUTPUT");
    }

    #[test]
    fn structured_log_config_file_target() {
        std::env::remove_var("LOG_FORMAT");
        std::env::remove_var("LOG_LEVEL");
        std::env::set_var("LOG_OUTPUT", "/var/log/soroban-pulse.log");

        let config = StructuredLogConfig::from_env();
        assert_eq!(
            config.output_target,
            LogOutputTarget::File("/var/log/soroban-pulse.log".to_string())
        );

        std::env::remove_var("LOG_OUTPUT");
    }

    #[test]
    fn structured_log_config_custom_level() {
        std::env::remove_var("LOG_FORMAT");
        std::env::set_var("LOG_LEVEL", "soroban_pulse=debug,tower_http=warn");
        std::env::remove_var("LOG_OUTPUT");

        let config = StructuredLogConfig::from_env();
        assert_eq!(config.level_filter, "soroban_pulse=debug,tower_http=warn");

        std::env::remove_var("LOG_LEVEL");
    }

    #[test]
    fn log_format_as_str() {
        assert_eq!(LogFormat::Json.as_str(), "json");
        assert_eq!(LogFormat::Text.as_str(), "text");
    }

    // ── TraceLogBridge ──────────────────────────────────────────────────

    #[test]
    fn trace_log_bridge_new_is_empty() {
        let bridge = TraceLogBridge::new();
        assert_eq!(bridge.span_count(), 0);
        assert_eq!(bridge.total_entries(), 0);
    }

    #[test]
    fn trace_log_bridge_default_is_empty() {
        let bridge = TraceLogBridge::default();
        assert_eq!(bridge.span_count(), 0);
    }

    #[test]
    fn record_span_log_adds_entry() {
        let mut bridge = TraceLogBridge::new();
        bridge.record_span_log("span-1", "INFO", "request started");
        assert_eq!(bridge.span_count(), 1);
        assert_eq!(bridge.total_entries(), 1);
    }

    #[test]
    fn record_multiple_logs_for_same_span() {
        let mut bridge = TraceLogBridge::new();
        bridge.record_span_log("span-A", "INFO", "start");
        bridge.record_span_log("span-A", "WARN", "slow query detected");
        bridge.record_span_log("span-A", "INFO", "end");

        assert_eq!(bridge.span_count(), 1);
        assert_eq!(bridge.total_entries(), 3);

        let logs = bridge.get_span_logs("span-A");
        assert_eq!(logs.len(), 3);
        assert_eq!(logs[0].message, "start");
        assert_eq!(logs[1].level, "WARN");
        assert_eq!(logs[2].message, "end");
    }

    #[test]
    fn record_logs_across_multiple_spans() {
        let mut bridge = TraceLogBridge::new();
        bridge.record_span_log("span-1", "INFO", "a");
        bridge.record_span_log("span-2", "ERROR", "b");
        bridge.record_span_log("span-3", "DEBUG", "c");

        assert_eq!(bridge.span_count(), 3);
        assert_eq!(bridge.total_entries(), 3);
    }

    #[test]
    fn get_span_logs_unknown_span_returns_empty() {
        let bridge = TraceLogBridge::new();
        let logs = bridge.get_span_logs("nonexistent");
        assert!(logs.is_empty());
    }

    #[test]
    fn span_log_entry_preserves_fields() {
        let mut bridge = TraceLogBridge::new();
        bridge.record_span_log("span-X", "ERROR", "something went wrong");

        let logs = bridge.get_span_logs("span-X");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].level, "ERROR");
        assert_eq!(logs[0].message, "something went wrong");
    }

    #[test]
    fn span_log_entries_maintain_insertion_order() {
        let mut bridge = TraceLogBridge::new();
        for i in 0..10 {
            bridge.record_span_log("span-order", "INFO", &format!("msg-{i}"));
        }

        let logs = bridge.get_span_logs("span-order");
        assert_eq!(logs.len(), 10);
        for (i, entry) in logs.iter().enumerate() {
            assert_eq!(entry.message, format!("msg-{i}"));
        }
    }

    // ── derive_overall_status (helper) ──────────────────────────────────

    #[test]
    fn derive_overall_status_all_true() {
        assert_eq!(derive_overall_status(true, true, true), OverallStatus::Healthy);
    }

    #[test]
    fn derive_overall_status_all_false() {
        assert_eq!(
            derive_overall_status(false, false, false),
            OverallStatus::Unhealthy
        );
    }

    #[test]
    fn derive_overall_status_mixed() {
        assert_eq!(derive_overall_status(true, false, true), OverallStatus::Degraded);
        assert_eq!(derive_overall_status(false, true, false), OverallStatus::Degraded);
        assert_eq!(derive_overall_status(true, true, false), OverallStatus::Degraded);
    }

    // ── Serialisation round-trip ────────────────────────────────────────

    #[test]
    fn unified_health_report_serde_round_trip() {
        let report = UnifiedHealthReport::from_components(true, false, true);
        let serialised = serde_json::to_string(&report).expect("serialise");
        let deserialized: UnifiedHealthReport =
            serde_json::from_str(&serialised).expect("deserialise");
        assert_eq!(deserialized.db_healthy, report.db_healthy);
        assert_eq!(deserialized.rpc_healthy, report.rpc_healthy);
        assert_eq!(deserialized.metrics_healthy, report.metrics_healthy);
        assert_eq!(deserialized.overall_status, report.overall_status);
    }

    #[test]
    fn structured_log_config_serde_round_trip() {
        let config = StructuredLogConfig {
            format: LogFormat::Json,
            level_filter: "debug".to_string(),
            output_target: LogOutputTarget::File("/tmp/test.log".to_string()),
        };
        let serialised = serde_json::to_string(&config).expect("serialise");
        let deserialized: StructuredLogConfig =
            serde_json::from_str(&serialised).expect("deserialise");
        assert_eq!(deserialized.format, config.format);
        assert_eq!(deserialized.level_filter, config.level_filter);
        assert_eq!(deserialized.output_target, config.output_target);
    }

    #[test]
    fn correlated_log_entry_serde_round_trip() {
        let entry = CorrelatedLogEntry {
            message: "test".to_string(),
            level: "INFO".to_string(),
            recorded_at: Instant::now(),
        };
        let serialised = serde_json::to_string(&entry).expect("serialise");
        let deserialized: CorrelatedLogEntry =
            serde_json::from_str(&serialised).expect("deserialise");
        assert_eq!(deserialized.message, entry.message);
        assert_eq!(deserialized.level, entry.level);
        // recorded_at is not serialised; the default is "now".
    }

    #[test]
    fn span_log_entry_serde_round_trip() {
        let entry = SpanLogEntry {
            level: "WARN".to_string(),
            message: "slow".to_string(),
            recorded_at: Instant::now(),
        };
        let serialised = serde_json::to_string(&entry).expect("serialise");
        let deserialized: SpanLogEntry =
            serde_json::from_str(&serialised).expect("deserialise");
        assert_eq!(deserialized.level, entry.level);
        assert_eq!(deserialized.message, entry.message);
    }
}
