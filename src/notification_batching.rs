//! Notification batching: groups outbound notifications to reduce per-message
//! overhead and improve delivery throughput.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Configuration controlling how notifications are grouped into batches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchConfig {
    pub enabled: bool,
    /// Flush the batch once it reaches this many notifications.
    pub max_batch_size: usize,
    /// Flush the batch after this much time has elapsed since the first item was added,
    /// even if `max_batch_size` has not been reached.
    pub max_batch_window: Duration,
    /// Maximum number of batches allowed to be in flight concurrently.
    pub max_concurrent_batches: usize,
    /// Drop duplicate notifications (same dedup key) within a batch.
    pub dedup_enabled: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_batch_size: 50,
            max_batch_window: Duration::from_secs(5),
            max_concurrent_batches: 10,
            dedup_enabled: true,
        }
    }
}

/// A single notification queued for batched delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedNotification {
    pub id: String,
    pub dedup_key: Option<String>,
    pub channel: String,
    pub payload: String,
}

/// Reason a batch was flushed, useful for metrics and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlushReason {
    SizeThreshold,
    TimeWindow,
    Manual,
}

/// Result of flushing a batch: the deduplicated notifications and stats about the flush.
#[derive(Debug, Clone)]
pub struct FlushedBatch {
    pub notifications: Vec<QueuedNotification>,
    pub duplicates_dropped: usize,
    pub reason: FlushReason,
}

/// Accumulates notifications for a single channel/tenant until a size or time
/// threshold is hit, then flushes them as one batch.
pub struct NotificationBatcher {
    config: BatchConfig,
    pending: Vec<QueuedNotification>,
    seen_dedup_keys: HashSet<String>,
    batch_started_at: Option<Instant>,
    metrics: Arc<BatchMetrics>,
}

impl NotificationBatcher {
    pub fn new(config: BatchConfig) -> Self {
        Self {
            config,
            pending: Vec::new(),
            seen_dedup_keys: HashSet::new(),
            batch_started_at: None,
            metrics: BatchMetrics::shared(),
        }
    }

    pub fn with_metrics(config: BatchConfig, metrics: Arc<BatchMetrics>) -> Self {
        Self {
            config,
            pending: Vec::new(),
            seen_dedup_keys: HashSet::new(),
            batch_started_at: None,
            metrics,
        }
    }

    pub fn metrics(&self) -> Arc<BatchMetrics> {
        self.metrics.clone()
    }

    /// Adds a notification to the pending batch. Returns `Some(FlushedBatch)` if
    /// adding this notification pushed the batch over the size threshold.
    pub fn add(&mut self, notification: QueuedNotification) -> Option<FlushedBatch> {
        if !self.config.enabled {
            return Some(FlushedBatch {
                notifications: vec![notification],
                duplicates_dropped: 0,
                reason: FlushReason::Manual,
            });
        }

        if self.config.dedup_enabled {
            if let Some(key) = &notification.dedup_key {
                if !self.seen_dedup_keys.insert(key.clone()) {
                    self.metrics.duplicates_dropped.fetch_add(1, Ordering::Relaxed);
                    return None;
                }
            }
        }

        if self.batch_started_at.is_none() {
            self.batch_started_at = Some(Instant::now());
        }
        self.pending.push(notification);
        self.metrics.notifications_enqueued.fetch_add(1, Ordering::Relaxed);

        if self.pending.len() >= self.config.max_batch_size {
            return Some(self.flush(FlushReason::SizeThreshold));
        }
        None
    }

    /// Checks whether the time window has elapsed and flushes if so.
    pub fn poll_time_window(&mut self) -> Option<FlushedBatch> {
        if !self.config.enabled || self.pending.is_empty() {
            return None;
        }
        if let Some(started) = self.batch_started_at {
            if started.elapsed() >= self.config.max_batch_window {
                return Some(self.flush(FlushReason::TimeWindow));
            }
        }
        None
    }

    /// Forces a flush of whatever is currently pending, e.g. during shutdown.
    pub fn flush_now(&mut self) -> Option<FlushedBatch> {
        if self.pending.is_empty() {
            return None;
        }
        Some(self.flush(FlushReason::Manual))
    }

