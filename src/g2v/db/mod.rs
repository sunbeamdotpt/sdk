//! Database utilities for Sunbeam services.
//!
//! # Quick start
//!
//! ```rust,no_run
//! use crate::g2v::db::{connect, Database};
//! use crate::g2v::config::DatabaseConfig;
//!
//! # async fn example() -> crate::g2v::error::ServiceResult<()> {
//! // Lazy connect — URL validated but no socket opened yet.
//! let db: Database = connect(&DatabaseConfig::default())?;
//! db.ping().await?;
//! # Ok(())
//! # }
//! ```

pub mod connection;
pub mod crypto;
pub mod migrations;
pub mod sqlx;

// ============================================================================
// Re-exports — public surface
// ============================================================================

/// Real `PgPool`-backed database handle. Ping, query, and pass to health
/// checks via [`Database::pool`].
pub use self::sqlx::Database;

/// Create a lazily-connected [`Database`] — validates the URL, defers I/O.
pub use connection::connect;

/// Create an eagerly-connected [`Database`] — opens a socket before returning.
pub use connection::connect_eager;

/// Migration runner wrapper around `sqlx::migrate::Migrator`.
pub use migrations::Migrator;

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g2v::config::DatabaseConfig;

    #[tokio::test]
    async fn test_connect_lazy_valid_url() {
        let config = DatabaseConfig {
            url: "postgres://user:pw@localhost:9999/db".to_string(),
            max_connections: 5,
            connect_timeout: 5,
        };
        // connect_lazy does not open a socket — must succeed.
        assert!(connect(&config).is_ok());
    }

    #[tokio::test]
    async fn test_connect_bad_url_is_database_error() {
        // A URL with no scheme is rejected synchronously by sqlx.
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
}
