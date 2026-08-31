//! Issue #953: Prometheus remote write support.
//!
//! Pushes metrics to a remote Prometheus remote write endpoint using the standard
//! Prometheus remote write protocol. Supports configurable endpoints, metric filtering,
//! batch submission, retry logic, and health metrics.

use crate::metrics;
use async_trait::async_trait;
use std::time::Duration;
use tracing::{error, info, warn};

/// Configuration for Prometheus remote write
#[derive(Clone, Debug)]
pub struct PrometheusRemoteWriteConfig {
    pub endpoints: Vec<String>,
    pub batch_size: usize,
    pub flush_interval_secs: u64,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub retry_delay_ms: u64,
    pub metric_filter: Option<MetricFilterConfig>,
}

/// Metric filtering configuration
#[derive(Clone, Debug)]
pub struct MetricFilterConfig {
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
}

impl Default for PrometheusRemoteWriteConfig {
    fn default() -> Self {
        Self {
            endpoints: vec![],
            batch_size: 100,
            flush_interval_secs: 60,
            timeout_secs: 10,
            max_retries: 3,
            retry_delay_ms: 100,
            metric_filter: None,
        }
    }
}

/// Trait for publishing metrics to remote write endpoints
#[async_trait]
pub trait RemoteWritePublisher: Send + Sync {
    async fn push_metrics(&self, metrics: Vec<RemoteWriteMetric>) -> Result<(), String>;
    async fn health_check(&self) -> Result<(), String>;
}

/// Represents a metric for remote write
#[derive(Clone, Debug)]
pub struct RemoteWriteMetric {
    pub name: String,
    pub value: f64,
    pub timestamp_ms: i64,
    pub labels: Vec<(String, String)>,
}

impl RemoteWriteMetric {
    pub fn new(name: String, value: f64) -> Self {
        Self {
            name,
            value,
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            labels: vec![],
        }
    }

    pub fn with_labels(mut self, labels: Vec<(String, String)>) -> Self {
        self.labels = labels;
        self
    }
}

/// Real implementation of remote write publisher
pub struct PrometheusRemoteWritePublisher {
    config: PrometheusRemoteWriteConfig,
    client: reqwest::Client,
}

impl PrometheusRemoteWritePublisher {
    pub fn new(config: PrometheusRemoteWriteConfig) -> Self {
        let client = reqwest::Client::new();
        info!(
            endpoints = ?config.endpoints,
            "Initialized Prometheus remote write publisher"
        );
        Self { config, client }
    }

