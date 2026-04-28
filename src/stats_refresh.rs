use sqlx::PgPool;
use std::time::Duration;
use tokio::sync::watch;
use tracing::{error, info};

const VIEWS: &[&str] = &[
    "events_daily_summary",
    "events_contract_summary",
    "events_hourly_volume",
];

/// Refresh all three materialized views using CONCURRENTLY (non-blocking).
pub async fn refresh_all(pool: &PgPool) {
    for view in VIEWS {
        let sql = format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {view}");
        match sqlx::query(&sql).execute(pool).await {
            Ok(_) => info!(view = view, "Materialized view refreshed"),
            Err(e) => error!(view = view, error = %e, "Failed to refresh materialized view"),
        }
    }
}

/// Spawn a background task that refreshes the materialized views every `interval_secs` seconds.
pub fn spawn(pool: PgPool, interval_secs: u64, mut shutdown: watch::Receiver<bool>) {
    tokio::spawn(async move {
        let interval = Duration::from_secs(interval_secs);
        // Initial refresh on startup
        refresh_all(&pool).await;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {
                    refresh_all(&pool).await;
                }
                _ = shutdown.changed() => {
                    info!("Stats refresh task shutting down");
                    break;
                }
            }
        }
    });
}
