//! Database connection factories for Sunbeam services.
//!
//! Two entry-points:
//!
//! - [`connect`] — lazy pool; the URL is validated but no socket is opened
//!   until the first query.  Service startup is not blocked by Postgres
//!   availability.
//! - [`connect_eager`] — opens a real connection during startup so you catch
//!   misconfigured credentials immediately.

use crate::g2v::config::DatabaseConfig;
use crate::g2v::error::ServiceResult;

use super::sqlx::{Database, build_pool_eager, build_pool_lazy};

// ============================================================================
// Public API
// ============================================================================

/// Create a lazily-connected [`Database`] from a [`DatabaseConfig`].
///
/// `connect_lazy` validates the URL but defers the first socket connection
/// until the pool receives its first query.  This means a service will start
/// successfully even if Postgres is momentarily down — the error surfaces on
/// first use instead.
///
/// Pool settings applied:
/// - `max_connections` from `config.max_connections`
/// - `acquire_timeout` from `config.connect_timeout` (seconds)
/// - `min_connections = 0` — idle services release connections
///
/// # Errors
///
/// Returns [`ServiceError::Database`](crate::g2v::error::ServiceError::Database) only if the URL cannot be parsed.
pub fn connect(config: &DatabaseConfig) -> ServiceResult<Database> {
    let pool = build_pool_lazy(config)?;
    Ok(Database::from_pool(pool))
}

/// Create an eagerly-connected [`Database`] that verifies connectivity at
/// call time.
///
/// Unlike [`connect`], this opens a physical TCP connection before returning.
/// Use this when you want startup to hard-fail if the database is unreachable
/// (e.g. in a migration runner or a `connect_eager` pre-flight check).
///
/// # Errors
///
/// Returns [`ServiceError::Database`](crate::g2v::error::ServiceError::Database) if the URL is invalid or if the
/// database cannot be reached within `connect_timeout` seconds.
pub async fn connect_eager(config: &DatabaseConfig) -> ServiceResult<Database> {
    let pool = build_pool_eager(config).await?;
    Ok(Database::from_pool(pool))
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_connect_bad_url_returns_database_error() {
        // A URL with no scheme at all is rejected synchronously by sqlx.
        let config = DatabaseConfig {
            url: "not-a-valid-postgres-url".to_string(),
            max_connections: 5,
            connect_timeout: 5,
        };
        let err = connect(&config).unwrap_err();
        match err {
            crate::g2v::error::ServiceError::Database(_) => {}
            other => panic!("expected ServiceError::Database, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_connect_lazy_valid_url_succeeds_without_server() {
        // connect_lazy must not open a socket — it should succeed even when
        // the target host/port is obviously unreachable.
        let config = DatabaseConfig {
            url: "postgres://user:pw@localhost:9998/db".to_string(),
            max_connections: 2,
            connect_timeout: 1,
        };
        assert!(
            connect(&config).is_ok(),
            "connect_lazy should succeed for a valid URL without a live server"
        );
    }
}
