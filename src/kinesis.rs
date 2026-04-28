//! Issue #265: AWS Kinesis Data Streams producer integration.
//!
//! When `KINESIS_STREAM_NAME` and `AWS_REGION` are configured, each indexed
//! event is published to the Kinesis stream as a JSON record with the event's
//! `contract_id` as the partition key. Authentication uses the standard AWS
//! credential chain (env vars, instance profile, ECS task role) via `aws-config`.
//!
//! This module is compiled only when the `kinesis` feature flag is enabled.

use crate::metrics;
use crate::models::SorobanEvent;
use async_trait::async_trait;
use tracing::{error, info};

/// Trait for publishing events to a stream, enabling mock testing.
#[async_trait]
pub trait KinesisPublisher: Send + Sync {
    async fn publish(&self, event: &SorobanEvent) -> Result<(), String>;
}

// ── Real AWS Kinesis implementation ──────────────────────────────────────────

#[cfg(feature = "kinesis")]
pub mod aws {
    use super::*;
    use aws_sdk_kinesis::primitives::Blob;
    use aws_sdk_kinesis::Client;

    pub struct AwsKinesisPublisher {
        client: Client,
        stream_name: String,
    }

    impl AwsKinesisPublisher {
        /// Build a publisher from the standard AWS credential chain.
        pub async fn from_env(stream_name: String, region: String) -> Self {
            let sdk_config = aws_config::from_env()
                .region(aws_sdk_kinesis::config::Region::new(region))
                .load()
                .await;
            let client = Client::new(&sdk_config);
            info!(stream = %stream_name, "Kinesis publisher initialised");
            Self {
                client,
                stream_name,
            }
        }
    }

    #[async_trait]
    impl KinesisPublisher for AwsKinesisPublisher {
        async fn publish(&self, event: &SorobanEvent) -> Result<(), String> {
            let payload =
                serde_json::to_vec(event).map_err(|e| format!("serialisation error: {e}"))?;

            self.client
                .put_record()
                .stream_name(&self.stream_name)
                .partition_key(&event.contract_id)
                .data(Blob::new(payload))
                .send()
                .await
                .map_err(|e| {
                    let msg = e.to_string();
                    error!(stream = %self.stream_name, error = %msg, "Kinesis publish failed");
                    metrics::record_kinesis_publish_failure();
                    msg
                })?;

            Ok(())
        }
    }
}

// ── Shared helper used by the indexer ────────────────────────────────────────

/// Publish an event, logging and metering failures without propagating them.
pub async fn publish_event(publisher: &dyn KinesisPublisher, event: &SorobanEvent) {
    if let Err(e) = publisher.publish(event).await {
        error!(error = %e, "Failed to publish event to Kinesis");
        metrics::record_kinesis_publish_failure();
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct MockKinesisPublisher {
        published: Arc<Mutex<Vec<String>>>,
        fail: bool,
    }

    #[async_trait]
    impl KinesisPublisher for MockKinesisPublisher {
        async fn publish(&self, event: &SorobanEvent) -> Result<(), String> {
            if self.fail {
                return Err("mock error".to_string());
            }
            self.published
                .lock()
                .unwrap()
                .push(event.contract_id.clone());
            Ok(())
        }
    }

    fn make_event() -> SorobanEvent {
        SorobanEvent {
            contract_id: "CABC123".into(),
            event_type: "contract".into(),
            tx_hash: "deadbeef".into(),
            ledger: 100,
            ledger_closed_at: "2026-04-27T00:00:00Z".into(),
            value: Value::Null,
            topic: None,
        }
    }

    #[tokio::test]
    async fn mock_publisher_records_event() {
        let published = Arc::new(Mutex::new(vec![]));
        let mock = MockKinesisPublisher {
            published: published.clone(),
            fail: false,
        };
        mock.publish(&make_event()).await.unwrap();
        assert_eq!(*published.lock().unwrap(), vec!["CABC123"]);
    }

    #[tokio::test]
    async fn mock_publisher_returns_error_on_failure() {
        let mock = MockKinesisPublisher {
            fail: true,
            ..Default::default()
        };
        assert!(mock.publish(&make_event()).await.is_err());
    }

    #[tokio::test]
    async fn publish_event_does_not_panic_on_failure() {
        let mock = MockKinesisPublisher {
            fail: true,
            ..Default::default()
        };
        // Should not panic or propagate the error
        publish_event(&mock, &make_event()).await;
    }

    #[tokio::test]
    async fn publish_event_uses_contract_id_as_partition_key() {
        let published = Arc::new(Mutex::new(vec![]));
        let mock = MockKinesisPublisher {
            published: published.clone(),
            fail: false,
        };
        let mut event = make_event();
        event.contract_id = "CDEF456".into();
        publish_event(&mock, &event).await;
        assert_eq!(*published.lock().unwrap(), vec!["CDEF456"]);
    }
}
