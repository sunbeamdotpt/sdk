//! Migration runner wrapper for Sunbeam services.
//!
//! Wraps `sqlx::migrate::Migrator` in a thin type that handles two common
//! usage patterns:
//!
//! 1. **Embedded migrations** — the service crate embeds its `migrations/`
//!    directory at compile time via `sqlx::migrate!()` and passes the
//!    resulting `&'static sqlx::migrate::Migrator` here.
//!
//! 2. **Runtime path** — migrations live on disk at a known path (useful for
//!    CLIs, migration tools, or tests that generate SQL on the fly).
//!
//! # Example — embedded (production pattern)
//!
//! In the service crate:
//!
//! ```rust,ignore
//! use crate::g2v::db::{Database, Migrator};
//!
//! static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
//!
//! async fn run_migrations(db: &Database) -> crate::g2v::error::ServiceResult<()> {
//!     Migrator::embedded(&MIGRATOR).run(db.pool()).await
//! }
//! ```
//!
//! # Example — runtime path (tests / CLI)
//!
//! ```rust,ignore
//! use std::path::Path;
//! use crate::g2v::db::Migrator;
//!
//! let migrator = Migrator::new(Path::new("./migrations")).await?;
//! migrator.run(&pool).await?;
//! ```

use crate::g2v::error::{ServiceError, ServiceResult};
use std::path::Path;

// ============================================================================
// Migrator
// ============================================================================

enum MigratorInner {
    Owned(sqlx::migrate::Migrator),
    Embedded(&'static sqlx::migrate::Migrator),
}

/// Thin wrapper around [`sqlx::migrate::Migrator`] that converts errors into
/// [`ServiceError::Database`].
pub struct Migrator {
    inner: MigratorInner,
}

impl Migrator {
    /// Load migrations from a directory on disk at runtime.
    ///
    /// Returns [`ServiceError::Database`] if the path doesn't exist or
    /// contains no valid migration files.
    ///
    /// This is the test / CLI pattern.  Production services prefer
    /// [`Migrator::embedded`] so migrations are baked into the binary.
    pub async fn new(migrations_path: &Path) -> ServiceResult<Self> {
        let inner = sqlx::migrate::Migrator::new(migrations_path)
            .await
            .map_err(|e| ServiceError::Database(format!("failed to load migrations: {e}")))?;
        Ok(Self {
            inner: MigratorInner::Owned(inner),
        })
    }

    /// Wrap a compile-time embedded migrator (from `sqlx::migrate!()`).
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
    /// let m = Migrator::embedded(&MIGRATOR);
    /// ```
    pub fn embedded(migrator: &'static sqlx::migrate::Migrator) -> Self {
        Self {
            inner: MigratorInner::Embedded(migrator),
        }
    }

    /// Run all pending migrations against `pool`.
    ///
    /// `sqlx` maintains the `_sqlx_migrations` ledger table and is
    /// idempotent — calling `run` a second time with no new migrations is
    /// a no-op.
    pub async fn run(&self, pool: &sqlx::PgPool) -> ServiceResult<()> {
        let result = match &self.inner {
            MigratorInner::Owned(m) => m.run(pool).await,
            MigratorInner::Embedded(m) => m.run(pool).await,
        };
        result.map_err(|e| ServiceError::Database(format!("migration failed: {e}")))?;
        Ok(())
    }
}

// ============================================================================
// Unit tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_new_nonexistent_path_returns_error() {
        let path = PathBuf::from("/tmp/sunbeam_g2v_no_such_migrations_dir_xyz");
        let result = Migrator::new(&path).await;
        assert!(result.is_err(), "expected error for nonexistent path");
        match result.err().unwrap() {
            ServiceError::Database(msg) => {
                assert!(
                    msg.contains("failed to load migrations"),
                    "unexpected message: {msg}"
                );
            }
            other => panic!("expected ServiceError::Database, got {other:?}"),
        }
    }
}