    fn flush(&mut self, reason: FlushReason) -> FlushedBatch {
        let notifications = std::mem::take(&mut self.pending);
        self.batch_started_at = None;
        self.seen_dedup_keys.clear();
        self.metrics.batches_flushed.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .notifications_delivered
            .fetch_add(notifications.len() as u64, Ordering::Relaxed);
        FlushedBatch {
            notifications,
            duplicates_dropped: 0,
            reason,
        }
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Errors that can occur while delivering a flushed batch downstream.
#[derive(Debug, thiserror::Error)]
pub enum BatchDeliveryError {
    #[error("batch delivery timed out after {0:?}")]
    Timeout(Duration),
    #[error("downstream channel rejected batch: {0}")]
    ChannelRejected(String),
    #[error("batch exceeded max concurrent batches limit ({0})")]
    ConcurrencyLimitExceeded(usize),
}

/// Handles a delivery failure by deciding whether to retry the whole batch,
/// split it, or drop it — recording the outcome in metrics either way.
pub fn handle_batch_error(
    batch: &FlushedBatch,
    error: &BatchDeliveryError,
    metrics: &BatchMetrics,
) -> BatchErrorAction {
    metrics.delivery_failures.fetch_add(1, Ordering::Relaxed);
    match error {
        BatchDeliveryError::Timeout(_) if batch.notifications.len() > 1 => {
            BatchErrorAction::SplitAndRetry
        }
        BatchDeliveryError::ConcurrencyLimitExceeded(_) => BatchErrorAction::RequeueLater,
        _ => BatchErrorAction::RetryWhole,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchErrorAction {
    RetryWhole,
    SplitAndRetry,
    RequeueLater,
}

/// Metrics for the batching subsystem, safe to share across batchers.
#[derive(Debug, Default)]
pub struct BatchMetrics {
    pub notifications_enqueued: AtomicU64,
    pub notifications_delivered: AtomicU64,
    pub batches_flushed: AtomicU64,
    pub duplicates_dropped: AtomicU64,
    pub delivery_failures: AtomicU64,
}

impl BatchMetrics {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn average_batch_size(&self) -> f64 {
        let flushed = self.batches_flushed.load(Ordering::Relaxed);
        if flushed == 0 {
            return 0.0;
        }
        self.notifications_delivered.load(Ordering::Relaxed) as f64 / flushed as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(id: &str, dedup_key: Option<&str>) -> QueuedNotification {
        QueuedNotification {
            id: id.to_string(),
            dedup_key: dedup_key.map(|s| s.to_string()),
            channel: "webhook".to_string(),
            payload: "{}".to_string(),
        }
    }

    #[test]
    fn flushes_on_size_threshold() {
        let config = BatchConfig {
            max_batch_size: 2,
            ..BatchConfig::default()
        };
        let mut batcher = NotificationBatcher::new(config);
        assert!(batcher.add(notification("1", None)).is_none());
        let flushed = batcher.add(notification("2", None)).unwrap();
        assert_eq!(flushed.notifications.len(), 2);
        assert_eq!(flushed.reason, FlushReason::SizeThreshold);
    }

    #[test]
    fn flushes_on_time_window() {
        let config = BatchConfig {
            max_batch_size: 100,
            max_batch_window: Duration::from_millis(1),
            ..BatchConfig::default()
        };
        let mut batcher = NotificationBatcher::new(config);
        batcher.add(notification("1", None));
        std::thread::sleep(Duration::from_millis(5));
        let flushed = batcher.poll_time_window().unwrap();
        assert_eq!(flushed.reason, FlushReason::TimeWindow);
        assert_eq!(flushed.notifications.len(), 1);
    }

    #[test]
    fn deduplicates_within_batch() {
        let mut batcher = NotificationBatcher::new(BatchConfig::default());
        batcher.add(notification("1", Some("key-a")));
        let result = batcher.add(notification("2", Some("key-a")));
        assert!(result.is_none());
        assert_eq!(batcher.pending_count(), 1);
        assert_eq!(
            batcher.metrics().duplicates_dropped.load(Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn disabled_batching_delivers_immediately() {
        let config = BatchConfig {
            enabled: false,
            ..BatchConfig::default()
        };
        let mut batcher = NotificationBatcher::new(config);
        let flushed = batcher.add(notification("1", None)).unwrap();
        assert_eq!(flushed.notifications.len(), 1);
    }

    #[test]
    fn flush_now_drains_pending() {
        let mut batcher = NotificationBatcher::new(BatchConfig::default());
        batcher.add(notification("1", None));
        let flushed = batcher.flush_now().unwrap();
        assert_eq!(flushed.notifications.len(), 1);
        assert_eq!(batcher.pending_count(), 0);
    }

    #[test]
    fn timeout_error_on_multi_item_batch_splits_and_retries() {
        let metrics = BatchMetrics::default();
        let batch = FlushedBatch {
            notifications: vec![notification("1", None), notification("2", None)],
            duplicates_dropped: 0,
            reason: FlushReason::SizeThreshold,
        };
        let action = handle_batch_error(
            &batch,
            &BatchDeliveryError::Timeout(Duration::from_secs(1)),
            &metrics,
        );
        assert_eq!(action, BatchErrorAction::SplitAndRetry);
    }

    #[test]
    fn average_batch_size_computed_from_metrics() {
        let metrics = BatchMetrics::default();
        metrics.notifications_delivered.store(10, Ordering::Relaxed);
        metrics.batches_flushed.store(2, Ordering::Relaxed);
        assert_eq!(metrics.average_batch_size(), 5.0);
    }
}
