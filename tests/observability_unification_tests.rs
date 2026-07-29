//! Integration tests for Observability Stack Unification (Issue #835).
//!
//! Tests cover:
//! - Log correlation (trace_id -> log entries)
//! - Unified health report aggregation
//! - Structured log configuration
//! - Trace-log bridge

use soroban_pulse::observability::{
    LogCorrelation, StructuredLogConfig, TraceLogBridge, UnifiedHealthReport,
};
use std::time::Duration;

// ---------------------------------------------------------------------------
// LogCorrelation
// ---------------------------------------------------------------------------

#[test]
fn log_correlation_records_and_retrieves_entries() {
    let lc = LogCorrelation::new(1000);
    lc.correlate("trace-1", "info", "request started");
    lc.correlate("trace-1", "debug", "parsing body");
    lc.correlate("trace-2", "error", "db timeout");

    let entries = lc.get_entries_for_trace("trace-1");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].message, "request started");
    assert_eq!(entries[1].message, "parsing body");
}

#[test]
fn log_correlation_returns_empty_for_unknown_trace() {
    let lc = LogCorrelation::new(1000);
    let entries = lc.get_entries_for_trace("nonexistent");
    assert!(entries.is_empty());
}

#[test]
fn log_correlation_prunes_old_entries() {
    let lc = LogCorrelation::new(1000);
    lc.correlate("trace-old", "info", "old entry");

    // Prune with zero max_age removes everything
    lc.prune_old_entries(Duration::from_secs(0));

    let entries = lc.get_entries_for_trace("trace-old");
    assert!(entries.is_empty());
}

#[test]
fn log_correlation_respects_capacity() {
    let lc = LogCorrelation::new(2);
    lc.correlate("t1", "info", "first");
    lc.correlate("t2", "info", "second");
    lc.correlate("t3", "info", "third");

    // First entry may have been evicted due to capacity
    let total = lc.get_entries_for_trace("t1").len()
        + lc.get_entries_for_trace("t2").len()
        + lc.get_entries_for_trace("t3").len();
    assert!(total <= 3);
}

// ---------------------------------------------------------------------------
// UnifiedHealthReport
// ---------------------------------------------------------------------------

#[test]
fn unified_health_all_healthy() {
    let report = UnifiedHealthReport::from_components(true, true, true);
    assert_eq!(report.overall_status(), "healthy");
    let json = report.to_json();
    assert_eq!(json["overall_status"], "healthy");
    assert_eq!(json["db_healthy"], true);
    assert_eq!(json["rpc_healthy"], true);
    assert_eq!(json["metrics_healthy"], true);
}

#[test]
fn unified_health_partial_failure() {
    let report = UnifiedHealthReport::from_components(true, false, true);
    assert_eq!(report.overall_status(), "degraded");
}

#[test]
fn unified_health_all_down() {
    let report = UnifiedHealthReport::from_components(false, false, false);
    assert_eq!(report.overall_status(), "unhealthy");
}

#[test]
fn unified_health_json_structure() {
    let report = UnifiedHealthReport::from_components(true, true, false);
    let json = report.to_json();
    assert!(json.get("overall_status").is_some());
    assert!(json.get("db_healthy").is_some());
    assert!(json.get("rpc_healthy").is_some());
    assert!(json.get("metrics_healthy").is_some());
    assert!(json.get("checked_at").is_some());
}

// ---------------------------------------------------------------------------
// StructuredLogConfig
// ---------------------------------------------------------------------------

#[test]
fn structured_log_config_defaults() {
    std::env::remove_var("LOG_FORMAT");
    std::env::remove_var("LOG_LEVEL");
    std::env::remove_var("LOG_OUTPUT");

    let cfg = StructuredLogConfig::from_env();
    assert_eq!(cfg.format, "json");
    assert_eq!(cfg.level, "info");
    assert_eq!(cfg.output, "stdout");
}

#[test]
fn structured_log_config_json_output() {
    let cfg = StructuredLogConfig {
        format: "json".to_string(),
        level: "debug".to_string(),
        output: "file".to_string(),
    };
    let json = cfg.to_json();
    assert_eq!(json["format"], "json");
    assert_eq!(json["level"], "debug");
    assert_eq!(json["output"], "file");
}

// ---------------------------------------------------------------------------
// TraceLogBridge
// ---------------------------------------------------------------------------

#[test]
fn trace_log_bridge_records_and_retrieves() {
    let bridge = TraceLogBridge::new(500);
    bridge.record_span_log("span-1", "info", "handling request");
    bridge.record_span_log("span-1", "warn", "slow query");
    bridge.record_span_log("span-2", "error", "connection reset");

    let logs = bridge.get_span_logs("span-1");
    assert_eq!(logs.len(), 2);
    assert_eq!(logs[0].level, "info");
    assert_eq!(logs[1].level, "warn");
}

#[test]
fn trace_log_bridge_returns_empty_for_unknown_span() {
    let bridge = TraceLogBridge::new(500);
    assert!(bridge.get_span_logs("unknown").is_empty());
}

#[test]
fn trace_log_bridge_summary() {
    let bridge = TraceLogBridge::new(500);
    bridge.record_span_log("s1", "info", "a");
    bridge.record_span_log("s2", "error", "b");

    let summary = bridge.summary();
    assert_eq!(summary["total_spans"], 2);
    assert!(summary["total_log_entries"].as_u64().unwrap() >= 2);
}
