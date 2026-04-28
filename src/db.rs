use sqlx::{postgres::PgPoolOptions, Executor, PgPool};
use std::time::Duration;
use tracing::{debug, info, info_span, Instrument};

/// Per-endpoint query timeout configuration (in milliseconds)
pub struct QueryTimeouts {
    pub fast_lookup: u64,     // Simple lookups by ID/hash (e.g., 1000ms)
    pub standard_query: u64,  // Standard paginated queries (e.g., 5000ms)
    pub expensive_query: u64, // Expensive queries like COUNT(*) (e.g., 10000ms)
}

impl Default for QueryTimeouts {
    fn default() -> Self {
        Self {
            fast_lookup: 1000,
            standard_query: 5000,
            expensive_query: 10000,
        }
    }
}

pub async fn create_pool(
    database_url: &str,
    db_max_connections: u32,
    db_min_connections: u32,
    db_statement_timeout_ms: u64,
    db_idle_timeout_secs: u64,
    db_max_lifetime_secs: u64,
    db_test_before_acquire: bool,
) -> Result<PgPool, sqlx::Error> {
    info!(
        min_connections = db_min_connections,
        max_connections = db_max_connections,
        statement_timeout_ms = db_statement_timeout_ms,
        idle_timeout_secs = db_idle_timeout_secs,
        max_lifetime_secs = db_max_lifetime_secs,
        test_before_acquire = db_test_before_acquire,
        "Configuring Postgres connection pool"
    );

    PgPoolOptions::new()
        .max_connections(db_max_connections)
        .min_connections(db_min_connections)
        .idle_timeout(Duration::from_secs(db_idle_timeout_secs))
        .max_lifetime(Duration::from_secs(db_max_lifetime_secs))
        .test_before_acquire(db_test_before_acquire)
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

/// Helper to set per-query timeout using SET LOCAL statement_timeout
/// This should be called at the beginning of a transaction
pub async fn set_query_timeout(
    conn: &mut sqlx::PgConnection,
    timeout_ms: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(&format!("SET LOCAL statement_timeout = '{timeout_ms}ms'"))
        .execute(&mut *conn)
        .await
        .map(|_| ())
}

/// Runs migrations under a Postgres session-level advisory lock so that
/// concurrent replicas starting simultaneously do not race each other.
/// The lock is always released — even if migration fails.
/// Returns the number of migrations applied.
pub async fn run_migrations(pool: &PgPool) -> Result<usize, sqlx::migrate::MigrateError> {
    const MIGRATION_LOCK_ID: i64 = 0xD0C0_1234_i64; // arbitrary stable key

    async move {
        let mut conn = pool
            .acquire()
            .await
            .map_err(sqlx::migrate::MigrateError::from)?;

        debug!(lock_id = MIGRATION_LOCK_ID, "Acquiring advisory lock");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(MIGRATION_LOCK_ID)
            .execute(&mut *conn)
            .await
            .map_err(sqlx::migrate::MigrateError::from)?;
        debug!(lock_id = MIGRATION_LOCK_ID, "Advisory lock acquired");

        // Record which migrations are already applied before running.
        let before: Vec<(String,)> =
            sqlx::query_as("SELECT version::text FROM _sqlx_migrations WHERE success = true")
                .fetch_all(&mut *conn)
                .await
                .unwrap_or_default();

        let result = sqlx::migrate!("./migrations").run(&mut *conn).await;

        // Always release — ignore unlock errors so the migration result is returned.
        let _ = sqlx::query("SELECT pg_advisory_unlock($1)")
            .bind(MIGRATION_LOCK_ID)
            .execute(&mut *conn)
            .await;
        debug!(lock_id = MIGRATION_LOCK_ID, "Advisory lock released");

        result?;

        // Count newly applied migrations by comparing before/after.
        let after_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = true")
                .fetch_one(&mut *conn)
                .await
                .unwrap_or(before.len() as i64);

        let newly_applied = (after_count as usize).saturating_sub(before.len());
        info!(count = newly_applied, "Migrations applied");
        Ok(newly_applied)
    }
    .instrument(info_span!("db.run_migrations"))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn run_migrations_returns_nonnegative_count(pool: PgPool) {
        // sqlx::test already runs migrations; calling again should return 0 (nothing new).
        let count = run_migrations(&pool)
            .await
            .expect("migrations must succeed");
        assert_eq!(
            count, 0,
            "re-running migrations on an up-to-date schema should apply 0"
        );
    }

    #[sqlx::test(migrations = false)]
    async fn run_migrations_on_fresh_db_returns_positive_count(pool: PgPool) {
        let count = run_migrations(&pool)
            .await
            .expect("migrations must succeed");
        assert!(count > 0, "fresh database should have migrations applied");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_pool_signature_accepts_new_options() {
        // Verify the function signature compiles with all new parameters.
        // Actual pool creation requires a live DB; this just validates types.
        let _f: fn(&str, u32, u32, u64, u64, u64, bool) -> _ = create_pool;
    }
}
