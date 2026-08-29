/// Issue #878: GraphQL subscriptions WebSocket transport layer
/// Provides WebSocket connection management with timeout and keep-alive support

use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade}, State},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::{interval, timeout};
use tracing::{error, info, warn};

use crate::{models::SorobanEvent, routes::AppState};

/// Default WebSocket connection timeout (5 minutes)
const DEFAULT_CONNECTION_TIMEOUT_SECS: u64 = 300;

/// Default keep-alive ping interval (30 seconds)
const DEFAULT_KEEPALIVE_INTERVAL_SECS: u64 = 30;

/// Configuration for GraphQL WebSocket subscriptions
#[derive(Debug, Clone)]
pub struct GraphQLSubscriptionConfig {
    /// Connection timeout in seconds
    pub connection_timeout_secs: u64,
    /// Keep-alive ping interval in seconds
    pub keepalive_interval_secs: u64,
    /// Maximum message size in bytes
    pub max_message_size: usize,
    /// Broadcast channel capacity
    pub channel_capacity: usize,
}

impl Default for GraphQLSubscriptionConfig {
    fn default() -> Self {
        Self {
            connection_timeout_secs: DEFAULT_CONNECTION_TIMEOUT_SECS,
            keepalive_interval_secs: DEFAULT_KEEPALIVE_INTERVAL_SECS,
            max_message_size: 65536,
            channel_capacity: 100,
        }
    }
}

/// Handle incoming WebSocket connection for GraphQL subscriptions
pub async fn handle_graphql_ws(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let config = GraphQLSubscriptionConfig::default();

    // Split the socket into sender and receiver
    let (mut sender, mut receiver) = socket.split();

    // Subscribe to the event broadcast channel
    let mut event_rx = state.event_tx.subscribe();

    // Spawn a task to handle incoming messages
    let sender_clone = sender.clone();
    let incoming_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
            match msg {
                Ok(axum::extract::ws::Message::Text(text)) => {
                    info!("Received WebSocket message: {}", text);
                }
                Ok(axum::extract::ws::Message::Close(frame)) => {
                    info!("WebSocket close frame received: {:?}", frame);
                    let _ = sender_clone.send(axum::extract::ws::Message::Close(frame)).await;
                    return;
                }
                Ok(axum::extract::ws::Message::Ping(data)) => {
                    let _ = sender_clone.send(axum::extract::ws::Message::Pong(data)).await;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    return;
                }
                _ => {}
            }
        }
    });

    // Spawn a task to handle keep-alive pings
    let mut sender_clone = sender.clone();
    let keepalive_task = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(config.keepalive_interval_secs));
        loop {
            ticker.tick().await;
            if let Err(e) = sender_clone.send(axum::extract::ws::Message::Ping(vec![])).await {
                error!("Failed to send keep-alive ping: {}", e);
                return;
            }
        }
    });

    // Spawn a task to broadcast events to the client
    let mut sender_clone = sender.clone();
    let event_task = tokio::spawn(async move {
        loop {
            match timeout(
                Duration::from_secs(config.connection_timeout_secs),
                event_rx.recv(),
            )
            .await
            {
                Ok(Ok(event)) => {
                    let msg = format!(
                        r#"{{"type":"event","data":{{"id":"{}","contract_id":"{}","event_type":"{}","ledger":{},"tx_hash":"{}"}}}}"#,
                        event.id.unwrap_or_default(),
                        event.contract_id,
                        event.event_type,
                        event.ledger,
                        event.tx_hash
                    );

                    if let Err(e) = sender_clone.send(axum::extract::ws::Message::Text(msg)).await {
                        error!("Failed to send event to WebSocket client: {}", e);
                        return;
                    }
                }
                Ok(Err(_)) => {
                    // Broadcast channel error, likely due to sender being dropped
                    info!("Broadcast channel closed");
                    return;
                }
                Err(_) => {
                    warn!("Connection timeout reached");
                    let _ = sender_clone.send(axum::extract::ws::Message::Close(None)).await;
                    return;
                }
            }
        }
    });

    // Wait for any task to finish
    tokio::select! {
        _ = &mut tokio::pin!(incoming_task) => {
            info!("Incoming message handler finished");
            keepalive_task.abort();
            event_task.abort();
        }
        _ = &mut tokio::pin!(keepalive_task) => {
            info!("Keep-alive task finished");
            incoming_task.abort();
            event_task.abort();
        }
        _ = &mut tokio::pin!(event_task) => {
            info!("Event broadcast task finished");
            incoming_task.abort();
            keepalive_task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_reasonable_values() {
        let config = GraphQLSubscriptionConfig::default();
        assert_eq!(config.connection_timeout_secs, 300);
        assert_eq!(config.keepalive_interval_secs, 30);
        assert!(config.max_message_size > 0);
        assert!(config.channel_capacity > 0);
    }

    #[test]
    fn connection_timeout_is_greater_than_keepalive() {
        let config = GraphQLSubscriptionConfig::default();
        assert!(config.connection_timeout_secs > config.keepalive_interval_secs);
    }
}
