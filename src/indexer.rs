use chrono::DateTime;
use serde_json::json;
use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::sleep;
use tracing::{error, info, instrument, span, warn, Instrument, Level};

use crate::{
    bloom_filter::{EventBloomFilter, SessionBloomFilter},
    config::{Config, HealthState, IndexerState},
    costs::calculator::CostCalculator,
    cursor_expiry_handler::{CursorExpiryBackoff, is_cursor_expiry_error},
    kinesis::KinesisPublisher,
    metrics,
    models::{GetEventsResult, LatestLedgerResult, RpcResponse, SorobanEvent},
    pubsub::PubSubPublisher,
    rpc_endpoints::{RpcEndpoints, RpcLatencyProbe},
    xdr_validation,
};

#[async_trait::async_trait]
pub trait RpcClient: Send + Sync {
    async fn get_latest_ledger(&self, rpc_url: &str) -> Result<u64, String>;
    async fn get_events(
        &self,
        rpc_url: &str,
        start_ledger: u64,
        cursor: Option<String>,
        event_types: &[String],
    ) -> Result<GetEventsResult, String>;
}

/// Postgres advisory lock key for the indexer singleton.
const INDEXER_LOCK_KEY: i64 = 0x536f726f62616e50; // "SorobanP"

/// Derive a per-network advisory lock key (Issue #1063) so that concurrent
/// indexer workers for different networks (e.g. testnet and mainnet) don't
/// contend for the same singleton lock. The primary network (whatever
/// `CHAIN_ID` is set to) keeps the original constant so single-network
/// deployments upgrading to this version don't lose their lock continuity;
/// every other chain_id gets a distinct derived key.
fn lock_key_for_chain(chain_id: &str) -> i64 {
    if chain_id == "mainnet" {
        return INDEXER_LOCK_KEY;
    }
    // FNV-1a over the chain_id, folded into a non-zero i64. Advisory lock
    // keys just need to be stable and (very likely) distinct per network —
    // they are not used cryptographically.
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in chain_id.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash as i64) ^ INDEXER_LOCK_KEY
}

#[derive(Debug, thiserror::Error)]
enum IndexerFetchError {
    #[error("{0}")]
    Rpc(String),
    #[error(transparent)]
    DbConnection(#[from] sqlx::Error),
}

/// Result of a fetch_and_store_events cycle containing both the next ledger to fetch
/// and the latest ledger from the RPC response (to avoid redundant RPC calls).
#[derive(Debug)]
struct FetchResult {
    next_ledger: u64,
    latest_ledger: u64,
}

pub struct SorobanRpcClient {
    client: reqwest::Client,
    /// Custom headers injected into every RPC request. Values are never logged.
    headers: reqwest::header::HeaderMap,
    /// Candidate endpoints with latency and health tracking (issue #1256).
    endpoints: Arc<RpcEndpoints>,
    /// Endpoint that served the last successful request, for failover metrics.
    active_idx: AtomicUsize,
}

impl SorobanRpcClient {
    pub fn new(config: &Config) -> Result<Self, crate::error::InitError> {
        let client = crate::net::outbound::build_http_client(
            reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(config.rpc_connect_timeout_secs))
                .timeout(Duration::from_secs(config.rpc_request_timeout_secs))
                .pool_max_idle_per_host(5)
                .pool_idle_timeout(Duration::from_secs(30))
                .tcp_keepalive(Duration::from_secs(60)),
            "RPC",
        )?;

        let mut headers = reqwest::header::HeaderMap::new();
        for (name, value) in &config.rpc_headers {
            let header_name = reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| {
                    crate::error::InitError::invalid_config("RPC", format!("invalid header name: {name}"))
                })?;
            let header_value = reqwest::header::HeaderValue::from_str(value).map_err(|_| {
                crate::error::InitError::invalid_config("RPC", format!("invalid header value for '{name}'"))
            })?;
            headers.insert(header_name, header_value);
        }

        let mut urls = vec![config.stellar_rpc_url.clone()];
        urls.extend_from_slice(&config.stellar_rpc_fallback_urls);
        let endpoints = Arc::new(RpcEndpoints::new(urls));
        metrics::set_rpc_active_endpoint(endpoints.url(0));

