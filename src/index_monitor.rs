/// Background task that periodically runs EXPLAIN on key queries and warns
/// if the query planner is not using the expected indexes.
use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;

/// Queries to check, paired with the index name expected to appear in the plan.
const CHECKS: &[(&str, &str, &str)] = &[
    (
        "main events query",
        "EXPLAIN (FORMAT JSON) SELECT id FROM events ORDER BY ledger DESC, id DESC LIMIT 20",
        "idx_events_ledger_desc",
    ),
    (
        "contract filter query",
        "EXPLAIN (FORMAT JSON) SELECT id FROM events WHERE contract_id = 'CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM' ORDER BY ledger DESC LIMIT 20",
        "idx_events_contract_ledger",
    ),
    (
        "tx hash query",
        "EXPLAIN (FORMAT JSON) SELECT id FROM events WHERE tx_hash = 'a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2' ORDER BY ledger DESC LIMIT 20",
        "idx_events_tx_ledger",
    ),
];

/// Run a single round of index usage checks.
async fn check_indexes(pool: &PgPool) {
    for (label, sql, expected_index) in CHECKS {
        match sqlx::query_scalar::<_, serde_json::Value>(sql)
            .fetch_one(pool)
            .await
        {
            Ok(plan) => {
                let plan_str = plan.to_string();
                let uses_index = plan_str.contains(expected_index)
                    || plan_str.contains("Index Scan")
                    || plan_str.contains("Index Only Scan")
                    || plan_str.contains("Bitmap Index Scan");
                let has_seq_scan = plan_str.contains("Seq Scan");

                if has_seq_scan && !uses_index {
                    tracing::warn!(
                        query = label,
                        expected_index = expected_index,
                        "Sequential scan detected — expected index not used"
                    );
                } else {
                    tracing::debug!(
                        query = label,
                        expected_index = expected_index,
                        "Index usage OK"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(query = label, error = %e, "Failed to run EXPLAIN for index check");
            }
        }
    }
}

/// Spawn the index monitoring background task.
///
/// Runs every `interval_hours` hours. Stops when `shutdown_rx` fires.
pub fn spawn(pool: PgPool, interval_hours: u64, mut shutdown_rx: watch::Receiver<bool>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_hours * 3600);
        // Run once shortly after startup, then on the configured interval.
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    tracing::debug!("Running index usage check");
                    check_indexes(&pool).await;
                }
                _ = shutdown_rx.changed() => {
                    tracing::debug!("Index monitor shutting down");
                    break;
                }
            }
        }
    });
}
