#![deny(clippy::all, clippy::pedantic)]
#![allow(
    clippy::module_name_repetitions, // e.g. AppError, AppState — idiomatic in Rust
    clippy::missing_errors_doc,      // internal handlers; not a public library
    clippy::missing_panics_doc,      // panics only on misconfiguration at startup
    clippy::wildcard_imports,        // used sparingly in test modules only
)]
mod audit_logging;
mod bloom_filter;
mod compliance_report;
mod config;
mod content_filter;
mod cross_chain_correlation;
mod cursor_expiry_handler;
mod db;
mod advisory_lock;
mod query_streaming;
mod serialization_cache;
mod streaming_response;
mod dedup;
mod distributed_tracing;
mod email;
mod encryption;
mod error;
mod event_hubs;
mod graceful_shutdown;
mod handlers;
mod idempotency;
mod log_analysis_tool;
mod index_monitor;
mod indexer;
mod kafka;
mod kinesis;
#[cfg(feature = "lua")]
mod lua_transform;
mod metrics;
mod prometheus_remote_write;
mod eventbridge;
mod middleware;
mod models;
mod normalizer;
mod notification_dedup;

#[cfg(feature = "parquet")]
mod parquet_export;

mod pruner;
mod pubsub;
mod queue_publisher;
mod rate_limiter;
mod reencrypt;
mod resource_metrics;
mod routes;
mod rpc_client;
mod schema_validator;
mod sqs;
mod stats_refresh;
mod subscriptions;
mod webhook;
mod webhook_verification;
mod notification_rate_limit;
mod notification_formatter;
mod pagerduty;
mod github;
mod discord;
mod slack;
mod teams;
mod telegram;
mod notification_channel;
mod integration_handlers;
mod retry_policy;
mod sms;
mod aggregation;
mod saved_queries;
mod abi;
mod oncall;
mod xdr_validation;
mod replica_monitor;
mod feature_flags;
#[cfg(feature = "graphql")]
mod graphql;
#[cfg(feature = "graphql")]
mod graphql_subscriptions;
mod event_dedup_replicas;
mod bulk_export;
mod sse_ring_buffer;
mod query_cache;
mod query_plan_cache;
mod query_optimizer;
mod partition_manager;
mod query_builder;
mod adaptive_pool;
mod pool_management;
mod statistics_management;
mod notification_admin;
mod push_preload;
mod financial_accuracy;
mod webhook_template;
mod event_aggregation;
mod anomaly_detection;
mod push_notification;
mod connection_pool;
mod pool_management;
mod adaptive_pool;
mod slo_tracker;
mod statistics_management;
mod notification_admin;
mod push_preload;
mod financial_accuracy;
mod webhook_template;
mod event_aggregation;
mod anomaly_detection;

// These modules were already part of the library target (see src/lib.rs) but
// missing here, leaving `crate::pool_management` and friends unresolved in
// handlers.rs when compiling the `soroban-pulse` binary. `clippy::pedantic`
// is scoped off since these files were never linted against it before.
#[allow(clippy::pedantic)]
mod anonymization;
#[allow(clippy::pedantic)]
mod event_compression;
#[allow(clippy::pedantic)]
mod health_check;
#[allow(clippy::pedantic)]
mod ledger_hashes;
#[allow(clippy::pedantic)]
mod networks;
#[allow(clippy::pedantic)]
mod pool_management;
#[allow(clippy::pedantic)]
mod push_preload;
#[allow(clippy::pedantic)]
mod statistics_management;

#[cfg(feature = "archive")]
mod archiver;

