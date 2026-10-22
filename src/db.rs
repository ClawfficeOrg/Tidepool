//! Database connection pool wiring.
//!
//! Provides a single helper, [`build_pool`], which constructs the application's
//! MySQL connection pool. Configuration arrives via the typed [`crate::config::Config`]
//! struct populated in `main`.

use sqlx::mysql::{MySqlPool, MySqlPoolOptions};
use std::time::Duration;

/// Build a MySQL connection pool from a database URL.
///
/// Pool tuning:
/// - `max_connections = 10` — sane default for a single application instance.
/// - `acquire_timeout = 5s` — fail fast under sustained pressure rather than
///   stacking request latency.
///
/// The function is `async` because [`MySqlPoolOptions::connect`] performs a
/// handshake against the database; it returns immediately once the pool is
/// initialised (subsequent connections are lazy on demand).
pub async fn build_pool(url: &str) -> sqlx::Result<MySqlPool> {
    MySqlPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(url)
        .await
}

/// Startup failures that must stop the pool before it listens.
#[derive(Debug, thiserror::Error)]
pub enum StartupError {
    /// The database could not be reached or a pool could not be built.
    #[error("database unavailable")]
    Database(#[source] sqlx::Error),
    /// A migration failed; the schema may be partially applied.
    #[error("migrations failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
}

/// Build the pool and apply the embedded migrations.
///
/// This is the only supported startup path: a failure here means the pool must
/// not start listening, because serving a partially migrated schema is worse
/// than not serving at all. `main` exits with a clear error when this returns
/// `Err`.
pub async fn prepare_database(url: &str) -> Result<MySqlPool, StartupError> {
    let pool = build_pool(url).await.map_err(StartupError::Database)?;
    run_migrations(&pool)
        .await
        .map_err(StartupError::Migration)?;
    Ok(pool)
}

/// Apply the embedded migrations.
///
/// The migrations live in `migrations/` and are compiled into the binary, so a
/// fresh install needs no manual SQL and no separate migration container. The
/// call is idempotent: `sqlx` records applied versions in `_sqlx_migrations`.
pub async fn run_migrations(pool: &MySqlPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test: only runs when `MYSQL_URL` is set in the environment.
    /// Provides an opt-in integration check that the pool can hand out a
    /// connection capable of executing a trivial query.
    #[tokio::test]
    #[ignore = "requires a live MySQL; set MYSQL_URL to enable"]
    async fn pool_can_select_one() {
        let url = std::env::var("MYSQL_URL").expect("MYSQL_URL must be set for this test");
        let pool = build_pool(&url).await.expect("pool builds");
        let row: (i64,) = sqlx::query_as("SELECT 1")
            .fetch_one(&pool)
            .await
            .expect("select 1");
        assert_eq!(row.0, 1);
    }
}