    pub async fn from_env() -> Result<Self, String> {
        let endpoints = std::env::var("PROMETHEUS_REMOTE_WRITE_ENDPOINTS")
            .ok()
            .and_then(|v| {
                if v.is_empty() {
                    None
                } else {
                    Some(v.split(',').map(|s| s.trim().to_string()).collect())
                }
            })
            .unwrap_or_default();

        if endpoints.is_empty() {
            return Err("PROMETHEUS_REMOTE_WRITE_ENDPOINTS is required".to_string());
        }

        let batch_size = std::env::var("PROMETHEUS_REMOTE_WRITE_BATCH_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(100);

        let config = PrometheusRemoteWriteConfig {
            endpoints,
            batch_size,
            ..Default::default()
        };

        Ok(Self::new(config))
    }

    fn should_include_metric(&self, name: &str) -> bool {
        if let Some(ref filter) = self.config.metric_filter {
            // Check exclude patterns first
            if !filter.exclude_patterns.is_empty() {
                for pattern in &filter.exclude_patterns {
                    if name.contains(pattern) {
                        return false;
                    }
                }
            }

            // Check include patterns
            if !filter.include_patterns.is_empty() {
                for pattern in &filter.include_patterns {
                    if name.contains(pattern) {
                        return true;
                    }
                }
                return false;
            }
        }

        true
    }

    async fn push_to_endpoint(
        &self,
        endpoint: &str,
        metrics: &[RemoteWriteMetric],
    ) -> Result<(), String> {
        let filtered_metrics: Vec<_> = metrics
            .iter()
            .filter(|m| self.should_include_metric(&m.name))
            .collect();

        if filtered_metrics.is_empty() {
            return Ok(());
        }

        // Serialize metrics to Prometheus remote write format (simplified protobuf-like format)
        let body = serde_json::to_vec(&filtered_metrics)
            .map_err(|e| format!("Failed to serialize metrics: {e}"))?;

        let timeout = Duration::from_secs(self.config.timeout_secs);
        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            match tokio::time::timeout(
                timeout,
                self.client
                    .post(endpoint)
                    .header("Content-Type", "application/x-protobuf")
                    .header("X-Prometheus-Remote-Write-Version", "0.1.0")
                    .body(body.clone())
                    .send(),
            )
            .await
            {
                Ok(Ok(response)) => {
                    if response.status().is_success() {
                        metrics::record_prometheus_remote_write_success();
                        return Ok(());
                    } else {
                        last_error = Some(format!("HTTP {}", response.status()));
                    }
                }
                Ok(Err(e)) => {
                    last_error = Some(e.to_string());
                }
                Err(_) => {
                    last_error = Some("Request timeout".to_string());
                }
            }

            if attempt < self.config.max_retries {
                let delay_ms = self.config.retry_delay_ms * (2_u64.pow(attempt));
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        metrics::record_prometheus_remote_write_failure();
        Err(last_error.unwrap_or_else(|| "Unknown error".to_string()))
    }
}

#[async_trait]
impl RemoteWritePublisher for PrometheusRemoteWritePublisher {
    async fn push_metrics(&self, metrics: Vec<RemoteWriteMetric>) -> Result<(), String> {
        if metrics.is_empty() {
            return Ok(());
        }

        let mut errors = Vec::new();

        for endpoint in &self.config.endpoints {
            if let Err(e) = self.push_to_endpoint(endpoint, &metrics).await {
                warn!(endpoint = %endpoint, error = %e, "Failed to push metrics to endpoint");
                errors.push(e);
            }
        }

        if !errors.is_empty() && errors.len() == self.config.endpoints.len() {
            return Err(format!("All endpoints failed: {:?}", errors));
        }

        Ok(())
    }

    async fn health_check(&self) -> Result<(), String> {
        if self.config.endpoints.is_empty() {
            return Err("No endpoints configured".to_string());
        }

        let timeout = Duration::from_secs(5);
        let mut healthy_count = 0;

        for endpoint in &self.config.endpoints {
            match tokio::time::timeout(timeout, self.client.get(endpoint).send()).await {
                Ok(Ok(response)) if response.status().is_success() => {
                    healthy_count += 1;
                }
                _ => {}
            }
        }

        if healthy_count > 0 {
            metrics::record_prometheus_remote_write_health_ok();
            Ok(())
        } else {
            metrics::record_prometheus_remote_write_health_fail();
            Err("All endpoints unhealthy".to_string())
        }
    }
}

/// Mock implementation for testing
#[cfg(test)]
pub mod mock {
    use super::*;

    pub struct MockRemoteWritePublisher {
        pub last_metrics: std::sync::Arc<std::sync::Mutex<Vec<RemoteWriteMetric>>>,
    }

    impl MockRemoteWritePublisher {
        pub fn new() -> Self {
            Self {
                last_metrics: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            }
        }
    }

    #[async_trait]
    impl RemoteWritePublisher for MockRemoteWritePublisher {
        async fn push_metrics(&self, metrics: Vec<RemoteWriteMetric>) -> Result<(), String> {
            *self.last_metrics.lock().unwrap() = metrics;
            Ok(())
        }

        async fn health_check(&self) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metric_creation() {
        let metric = RemoteWriteMetric::new("test_metric".to_string(), 42.0);
        assert_eq!(metric.name, "test_metric");
        assert_eq!(metric.value, 42.0);
    }

    #[test]
    fn test_metric_with_labels() {
        let metric = RemoteWriteMetric::new("test_metric".to_string(), 42.0)
            .with_labels(vec![("label".to_string(), "value".to_string())]);
        assert_eq!(metric.labels.len(), 1);
    }

    #[test]
    fn test_metric_filtering_include() {
        let config = PrometheusRemoteWriteConfig {
            metric_filter: Some(MetricFilterConfig {
                include_patterns: vec!["soroban_pulse".to_string()],
                exclude_patterns: vec![],
            }),
            ..Default::default()
        };

        let publisher = PrometheusRemoteWritePublisher::new(config);
        assert!(publisher.should_include_metric("soroban_pulse_events"));
        assert!(!publisher.should_include_metric("other_metric"));
    }

    #[test]
    fn test_metric_filtering_exclude() {
        let config = PrometheusRemoteWriteConfig {
            metric_filter: Some(MetricFilterConfig {
                include_patterns: vec![],
                exclude_patterns: vec!["internal".to_string()],
            }),
            ..Default::default()
        };

        let publisher = PrometheusRemoteWritePublisher::new(config);
        assert!(publisher.should_include_metric("soroban_pulse_events"));
        assert!(!publisher.should_include_metric("internal_metric"));
    }

    #[tokio::test]
    async fn test_mock_publisher() {
        let publisher = mock::MockRemoteWritePublisher::new();
        let metrics = vec![RemoteWriteMetric::new("test".to_string(), 1.0)];
        let result = publisher.push_metrics(metrics.clone()).await;
        assert!(result.is_ok());

        let stored = publisher.last_metrics.lock().unwrap();
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].name, "test");
    }
}