        Ok(Self {
            client,
            headers,
            endpoints,
            active_idx: AtomicUsize::new(0),
        })
    }

    /// Background task that measures endpoint latency and revives
    /// unavailable endpoints. Spawn it alongside the indexer:
    /// `tokio::spawn(client.latency_probe().run(shutdown_rx))`.
    pub fn latency_probe(&self) -> RpcLatencyProbe {
        RpcLatencyProbe::new(
            self.client.clone(),
            self.headers.clone(),
            Arc::clone(&self.endpoints),
        )
    }

    /// Send one JSON-RPC request to `url`.
    async fn post_once<T: serde::de::DeserializeOwned>(
        &self,
        url: &str,
        body: &serde_json::Value,
    ) -> Result<RpcResponse<T>, String> {
        let response = self
            .client
            .post(url)
            .headers(self.headers.clone())
            .json(body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    warn!("RPC request timeout");
                }
                e.to_string()
            })?;
        response.json().await.map_err(|e| e.to_string())
    }

    /// Send `body` to each endpoint in latency order until one returns a result.
    ///
    /// Transport and decoding failures count towards marking an endpoint
    /// unavailable. A JSON-RPC error object means the endpoint is reachable,
    /// so it does not, but the next endpoint is still tried; if every
    /// endpoint fails, that RPC error message is returned in preference to a
    /// transport error so callers can still recognise e.g. cursor expiry.
    async fn call_with_failover<T: serde::de::DeserializeOwned>(
        &self,
        method: &'static str,
        body: &serde_json::Value,
    ) -> Result<T, String> {
        let mut rpc_err: Option<String> = None;
        let mut transport_err = String::from("no RPC endpoint reachable");

        for idx in self.endpoints.candidates() {
            let url = self.endpoints.url(idx).to_owned();
            let span = span!(
                Level::INFO,
                "rpc_call",
                rpc.method = method,
                endpoint = %self.endpoints.label(idx)
            );
            match self.post_once::<T>(&url, body).instrument(span).await {
                Ok(RpcResponse { result: Some(r), .. }) => {
                    self.endpoints.record_success(idx);
                    let previous = self.active_idx.swap(idx, Ordering::Relaxed);
                    if previous != idx {
                        metrics::record_rpc_failover();
                        metrics::set_rpc_active_endpoint(&url);
                        info!(
                            from = %self.endpoints.label(previous),
                            to = %self.endpoints.label(idx),
                            "RPC failover: switching endpoint"
                        );
                    }
                    return Ok(r);
                }
                Ok(RpcResponse { error, .. }) => {
                    metrics::record_rpc_error();
                    let message = match error {
                        Some(err) => {
                            warn!(code = err.code, message = %err.message, endpoint = %self.endpoints.label(idx), "RPC error");
                            err.message
                        }
                        None => "RPC returned no result".to_string(),
                    };
                    rpc_err.get_or_insert(message);
                }
                Err(e) => {
                    metrics::record_rpc_error();
                    warn!(endpoint = %self.endpoints.label(idx), error = %e, method, "RPC request failed");
                    self.endpoints.record_failure(idx);
                    transport_err = e;
                }
            }
        }

        Err(rpc_err.unwrap_or(transport_err))
    }
}

#[async_trait::async_trait]
impl RpcClient for SorobanRpcClient {
    async fn get_latest_ledger(&self, _rpc_url: &str) -> Result<u64, String> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getLatestLedger"
        });
        let r: LatestLedgerResult = self.call_with_failover("getLatestLedger", &body).await?;
        metrics::update_latest_ledger(r.sequence);
        Ok(r.sequence)
    }

    async fn get_events(
        &self,
        _rpc_url: &str,
        start_ledger: u64,
        cursor: Option<String>,
        event_types: &[String],
    ) -> Result<GetEventsResult, String> {
        let filters: serde_json::Value = if event_types.is_empty() {
            json!([])
        } else {
            let filter_list: Vec<serde_json::Value> =
                event_types.iter().map(|t| json!({ "type": t })).collect();
            json!(filter_list)
        };

        let mut params = json!({
            "filters": filters,
            "pagination": { "limit": 100 }
        });
        if let Some(c) = &cursor {
            params["pagination"]["cursor"] = json!(c);
        } else {
            params["startLedger"] = json!(start_ledger);
        }

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getEvents",
            "params": params
        });
        self.call_with_failover("getEvents", &body).await
    }
}

