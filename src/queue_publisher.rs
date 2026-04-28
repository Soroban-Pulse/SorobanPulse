//! Message queue publisher for indexed events
//!
//! Publishes events to Redis Streams (with Kafka/RabbitMQ support planned).
//! Publishing is non-blocking and includes retry logic with exponential backoff.

use crate::models::SorobanEvent;
use serde_json::json;
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{error, info, warn};

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 100;

#[cfg(feature = "redis-queue")]
mod redis_impl {
    use super::*;
    use redis::aio::ConnectionManager;
    use redis::{AsyncCommands, RedisError};

    pub struct RedisPublisher {
        client: ConnectionManager,
        stream_key: String,
    }

    impl RedisPublisher {
        pub async fn new(redis_url: &str, stream_key: String) -> Result<Self, RedisError> {
            let client = redis::Client::open(redis_url)?;
            let conn = ConnectionManager::new(client).await?;
            
            info!(
                redis_url = %Self::safe_redis_url(redis_url),
                stream_key = %stream_key,
                "Redis publisher initialized"
            );
            
            Ok(Self {
                client: conn,
                stream_key,
            })
        }

        /// Strip credentials from Redis URL for safe logging
        fn safe_redis_url(url: &str) -> String {
            if let Ok(parsed) = url::Url::parse(url) {
                let mut safe = parsed.clone();
                let _ = safe.set_username("");
                let _ = safe.set_password(None);
                safe.to_string()
            } else {
                "<unparseable>".to_string()
            }
        }

        pub async fn publish(&mut self, event: &SorobanEvent) -> Result<(), RedisError> {
            let event_json = json!({
                "contract_id": event.contract_id,
                "event_type": event.event_type,
                "tx_hash": event.tx_hash,
                "ledger": event.ledger,
                "ledger_closed_at": event.ledger_closed_at,
                "value": event.value,
                "topic": event.topic,
            });

            // Convert JSON to flat key-value pairs for Redis hash
            let fields: Vec<(&str, String)> = vec![
                ("contract_id", event.contract_id.clone()),
                ("event_type", event.event_type.clone()),
                ("tx_hash", event.tx_hash.clone()),
                ("ledger", event.ledger.to_string()),
                ("ledger_closed_at", event.ledger_closed_at.clone()),
                ("value", event.value.to_string()),
                ("topic", event.topic.as_ref().map(|t| t.to_string()).unwrap_or_else(|| "null".to_string())),
            ];

            self.client
                .xadd(&self.stream_key, "*", &fields)
                .await?;

            Ok(())
        }
    }

    pub async fn spawn_redis_publisher(
        redis_url: String,
        stream_key: String,
        mut event_rx: broadcast::Receiver<SorobanEvent>,
    ) {
        let mut publisher = match RedisPublisher::new(&redis_url, stream_key).await {
            Ok(p) => p,
            Err(e) => {
                error!(error = %e, "Failed to initialize Redis publisher");
                return;
            }
        };

        info!("Redis publisher task started");

        loop {
            match event_rx.recv().await {
                Ok(event) => {
                    if let Err(e) = publish_with_retry(&mut publisher, &event).await {
                        error!(
                            contract_id = %event.contract_id,
                            tx_hash = %event.tx_hash,
                            error = %e,
                            "Failed to publish event to Redis after retries"
                        );
                        crate::metrics::record_queue_publish_failure();
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!(skipped = n, "Redis publisher lagged, some events skipped");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("Event channel closed, stopping Redis publisher");
                    break;
                }
            }
        }
    }

    async fn publish_with_retry(
        publisher: &mut RedisPublisher,
        event: &SorobanEvent,
    ) -> Result<(), String> {
        let mut attempt = 0;
        let mut backoff_ms = INITIAL_BACKOFF_MS;

        loop {
            match publisher.publish(event).await {
                Ok(()) => return Ok(()),
                Err(e) => {
                    attempt += 1;
                    if attempt >= MAX_RETRIES {
                        return Err(format!("Max retries exceeded: {}", e));
                    }

                    warn!(
                        attempt = attempt,
                        backoff_ms = backoff_ms,
                        error = %e,
                        "Redis publish failed, retrying"
                    );

                    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                    backoff_ms *= 2; // Exponential backoff
                }
            }
        }
    }
}

#[cfg(feature = "redis-queue")]
pub use redis_impl::spawn_redis_publisher;

/// No-op stub when redis-queue feature is disabled
#[cfg(not(feature = "redis-queue"))]
pub async fn spawn_redis_publisher(
    _redis_url: String,
    _stream_key: String,
    _event_rx: broadcast::Receiver<SorobanEvent>,
) {
    warn!("Redis publisher requested but redis-queue feature is not enabled");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_retries_constant() {
        assert_eq!(MAX_RETRIES, 3);
    }

    #[test]
    fn test_initial_backoff() {
        assert_eq!(INITIAL_BACKOFF_MS, 100);
    }

    #[cfg(feature = "redis-queue")]
    #[test]
    fn test_safe_redis_url() {
        use super::redis_impl::RedisPublisher;
        
        let url = "redis://user:password@localhost:6379/0";
        let safe = RedisPublisher::safe_redis_url(url);
        assert!(!safe.contains("password"));
        assert!(!safe.contains("user"));
        assert!(safe.contains("localhost"));
    }

    #[cfg(feature = "redis-queue")]
    #[test]
    fn test_safe_redis_url_unparseable() {
        use super::redis_impl::RedisPublisher;
        
        let url = "not-a-url";
        let safe = RedisPublisher::safe_redis_url(url);
        assert_eq!(safe, "<unparseable>");
    }
}
