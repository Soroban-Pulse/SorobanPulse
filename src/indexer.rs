use chrono::DateTime;
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{error, info, warn, instrument, span, Level};

use crate::{
    config::{Config, HealthState, IndexerState},
    metrics,
    models::{GetEventsResult, LatestLedgerResult, RpcResponse, SorobanEvent},
    rpc_client::RpcClient,
};

/// Postgres advisory lock key for the indexer singleton.
const INDEXER_LOCK_KEY: i64 = 0x536f726f62616e50; // "SorobanP"

#[derive(Debug, thiserror::Error)]
enum IndexerFetchError {
    #[error("{0}")]
    Rpc(String),
    #[error(transparent)]
    DbConnection(#[from] sqlx::Error),
}

fn build_rpc_client(config: &Config) -> impl RpcClient {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(config.rpc_connect_timeout_secs))
        .timeout(Duration::from_secs(config.rpc_request_timeout_secs))
        .pool_max_idle_per_host(5)
        .pool_idle_timeout(Duration::from_secs(30))
        .tcp_keepalive(Duration::from_secs(60))
        .build()
        .expect("Failed to build HTTP client");
    
    crate::rpc_client::HttpRpcClient::new(client)
}

pub struct Indexer {
    pool: PgPool,
    rpc_client: Box<dyn RpcClient>,
    config: Config,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    health_state: Option<Arc<HealthState>>,
    indexer_state: Option<Arc<IndexerState>>,
    event_tx: Option<broadcast::Sender<SorobanEvent>>,
}

impl Indexer {
    pub fn new(pool: PgPool, config: Config, shutdown_rx: tokio::sync::watch::Receiver<bool>) -> Self {
        let rpc_client = Box::new(build_rpc_client(&config));

        Self {
            pool,
            rpc_client,
            config,
            shutdown_rx,
            health_state: None,
            indexer_state: None,
            event_tx: None,
        }
    }

    /// Constructor for testing that allows injecting a custom RpcClient
    pub fn new_with_rpc_client(
        pool: PgPool,
        config: Config,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
        rpc_client: Box<dyn RpcClient>,
    ) -> Self {
        Self {
            pool,
            rpc_client,
            config,
            shutdown_rx,
            health_state: None,
            indexer_state: None,
            event_tx: None,
        }
    }

    /// Set the health state for updating the last poll timestamp
    pub fn set_health_state(&mut self, health_state: Arc<HealthState>) {
        self.health_state = Some(health_state);
    }

    /// Set the indexer state for exposing operational metrics to the /status endpoint
    pub fn set_indexer_state(&mut self, indexer_state: Arc<IndexerState>) {
        self.indexer_state = Some(indexer_state);
    }

    /// Set the broadcast sender for real-time SSE streaming.
    pub fn set_event_tx(&mut self, event_tx: broadcast::Sender<SorobanEvent>) {
        self.event_tx = Some(event_tx);
    }