pub struct Indexer<R: RpcClient> {
    pool: PgPool,
    rpc_client: R,
    config: Config,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    health_state: Option<Arc<HealthState>>,
    indexer_state: Option<Arc<IndexerState>>,
    event_tx: Option<broadcast::Sender<SorobanEvent>>,
    event_counter: AtomicU64,
    bloom_filter: Option<Arc<EventBloomFilter>>,
    /// Issue #615: session-level bloom filter reset on each new ledger.
    /// `None` only if the filter could not be built; the DB unique constraint
    /// remains the authoritative dedup guard in that case.
    session_bloom: Option<SessionBloomFilter>,
    kinesis_publisher: Option<Arc<dyn KinesisPublisher>>,
    pubsub_publisher: Option<Arc<dyn PubSubPublisher>>,
    sqs_publisher: Option<Arc<dyn crate::sqs::SqsPublisher>>,
    event_hubs_publisher: Option<Arc<dyn crate::event_hubs::EventHubsPublisher>>,
    sse_ring_buffer: Option<Arc<crate::sse_ring_buffer::SseRingBuffer>>,
    /// Issue #1264: operational cost calculator for RPC call accounting.
    cost_calculator: Option<Arc<CostCalculator>>,
    #[cfg(feature = "kafka")]
    kafka_publisher: Option<Arc<dyn crate::kafka::KafkaPublisher>>,
    #[cfg(feature = "kafka")]
    kafka_topic: Option<String>,
    #[cfg(feature = "lua")]
    lua_transformer: Option<Arc<crate::lua_transform::LuaTransformer>>,
    /// Issue #1254: sha256(network passphrase), used to recognise SAC events.
    sac_network_id: [u8; 32],
}

