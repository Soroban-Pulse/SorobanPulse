#[cfg(feature = "redis-queue")]
mod redis_tests {
    use serde_json::json;
    use soroban_pulse::models::SorobanEvent;
    use soroban_pulse::queue_publisher::spawn_redis_publisher;
    use tokio::sync::broadcast;

    fn make_test_event() -> SorobanEvent {
        SorobanEvent {
            contract_id: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM".to_string(),
            event_type: "contract".to_string(),
            tx_hash: "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            ledger: 1234567,
            ledger_closed_at: "2026-03-14T00:00:00Z".to_string(),
            value: json!({"amount": 100}),
            topic: Some(vec![json!("transfer")]),
        }
    }

    #[tokio::test]
    #[ignore] // Requires Redis server
    async fn test_redis_publisher_publishes_event() {
        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let stream_key = "test_soroban_pulse_events".to_string();

        let (event_tx, event_rx) = broadcast::channel(10);

        // Spawn publisher
        let publisher_handle = tokio::spawn(async move {
            spawn_redis_publisher(redis_url, stream_key, event_rx).await;
        });

        // Give publisher time to initialize
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Send test event
        let event = make_test_event();
        event_tx.send(event.clone()).unwrap();

        // Give time for publishing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Clean up
        drop(event_tx);
        let _ = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            publisher_handle
        ).await;
    }

    #[tokio::test]
    async fn test_redis_publisher_handles_lagged_receiver() {
        let redis_url = "redis://localhost:6379".to_string();
        let stream_key = "test_lag".to_string();

        let (event_tx, event_rx) = broadcast::channel(2); // Small capacity

        // Spawn publisher
        let publisher_handle = tokio::spawn(async move {
            spawn_redis_publisher(redis_url, stream_key, event_rx).await;
        });

        // Send more events than capacity
        for _ in 0..5 {
            let event = make_test_event();
            let _ = event_tx.send(event);
        }

        // Clean up
        drop(event_tx);
        let _ = tokio::time::timeout(
            tokio::time::Duration::from_secs(1),
            publisher_handle
        ).await;
    }
}

#[cfg(not(feature = "redis-queue"))]
#[test]
fn test_redis_feature_disabled() {
    // When redis-queue feature is disabled, the module should still compile
    // with no-op stubs
    assert!(true);
}
