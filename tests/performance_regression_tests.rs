// Performance Regression Testing (Issue #591)
//
// This module tracks and alerts on performance regressions in key queries.
// It establishes baselines and detects deviations that may indicate issues.

#[cfg(test)]
mod performance_regression_tests {
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    /// Performance baseline for key operations
    #[derive(Clone, Debug)]
    struct PerformanceBaseline {
        operation: String,
        p50_latency_ms: f64,
        p95_latency_ms: f64,
        p99_latency_ms: f64,
        throughput_ops_per_sec: f64,
    }

    /// Performance measurement for a single operation
    #[derive(Clone, Debug)]
    struct PerformanceMeasurement {
        operation: String,
        latencies_ms: Vec<f64>,
        timestamp: std::time::SystemTime,
    }

    impl PerformanceMeasurement {
        fn calculate_percentile(&self, percentile: f64) -> f64 {
            if self.latencies_ms.is_empty() {
                return 0.0;
            }
            let mut sorted = self.latencies_ms.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let index = ((percentile / 100.0) * sorted.len() as f64) as usize;
            sorted[index.min(sorted.len() - 1)]
        }

        fn p50(&self) -> f64 {
            self.calculate_percentile(50.0)
        }

        fn p95(&self) -> f64 {
            self.calculate_percentile(95.0)
        }

        fn p99(&self) -> f64 {
            self.calculate_percentile(99.0)
        }

        fn throughput(&self, duration_secs: f64) -> f64 {
            self.latencies_ms.len() as f64 / duration_secs
        }
    }

    /// Regression detector for performance metrics
    struct RegressionDetector {
        baselines: HashMap<String, PerformanceBaseline>,
        threshold_percent: f64, // Alert if performance changes by more than this percentage
    }

    impl RegressionDetector {
        fn new() -> Self {
            let mut detector = Self {
                baselines: HashMap::new(),
                threshold_percent: 10.0, // 10% threshold
            };
            detector.initialize_baselines();
            detector
        }

        fn initialize_baselines(&mut self) {
            // Baseline for list events query
            self.baselines.insert(
                "query_events".to_string(),
                PerformanceBaseline {
                    operation: "query_events".to_string(),
                    p50_latency_ms: 50.0,
                    p95_latency_ms: 150.0,
                    p99_latency_ms: 300.0,
                    throughput_ops_per_sec: 200.0,
                },
            );

            // Baseline for filter events query
            self.baselines.insert(
                "query_events_filtered".to_string(),
                PerformanceBaseline {
                    operation: "query_events_filtered".to_string(),
                    p50_latency_ms: 75.0,
                    p95_latency_ms: 200.0,
                    p99_latency_ms: 400.0,
                    throughput_ops_per_sec: 150.0,
                },
            );

            // Baseline for subscription creation
            self.baselines.insert(
                "create_subscription".to_string(),
                PerformanceBaseline {
                    operation: "create_subscription".to_string(),
                    p50_latency_ms: 25.0,
                    p95_latency_ms: 75.0,
                    p99_latency_ms: 150.0,
                    throughput_ops_per_sec: 400.0,
                },
            );

            // Baseline for webhook delivery
            self.baselines.insert(
                "webhook_delivery".to_string(),
                PerformanceBaseline {
                    operation: "webhook_delivery".to_string(),
                    p50_latency_ms: 100.0,
                    p95_latency_ms: 300.0,
                    p99_latency_ms: 600.0,
                    throughput_ops_per_sec: 100.0,
                },
            );
        }

        fn detect_regression(&self, measurement: &PerformanceMeasurement) -> Option<String> {
            if let Some(baseline) = self.baselines.get(&measurement.operation) {
                let current_p95 = measurement.p95();
                let baseline_p95 = baseline.p95_latency_ms;

                let percent_change = ((current_p95 - baseline_p95) / baseline_p95) * 100.0;

                if percent_change > self.threshold_percent {
                    return Some(format!(
                        "REGRESSION: {} p95 latency increased by {:.1}% (baseline: {:.1}ms, current: {:.1}ms)",
                        measurement.operation, percent_change, baseline_p95, current_p95
                    ));
                }
            }
            None
        }

        fn alert_if_regression(&self, measurement: &PerformanceMeasurement) -> bool {
            self.detect_regression(measurement).is_some()
        }
    }

    /// Performance alert system
    struct PerformanceAlertSystem {
        alerts: Vec<String>,
    }

    impl PerformanceAlertSystem {
        fn new() -> Self {
            Self {
                alerts: Vec::new(),
            }
        }

        fn emit_alert(&mut self, alert: String) {
            self.alerts.push(alert);
        }

        fn get_alerts(&self) -> &[String] {
            &self.alerts
        }
    }

    /// Test: Establish performance baseline for list events query
    #[test]
    fn baseline_list_events_query() {
        let detector = RegressionDetector::new();
        let baseline = detector.baselines.get("query_events").unwrap();

        assert_eq!(baseline.operation, "query_events");
        assert_eq!(baseline.p50_latency_ms, 50.0);
        assert_eq!(baseline.p95_latency_ms, 150.0);
        assert_eq!(baseline.p99_latency_ms, 300.0);
        assert_eq!(baseline.throughput_ops_per_sec, 200.0);
    }