impl<R: RpcClient> Indexer<R> {
    pub fn new(
        pool: PgPool,
        config: Config,
        shutdown_rx: tokio::sync::watch::Receiver<bool>,
        rpc_client: R,
    ) -> Self {
        let sac_network_id = crate::sac::network_id(&crate::sac::network_passphrase(&config.chain_id));
        Self {
            pool,
            rpc_client,
            config,
            shutdown_rx,
            health_state: None,
            indexer_state: None,
            event_tx: None,
            event_counter: AtomicU64::new(0),
            bloom_filter: None,
            session_bloom: SessionBloomFilter::new(50_000, 0.001)
                .map_err(|e| warn!(error = %e, "Session bloom filter disabled"))
                .ok(),
            kinesis_publisher: None,
            pubsub_publisher: None,
            sqs_publisher: None,
            event_hubs_publisher: None,
            sse_ring_buffer: None,
            cost_calculator: None,
            #[cfg(feature = "kafka")]
            kafka_publisher: None,
            #[cfg(feature = "kafka")]
            kafka_topic: None,
            #[cfg(feature = "lua")]
            lua_transformer: None,
            sac_network_id,
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

    /// Attach the SSE ring buffer so broadcasted events are also stored for replay.
    pub fn set_sse_ring_buffer(&mut self, buf: Arc<crate::sse_ring_buffer::SseRingBuffer>) {
        self.sse_ring_buffer = Some(buf);
    }

    /// Set the bloom filter for pre-filtering duplicate events (issue #266).
    pub fn set_bloom_filter(&mut self, bloom_filter: Arc<EventBloomFilter>) {
        self.bloom_filter = Some(bloom_filter);
    }

    /// Set the Kinesis publisher for streaming events (issue #265).
    pub fn set_kinesis_publisher(&mut self, publisher: Arc<dyn KinesisPublisher>) {
        self.kinesis_publisher = Some(publisher);
    }

    /// Set the Pub/Sub publisher for streaming events (issue #264).
    pub fn set_pubsub_publisher(&mut self, publisher: Arc<dyn PubSubPublisher>) {
        self.pubsub_publisher = Some(publisher);
    }

    /// Set the SQS publisher for serverless event processing.
    pub fn set_sqs_publisher(&mut self, publisher: Arc<dyn crate::sqs::SqsPublisher>) {
        self.sqs_publisher = Some(publisher);
    }

    /// Set the Event Hubs publisher for multi-cloud event streaming.
    pub fn set_event_hubs_publisher(
        &mut self,
        publisher: Arc<dyn crate::event_hubs::EventHubsPublisher>,
    ) {
        self.event_hubs_publisher = Some(publisher);
    }

    /// Issue #1264: attach the operational cost calculator so RPC calls made by
    /// the indexer are accounted for in cost reports.
    pub fn set_cost_calculator(&mut self, calculator: Arc<CostCalculator>) {
        self.cost_calculator = Some(calculator);
    }

    /// Set the Kafka publisher and topic for event streaming.
    #[cfg(feature = "kafka")]
    pub fn set_kafka_publisher(
        &mut self,
        publisher: Arc<dyn crate::kafka::KafkaPublisher>,
        topic: String,
    ) {
        self.kafka_publisher = Some(publisher);
        self.kafka_topic = Some(topic);
    }

    /// Set the Lua transformer for event transformation.
    #[cfg(feature = "lua")]
    pub fn set_lua_transformer(&mut self, transformer: Arc<crate::lua_transform::LuaTransformer>) {
        self.lua_transformer = Some(transformer);
    }

    pub async fn run(&self) {
        // Attempt to acquire a Postgres session-level advisory lock.
        // Only one replica will hold this lock at a time; others poll until they can promote.
        let lock_conn = match self.pool.acquire().await {
            Ok(c) => c,
            Err(e) => {
                error!(error = %e, "Failed to acquire DB connection for advisory lock");
                return;
            }
        };

        let retry_interval = Duration::from_secs(self.config.indexer_lock_retry_secs.max(1));
        let mut interval = tokio::time::interval(retry_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let lock_wait_start = std::time::Instant::now();
        // Issue #1063: each network's indexer worker takes its own advisory
        // lock key, so testnet/mainnet/etc. can each have a leader running
        // concurrently instead of contending for one global lock.
        let lock_key = lock_key_for_chain(&self.config.chain_id);

        loop {
            // Respect shutdown signal while waiting to acquire the lock.
            let mut shutdown_rx = self.shutdown_rx.clone();
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Indexer shutting down before acquiring lock");
                        metrics::record_indexer_is_leader(false);
                        return;
                    }
                }
            }

            let acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
                .bind(lock_key)
                .fetch_one(&self.pool)
                .await
                .unwrap_or(false);

            if acquired {
                info!(
                    lock_key = lock_key,
                    "Indexer lock acquired, starting indexing"
                );
                if let Some(ref s) = self.indexer_state {
                    s.is_active_indexer.store(true, Ordering::Relaxed);
                }
                metrics::record_indexer_is_leader(true);
                metrics::update_indexer_lock_wait_duration(0.0);
                break;
            }

            // Issue #427: Query pg_locks to find current lock holder's PID
            let lock_holder_pid: Option<i32> = sqlx::query_scalar(
                "SELECT pid FROM pg_locks WHERE locktype = 'advisory' AND database = (SELECT oid FROM pg_database WHERE datname = current_database()) AND objid = $1 LIMIT 1"
            )
            .bind(lock_key as i32)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();

            let lock_wait_secs = lock_wait_start.elapsed().as_secs_f64();
            metrics::update_indexer_lock_wait_duration(lock_wait_secs);

            warn!(
                lock_key = lock_key,
                lock_holder_pid = lock_holder_pid,
                retry_secs = self.config.indexer_lock_retry_secs,
                wait_secs = lock_wait_secs,
                "Indexer lock not acquired, running in standby mode — will retry"
            );
            if let Some(ref s) = self.indexer_state {
                s.is_active_indexer.store(false, Ordering::Relaxed);
            }
            metrics::record_indexer_is_leader(false);
        }

        // Issue #608: validate ledger hash chain on startup when enabled.
        if self.config.ledger_hash_tracking_enabled {
            crate::ledger_hashes::startup_hash_chain_validation(&self.pool).await;
        }

        // Run the actual indexing loop; release lock on exit.
        self.run_loop().await;

        // Explicitly release the advisory lock on graceful shutdown.
        let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(lock_key)
            .execute(&self.pool)
            .await;
        metrics::record_indexer_is_leader(false);
        if let Some(ref s) = self.indexer_state {
            s.is_active_indexer.store(false, Ordering::Relaxed);
        }

        drop(lock_conn);
    }