mod cloud_provider;
mod cloud_replication;
mod deployment_orchestrator;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[cfg(feature = "otel")]
use opentelemetry::global;
#[cfg(feature = "otel")]
use opentelemetry_otlp::WithExportConfig;
#[cfg(feature = "otel")]
use tracing_opentelemetry::OpenTelemetryLayer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    let log_format = std::env::var("RUST_LOG_FORMAT").unwrap_or_else(|_| "text".to_string());

    let registry = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()));

    #[cfg(feature = "otel")]
    let registry = {
        let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());

        let tracer = opentelemetry_otlp::new_pipeline()
            .tracing()
            .with_exporter(
                opentelemetry_otlp::new_exporter()
                    .tonic()
                    .with_endpoint(otel_endpoint),
            )
            .install_simple()
            .expect("Failed to initialize OpenTelemetry tracer");

        registry.with(OpenTelemetryLayer::new(tracer))
    };

    if log_format == "json" {
        registry
            .with(tracing_subscriber::fmt::layer().json())
            .init();
    } else {
        registry.with(tracing_subscriber::fmt::layer()).init();
    }

    // Initialize metrics exporter
    let prometheus_handle = metrics::init_metrics();

    #[cfg(target_os = "linux")]
    metrics::spawn_memory_collector();

    let config = config::Config::from_env();

    info!(
        rpc_url = %config.stellar_rpc_url,
        rpc_fallback_urls = ?config.stellar_rpc_fallback_urls,
        rpc_headers = ?config.safe_rpc_headers(),
        start_ledger = config.start_ledger,
        port = config.port,
        db_url = %config.safe_db_url(),
        db_max_connections = config.db_max_connections,
        db_min_connections = config.db_min_connections,
        indexer_event_types = ?config.indexer_event_types,
        "Resolved configuration",
    );

    let pool = {
        let mut attempt = 0;
        loop {
            attempt += 1;
            match db::create_pool(
                &config.database_url,
                config.db_max_connections,
                config.db_min_connections,
                config.db_statement_timeout_ms,
                config.db_idle_timeout_secs,
                config.db_max_lifetime_secs,
                config.db_test_before_acquire,
            )
            .await
            {
                Ok(p) => break p,
                Err(e) => {
                    if attempt >= 3 {
                        tracing::error!(error = %e, "Failed to connect to database after 3 attempts");
                        return Err(anyhow::anyhow!("Database connection failed after 3 attempts"));
                    }
                    tracing::warn!(attempt = attempt, "DB connection failed, retrying...");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }
    };

    let _ = db::run_migrations(&pool).await;

    // Create read pool: use replica URL if configured, otherwise reuse primary pool.
    let read_pool = if let Some(ref replica_url) = config.database_replica_url {
        info!("DATABASE_REPLICA_URL set — HTTP handlers will use read replica");
        match db::create_pool(
            replica_url,
            config.db_max_connections,
            config.db_min_connections,
            config.db_statement_timeout_ms,
            config.db_idle_timeout_secs,
            config.db_max_lifetime_secs,
            config.db_test_before_acquire,
        )
        .await
        {
            Ok(p) => p,
            Err(e) => {
                tracing::error!(error = %e, "Failed to connect to read replica, falling back to primary");
                pool.clone()
            }
        }
    } else {
        pool.clone()
    };

    info!("Migrations applied successfully");
    info!(environment = ?config.environment, "Running environment");

    // Warm the query plan cache with the five canonical query patterns so
    // the first real requests are never cold misses. (#802)
    {
        let plan_cache = query_plan_cache::QueryPlanCache::with_defaults();
        match plan_cache.warm_cache(&pool).await {
            Ok(n) => info!(warmed = n, "Query plan cache warmed at startup"),
            Err(e) => warn!(error = %e, "Query plan cache warming failed; continuing"),
        }
    }

    // Initialize schema validator and load schemas
    let schema_validator = Arc::new(schema_validator::SchemaValidator::new(pool.clone()));
    if let Err(e) = schema_validator.load_schemas().await {
        warn!(error = %e, "Failed to load schemas from database");
    } else {
        info!("Schema validator initialized");
    }

    // Create shared health state for indexer and HTTP handlers
    let health_state = Arc::new(config::HealthState::new(config.indexer_stall_timeout_secs));

    // Create shared indexer state for /status endpoint
    let indexer_state = Arc::new(config::IndexerState::new());

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let mut shutdown_rx_axum = shutdown_rx.clone();

    // Broadcast channel for real-time SSE streaming (capacity 256 events)
    let (event_tx, _) = tokio::sync::broadcast::channel::<models::SorobanEvent>(256);

    // Spawn webhook delivery task if WEBHOOK_URL is configured.
    if let Some(ref webhook_url) = config.webhook_url {
        let webhook_rx = event_tx.subscribe();
        let webhook_url = webhook_url.clone();
        let webhook_secret = config.webhook_secret.clone();
        let webhook_contract_filter = config.webhook_contract_filter.clone();
        let webhook_retry_policy = config.webhook_retry_policy.clone();
        let webhook_pool = pool.clone();
        // Per-channel rate limiter (Issue #476). `None` when no limit configured.
        let webhook_rate_limiter = notification_rate_limit::ChannelRateLimiter::from_config(
            &notification_rate_limit::RateLimitConfig::new(
                config.webhook_rate_limit_per_minute,
                config.webhook_rate_limit_per_hour,
            ),
        );
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("Failed to build webhook HTTP client");

        info!(
            url = %webhook_url,
            rate_limit_per_minute = ?config.webhook_rate_limit_per_minute,
            rate_limit_per_hour = ?config.webhook_rate_limit_per_hour,
            "Webhook delivery enabled"
        );

        tokio::spawn(async move {
            let mut rx = webhook_rx;
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        // Apply contract filter if configured
                        if !webhook_contract_filter.is_empty()
                            && !webhook_contract_filter.contains(&event.contract_id)
                        {
                            continue;
                        }
                        // Apply content filter if configured (Issue #477).
                        if let Some(ref cf) = webhook_content_filter {
                            if !cf.evaluate(&event.value) {
                                continue;
                            }
                        }
                        let client = http_client.clone();
                        let url = webhook_url.clone();
                        let secret = webhook_secret.clone();
                        let pool_ref = Some(pool.as_ref());
                        let priority = webhook::evaluate_priority(
                            &event,
                            config.notification_priority_rule_path.as_deref(),
                            config.notification_priority_rule_value.as_deref(),
                            config.notification_priority_rule_priority.as_deref(),
                            &config.notification_default_priority,
                        ).to_string();
                        tokio::spawn(webhook::deliver_with_retry_policy(
                            client,
                            url,
                            secret,
                            event,
                            pool_ref,
                            &config.webhook_retry_policy,
                            priority,
                        ));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            skipped = n,
                            "Webhook subscriber lagged, some events skipped"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    // Spawn email notification task if EMAIL_SMTP_HOST is configured.
    if let Some(ref smtp_host) = config.email_smtp_host {
        if let Some(ref from) = config.email_from {
            if !config.email_to.is_empty() {
                let email_rx = event_tx.subscribe();
                // Base URL for unsubscribe links (Issue #483): honor the
                // configured public URL, otherwise fall back to localhost:<port>.
                let base_url = config
                    .email_public_base_url
                    .clone()
                    .unwrap_or_else(|| format!("http://localhost:{}", config.port));
                let notifier = email::EmailNotifier::new(
                    smtp_host.clone(),
                    config.email_smtp_port,
                    config.email_smtp_user.clone(),
                    config.email_smtp_password.clone(),
                    from.clone(),
                    config.email_to.clone(),
                    config.email_contract_filter.clone(),
                    config.email_retry_policy.clone(),
                    email::Schedule::parse(
                        &config.email_schedule,
                        config.email_daily_digest_hour,
                        config.email_cron.clone(),
                    ),
                    email::QuietHours::parse(
                        config.email_quiet_hours_start.as_deref(),
                        config.email_quiet_hours_end.as_deref(),
                    ),
                    config.email_language.clone(),
                    pool.clone(),
                    base_url,
                );

                info!(
                    smtp_host = %smtp_host,
                    recipients = config.email_to.len(),
                    schedule = %config.email_schedule,
                    language = %config.email_language,
                    "Email notifications enabled"
                );

                // SPF deliverability check (Issue #486): warn at startup if the
                // sending domain has no SPF record. Best-effort and non-blocking.
                let spf_from = from.clone();
                tokio::spawn(async move {
                    email::verify_spf_record(&spf_from).await;
                });

                notifier.spawn(email_rx);
            } else {
                warn!(
                    "EMAIL_SMTP_HOST is set but EMAIL_TO is empty — email notifications disabled"
                );
            }
        } else {
            warn!("EMAIL_SMTP_HOST is set but EMAIL_FROM is not — email notifications disabled");
        }
    }

    // Spawn SMS notification task if Twilio is configured (Issue #473)
    if let (Some(account_sid), Some(auth_token), Some(from_number)) = (
        &config.twilio_account_sid,
        &config.twilio_auth_token,
        &config.twilio_from_number,
    ) {
        if !config.sms_to_numbers.is_empty() {
            let sms_rx = event_tx.subscribe();
            let twilio_config = sms::TwilioConfig {
                account_sid: account_sid.clone(),
                auth_token: auth_token.clone(),
                from_number: from_number.clone(),
                to_numbers: config.sms_to_numbers.clone(),
            };

            let notifier = sms::SmsNotifier::new(
                twilio_config,
                config.sms_contract_filter.clone(),
                config.sms_retry_policy.clone(),
                pool.clone(),
            );

            info!(
                from_number = %from_number,
                recipients = config.sms_to_numbers.len(),
                "SMS notifications enabled"
            );

            notifier.spawn(sms_rx);
        } else {
            warn!("Twilio credentials set but SMS_TO_NUMBERS is empty — SMS notifications disabled");
        }
    }

    // Issue #619: Spawn subscription email delivery worker.
    {
        let email_pool = pool.clone();
        tokio::spawn(subscriptions::run_email_delivery_worker(email_pool));
    }

    // Issue #620: Spawn push notification delivery worker.
    {
        let push_pool = pool.clone();
        tokio::spawn(push_notification::run_push_delivery_worker(push_pool));
    }

    // Issue #622: Spawn connection pool monitor.
    {
        let pool_cfg = connection_pool::PoolMonitorConfig {
            max_connections: config.db_max_connections,
            min_connections: config.db_min_connections,
            exhaustion_threshold: 0.9,
            sample_interval: std::time::Duration::from_secs(15),
        };
        connection_pool::spawn_pool_monitor(pool.clone(), pool_cfg);
        connection_pool::log_pool_snapshot(&pool, config.db_max_connections);
    }

    // Spawn Redis publisher if configured
    if let (Some(redis_url), Some(redis_stream_key)) = (&config.redis_url, &config.redis_stream_key)
    {
        let redis_rx = event_tx.subscribe();
        let redis_url = redis_url.clone();
        let redis_stream_key = redis_stream_key.clone();

        info!(stream_key = %redis_stream_key, "Redis publisher enabled");

        let redis_buffer_max_size = config.redis_buffer_max_size;
        tokio::spawn(async move {
            queue_publisher::spawn_redis_publisher(
                redis_url,
                redis_stream_key,
                redis_buffer_max_size,
                redis_rx,
            )
            .await;
        });
    }

    // Spawn background indexer with health state
    let rpc_client = indexer::SorobanRpcClient::new(&config);
    let mut indexer = indexer::Indexer::new(
        pool.clone(),
        config.clone(),
        shutdown_rx.clone(),
        rpc_client,
    );
    indexer.set_health_state(health_state.clone());
    indexer.set_indexer_state(indexer_state.clone());
    indexer.set_event_tx(event_tx.clone());

    // Create the SSE ring buffer — shared between the indexer (writer) and HTTP
    // handlers (reader for replay).
    let sse_ring_buf = sse_ring_buffer::SseRingBuffer::new(config.sse_ring_buffer_capacity);
    indexer.set_sse_ring_buffer(std::sync::Arc::clone(&sse_ring_buf));

    // Issue #266: Initialize and seed bloom filter
    {
        let bloom = std::sync::Arc::new(bloom_filter::EventBloomFilter::new(
            config.bloom_filter_capacity,
            config.bloom_filter_fp_rate,
        ));
        match bloom_filter::seed_from_db(&bloom, &pool, 100_000).await {
            Ok(n) => info!(seeded = n, "Bloom filter seeded from DB"),
            Err(e) => tracing::warn!(error = %e, "Failed to seed bloom filter from DB"),
        }
        indexer.set_bloom_filter(bloom);
    }

    // Issue #265: Initialize Kinesis publisher if configured
    #[cfg(feature = "kinesis")]
    if let (Some(stream_name), Some(region)) = (
        config.kinesis_stream_name.clone(),
        config.aws_region.clone(),
    ) {
        let partition_key_field = config.kinesis_partition_key_field.clone();
        let publisher =
            kinesis::aws::AwsKinesisPublisher::from_env(stream_name, region, partition_key_field)
                .await;
        indexer.set_kinesis_publisher(std::sync::Arc::new(publisher));
        info!("Kinesis publisher enabled");
    }

    // Issue #264: Initialize Pub/Sub publisher if configured
    #[cfg(feature = "pubsub")]
    if let (Some(project_id), Some(topic_id)) = (
        config.pubsub_project_id.clone(),
        config.pubsub_topic_id.clone(),
    ) {
        match pubsub::gcp::GcpPubSubPublisher::from_env(project_id, topic_id, config.pubsub_enable_message_ordering).await {
            Ok(publisher) => {
                indexer.set_pubsub_publisher(std::sync::Arc::new(publisher));
                info!("Pub/Sub publisher enabled");
            }
            Err(e) => tracing::warn!(error = %e, "Failed to initialize Pub/Sub publisher"),
        }
    }

    // Issue #706: Initialize SQS publisher if configured
    #[cfg(feature = "sqs")]
    if let (Some(queue_url), Some(region)) =
        (config.sqs_queue_url.clone(), config.aws_region.clone())
    {
        let publisher = sqs::aws::AwsSqsPublisher::from_env(
            queue_url,
            region,
            config.sqs_dlq_url.clone(),
            config.sqs_batch_size,
        )
        .await;
        indexer.set_sqs_publisher(std::sync::Arc::new(publisher));
        info!("SQS publisher enabled");
    }

    // Issue #707: Initialize Azure Event Hubs publisher if configured
    if let Some(connection_string) = config.event_hubs_connection_string.clone() {
        match event_hubs::HttpEventHubsPublisher::from_connection_string(
            &connection_string,
            config.event_hub_name.clone().unwrap_or_default(),
            config.event_hubs_partition_strategy.clone(),
        ) {
            Ok(publisher) => {
                indexer.set_event_hubs_publisher(std::sync::Arc::new(publisher));
                info!("Event Hubs publisher enabled");
            }
            Err(e) => warn!(error = %e, "Failed to initialize Event Hubs publisher"),
        }
    }

    #[cfg(feature = "kafka")]
    if let (Some(brokers), Some(topic)) = (&config.kafka_brokers, &config.kafka_topic) {
        match crate::kafka::RdKafkaProducer::new(
            brokers,
            config.kafka_batch_size,
            config.kafka_linger_ms,
        ) {
            Ok(producer) => {
                info!(brokers = %brokers, topic = %topic, "Kafka publishing enabled");
                indexer.set_kafka_publisher(std::sync::Arc::new(producer), topic.clone());
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create Kafka producer — Kafka publishing disabled");
            }
        }
    }

    // Initialize Lua transformer if configured
    #[cfg(feature = "lua")]
    if let Some(ref script_path) = config.event_transform_script {
        match soroban_pulse::lua_transform::LuaTransformer::new(
            std::path::Path::new(script_path),
            config.event_transform_timeout_ms,
        ) {
            Ok(transformer) => {
                info!(
                    script_path = %script_path,
                    timeout_ms = config.event_transform_timeout_ms,
                    "Lua event transformer enabled"
                );
                indexer.set_lua_transformer(std::sync::Arc::new(transformer));
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to initialize Lua transformer — transformation disabled");
            }
        }
    }

    let indexer_handle = tokio::spawn(async move {
        indexer.run().await;
    });

    // Spawn index usage monitoring background task
    index_monitor::spawn(
        pool.clone(),
        config.index_check_interval_hours,
        shutdown_rx.clone(),
        index_monitor::FragmentationThresholds {
            warn_ratio: config.fragmentation_warn_threshold,
            critical_ratio: config.fragmentation_critical_threshold,
            auto_reindex: config.fragmentation_auto_reindex,
        },
    );

    // Spawn replica sync monitoring background task (#586)
    replica_monitor::spawn(pool.clone(), 60, shutdown_rx.clone());

    // Spawn feature flag rollback watcher (#587)
    feature_flags::spawn(pool.clone(), 60, shutdown_rx.clone());

    // Spawn resource metrics collector (#630)
    resource_metrics::spawn_resource_collector(shutdown_rx.clone());

    // Spawn materialized-view refresh background task
    stats_refresh::spawn(
        pool.clone(),
        config.stats_refresh_interval_secs,
        shutdown_rx.clone(),
    );

    // #496: Spawn audit log purge job
    {
        let audit_pool = pool.clone();
        let retention_days = config.notification_audit_log_retention_days;
        tokio::spawn(notification_admin::purge_old_audit_log_entries(audit_pool, retention_days));
    }

    // #498: Spawn channel health checker
    {
        let health_pool = pool.clone();
        let health_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("Failed to build health check HTTP client");
        let interval_secs = config.notification_health_check_interval_secs;
        let health_check_email = config.notification_health_check_email.clone();
        notification_admin::spawn_channel_health_checker(
            health_pool,
            health_client,
            interval_secs,
            health_check_email,
        );
    }

    // #696: Install the SLO tracker and spawn its background evaluator. Must
    // happen before the HTTP server begins serving so that the
    // `/v1/admin/slo/report` endpoint can resolve `slo_tracker::current()`.
    {
        let slo_tracker = slo_tracker::shared_tracker();
        let installed = slo_tracker::install(slo_tracker.clone());
        if !installed {
            warn!("SLO tracker was already installed — replacing with the default tracker");
            // Re-install by clearing the OnceLock via a fresh tracker. In
            // practice this branch only fires if main is re-entered, which
            // does not happen, so we just log and move on.
        }
        slo_tracker::SloTracker::spawn_evaluator(
            slo_tracker,
            shutdown_rx.clone(),
            slo_tracker::DEFAULT_EVAL_INTERVAL_SECS,
        );
        info!(
            interval_secs = slo_tracker::DEFAULT_EVAL_INTERVAL_SECS,
            slos_tracked = slo_tracker.definition_count(),
            "SLO tracker installed and evaluator started",
        );
    }

    async fn shutdown_signal(
        event_tx: broadcast::Sender<models::SorobanEvent>,
        sse_drain_timeout_secs: u64,
        sse_shutdown_grace_period_ms: u64,
        shutdown_tx: tokio::sync::watch::Sender<bool>,
    ) {
        #[cfg(unix)]
        {
            let mut sigterm =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).unwrap();
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = sigterm.recv() => {},
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c().await.ok();
        }

        tracing::info!("Graceful shutdown initiated, sending close event to SSE clients...");

        // Signal SSE streams to emit close event
        let _ = shutdown_tx.send(true);

        // Wait for SSE clients to receive the close event
        tokio::time::sleep(Duration::from_millis(sse_shutdown_grace_period_ms)).await;

        // Drop the broadcast sender to signal SSE streams to close
        drop(event_tx);

        // Wait for SSE connections to drain
        tokio::time::sleep(Duration::from_secs(sse_drain_timeout_secs)).await;

        tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            tracing::info!("Graceful shutdown timeout reached (30s), forcing exit");
            std::process::exit(0);
        });
    }

    let event_tx_clone = event_tx.clone();
    let sse_drain_timeout = config.sse_drain_timeout_secs;
    let sse_shutdown_grace_period = config.sse_shutdown_grace_period_ms;
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        shutdown_signal(event_tx_clone, sse_drain_timeout, sse_shutdown_grace_period, shutdown_tx_clone).await;
    });

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    info!(origins = ?config.allowed_origins, "Allowed CORS origins");
    info!(
        rate_limit = config.rate_limit_per_minute,
        "Rate limit per IP"
    );

    // Clone pool before it is moved into the router for the tenant map load.
    let pool_for_tenant_map = pool.clone();

    let tenant_map = if config.multi_tenant {
        match routes::load_tenant_map(&pool_for_tenant_map).await {
            Ok(map) => {
                info!(count = map.len(), "Loaded tenant map from database");
                std::sync::Arc::new(map)
            }
            Err(e) => {
                error!(error = %e, "Failed to load tenant map — aborting startup");
                return Err(e.into());
            }
        }
    } else {
        std::sync::Arc::new(std::collections::HashMap::new())
    };

    let router = routes::create_router_with_tx_and_tenant_map(
        pool,
        read_pool,
        config.api_keys.clone(),
        &config.allowed_origins,
        config.rate_limit_per_minute,
        config.behind_proxy,
        health_state,
        indexer_state,
        prometheus_handle,
        event_tx,
        config.sse_keepalive_interval_ms,
        config.sse_max_connections,
        2000,
        config.event_data_encryption_key,
        config.event_data_encryption_key_old,
        config.clone(),
        Some(schema_validator),
        tenant_map,
        shutdown_rx.clone(),
        sse_ring_buf,
    );

    info!(addr = %addr, "Soroban Pulse listening");

    let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
        error!(addr = %addr, "Address already in use");
        e
    })?;

    info!(
        behind_proxy = config.behind_proxy,
        "Running server - trusting X-Forwarded-For"
    );

    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx_axum.changed().await;
        })
        .await?;
    let _ = indexer_handle.await;

    #[cfg(feature = "otel")]
    global::shutdown_tracer_provider();

    Ok(())
}
