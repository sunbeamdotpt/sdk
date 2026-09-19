//! SQLx-backed `Database` wrapper for Sunbeam services.
//!
//! Wraps `sqlx::PgPool` with Sunbeam-specific error conversion and a 2-second
//! ping timeout. Kept opaque (no `Deref`) so future instrumentation hooks can
//! intercept pool access.

use crate::g2v::config::DatabaseConfig;
use crate::g2v::error::{ServiceError, ServiceResult};
use sqlx::postgres::PgPoolOptions;

// ============================================================================
// Database
// ============================================================================

/// Opaque wrapper around a `sqlx::PgPool`.
///
/// Obtain one via [`connect`][super::connect] (lazy) or
/// [`connect_eager`][super::connect_eager] (eager / connectivity-verified).
///
/// # Example
///
/// ```rust,no_run
/// use crate::g2v::db::{Database, connect};
/// use crate::g2v::config::DatabaseConfig;
///
/// # async fn example() -> crate::g2v::error::ServiceResult<()> {
/// let db = connect(&DatabaseConfig::default())?;
/// db.ping().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Database {
    pool: sqlx::PgPool,
}

impl Database {
    /// Build directly from an already-constructed `PgPool`.
    pub(crate) fn from_pool(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Build from a `DatabaseConfig`, returning an error if the URL is invalid.
    ///
    /// Equivalent to calling [`connect`][super::connect] and is provided here
    /// for ergonomics:
    ///
    /// ```rust,no_run
    /// # async fn example() -> crate::g2v::error::ServiceResult<()> {
    /// use crate::g2v::db::Database;
    /// use crate::g2v::config::DatabaseConfig;
    ///
    /// let db = Database::from_config(&DatabaseConfig::default())?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_config(config: &DatabaseConfig) -> ServiceResult<Self> {
        super::connect(config)
    }

    /// Return a shared reference to the underlying pool.
    ///
    /// Use this when passing the pool to `sqlx` query macros or
    /// [`DatabaseHealthCheck`][crate::g2v::health::DatabaseHealthCheck].
    pub fn pool(&self) -> &sqlx::PgPool {
        &self.pool
    }

    /// Issue `SELECT 1` with a 2-second wall-clock timeout.
    ///
    /// Returns `Ok(())` if the database is reachable, or a
    /// [`ServiceError::Database`] / [`ServiceError::DeadlineExceeded`] variant
    /// otherwise.
    pub async fn ping(&self) -> ServiceResult<()> {
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            sqlx::query("SELECT 1").execute(&self.pool),
        )
        .await;

        match result {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(e)) => Err(ServiceError::Database(e.to_string())),
            Err(_) => Err(ServiceError::DeadlineExceeded(
                "database ping timed out after 2s".to_string(),
            )),
        }
    }
}

// ============================================================================
// Pool builder helpers (used by connection.rs)
// ============================================================================

/// Build a `PgPool` from a `DatabaseConfig` using `connect_lazy`.
///
/// Errors only if the URL is unparseable. The pool will not attempt any
/// physical connections until the first query is issued.
pub(super) fn build_pool_lazy(config: &DatabaseConfig) -> ServiceResult<sqlx::PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(0)
        .acquire_timeout(std::time::Duration::from_secs(config.connect_timeout))
        .connect_lazy(&config.url)
        .map_err(|e| ServiceError::Database(e.to_string()))?;
    Ok(pool)
}

/// Build a `PgPool` from a `DatabaseConfig` using an eager `connect`.
///
/// Blocks until a physical connection is established (or `connect_timeout`
/// elapses), verifying connectivity before returning.
pub(super) async fn build_pool_eager(config: &DatabaseConfig) -> ServiceResult<sqlx::PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(0)
        .acquire_timeout(std::time::Duration::from_secs(config.connect_timeout))
        .connect(&config.url)
        .await
        .map_err(|e| ServiceError::Database(e.to_string()))?;
    Ok(pool)
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_build_pool_lazy_rejects_bad_url() {
        // A URL with no scheme is rejected synchronously by sqlx.
        let config = DatabaseConfig {
            url: "not-a-valid-postgres-url".to_string(),
            max_connections: 5,
            connect_timeout: 5,
        };
        let result = build_pool_lazy(&config);
        assert!(result.is_err(), "expected error for malformed URL, got Ok");
        match result.unwrap_err() {
            ServiceError::Database(_) => {}
            other => panic!("expected Database variant, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_from_config_delegates_to_connect() {
        // A syntactically valid URL but unreachable host.
        // connect_lazy should succeed (no I/O at build time).
        let config = DatabaseConfig {
            url: "postgres://user:pass@localhost:9999/nonexistent".to_string(),
            max_connections: 2,
            connect_timeout: 2,
        };
        // connect_lazy only validates the URL — should return Ok.
        let result = Database::from_config(&config);
        assert!(result.is_ok(), "connect_lazy should not fail for valid URL");
    }
}