    async fn run_loop(&self) {
        crate::resources::spawn_sync(self.pool.clone(), self.config.stellar_rpc_url.clone());
        crate::rpc_meta::spawn_monitor(
            self.pool.clone(),
            self.config.stellar_rpc_url.clone(),
            self.indexer_state.clone(),
            self.config.start_ledger,
        );
        let mut current_ledger = self.config.start_ledger;
        let mut consecutive_db_errors = 0u32;
        let mut rpc_backoff_ms = 1000u64; // Start with 1 second backoff

        // Restore persisted state so /status is accurate before the first poll.
        if let Some((persisted_current, persisted_latest)) = self.load_indexer_state().await {
            if persisted_current > 0 {
                if current_ledger == 0 {
                    current_ledger = persisted_current;
                }
                if let Some(ref s) = self.indexer_state {
                    s.current_ledger
                        .store(persisted_current, std::sync::atomic::Ordering::Relaxed);
                    s.latest_ledger
                        .store(persisted_latest, std::sync::atomic::Ordering::Relaxed);
                }
                metrics::update_current_ledger(persisted_current);
                info!(
                    current_ledger = persisted_current,
                    latest_ledger = persisted_latest,
                    "Restored persisted indexer state"
                );
            }
        }

        if current_ledger == 0 {
            loop {
                match self
                    .rpc_client
                    .get_latest_ledger(&self.config.stellar_rpc_url)
                    .await
                {
                    Ok(ledger) => {
                        current_ledger = ledger;
                        info!(ledger = current_ledger, "Starting from latest ledger");
                        metrics::update_current_ledger(current_ledger);
                        if let Some(ref s) = self.indexer_state {
                            s.current_ledger
                                .store(current_ledger, std::sync::atomic::Ordering::Relaxed);
                        }
                        rpc_backoff_ms = 1000; // Reset backoff on success
                        break;
                    }
                    Err(e) => {
                        error!(error = %e, backoff_ms = rpc_backoff_ms, "Failed to get latest ledger, retrying with exponential backoff");
                        sleep(Duration::from_millis(rpc_backoff_ms)).await;
                        // Exponential backoff with max 60 seconds
                        rpc_backoff_ms = std::cmp::min(rpc_backoff_ms * 2, 60000);
                    }
                }
            }
        }

        loop {
            if *self.shutdown_rx.borrow() {
                info!("Indexer shutting down gracefully");
                break;
            }

            // Check pause state — sleep and re-check rather than blocking.
            if let Some(ref s) = self.indexer_state {
                if s.is_paused.load(std::sync::atomic::Ordering::Relaxed) {
                    sleep(Duration::from_millis(500)).await;
                    continue;
                }
            }

            match self.fetch_and_store_events(current_ledger).await {
                Ok(result) => {
                    consecutive_db_errors = 0;
                    // Update the last poll timestamp on success
                    if let Some(ref health_state) = self.health_state {
                        health_state.update_last_poll();
                    }
                    if result.next_ledger > current_ledger {
                        current_ledger = result.next_ledger;
                        metrics::update_current_ledger(current_ledger);
                        if let Some(ref s) = self.indexer_state {
                            s.current_ledger
                                .store(current_ledger, std::sync::atomic::Ordering::Relaxed);
                        }

                        // Use latest_ledger from RPC response to calculate lag (no extra RPC call)
                        if result.latest_ledger > current_ledger {
                            if let Some(ref s) = self.indexer_state {
                                s.latest_ledger
                                    .store(result.latest_ledger, std::sync::atomic::Ordering::Relaxed);
                            }
                            let lag = result.latest_ledger - current_ledger;
                            metrics::update_indexer_lag(lag);
                            metrics::record_indexer_lag_observation(lag);
                            metrics::update_network_indexer_lag(&self.config.chain_id, lag);

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
                        // On idle cycles, refresh the latest_ledger metric
                        if let Ok(latest) = self.rpc_client.get_latest_ledger(&self.config.stellar_rpc_url).await {
                            if let Some(ref s) = self.indexer_state {
                                s.latest_ledger.store(latest, std::sync::atomic::Ordering::Relaxed);
                            }
                            let lag = latest.saturating_sub(current_ledger);
                            metrics::update_indexer_lag(lag);
                            metrics::record_indexer_lag_observation(lag);
                            metrics::update_network_indexer_lag(&self.config.chain_id, lag);
                        }
                        sleep(Duration::from_millis(self.config.indexer_poll_interval_ms)).await;
                    }
                }
                Err(IndexerFetchError::DbConnection(e)) => {
                    consecutive_db_errors += 1;
                    let backoff_secs = if consecutive_db_errors >= 5 { 60 } else { 10 };
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
                    sleep(Duration::from_millis(self.config.indexer_error_backoff_ms)).await;
                }
            }
        }
    }

    async fn get_latest_ledger(&self) -> Result<u64, String> {
        self.rpc_client
            .get_latest_ledger(&self.config.stellar_rpc_url)
            .await
    }

    /// Load the last persisted cursor from the database, if any.
    async fn load_checkpoint(&self) -> Option<String> {
        sqlx::query_scalar::<_, String>(
            "SELECT last_cursor FROM indexer_checkpoints WHERE id = 'singleton'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    /// Load the last persisted indexer state (current_ledger, latest_ledger) from the DB.
    async fn load_indexer_state(&self) -> Option<(u64, u64)> {
        sqlx::query_as::<_, (i64, i64)>(
            "SELECT current_ledger, latest_ledger FROM indexer_state WHERE id = 'singleton'",
        )
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .map(|(c, l)| (c as u64, l as u64))
    }

    /// Persist current_ledger and latest_ledger atomically inside an existing transaction.
    async fn save_indexer_state(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        current_ledger: u64,
        latest_ledger: u64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO indexer_state (id, current_ledger, latest_ledger, updated_at)
             VALUES ('singleton', $1, $2, NOW())
             ON CONFLICT (id) DO UPDATE
               SET current_ledger = EXCLUDED.current_ledger,
                   latest_ledger  = EXCLUDED.latest_ledger,
                   updated_at     = NOW()",
        )
        .bind(current_ledger as i64)
        .bind(latest_ledger as i64)
        .execute(&mut **tx)
        .await
        .map(|_| ())
    }

    /// Persist the cursor atomically alongside the event inserts.
    async fn save_checkpoint(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        cursor: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO indexer_checkpoints (id, last_cursor, updated_at)
             VALUES ('singleton', $1, NOW())
             ON CONFLICT (id) DO UPDATE SET last_cursor = EXCLUDED.last_cursor, updated_at = NOW()",
        )
        .bind(cursor)
        .execute(&mut **tx)
        .await
        .map(|_| ())
    }

    /// Public wrapper around `fetch_and_store_events` for integration/resilience tests.
    pub async fn fetch_and_store_events_pub(&self, start_ledger: u64) -> Result<u64, String> {
        self.fetch_and_store_events(start_ledger)
            .await
            .map(|result| result.next_ledger)
            .map_err(|e| e.to_string())
    }

    #[instrument(skip(self), fields(start_ledger = start_ledger))]
    async fn fetch_and_store_events(&self, start_ledger: u64) -> Result<FetchResult, IndexerFetchError> {
        let cycle_start = std::time::Instant::now();
        // Resume from persisted cursor if available (unless INDEXER_IGNORE_CHECKPOINT is set), otherwise start from ledger.
        let mut cursor: Option<String> = if self.config.indexer_ignore_checkpoint {
            None
        } else {
            self.load_checkpoint().await
        };
        let mut latest_ledger = start_ledger;
        let mut total_fetched = 0;
        let mut total_inserted = 0;
        let mut total_skipped_duplicate = 0;
        let mut total_skipped_validation = 0;
        let mut total_skipped_out_of_order = 0;
        let mut total_pages = 0u32;
        let mut total_rpc_ms = 0u128;
        let mut total_db_ms = 0u128;
        let start_ledger_for_log = start_ledger;
        // Issue #685: Handle cursor expiry with exponential backoff
        let mut cursor_expiry_backoff = CursorExpiryBackoff::new();

        tracing::debug!(
            start_ledger = start_ledger,
            cursor = cursor.as_deref().unwrap_or("<none>"),
            ignore_checkpoint = self.config.indexer_ignore_checkpoint,
            "Indexer cycle starting",
        );

        loop {
            let rpc_start = std::time::Instant::now();
            let mut result = match self
                .rpc_client
                .get_events(
                    &self.config.stellar_rpc_url,
                    start_ledger,
                    cursor.clone(),
                    &self.config.indexer_event_types,
                )
                .await
            {
                Ok(r) => {
                    // Reset backoff on successful RPC call
                    cursor_expiry_backoff.reset();
                    r
                }
                Err(msg) => {
                    warn!(error = %msg, "RPC error");
                    metrics::record_rpc_error();

                    // Issue #685: Handle cursor expiry with exponential backoff and fallback
                    if is_cursor_expiry_error(&msg) {
                        if cursor_expiry_backoff.should_fallback() {
                            warn!(
                                retry_count = cursor_expiry_backoff.retry_count(),
                                "Cursor expired: max retries reached, falling back to ledger-based pagination"
                            );
                            // Fallback: clear cursor and retry from ledger position
                            cursor = None;
                            cursor_expiry_backoff.reset();
                            // Wait before retry
                            sleep(Duration::from_millis(100)).await;
                            continue;
                        } else {
                            // Apply exponential backoff and retry
                            let backoff_duration = cursor_expiry_backoff.next_backoff();
                            warn!(
                                retry_count = cursor_expiry_backoff.retry_count(),
                                backoff_ms = backoff_duration.as_millis(),
                                "Cursor expired: applying exponential backoff and retrying"
                            );
                            sleep(backoff_duration).await;
                            continue;
                        }
                    }

                    return Err(IndexerFetchError::Rpc(msg));
                }
            };
            let rpc_elapsed = rpc_start.elapsed();
            total_rpc_ms += rpc_elapsed.as_millis();
            total_pages += 1;

            // Issue #1264: account for the successful getEvents RPC call in the
            // operational cost calculator so it shows up in cost reports.
            if let Some(ref calculator) = self.cost_calculator {
                calculator
                    .record_rpc_call(
                        "indexer",
                        "getEvents",
                        rpc_elapsed.as_millis() as u64,
                        result.events.len() as u64,
                    )
                    .await;
            }

            // Issue #426: Validate RPC response doesn't exceed max page size
            let requested_limit = self.config.rpc_max_events_per_page;
            if result.events.len() > requested_limit {
                warn!(
                    actual = result.events.len(),
                    expected = requested_limit,
                    "RPC response exceeded requested page size, truncating"
                );
                metrics::record_oversized_rpc_response();
                result.events.truncate(requested_limit);
            }

            latest_ledger = result.latest_ledger;
            let current_count = result.events.len();
            total_fetched += current_count;
            let schema_version = result.protocol_version.unwrap_or(1) as i32;
            let next_cursor = result.rpc_cursor.clone();

            // Store events and checkpoint atomically in one transaction.
            let mut db_tx = self.pool.begin().await?;
            let db_start = std::time::Instant::now();
            for event in result.events {
                // Issue #428: Validate ledger sequence is monotonically increasing
                if event.ledger < start_ledger {
                    warn!(
                        event_ledger = event.ledger,
                        current_cursor = start_ledger,
                        tx_hash = %event.tx_hash,
                        contract_id = %event.contract_id,
                        "Out-of-order event: ledger sequence below current cursor, skipping"
                    );
                    metrics::record_out_of_order_events(1);
                    total_skipped_out_of_order += 1;
                    continue;
                }

                // Apply Lua transformation if configured
                #[cfg(feature = "lua")]
                let event = if let Some(ref transformer) = self.lua_transformer {
                    match transformer.transform(event).await {
                        Ok(Some(transformed)) => transformed,
                        Ok(None) => {
                            // Script returned nil, skip this event
                            total_skipped_validation += 1;
                            continue;
                        }
                        Err(e) => {
                            warn!(
                                error = %e,
                                "Lua transformation failed, skipping event"
                            );
                            total_skipped_validation += 1;
                            continue;
                        }
                    }
                } else {
                    event
                };

                match self
                    .store_event_in_tx(&mut db_tx, &