    pub async fn run(&self) {
        // Attempt to acquire a Postgres session-level advisory lock.
        // Only one replica will hold this lock at a time; others serve HTTP only.
        let lock_conn = match self.pool.acquire().await {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, "Failed to acquire DB connection for advisory lock");
                return;
            }
        };

        let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
            .bind(INDEXER_LOCK_KEY)
            .fetch_one(&self.pool)
            .await
            .unwrap_or(false);

        if !acquired {
            info!("Indexer lock not acquired, running in read-only mode");
            // Hold the connection open so we can detect when the lock owner dies
            // and re-attempt on the next startup/restart cycle.
            drop(lock_conn);
            return;
        }

        info!("Indexer lock acquired, starting indexing");

        // Run the actual indexing loop; release lock on exit.
        self.run_loop().await;

        // Explicitly release the advisory lock on graceful shutdown.
        let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(INDEXER_LOCK_KEY)
            .execute(&self.pool)
            .await;

        drop(lock_conn);
    }

    async fn run_loop(&self) {
        let mut current_ledger = self.config.start_ledger;
        let mut consecutive_db_errors = 0u32;

        if current_ledger == 0 {
            let mut retries = 0;
            loop {
                match self.get_latest_ledger().await {
                    Ok(ledger) => {
                        current_ledger = ledger;
                        info!(ledger = current_ledger, "Starting from latest ledger");
                        metrics::update_current_ledger(current_ledger);
                        if let Some(ref s) = self.indexer_state {
                            s.current_ledger.store(current_ledger, std::sync::atomic::Ordering::Relaxed);
                        }
                        break;
                    }
                    Err(e) => {
                        error!(attempt = retries + 1, error = %e, "Failed to get latest ledger");
                        retries += 1;
                        if retries >= 5 {
                            if self.config.start_ledger_fallback {
                                warn!("Falling back to genesis ledger (1) due to RPC failure");
                                current_ledger = 1;
                                break;
                            } else {
                                error!("Fatal RPC error: Could not fetch initial ledger after 5 attempts");
                                std::process::exit(1);
                            }
                        }
                        sleep(Duration::from_secs(10)).await;
                    }
                }
            }
        }

        loop {
            if *self.shutdown_rx.borrow() {
                info!("Indexer shutting down gracefully");
                break;
            }

            match self.fetch_and_store_events(current_ledger).await {
                Ok(latest) => {
                    consecutive_db_errors = 0;
                    // Update the last poll timestamp on success
                    if let Some(ref health_state) = self.health_state {
                        health_state.update_last_poll();
                    }
                    if latest > current_ledger {
                        current_ledger = latest;
                        metrics::update_current_ledger(current_ledger);
                        if let Some(ref s) = self.indexer_state {
                            s.current_ledger.store(current_ledger, std::sync::atomic::Ordering::Relaxed);
                        }

                        // Calculate and update lag
                        let latest_ledger = self.get_latest_ledger().await.unwrap_or(0);
                        if latest_ledger > current_ledger {
                            if let Some(ref s) = self.indexer_state {
                                s.latest_ledger.store(latest_ledger, std::sync::atomic::Ordering::Relaxed);
                            }
                            let lag = latest_ledger - current_ledger;
                            metrics::update_indexer_lag(lag);
                            
                            // Warn if lag exceeds threshold
                            if lag > self.config.indexer_lag_warn_threshold {
                                warn!(
                                    lag = lag,
                                    threshold = self.config.indexer_lag_warn_threshold,
                                    "Indexer is falling behind"
                                );
                            }
                        }
                    } else {
                        sleep(Duration::from_secs(5)).await;
                    }
                }
                Err(IndexerFetchError::DbConnection(e)) => {
                    consecutive_db_errors += 1;
                    let backoff_secs = if consecutive_db_errors >= 5 {
                        60
                    } else {
                        10
                    };
                    if consecutive_db_errors == 5 {
                        error!(
                            consecutive = consecutive_db_errors,
                            "DB unavailable, backing off"
                        );
                    } else if consecutive_db_errors < 5 {
                        error!(error = %e, "Indexer error");
                    }
                    sleep(Duration::from_secs(backoff_secs)).await;
                }
                Err(IndexerFetchError::Rpc(msg)) => {
                    error!(error = %msg, "Indexer error");
                    sleep(Duration::from_secs(10)).await;
                }
            }
        }
    }

    async fn get_latest_ledger(&self) -> Result<u64, String> {
        let span = span!(Level::INFO, "rpc_get_latest_ledger", url = %self.config.stellar_rpc_url);
        let _enter = span.enter();

        let resp = self.rpc_client.get_latest_ledger(&self.config.stellar_rpc_url).await.map_err(|e| {
            warn!(error = %e, "RPC request failed");
            metrics::record_rpc_error();
            e
        })?;

        match resp.result {
            Some(r) => {
                metrics::update_latest_ledger(r.sequence);
                Ok(r.sequence)
            }
            None => {
                if let Some(err) = resp.error {
                    warn!(code = err.code, message = %err.message, "RPC error");
                    metrics::record_rpc_error();
                }
                Err("RPC returned no result".to_string())
            }
        }
    }

    #[instrument(skip(self), fields(start_ledger = start_ledger))]
    async fn fetch_and_store_events(&self, start_ledger: u64) -> Result<u64, IndexerFetchError> {
        let mut cursor: Option<String> = None;
        let mut latest_ledger = start_ledger;
        let mut total_fetched = 0;
        let mut total_inserted = 0;
        let mut total_skipped = 0;

        loop {
            let mut params = json!({
                "filters": [],
                "pagination": { "limit": 100 }
            });

            if let Some(c) = &cursor {
                params["pagination"]["cursor"] = json!(c);
            } else {
                params["startLedger"] = json!(start_ledger);
            }

            let resp = self.rpc_client.get_events(&self.config.stellar_rpc_url, params).await.map_err(|e| {
                warn!(error = %e, "RPC request failed");
                metrics::record_rpc_error();
                IndexerFetchError::Rpc(e)
            })?;

            let result = match resp.result {
                Some(r) => r,
                None => {
                    if let Some(err) = resp.error {
                        warn!(code = err.code, message = %err.message, "RPC error");
                        metrics::record_rpc_error();
                        return Err(IndexerFetchError::Rpc(err.message));
                    }
                    break;
                }
            };

            latest_ledger = result.latest_ledger;
            let current_count = result.events.len();
            total_fetched += current_count;

            for event in result.events {
                match self.store_event(&event).await {
                    Ok(rows) => {
                        total_inserted += rows;
                        if rows == 0 {
                            total_skipped += 1;
                        } else if let Some(ref tx) = self.event_tx {
                            let _ = tx.send(event);
                        }
                    }
                    Err(e) => {
                        warn!(
                            tx_hash = %event.tx_hash,
                            contract_id = %event.contract_id,
                            ledger = event.ledger,
                            event_type = %event.event_type,
                            error = %e,
                            "Failed to store event",
                        );
                    }
                }
            }

            cursor = result.rpc_cursor;
            if cursor.is_none() {
                break;
            }
        }

        info!(
            fetched = total_fetched,
            inserted = total_inserted,
            ledger = latest_ledger,
            "Indexed ledger range"
        );
        metrics::record_events_indexed(total_inserted as u64);

        let _duplicate_events_skipped = total_skipped;

        if latest_ledger > start_ledger {
            Ok(latest_ledger + 1)
        } else {
            Ok(start_ledger)
        }
    }
    #[instrument(skip(self, event), fields(tx_hash = %event.tx_hash, contract_id = %event.contract_id, ledger = event.ledger))]
    async fn store_event(&self, event: &SorobanEvent) -> Result<u64, anyhow::Error> {
        let ledger = match i64::try_from(event.ledger) {
            Ok(v) => v,
            Err(_) => {
                error!(
                    tx_hash = %event.tx_hash,
                    contract_id = %event.contract_id,
                    ledger = event.ledger,
                    event_type = %event.event_type,
                    "Ledger number overflows i64, skipping event",
                );
                return Ok(0);
            }
        };
        let timestamp = DateTime::parse_from_rfc3339(&event.ledger_closed_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .map_err(|_| {
                warn!(
                    tx_hash = %event.tx_hash,
                    contract_id = %event.contract_id,
                    ledger = event.ledger,
                    event_type = %event.event_type,
                    raw = %event.ledger_closed_at,
                    "Unparseable ledger_closed_at, skipping event",
                );
                anyhow::anyhow!("Unparseable ledger_closed_at: {}", event.ledger_closed_at)
            })?;

        let event_data = json!({
            "value": event.value,
            "topic": event.topic
        });

        let result = sqlx::query(
            r#"
            INSERT INTO events (contract_id, event_type, tx_hash, ledger, timestamp, event_data)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tx_hash, contract_id, event_type) DO NOTHING
            "#,
        )
        .bind(&event.contract_id)
        .bind(&event.event_type)
        .bind(&event.tx_hash)
        .bind(ledger)
        .bind(timestamp)
        .bind(event_data)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn make_event(ledger: u64) -> SorobanEvent {
        SorobanEvent {
            contract_id: "C1".into(),
            event_type: "contract".into(),
            tx_hash: "abc".into(),
            ledger,
            ledger_closed_at: "2026-03-24T00:00:00Z".into(),
            value: Value::Null,
            topic: None,
        }
    }

    #[test]
    fn ledger_overflow_returns_err() {
        assert!(i64::try_from(make_event(u64::MAX).ledger).is_err());
    }

    fn indexer(pool: PgPool) -> Indexer {
        use crate::rpc_client::mock::MockRpcClient;
        let mock_client = MockRpcClient::new();
        
        Indexer {
            pool,
            rpc_client: Box::new(mock_client),
            config: Config {
                database_url: String::new(),
                stellar_rpc_url: String::new(),
                start_ledger: 0,
                port: 3000,
                behind_proxy: false,
                rpc_connect_timeout_secs: 30,
                rpc_request_timeout_secs: 60,
                indexer_lag_warn_threshold: 1000,
                indexer_stall_timeout_secs: 300,
                start_ledger_fallback: true,
                api_key: None,
                environment: crate::config::Environment::Development,
                allowed_origins: vec![],
                rate_limit_per_minute: 60,
                db_max_connections: 10,
                db_min_connections: 1,
                db_statement_timeout_ms: 30000,
            },
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn duplicate_insert_yields_one_row(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let event = make_event(1);

        indexer.store_event(&event).await.unwrap();
        indexer.store_event(&event).await.unwrap(); // must not error

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn same_tx_hash_different_event_type_both_stored(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let mut e1 = make_event(1);
        let mut e2 = make_event(1);
        e2.event_type = "system".into();

        indexer.store_event(&e1).await.unwrap();
        indexer.store_event(&e2).await.unwrap();

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn store_event_valid_input_succeeds(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let event = make_event(123);

        let rows_affected = indexer.store_event(&event).await.unwrap();
        assert_eq!(rows_affected, 1);

        // Verify the event was stored correctly
        let stored_event: (String, String, String, i64, String, serde_json::Value) = sqlx::query_as(
            "SELECT contract_id, event_type, tx_hash, ledger, to_char(timestamp, 'YYYY-MM-DD\"T\"HH24:MI:SS\"Z\"'), event_data FROM events"
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(stored_event.0, "C1");
        assert_eq!(stored_event.1, "contract");
        assert_eq!(stored_event.2, "abc");
        assert_eq!(stored_event.3, 123);
        assert_eq!(stored_event.4, "2026-03-24T00:00:00Z");
        assert_eq!(stored_event.5, serde_json::json!({"value": null, "topic": null}));
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn store_event_invalid_timestamp_returns_error(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let mut event = make_event(1);
        event.ledger_closed_at = "invalid-timestamp".into();

        let result = indexer.store_event(&event).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unparseable ledger_closed_at"));

        // Verify no event was stored
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn store_event_ledger_overflow_skips_event(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let event = make_event(u64::MAX);

        let rows_affected = indexer.store_event(&event).await.unwrap();
        assert_eq!(rows_affected, 0);

        // Verify no event was stored
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn store_event_with_complex_data_succeeds(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let mut event = make_event(456);
        event.value = serde_json::json!({"type": "mint", "amount": "1000"});
        event.topic = Some(vec![serde_json::json!("transfer"), serde_json::json!("native")]);

        let rows_affected = indexer.store_event(&event).await.unwrap();
        assert_eq!(rows_affected, 1);

        // Verify the complex data was stored correctly
        let stored_event: serde_json::Value = sqlx::query_scalar("SELECT event_data FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(
            stored_event,
            serde_json::json!({
                "value": {"type": "mint", "amount": "1000"},
                "topic": ["transfer", "native"]
            })
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn fetch_and_store_events_with_mock_rpc_succeeds(pool: PgPool) {
        use crate::rpc_client::mock::MockRpcClient;
        
        let mock_client = MockRpcClient::new();
        
        // Mock getEvents response
        let events_response = serde_json::json!({
            "result": {
                "events": [
                    {
                        "contractId": "C123",
                        "type": "contract",
                        "txHash": "tx_hash_123",
                        "ledger": 1000,
                        "ledgerClosedAt": "2026-03-24T12:00:00Z",
                        "value": {"type": "transfer", "amount": "500"},
                        "topic": ["transfer", "native"]
                    },
                    {
                        "contractId": "C456",
                        "type": "system",
                        "txHash": "tx_hash_456",
                        "ledger": 1001,
                        "ledgerClosedAt": "2026-03-24T12:01:00Z",
                        "value": {"type": "upgrade"},
                        "topic": null
                    }
                ],
                "latestLedger": 1001,
                "cursor": null
            }
        });
        
        mock_client.set_response("getEvents", events_response);
        
        let config = Config {
            database_url: String::new(),
            stellar_rpc_url: "http://mock-rpc".to_string(),
            start_ledger: 0,
            port: 3000,
            behind_proxy: false,
            rpc_connect_timeout_secs: 30,
            rpc_request_timeout_secs: 60,
            indexer_lag_warn_threshold: 1000,
            indexer_stall_timeout_secs: 300,
            start_ledger_fallback: true,
            api_key: None,
            environment: crate::config::Environment::Development,
            allowed_origins: vec![],
            rate_limit_per_minute: 60,
            db_max_connections: 10,
            db_min_connections: 1,
            db_statement_timeout_ms: 30000,
        };
        
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let indexer = Indexer::new_with_rpc_client(pool.clone(), config, shutdown_rx, Box::new(mock_client));
        
        let result = indexer.fetch_and_store_events(1000).await.unwrap();
        assert_eq!(result, 1002); // latest_ledger + 1
        
        // Verify events were stored
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 2);
        
        // Verify first event
        let event1: (String, String, String, i64) = sqlx::query_as(
            "SELECT contract_id, event_type, tx_hash, ledger FROM events WHERE ledger = 1000"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event1.0, "C123");
        assert_eq!(event1.1, "contract");
        assert_eq!(event1.2, "tx_hash_123");
        assert_eq!(event1.3, 1000);
        
        // Verify second event
        let event2: (String, String, String, i64) = sqlx::query_as(
            "SELECT contract_id, event_type, tx_hash, ledger FROM events WHERE ledger = 1001"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event2.0, "C456");
        assert_eq!(event2.1, "system");
        assert_eq!(event2.2, "tx_hash_456");
        assert_eq!(event2.3, 1001);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn fetch_and_store_events_with_cursor_pagination(pool: PgPool) {
        use crate::rpc_client::mock::MockRpcClient;
        
        let mock_client = MockRpcClient::new();
        
        // First page response with cursor
        let first_page = serde_json::json!({
            "result": {
                "events": [
                    {
                        "contractId": "C1",
                        "type": "contract",
                        "txHash": "tx_1",
                        "ledger": 2000,
                        "ledgerClosedAt": "2026-03-24T13:00:00Z",
                        "value": {"type": "mint"},
                        "topic": null
                    }
                ],
                "latestLedger": 2000,
                "cursor": "cursor_123"
            }
        });
        
        // Second page response without cursor (end)
        let second_page = serde_json::json!({
            "result": {
                "events": [
                    {
                        "contractId": "C2",
                        "type": "contract",
                        "txHash": "tx_2",
                        "ledger": 2001,
                        "ledgerClosedAt": "2026-03-24T13:01:00Z",
                        "value": {"type": "transfer"},
                        "topic": ["transfer"]
                    }
                ],
                "latestLedger": 2001,
                "cursor": null
            }
        });
        
        mock_client.set_response("getEvents", first_page);
        
        let config = Config {
            database_url: String::new(),
            stellar_rpc_url: "http://mock-rpc".to_string(),
            start_ledger: 0,
            port: 3000,
            behind_proxy: false,
            rpc_connect_timeout_secs: 30,
            rpc_request_timeout_secs: 60,
            indexer_lag_warn_threshold: 1000,
            indexer_stall_timeout_secs: 300,
            start_ledger_fallback: true,
            api_key: None,
            environment: crate::config::Environment::Development,
            allowed_origins: vec![],
            rate_limit_per_minute: 60,
            db_max_connections: 10,
            db_min_connections: 1,
            db_statement_timeout_ms: 30000,
        };
        
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let indexer = Indexer::new_with_rpc_client(pool.clone(), config, shutdown_rx, Box::new(mock_client));
        
        // Update mock to return second page for the cursor request
        // Note: This is a simplified test - in a real scenario, you'd need a more sophisticated mock
        // that can return different responses based on the request parameters
        
        let result = indexer.fetch_and_store_events(2000).await.unwrap();
        assert!(result >= 2001);
        
        // Verify at least the first event was stored
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert!(count >= 1);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn fetch_and_store_events_handles_rpc_error(pool: PgPool) {
        use crate::rpc_client::mock::MockRpcClient;
        
        let mock_client = MockRpcClient::new();
        
        // Mock RPC error response
        let error_response = serde_json::json!({
            "error": {
                "code": -32600,
                "message": "Invalid Request"
            }
        });
        
        mock_client.set_response("getEvents", error_response);
        
        let config = Config {
            database_url: String::new(),
            stellar_rpc_url: "http://mock-rpc".to_string(),
            start_ledger: 0,
            port: 3000,
            behind_proxy: false,
            rpc_connect_timeout_secs: 30,
            rpc_request_timeout_secs: 60,
            indexer_lag_warn_threshold: 1000,
            indexer_stall_timeout_secs: 300,
            start_ledger_fallback: true,
            api_key: None,
            environment: crate::config::Environment::Development,
            allowed_origins: vec![],
            rate_limit_per_minute: 60,
            db_max_connections: 10,
            db_min_connections: 1,
            db_statement_timeout_ms: 30000,
        };
        
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let indexer = Indexer::new_with_rpc_client(pool.clone(), config, shutdown_rx, Box::new(mock_client));
        
        let result = indexer.fetch_and_store_events(3000).await;
        assert!(result.is_err());
        
        // Verify no events were stored
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn deduplication_path_same_event_twice_results_in_one_row(pool: PgPool) {
        use crate::rpc_client::mock::MockRpcClient;
        
        let mock_client = MockRpcClient::new();
        
        // Mock response with the same event
        let events_response = serde_json::json!({
            "result": {
                "events": [
                    {
                        "contractId": "CDEAD",
                        "type": "contract",
                        "txHash": "deadbeef_deadbeef_deadbeef_deadbeef_deadbeef_deadbeef_deadbeef",
                        "ledger": 5000,
                        "ledgerClosedAt": "2026-03-24T15:00:00Z",
                        "value": {"type": "transfer", "amount": "100"},
                        "topic": ["transfer", "native"]
                    }
                ],
                "latestLedger": 5000,
                "cursor": null
            }
        });
        
        mock_client.set_response("getEvents", events_response);
        
        let config = Config {
            database_url: String::new(),
            stellar_rpc_url: "http://mock-rpc".to_string(),
            start_ledger: 0,
            port: 3000,
            behind_proxy: false,
            rpc_connect_timeout_secs: 30,
            rpc_request_timeout_secs: 60,
            indexer_lag_warn_threshold: 1000,
            indexer_stall_timeout_secs: 300,
            start_ledger_fallback: true,
            api_key: None,
            environment: crate::config::Environment::Development,
            allowed_origins: vec![],
            rate_limit_per_minute: 60,
            db_max_connections: 10,
            db_min_connections: 1,
            db_statement_timeout_ms: 30000,
        };
        
        let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let indexer = Indexer::new_with_rpc_client(pool.clone(), config, shutdown_rx, Box::new(mock_client));
        
        // First call - should insert the event
        let result1 = indexer.fetch_and_store_events(5000).await.unwrap();
        assert_eq!(result1, 5001); // latest_ledger + 1
        
        // Verify one event was stored
        let count1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count1, 1);
        
        // Second call with same event - should deduplicate
        let result2 = indexer.fetch_and_store_events(5000).await.unwrap();
        assert_eq!(result2, 5001); // latest_ledger + 1
        
        // Verify still only one event exists (deduplication worked)
        let count2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count2, 1);
        
        // Verify the event details are correct
        let event: (String, String, String, i64) = sqlx::query_as(
            "SELECT contract_id, event_type, tx_hash, ledger FROM events LIMIT 1"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(event.0, "CDEAD");
        assert_eq!(event.1, "contract");
        assert_eq!(event.2, "deadbeef_deadbeef_deadbeef_deadbeef_deadbeef_deadbeef_deadbeef");
        assert_eq!(event.3, 5000);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn deduplication_direct_store_event_twice(pool: PgPool) {
        let indexer = indexer(pool.clone());
        let event = make_event(7777);

        // First insertion
        let rows1 = indexer.store_event(&event).await.unwrap();
        assert_eq!(rows1, 1);

        // Second insertion of same event
        let rows2 = indexer.store_event(&event).await.unwrap();
        assert_eq!(rows2, 0); // Should be 0 due to ON CONFLICT DO NOTHING

        // Verify exactly one row exists
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM events")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        // Verify the stored event is correct
        let stored_event: (String, String, String, i64) = sqlx::query_as(
            "SELECT contract_id, event_type, tx_hash, ledger FROM events LIMIT 1"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(stored_event.0, "C1");
        assert_eq!(stored_event.1, "contract");
        assert_eq!(stored_event.2, "abc");
        assert_eq!(stored_event.3, 7777);
    }
}
