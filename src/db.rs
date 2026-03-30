use sqlx::{postgres::PgPoolOptions, Executor, PgPool};
use tracing::info;

pub async fn create_pool(
    database_url: &str,
    db_max_connections: u32,
    db_min_connections: u32,
    db_statement_timeout_ms: u64,
) -> Result<PgPool, sqlx::Error> {
    info!(
        min_connections = db_min_connections,
        max_connections = db_max_connections,
        statement_timeout_ms = db_statement_timeout_ms,
        "Configuring Postgres connection pool"
    );

    PgPoolOptions::new()
        .max_connections(db_max_connections)
        .min_connections(db_min_connections)
        .after_connect(move |conn, _| {
            Box::pin(async move {
                conn.execute(
                    format!("SET statement_timeout = '{db_statement_timeout_ms}ms'").as_str(),
                )
                .await
                .map(|_| ())
            })
        })
        .connect(database_url)
        .await
}

/// Runs migrations under a Postgres session-level advisory lock so that
/// concurrent replicas starting simultaneously do not race each other.
/// The lock is always released — even if migration fails.
pub async fn run_migrations(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    const MIGRATION_LOCK_ID: i64 = 0xD0C0_1234_i64; // arbitrary stable key

    let mut conn = pool.acquire().await.map_err(sqlx::migrate::MigrateError::from)?;

    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(MIGRATION_LOCK_ID)
        .execute(&mut *conn)
        .await
        .map_err(sqlx::migrate::MigrateError::from)?;

    let result = sqlx::migrate!("./migrations").run(&mut *conn).await;

    // Always release — ignore unlock errors so the migration result is returned.
    let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(MIGRATION_LOCK_ID)
        .execute(&mut *conn)
        .await;

    result
}
