//! Kafka producer integration (feature = "kafka").
//!
//! Defines a `KafkaPublisher` trait so the indexer can be tested with a mock
//! without pulling in the real rdkafka machinery.

use crate::models::SorobanEvent;
use async_trait::async_trait;

/// Publish a single event to Kafka.
#[async_trait]
pub trait KafkaPublisher: Send + Sync {
    async fn publish(&self, topic: &str, event: &SorobanEvent) -> Result<(), String>;
}

// ── Real producer (only compiled when the kafka feature is active) ────────────

#[cfg(feature = "kafka")]
pub use real::RdKafkaProducer;

#[cfg(feature = "kafka")]
mod real {
    use super::*;
    use rdkafka::config::ClientConfig;
    use rdkafka::producer::{FutureProducer, FutureRecord};
    use std::time::Duration;

    pub struct RdKafkaProducer {
        inner: FutureProducer,
    }

    impl RdKafkaProducer {
        /// Build a producer from the application config.
        pub fn new(brokers: &str, batch_size: usize, linger_ms: u64) -> Result<Self, String> {
            let producer: FutureProducer = ClientConfig::new()
                .set("bootstrap.servers", brokers)
                .set("batch.size", batch_size.to_string())
                .set("linger.ms", linger_ms.to_string())
                // Async delivery: fire-and-forget with acks=1 for throughput.
                .set("acks", "1")
                .create()
                .map_err(|e| format!("Failed to create Kafka producer: {e}"))?;
            Ok(Self { inner: producer })
        }
    }

    #[async_trait]
    impl super::KafkaPublisher for RdKafkaProducer {
        async fn publish(&self, topic: &str, event: &SorobanEvent) -> Result<(), String> {
            let payload = serde_json::to_string(event)
                .map_err(|e| format!("Failed to serialize event: {e}"))?;
            let key = event.contract_id.as_str();

            self.inner
                .send(
                    FutureRecord::to(topic).key(key).payload(&payload),
                    Duration::from_secs(5),
                )
                .await
                .map(|_| ())
                .map_err(|(e, _)| format!("Kafka delivery failed: {e}"))
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    /// Mock publisher that records every (topic, event) pair it receives.
    #[derive(Clone, Default)]
    pub struct MockKafkaPublisher {
        pub published: Arc<Mutex<Vec<(String, SorobanEvent)>>>,
        /// When set, every publish call returns this error.
        pub fail_with: Arc<Mutex<Option<String>>>,
    }

    #[async_trait]
    impl KafkaPublisher for MockKafkaPublisher {
        async fn publish(&self, topic: &str, event: &SorobanEvent) -> Result<(), String> {
            if let Some(ref msg) = *self.fail_with.lock().unwrap() {
                return Err(msg.clone());
            }
            self.published
                .lock()
                .unwrap()
                .push((topic.to_string(), event.clone()));
            Ok(())
        }
    }

    fn make_event(contract_id: &str) -> SorobanEvent {
        SorobanEvent {
            contract_id: contract_id.to_string(),
            event_type: "contract".to_string(),
            tx_hash: "abc123".to_string(),
            ledger: 42,
            ledger_closed_at: "2026-03-14T00:00:00Z".to_string(),
            value: Value::Null,
            topic: None,
        }
    }

    #[tokio::test]
    async fn mock_records_published_events() {
        let publisher = MockKafkaPublisher::default();
        let event = make_event("CABC");

        publisher.publish("events", &event).await.unwrap();

        let published = publisher.published.lock().unwrap();
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, "events");
        assert_eq!(published[0].1.contract_id, "CABC");
    }

    #[tokio::test]
    async fn mock_uses_contract_id_as_key() {
        // The real producer uses contract_id as the Kafka message key.
        // Verify the mock stores the event with the correct contract_id so
        // downstream assertions can confirm key routing.
        let publisher = MockKafkaPublisher::default();
        let e1 = make_event("CONTRACT_A");
        let e2 = make_event("CONTRACT_B");

        publisher.publish("events", &e1).await.unwrap();
        publisher.publish("events", &e2).await.unwrap();

        let published = publisher.published.lock().unwrap();
        assert_eq!(published[0].1.contract_id, "CONTRACT_A");
        assert_eq!(published[1].1.contract_id, "CONTRACT_B");
    }

    #[tokio::test]
    async fn mock_returns_error_when_configured() {
        let publisher = MockKafkaPublisher::default();
        *publisher.fail_with.lock().unwrap() = Some("broker unavailable".to_string());

        let result = publisher.publish("events", &make_event("CABC")).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("broker unavailable"));
    }

    #[tokio::test]
    async fn failed_publish_does_not_record_event() {
        let publisher = MockKafkaPublisher::default();
        *publisher.fail_with.lock().unwrap() = Some("timeout".to_string());

        let _ = publisher.publish("events", &make_event("CABC")).await;
        assert!(publisher.published.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn multiple_events_same_topic_all_recorded() {
        let publisher = MockKafkaPublisher::default();
        for i in 0..5 {
            let event = make_event(&format!("CONTRACT_{i}"));
            publisher.publish("my-topic", &event).await.unwrap();
        }
        assert_eq!(publisher.published.lock().unwrap().len(), 5);
    }
}
