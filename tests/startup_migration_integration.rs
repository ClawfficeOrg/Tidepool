//! Integration tests for startup migrations (task 1.6.2).
//!
//! Require a live MySQL; skipped by default (`#[ignore]`).
//! Run with: `cargo test --test startup_migration_integration -- --ignored`
//!
//! These tests prove the two halves of the startup contract:
//!
//! - a fresh database is migrated by [`db::prepare_database`] and a second run
//!   is a no-op;
//! - a database whose schema conflicts with a migration makes
//!   `prepare_database` return an error, which is what stops `main` from
//!   listening. No server is started here, which is the point.

use sqlx::mysql::MySqlPoolOptions;
use tidepool::db::{self, StartupError};

const DEFAULT_ADMIN_URL: &str = "mysql://root:rootpassword@127.0.0.1:3306";

fn admin_url() -> String {
    std::env::var("TIDEPOOL_TEST_ADMIN_URL").unwrap_or_else(|_| DEFAULT_ADMIN_URL.into())
}

fn with_database(url: &str, database: &str) -> String {
    let authority_start = url.find("://").map(|index| index + 3).unwrap_or(0);
    match url[authority_start..].find('/') {
        Some(offset) => format!("{}/{}", &url[..authority_start + offset], database),
        None => format!("{}/{database}", url.trim_end_matches('/')),
    }
}

async fn scratch_database() -> (sqlx::MySqlPool, String, String) {
    let admin = MySqlPoolOptions::new()
        .max_connections(1)
        .connect(&admin_url())
        .await
        .expect("admin connection — is Docker up?");
    let name = format!("tidepool_startup_{}", uuid::Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE DATABASE `{name}`"))
        .execute(&admin)
        .await
        .expect("create scratch database");
    let url = with_database(&admin_url(), &name);
    (admin, url, name)
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_fresh_database_migrates_and_a_second_start_is_a_no_op() {
    let (admin, url, name) = scratch_database().await;

    let pool = db::prepare_database(&url).await.expect("first start");
    let applied: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 1")
        .fetch_one(&pool)
        .await
        .expect("migration count");
    assert!(applied.0 > 0, "migrations were recorded");
    let tables: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE()",
    )
    .fetch_one(&pool)
    .await
    .expect("table count");
    assert!(tables.0 > 10);
    pool.close().await;

    // Second start applies nothing and still succeeds.
    let pool = db::prepare_database(&url).await.expect("second start");
    pool.close().await;

    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .expect("drop");
}

#[actix_web::test]
#[ignore = "requires live MySQL (docker compose -f config/docker-compose.dev.yml up -d)"]
async fn a_conflicting_schema_stops_startup() {
    let (admin, url, name) = scratch_database().await;

    // An `organizations.id` of the wrong type makes the first foreign key that
    // references it fail, which is exactly what a half-applied or conflicting
    // schema looks like to the migrator.
    let connection = MySqlPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .expect("scratch connection");
    sqlx::query("CREATE TABLE organizations (id INT NOT NULL PRIMARY KEY)")
        .execute(&connection)
        .await
        .expect("conflicting table");
    connection.close().await;

    let error = db::prepare_database(&url)
        .await
        .expect_err("a failed migration must not return a pool");
    assert!(
        matches!(error, StartupError::Migration(_)),
        "expected a migration error, got {error:?}"
    );
    assert!(error.to_string().contains("migrations failed"));

    sqlx::query(&format!("DROP DATABASE `{name}`"))
        .execute(&admin)
        .await
        .expect("drop");
}