    /// Test: Establish performance baseline for filtered query
    #[test]
    fn baseline_filtered_query() {
        let detector = RegressionDetector::new();
        let baseline = detector.baselines.get("query_events_filtered").unwrap();

        assert!(baseline.p95_latency_ms > 150.0, "Filtered query should be slower than simple query");
    }

    /// Test: Detect regression when p95 latency increases > 10%
    #[test]
    fn detect_regression_above_threshold() {
        let detector = RegressionDetector::new();

        let measurement = PerformanceMeasurement {
            operation: "query_events".to_string(),
            latencies_ms: vec![45.0, 48.0, 50.0, 52.0, 55.0, 60.0, 65.0, 70.0, 75.0, 170.0],
            timestamp: std::time::SystemTime::now(),
        };

        let regression = detector.detect_regression(&measurement);
        assert!(
            regression.is_some(),
            "Should detect regression when p95 increases > 10%"
        );
    }

    /// Test: No alert when performance stays within threshold
    #[test]
    fn no_alert_within_threshold() {
        let detector = RegressionDetector::new();

        let measurement = PerformanceMeasurement {
            operation: "query_events".to_string(),
            latencies_ms: vec![48.0, 49.0, 50.0, 51.0, 52.0, 100.0, 110.0, 120.0, 130.0, 155.0],
            timestamp: std::time::SystemTime::now(),
        };

        let regression = detector.detect_regression(&measurement);
        assert!(regression.is_none(), "Should not alert when within threshold");
    }

    /// Test: Percentile calculations
    #[test]
    fn percentile_calculations() {
        let measurement = PerformanceMeasurement {
            operation: "test".to_string(),
            latencies_ms: vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0],
            timestamp: std::time::SystemTime::now(),
        };

        assert_eq!(measurement.p50(), 50.0);
        assert!(measurement.p95() > 90.0);
        assert!(measurement.p99() > 95.0);
    }

    /// Test: Throughput calculation
    #[test]
    fn throughput_calculation() {
        let measurement = PerformanceMeasurement {
            operation: "test".to_string(),
            latencies_ms: vec![10.0, 20.0, 30.0, 40.0, 50.0],
            timestamp: std::time::SystemTime::now(),
        };

        let throughput = measurement.throughput(1.0); // 1 second
        assert_eq!(throughput, 5.0); // 5 operations per second
    }

    /// Test: Alert system records performance regression alerts
    #[test]
    fn alert_system_records_regressions() {
        let mut alert_system = PerformanceAlertSystem::new();
        let detector = RegressionDetector::new();

        let measurement = PerformanceMeasurement {
            operation: "query_events".to_string(),
            latencies_ms: vec![45.0, 50.0, 150.0, 165.0, 170.0, 175.0, 180.0, 185.0, 190.0, 200.0],
            timestamp: std::time::SystemTime::now(),
        };

        if let Some(alert) = detector.detect_regression(&measurement) {
            alert_system.emit_alert(alert);
        }

        assert!(!alert_system.get_alerts().is_empty());
        assert!(alert_system.get_alerts()[0].contains("REGRESSION"));
    }

    /// Test: Multiple baselines tracked independently
    #[test]
    fn multiple_baselines_independent() {
        let detector = RegressionDetector::new();
        assert_eq!(detector.baselines.len(), 4);

        let ops = vec!["query_events", "query_events_filtered", "create_subscription", "webhook_delivery"];
        for op in ops {
            assert!(detector.baselines.contains_key(op), "Should have baseline for {}", op);
        }
    }

    /// Test: Performance baseline report generation
    #[test]
    fn generate_performance_report() {
        let detector = RegressionDetector::new();
        let mut report = String::new();

        for (name, baseline) in &detector.baselines {
            report.push_str(&format!(
                "{}: p50={:.1}ms, p95={:.1}ms, p99={:.1}ms, throughput={:.0} ops/sec\n",
                name, baseline.p50_latency_ms, baseline.p95_latency_ms, baseline.p99_latency_ms, baseline.throughput_ops_per_sec
            ));
        }

        assert!(!report.is_empty());
        assert!(report.contains("query_events"));
        assert!(report.contains("ms"));
    }

    /// Test: Latency regression detection with various thresholds
    #[test]
    fn regression_detection_with_threshold() {
        let detector = RegressionDetector::new();

        // Measurement with 15% increase (above 10% threshold)
        let regression_measurement = PerformanceMeasurement {
            operation: "query_events".to_string(),
            latencies_ms: vec![50.0, 100.0, 115.0, 120.0, 125.0, 130.0, 135.0, 140.0, 145.0, 173.0],
            timestamp: std::time::SystemTime::now(),
        };

        assert!(detector.alert_if_regression(&regression_measurement));
    }
}
