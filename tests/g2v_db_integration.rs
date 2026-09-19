#![cfg(all(feature = "g2v-sqlx", feature = "testing", feature = "g2v-sqlx"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for the `db` module against a real Postgres.
//!
//! By default these tests start a Postgres container via `testcontainers`. Set
//! `DATABASE_URL` to run against an existing Postgres instead.
//!
//! ```text
//! cargo test -p sunbeam-g2v --test db_integration -- --nocapture --test-threads=1
//! ```

use std::fs;
use std::path::PathBuf;

use sdk::g2v::config::DatabaseConfig;
use sdk::g2v::db::{Database, Migrator, connect, connect_eager};
use sdk::g2v::error::ServiceError;

#[path = "g2v_support/mod.rs"]
mod support;

// ============================================================================
// Config helper
// ============================================================================

async fn db_config() -> DatabaseConfig {
    let url = support::containers::postgres_url().await;
    DatabaseConfig {
        url,
        max_connections: 5,
        connect_timeout: 5,
    }
}

// ============================================================================
// Schema isolation helper
// ============================================================================

/// Create a fresh Postgres schema for one test, run `f` with a pool whose
/// `search_path` is set to that schema, then drop the schema unconditionally.
///
/// This gives every test its own `_sqlx_migrations` table and tables, so
/// tests are fully isolated even when they share the same database.
async fn with_isolated_schema<F, Fut>(db: &Database, f: F)
where
    F: FnOnce(sqlx::PgPool) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let schema = format!("g2v_test_{}", uuid::Uuid::new_v4().simple());

    // Create the schema.
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE SCHEMA \"{schema}\"")))
        .execute(db.pool())
        .await
        .expect("create test schema");

    // Build a pool whose search_path is pinned to the test schema.
    let base = support::containers::postgres_url().await;
    let separator = if base.contains('?') { '&' } else { '?' };
    let url = format!("{base}{separator}options=-csearch_path%3D{schema}");
    let test_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .connect(&url)
        .await
        .expect("connect to test schema pool");

    f(test_pool).await;

    // Drop the schema and everything in it (idempotent cleanup).
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP SCHEMA IF EXISTS \"{schema}\" CASCADE"
    )))
    .execute(db.pool())
    .await
    .expect("drop test schema");
}

// ============================================================================
// Tests
// ============================================================================

/// `connect` (lazy) + `ping` should succeed against a real database.
#[tokio::test]
async fn test_connect_lazy_succeeds_with_real_db() {
    let db = connect(&db_config().await).expect("connect_lazy should not fail for valid URL");
    db.ping()
        .await
        .expect("ping should succeed against real Postgres");
}

/// `ping` against a database at a bad port should return `ServiceError::Database`
/// or `ServiceError::DeadlineExceeded` — not panic.
#[tokio::test]
async fn test_ping_fails_when_db_unreachable() {
    // Port 9 is the discard port — connections are refused immediately on most
    // systems. We only need any port that won't accept Postgres connections.
    let config = DatabaseConfig {
        url: "postgres://user:pw@localhost:9/db".to_string(),
        max_connections: 2,
        connect_timeout: 2,
    };

    let db = connect(&config).expect("connect_lazy must succeed for valid URL");

    let result = db.ping().await;
    assert!(result.is_err(), "ping to unreachable host must fail");

    match result.err().unwrap() {
        ServiceError::Database(_) | ServiceError::DeadlineExceeded(_) => {}
        other => panic!("expected Database or DeadlineExceeded, got {other:?}"),
    }
}

/// `Migrator::new` + `run` should execute a migration and leave the table behind.
#[tokio::test]
async fn test_migrator_runs_against_real_db() {
    let db = connect_eager(&db_config().await)
        .await
        .expect("connect_eager should succeed");

    // UUID-suffixed table name — unique per run, no cross-run collision.
    let table = format!("tbl_{}", uuid::Uuid::new_v4().simple());

    with_isolated_schema(&db, |pool| async move {
        let tmpdir = tempdir_with_migration(&table);
        let migrations_path = tmpdir.join("migrations");

        let migrator = Migrator::new(&migrations_path)
            .await
            .expect("Migrator::new should succeed for valid directory");

        migrator
            .run(&pool)
            .await
            .expect("migration should run without error");

        // Verify the table was created inside the test schema.
        let row: (bool,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_name = '{table}'
            )"
        )))
        .fetch_one(&pool)
        .await
        .expect("existence query should succeed");

        assert!(row.0, "table {table} should exist after migration");
        // Schema and its contents are dropped by `with_isolated_schema`.
    })
    .await;
}

/// Running the same migrator twice must be a no-op (idempotent).
#[tokio::test]
async fn test_migrator_idempotent() {
    let db = connect_eager(&db_config().await)
        .await
        .expect("connect_eager should succeed");

    let table = format!("tbl_{}", uuid::Uuid::new_v4().simple());

    with_isolated_schema(&db, |pool| async move {
        let tmpdir = tempdir_with_migration(&table);
        let migrations_path = tmpdir.join("migrations");

        let migrator = Migrator::new(&migrations_path)
            .await
            .expect("Migrator::new should succeed");

        // First run.
        migrator
            .run(&pool)
            .await
            .expect("first migration run should succeed");

        // Second run — must be a no-op, not an error.
        migrator
            .run(&pool)
            .await
            .expect("second migration run should be idempotent");
    })
    .await;
}

// ============================================================================
// Helpers
// ============================================================================

/// Create a temporary directory containing a `migrations/` subdirectory with
/// a single deterministic SQL file that creates `table_name`.
///
/// Uses a fixed version number (`20260101000000`) — safe because each test
/// gets its own schema with its own `_sqlx_migrations` ledger.
fn tempdir_with_migration(table_name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sunbeam_g2v_migtest_{}",
        uuid::Uuid::new_v4().simple()
    ));
    let migrations = dir.join("migrations");
    fs::create_dir_all(&migrations).expect("create temp migrations dir");

    let sql_path = migrations.join("20260101000000_test.sql");
    fs::write(
        &sql_path,
        format!(
            "CREATE TABLE IF NOT EXISTS {table_name} (\
                id SERIAL PRIMARY KEY, \
                name TEXT\
            );\n"
        ),
    )
    .expect("write migration SQL file");

    dir
}
